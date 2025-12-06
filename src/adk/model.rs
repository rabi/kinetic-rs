use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    pub role: String,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Part {
    Text(String),
    FunctionCall {
        name: String,
        args: serde_json::Value,
        thought_signature: Option<String>,
    },
    FunctionResponse {
        name: String,
        response: serde_json::Value,
    },
}

use crate::adk::tool::Tool;
use std::sync::Arc;

#[async_trait]
pub trait Model: Send + Sync {
    async fn generate_content(
        &self,
        history: &[Content],
        config: Option<&GenerationConfig>,
        tools: Option<&[Arc<dyn Tool>]>,
    ) -> Result<Content, Box<dyn Error + Send + Sync>>;
}
