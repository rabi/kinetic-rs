//! Anthropic Model - Claude API implementation

use super::{Content, GenerationConfig, Model, Part};
use crate::adk::tool::Tool;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::env;
use std::error::Error;
use std::sync::Arc;

/// Anthropic Claude model implementation
pub struct AnthropicModel {
    client: Client,
    api_key: String,
    model_name: String,
    base_url: String,
}

impl AnthropicModel {
    /// Create a new AnthropicModel
    ///
    /// Requires `ANTHROPIC_API_KEY` environment variable to be set.
    /// Optionally uses `ANTHROPIC_BASE_URL` for custom endpoints.
    pub fn new(model_name: String) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let api_key = env::var("ANTHROPIC_API_KEY").map_err(|_| "ANTHROPIC_API_KEY must be set")?;
        let base_url = env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com/v1".to_string());

        Ok(Self {
            client: Client::new(),
            api_key,
            model_name,
            base_url,
        })
    }

    /// Extract system message from history
    fn extract_system_message(history: &[Content]) -> Option<String> {
        history
            .iter()
            .find(|c| c.role == "system")
            .and_then(|c| c.parts.first())
            .and_then(|p| match p {
                Part::Text(t) => Some(t.clone()),
                _ => None,
            })
    }

    /// Convert internal Content to Anthropic message format
    fn content_to_anthropic_message(content: &Content) -> Option<serde_json::Value> {
        // Skip system messages (handled separately)
        if content.role == "system" {
            return None;
        }

        let role = match content.role.as_str() {
            "user" => "user",
            "model" => "assistant",
            other => other,
        };

        let mut message_content = Vec::new();

        for part in &content.parts {
            match part {
                Part::Text(t) => {
                    message_content.push(json!({
                        "type": "text",
                        "text": t
                    }));
                }
                Part::Thinking(t) => {
                    // Anthropic supports thinking blocks natively
                    message_content.push(json!({
                        "type": "thinking",
                        "thinking": t
                    }));
                }
                Part::FunctionCall { name, args, .. } => {
                    message_content.push(json!({
                        "type": "tool_use",
                        "id": format!("tool_{}", name), // Generate an ID
                        "name": name,
                        "input": args
                    }));
                }
                Part::FunctionResponse { name, response } => {
                    message_content.push(json!({
                        "type": "tool_result",
                        "tool_use_id": format!("tool_{}", name),
                        "content": serde_json::to_string(response).unwrap_or_default()
                    }));
                }
            }
        }

        if message_content.is_empty() {
            return None;
        }

        Some(json!({
            "role": role,
            "content": message_content
        }))
    }

    /// Convert tools to Anthropic tool format
    fn tools_to_anthropic_format(tools: &[Arc<dyn Tool>]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name(),
                    "description": t.description(),
                    "input_schema": t.schema()
                })
            })
            .collect()
    }

    /// Parse Anthropic response into Content
    fn parse_anthropic_response(
        response: &serde_json::Value,
    ) -> Result<Content, Box<dyn Error + Send + Sync>> {
        let content_blocks = response["content"]
            .as_array()
            .ok_or("No content in Anthropic response")?;

        let mut parts = Vec::new();

        for block in content_blocks {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(text) = block["text"].as_str() {
                        if !text.is_empty() {
                            parts.push(Part::Text(text.to_string()));
                        }
                    }
                }
                Some("thinking") => {
                    if let Some(thinking) = block["thinking"].as_str() {
                        if !thinking.is_empty() {
                            parts.push(Part::Thinking(thinking.to_string()));
                        }
                    }
                }
                Some("tool_use") => {
                    let name = block["name"].as_str().unwrap_or_default().to_string();
                    let args = block["input"].clone();

                    parts.push(Part::FunctionCall {
                        name,
                        args,
                        thought_signature: None, // Anthropic doesn't use thought signatures
                    });
                }
                _ => {}
            }
        }

        // Check stop reason
        if let Some(stop_reason) = response["stop_reason"].as_str() {
            log::debug!("Anthropic stop reason: {}", stop_reason);
        }

        Ok(Content {
            role: "model".to_string(),
            parts,
        })
    }
}

