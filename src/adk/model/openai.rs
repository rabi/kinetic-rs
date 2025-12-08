// SPDX-License-Identifier: MIT

//! OpenAI Model - ChatGPT API implementation

use super::{Content, GenerationConfig, Model, Part};
use crate::adk::tool::Tool;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::env;
use std::error::Error;
use std::sync::Arc;

/// OpenAI ChatGPT model implementation
pub struct OpenAIModel {
    client: Client,
    api_key: String,
    model_name: String,
    base_url: String,
}

impl OpenAIModel {
    /// Create a new OpenAIModel
    ///
    /// Requires `OPENAI_API_KEY` environment variable to be set.
    /// Optionally uses `OPENAI_BASE_URL` for custom endpoints.
    pub fn new(model_name: String) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let api_key = env::var("OPENAI_API_KEY").map_err(|_| "OPENAI_API_KEY must be set")?;
        let base_url =
            env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

        Ok(Self {
            client: Client::new(),
            api_key,
            model_name,
            base_url,
        })
    }

    /// Convert internal Content to OpenAI message format
    fn content_to_openai_message(content: &Content) -> serde_json::Value {
        let role = match content.role.as_str() {
            "system" => "system",
            "user" => "user",
            "model" => "assistant",
            other => other,
        };

        // Check if this is a tool response
        for part in &content.parts {
            if let Part::FunctionResponse { name, response } = part {
                return json!({
                    "role": "tool",
                    "tool_call_id": name, // OpenAI requires matching the tool_call_id
                    "content": serde_json::to_string(response).unwrap_or_default()
                });
            }
        }

        // Check for function calls (assistant message with tool_calls)
        let mut tool_calls = Vec::new();
        let mut text_content = String::new();

        for part in &content.parts {
            match part {
                Part::Text(t) => text_content.push_str(t),
                Part::Thinking(t) => text_content.push_str(t), // Include thinking as text
                Part::FunctionCall { name, args, .. } => {
                    tool_calls.push(json!({
                        "id": name, // Use name as ID for simplicity
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(args).unwrap_or_default()
                        }
                    }));
                }
                Part::FunctionResponse { .. } => {} // Handled above
            }
        }

        if !tool_calls.is_empty() {
            json!({
                "role": role,
                "content": if text_content.is_empty() { serde_json::Value::Null } else { json!(text_content) },
                "tool_calls": tool_calls
            })
        } else {
            json!({
                "role": role,
                "content": text_content
            })
        }
    }

    /// Convert tools to OpenAI function format
    fn tools_to_openai_format(tools: &[Arc<dyn Tool>]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name(),
                        "description": t.description(),
                        "parameters": t.schema()
                    }
                })
            })
            .collect()
    }

    /// Parse OpenAI response into Content
    fn parse_openai_response(
        response: &serde_json::Value,
    ) -> Result<Content, Box<dyn Error + Send + Sync>> {
        let choice = response["choices"]
            .as_array()
            .and_then(|c| c.first())
            .ok_or("No choices in OpenAI response")?;

        let message = &choice["message"];
        let mut parts = Vec::new();

        // Parse text content
        if let Some(content) = message["content"].as_str() {
            if !content.is_empty() {
                parts.push(Part::Text(content.to_string()));
            }
        }

        // Parse tool calls
        if let Some(tool_calls) = message["tool_calls"].as_array() {
            for tc in tool_calls {
                let name = tc["function"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or(json!({}));

                parts.push(Part::FunctionCall {
                    name,
                    args,
                    thought_signature: None, // OpenAI doesn't use thought signatures
                });
            }
        }

        Ok(Content {
            role: "model".to_string(),
            parts,
        })
    }
}

