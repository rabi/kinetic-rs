use crate::adk::model::{Content, Model, Part};
use crate::adk::tool::Tool;
use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;

#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> String;
    async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>>;
}

pub struct LLMAgent {
    pub name: String,
    pub description: String,
    pub instruction: String,
    pub model: Arc<dyn Model>,
    pub tools: Vec<Arc<dyn Tool>>,
}

impl LLMAgent {
    pub fn new(
        name: String,
        description: String,
        instruction: String,
        model: Arc<dyn Model>,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Self {
        Self {
            name,
            description,
            instruction,
            model,
            tools,
        }
    }
}

#[async_trait]
impl Agent for LLMAgent {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut history = vec![
            Content {
                role: "system".to_string(),
                parts: vec![Part::Text(self.instruction.clone())],
            },
            Content {
                role: "user".to_string(),
                parts: vec![Part::Text(input)],
            },
        ];

        let max_turns = 10;
        for turn in 0..max_turns {
            log::info!("Agent {} turn {}/{}", self.name, turn + 1, max_turns);
            let response = self
                .model
                .generate_content(&history, None, Some(&self.tools))
                .await?;

            log::info!(
                "Agent {} received response with {} parts",
                self.name,
                response.parts.len()
            );

            // Add model response to history
            history.push(response.clone());

            // Check if response contains a text part (final answer)
            for part in &response.parts {
                if let Part::Text(text) = part {
                    if !text.is_empty() {
                        log::info!(
                            "Agent {} returning text response (length: {}, preview: '{}')",
                            self.name,
                            text.len(),
                            if text.len() > 100 { &text[..100] } else { text }
                        );
                        return Ok(text.clone());
                    }
                }
            }

            // Collect all function calls from the response
            let function_calls: Vec<_> = response
                .parts
                .iter()
                .filter_map(|part| {
                    if let Part::FunctionCall { name, args, .. } = part {
                        Some((name.clone(), args.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            if function_calls.is_empty() {
                // No text and no function calls - this is unexpected
                log::warn!("Agent {} received empty response with no function calls", self.name);
                return Ok(String::new());
            }

            // Execute all function calls and collect responses
            let mut function_responses = Vec::new();
            for (name, args) in function_calls {
                log::info!("Tool call: {} {:?}", name, args);
                
                let tool = self.tools.iter().find(|t| t.name() == name);
                let tool_response = if let Some(t) = tool {
                    match t.execute(args.clone()).await {
                        Ok(res) => res,
                        Err(e) => {
                            log::error!("Tool {} failed: {}", name, e);
                            serde_json::json!({ "error": e.to_string() })
                        }
                    }
                } else {
                    log::error!("Tool {} not found", name);
                    serde_json::json!({ "error": format!("Tool {} not found", name) })
                };

                log::info!(
                    "Tool {} response: {}",
                    name,
                    serde_json::to_string(&tool_response).unwrap_or_default()
                );

                function_responses.push(Part::FunctionResponse {
                    name: name.clone(),
                    response: tool_response,
                });
            }

            // Add all tool responses to history in a single message
            history.push(Content {
                role: "user".to_string(),
                parts: function_responses,
            });

            log::info!("Continuing to next turn to get model summary...");
        }

        log::error!(
            "Agent {} reached max turns without text response",
            self.name
        );
        Err("Max turns reached".into())
    }
}

pub struct SequentialAgent {
    pub name: String,
    pub description: String,
    pub sub_agents: Vec<Arc<dyn Agent>>,
}

impl SequentialAgent {
    pub fn new(name: String, description: String, sub_agents: Vec<Arc<dyn Agent>>) -> Self {
        Self {
            name,
            description,
            sub_agents,
        }
    }
}

#[async_trait]
impl Agent for SequentialAgent {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut current_input = input;
        for agent in &self.sub_agents {
            current_input = agent.run(current_input).await?;
        }
        Ok(current_input)
    }
}

pub struct ParallelAgent {
    pub name: String,
    pub description: String,
    pub sub_agents: Vec<Arc<dyn Agent>>,
}

impl ParallelAgent {
    pub fn new(name: String, description: String, sub_agents: Vec<Arc<dyn Agent>>) -> Self {
        Self {
            name,
            description,
            sub_agents,
        }
    }
}

#[async_trait]
impl Agent for ParallelAgent {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut handles = vec![];

        for agent in &self.sub_agents {
            let agent = agent.clone();
            let input = input.clone();
            handles.push(tokio::spawn(async move { agent.run(input).await }));
        }

        let mut results = Vec::new();
        for handle in handles {
            let res = handle.await??;
            results.push(res);
        }

        // Combine results (simplified)
        Ok(results.join("\n---\n"))
    }
}

pub struct LoopAgent {
    pub name: String,
    pub description: String,
    pub agent: Arc<dyn Agent>,
    pub max_iterations: u32,
}

impl LoopAgent {
    pub fn new(
        name: String,
        description: String,
        agent: Arc<dyn Agent>,
        max_iterations: u32,
    ) -> Self {
        Self {
            name,
            description,
            agent,
            max_iterations,
        }
    }
}

#[async_trait]
impl Agent for LoopAgent {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut current_input = input;
        for _ in 0..self.max_iterations {
            current_input = self.agent.run(current_input).await?;
            // In a real loop agent, we'd check for a termination condition here
        }
        Ok(current_input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple mock agent that transforms input
    struct MockAgent {
        name: String,
        transform: fn(String) -> String,
    }

    impl MockAgent {
        fn new(name: &str, transform: fn(String) -> String) -> Self {
            Self {
                name: name.to_string(),
                transform,
            }
        }
    }

    #[async_trait]
    impl Agent for MockAgent {
        fn name(&self) -> String {
            self.name.clone()
        }

        async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
            Ok((self.transform)(input))
        }
    }

    #[tokio::test]
    async fn test_sequential_agent_chains_output() {
        let agent1 = Arc::new(MockAgent::new("agent1", |s| format!("{}-step1", s)));
        let agent2 = Arc::new(MockAgent::new("agent2", |s| format!("{}-step2", s)));
        let agent3 = Arc::new(MockAgent::new("agent3", |s| format!("{}-step3", s)));

        let seq = SequentialAgent::new(
            "sequential".to_string(),
            "test".to_string(),
            vec![agent1, agent2, agent3],
        );

        let result = seq.run("input".to_string()).await.unwrap();
        assert_eq!(result, "input-step1-step2-step3");
    }

    #[tokio::test]
    async fn test_sequential_agent_empty() {
        let seq = SequentialAgent::new(
            "empty".to_string(),
            "test".to_string(),
            vec![],
        );

        let result = seq.run("input".to_string()).await.unwrap();
        assert_eq!(result, "input");
    }

    #[tokio::test]
    async fn test_parallel_agent_combines_output() {
        let agent1 = Arc::new(MockAgent::new("agent1", |_| "result1".to_string()));
        let agent2 = Arc::new(MockAgent::new("agent2", |_| "result2".to_string()));

        let parallel = ParallelAgent::new(
            "parallel".to_string(),
            "test".to_string(),
            vec![agent1, agent2],
        );

        let result = parallel.run("input".to_string()).await.unwrap();
        // Results are joined with \n---\n
        assert!(result.contains("result1"));
        assert!(result.contains("result2"));
        assert!(result.contains("---"));
    }

    #[tokio::test]
    async fn test_loop_agent_iterates() {
        let agent = Arc::new(MockAgent::new("appender", |s| format!("{}-iter", s)));

        let loop_agent = LoopAgent::new(
            "loop".to_string(),
            "test".to_string(),
            agent,
            3,
        );

        let result = loop_agent.run("start".to_string()).await.unwrap();
        assert_eq!(result, "start-iter-iter-iter");
    }

    #[tokio::test]
    async fn test_loop_agent_zero_iterations() {
        let agent = Arc::new(MockAgent::new("agent", |s| format!("{}-iter", s)));

        let loop_agent = LoopAgent::new(
            "loop".to_string(),
            "test".to_string(),
            agent,
            0,
        );

        let result = loop_agent.run("input".to_string()).await.unwrap();
        assert_eq!(result, "input"); // No iterations, returns original input
    }

    #[tokio::test]
    async fn test_agent_names() {
        let mock = MockAgent::new("test_name", |s| s);
        assert_eq!(mock.name(), "test_name");

        let seq = SequentialAgent::new("seq_name".to_string(), "desc".to_string(), vec![]);
        assert_eq!(seq.name(), "seq_name");

        let parallel = ParallelAgent::new("par_name".to_string(), "desc".to_string(), vec![]);
        assert_eq!(parallel.name(), "par_name");
    }
}