#[async_trait]
impl Model for AnthropicModel {
    async fn generate_content(
        &self,
        history: &[Content],
        config: Option<&GenerationConfig>,
        tools: Option<&[Arc<dyn Tool>]>,
    ) -> Result<Content, Box<dyn Error + Send + Sync>> {
        let url = format!("{}/messages", self.base_url);

        // Extract system message
        let system = Self::extract_system_message(history);

        // Convert history to Anthropic message format (excluding system)
        let messages: Vec<serde_json::Value> = history
            .iter()
            .filter_map(Self::content_to_anthropic_message)
            .collect();

        let mut body = json!({
            "model": self.model_name,
            "messages": messages,
            "max_tokens": config.and_then(|c| c.max_output_tokens).unwrap_or(4096)
        });

        // Add system message if present
        if let Some(sys) = system {
            body["system"] = json!(sys);
        }

        // Add generation config
        if let Some(cfg) = config {
            if let Some(temp) = cfg.temperature {
                body["temperature"] = json!(temp);
            }
            if let Some(top_p) = cfg.top_p {
                body["top_p"] = json!(top_p);
            }
            if let Some(top_k) = cfg.top_k {
                body["top_k"] = json!(top_k);
            }
        }

        // Add tools if provided
        if let Some(tools) = tools {
            if !tools.is_empty() {
                body["tools"] = json!(Self::tools_to_anthropic_format(tools));

                log::info!(
                    "Sending tools to Anthropic: {}",
                    serde_json::to_string_pretty(&body["tools"]).unwrap_or_default()
                );
            }
        }

        log::debug!(
            "Anthropic request body: {}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await?;
            return Err(format!("Anthropic API error: {}", text).into());
        }

        let resp_json: serde_json::Value = resp.json().await?;
        log::info!("Anthropic response: {}", resp_json);

        Self::parse_anthropic_response(&resp_json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_system_message() {
        let history = vec![
            Content {
                role: "system".to_string(),
                parts: vec![Part::Text("You are helpful".to_string())],
            },
            Content {
                role: "user".to_string(),
                parts: vec![Part::Text("Hello".to_string())],
            },
        ];

        let system = AnthropicModel::extract_system_message(&history);
        assert_eq!(system, Some("You are helpful".to_string()));
    }

    #[test]
    fn test_content_to_anthropic_user_message() {
        let content = Content {
            role: "user".to_string(),
            parts: vec![Part::Text("Hello".to_string())],
        };

        let msg = AnthropicModel::content_to_anthropic_message(&content).unwrap();
        assert_eq!(msg["role"], "user");
        assert_eq!(msg["content"][0]["type"], "text");
        assert_eq!(msg["content"][0]["text"], "Hello");
    }

    #[test]
    fn test_content_to_anthropic_assistant_message() {
        let content = Content {
            role: "model".to_string(),
            parts: vec![Part::Text("I can help".to_string())],
        };

        let msg = AnthropicModel::content_to_anthropic_message(&content).unwrap();
        assert_eq!(msg["role"], "assistant");
        assert_eq!(msg["content"][0]["text"], "I can help");
    }

    #[test]
    fn test_content_to_anthropic_system_returns_none() {
        let content = Content {
            role: "system".to_string(),
            parts: vec![Part::Text("System prompt".to_string())],
        };

        assert!(AnthropicModel::content_to_anthropic_message(&content).is_none());
    }

    #[test]
    fn test_content_to_anthropic_with_tool_use() {
        let content = Content {
            role: "model".to_string(),
            parts: vec![Part::FunctionCall {
                name: "search".to_string(),
                args: json!({"query": "rust"}),
                thought_signature: None,
            }],
        };

        let msg = AnthropicModel::content_to_anthropic_message(&content).unwrap();
        assert_eq!(msg["content"][0]["type"], "tool_use");
        assert_eq!(msg["content"][0]["name"], "search");
    }

    #[test]
    fn test_parse_anthropic_text_response() {
        let response = json!({
            "content": [{
                "type": "text",
                "text": "Hello, how can I help?"
            }],
            "stop_reason": "end_turn"
        });

        let content = AnthropicModel::parse_anthropic_response(&response).unwrap();
        assert_eq!(content.role, "model");
        assert_eq!(content.parts.len(), 1);

        match &content.parts[0] {
            Part::Text(t) => assert_eq!(t, "Hello, how can I help?"),
            _ => panic!("Expected Text part"),
        }
    }

    #[test]
    fn test_parse_anthropic_tool_use_response() {
        let response = json!({
            "content": [{
                "type": "tool_use",
                "id": "tool_123",
                "name": "get_weather",
                "input": {"city": "London"}
            }],
            "stop_reason": "tool_use"
        });

        let content = AnthropicModel::parse_anthropic_response(&response).unwrap();
        assert_eq!(content.parts.len(), 1);

        match &content.parts[0] {
            Part::FunctionCall { name, args, .. } => {
                assert_eq!(name, "get_weather");
                assert_eq!(args["city"], "London");
            }
            _ => panic!("Expected FunctionCall part"),
        }
    }

    #[test]
    fn test_parse_anthropic_thinking_response() {
        let response = json!({
            "content": [
                {
                    "type": "thinking",
                    "thinking": "Let me think about this..."
                },
                {
                    "type": "text",
                    "text": "The answer is 42"
                }
            ],
            "stop_reason": "end_turn"
        });

        let content = AnthropicModel::parse_anthropic_response(&response).unwrap();
        assert_eq!(content.parts.len(), 2);

        match &content.parts[0] {
            Part::Thinking(t) => assert_eq!(t, "Let me think about this..."),
            _ => panic!("Expected Thinking part"),
        }

        match &content.parts[1] {
            Part::Text(t) => assert_eq!(t, "The answer is 42"),
            _ => panic!("Expected Text part"),
        }
    }
}