#[async_trait]
impl Model for OpenAIModel {
    async fn generate_content(
        &self,
        history: &[Content],
        config: Option<&GenerationConfig>,
        tools: Option<&[Arc<dyn Tool>]>,
    ) -> Result<Content, Box<dyn Error + Send + Sync>> {
        let url = format!("{}/chat/completions", self.base_url);

        // Convert history to OpenAI message format
        let messages: Vec<serde_json::Value> = history
            .iter()
            .map(Self::content_to_openai_message)
            .collect();

        let mut body = json!({
            "model": self.model_name,
            "messages": messages
        });

        // Add generation config if provided
        if let Some(cfg) = config {
            if let Some(temp) = cfg.temperature {
                body["temperature"] = json!(temp);
            }
            if let Some(max_tokens) = cfg.max_output_tokens {
                body["max_tokens"] = json!(max_tokens);
            }
            if let Some(top_p) = cfg.top_p {
                body["top_p"] = json!(top_p);
            }
        }

        // Add tools if provided
        if let Some(tools) = tools {
            if !tools.is_empty() {
                body["tools"] = json!(Self::tools_to_openai_format(tools));
                body["tool_choice"] = json!("auto");

                log::info!(
                    "Sending tools to OpenAI: {}",
                    serde_json::to_string_pretty(&body["tools"]).unwrap_or_default()
                );
            }
        }

        log::debug!(
            "OpenAI request body: {}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await?;
            return Err(format!("OpenAI API error: {}", text).into());
        }

        let resp_json: serde_json::Value = resp.json().await?;
        log::info!("OpenAI response: {}", resp_json);

        Self::parse_openai_response(&resp_json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_content_to_openai_user_message() {
        let content = Content {
            role: "user".to_string(),
            parts: vec![Part::Text("Hello".to_string())],
        };

        let msg = OpenAIModel::content_to_openai_message(&content);
        assert_eq!(msg["role"], "user");
        assert_eq!(msg["content"], "Hello");
    }

    #[test]
    fn test_content_to_openai_system_message() {
        let content = Content {
            role: "system".to_string(),
            parts: vec![Part::Text("You are helpful".to_string())],
        };

        let msg = OpenAIModel::content_to_openai_message(&content);
        assert_eq!(msg["role"], "system");
        assert_eq!(msg["content"], "You are helpful");
    }

    #[test]
    fn test_content_to_openai_assistant_message() {
        let content = Content {
            role: "model".to_string(),
            parts: vec![Part::Text("I can help".to_string())],
        };

        let msg = OpenAIModel::content_to_openai_message(&content);
        assert_eq!(msg["role"], "assistant");
        assert_eq!(msg["content"], "I can help");
    }

    #[test]
    fn test_content_to_openai_with_function_call() {
        let content = Content {
            role: "model".to_string(),
            parts: vec![Part::FunctionCall {
                name: "search".to_string(),
                args: json!({"query": "rust"}),
                thought_signature: None,
            }],
        };

        let msg = OpenAIModel::content_to_openai_message(&content);
        assert_eq!(msg["role"], "assistant");
        assert!(msg["tool_calls"].is_array());

        let tool_call = &msg["tool_calls"][0];
        assert_eq!(tool_call["function"]["name"], "search");
    }

    #[test]
    fn test_parse_openai_text_response() {
        let response = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello, how can I help?"
                }
            }]
        });

        let content = OpenAIModel::parse_openai_response(&response).unwrap();
        assert_eq!(content.role, "model");
        assert_eq!(content.parts.len(), 1);

        match &content.parts[0] {
            Part::Text(t) => assert_eq!(t, "Hello, how can I help?"),
            _ => panic!("Expected Text part"),
        }
    }

    #[test]
    fn test_parse_openai_function_call_response() {
        let response = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"city\": \"London\"}"
                        }
                    }]
                }
            }]
        });

        let content = OpenAIModel::parse_openai_response(&response).unwrap();
        assert_eq!(content.parts.len(), 1);

        match &content.parts[0] {
            Part::FunctionCall { name, args, .. } => {
                assert_eq!(name, "get_weather");
                assert_eq!(args["city"], "London");
            }
            _ => panic!("Expected FunctionCall part"),
        }
    }
}
