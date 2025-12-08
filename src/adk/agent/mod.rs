// SPDX-License-Identifier: MIT

//! Agent module - defines agent types for AI workflows
//!
//! This module provides the core Agent trait and implementations:
//! - `LLMAgent` - Standard LLM agent with tool calling
//! - `ReActAgent` - Reasoning + Acting pattern agent

mod llm;
mod react;

pub use llm::LLMAgent;
pub use react::ReActAgent;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    Thought(String),
    ToolCall {
        name: String,
        args: serde_json::Value,
    },
    ToolResult {
        name: String,
        result: serde_json::Value,
    },
    Answer(String),
    Error(String),
    Log(String),
}

/// Core agent trait for all agent types
#[async_trait]
pub trait Agent: Send + Sync {
    /// Returns the agent name
    fn name(&self) -> &str;

    /// Run the agent with the given input
    async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>>;

    /// Run the agent with streaming events
    async fn run_stream(
        &self,
        input: String,
        tx: mpsc::Sender<AgentEvent>,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        // Default implementation falls back to run()
        match self.run(input).await {
            Ok(res) => {
                let _ = tx.send(AgentEvent::Answer(res.clone())).await;
                Ok(res)
            }
            Err(e) => {
                let _ = tx.send(AgentEvent::Error(e.to_string())).await;
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple mock agent that transforms input (used in tests)
    pub struct MockAgent {
        name: String,
        transform: fn(String) -> String,
    }

    impl MockAgent {
        pub fn new(name: &str, transform: fn(String) -> String) -> Self {
            Self {
                name: name.to_string(),
                transform,
            }
        }
    }

    #[async_trait]
    impl Agent for MockAgent {
        fn name(&self) -> &str {
            &self.name
        }

        async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
            Ok((self.transform)(input))
        }
    }

    #[tokio::test]
    async fn test_mock_agent() {
        let agent = MockAgent::new("test", |s| format!("{}-transformed", s));
        assert_eq!(agent.name(), "test");

        let result = agent.run("input".to_string()).await.unwrap();
        assert_eq!(result, "input-transformed");
    }
}
