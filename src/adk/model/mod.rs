// SPDX-License-Identifier: MIT

//! Model module - defines LLM model trait and implementations
//!
//! This module provides the core Model trait and shared types.
//! Model implementations are in their own submodules:
//! - [anthropic] - Anthropic's Claude API
//! - [gemini] - Google's Gemini API
//! - [openai] - OpenAI's ChatGPT API

pub mod anthropic;
pub mod gemini;
pub mod openai;

use crate::adk::tool::Tool;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::Arc;

/// Configuration for model generation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GenerationConfig {
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    pub role: String,
    pub parts: Vec<Part>,
}

/// Parts of a message - text, thinking, function calls, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Part {
    /// Regular text output from the model
    Text(String),
    /// Thinking/reasoning content from thinking models (e.g., Gemini's thinking mode)
    Thinking(String),
    /// Function/tool call requested by the model
    FunctionCall {
        name: String,
        args: serde_json::Value,
        /// Thought signature from Gemini thinking models - must be preserved and sent back
        #[serde(skip_serializing_if = "Option::is_none")]
        thought_signature: Option<String>,
    },
    /// Response from executing a function/tool
    FunctionResponse {
        name: String,
        response: serde_json::Value,
    },
}

/// Core trait for LLM model implementations
#[async_trait]
pub trait Model: Send + Sync {
    async fn generate_content(
        &self,
        history: &[Content],
        config: Option<&GenerationConfig>,
        tools: Option<&[Arc<dyn Tool>]>,
    ) -> Result<Content, Box<dyn Error + Send + Sync>>;
}
