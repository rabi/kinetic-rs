// SPDX-License-Identifier: MIT

//! Gemini Model - Google's Gemini API implementation

use super::{Content, GenerationConfig, Model, Part};
use crate::adk::tool::Tool;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::env;
use std::error::Error;
use std::sync::Arc;

/// Google Gemini model implementation
pub struct GeminiModel {
    client: Client,
    api_key: String,
    model_name: String,
}

impl GeminiModel {
    /// Create a new GeminiModel
    ///
    /// Requires `GOOGLE_API_KEY` environment variable to be set.
    pub fn new(model_name: String) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let api_key = env::var("GOOGLE_API_KEY").map_err(|_| "GOOGLE_API_KEY must be set")?;
        Ok(Self {
            client: Client::new(),
            api_key,
            model_name,
        })
    }
}

#[async_trait]
impl Model for GeminiModel {
    async fn generate_content(
        &self,
        history: &[Content],
        _config: Option<&GenerationConfig>,
        tools: Option<&[Arc<dyn Tool>]>,
    ) -> Result<Content, Box<dyn Error + Send + Sync>> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model_name, self.api_key
        );

        let contents: Vec<serde_json::Value> = history
            .iter()
            .map(|c| {
                let parts: Vec<serde_json::Value> = c
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Text(t) => Some(json!({ "text": t })),
                        Part::Thinking(_) => None, // Thinking is internal, not sent to API
                        Part::FunctionCall {
                            name,
                            args,
                            thought_signature,
                        } => {
                            let mut fc = json!({ "functionCall": { "name": name, "args": args } });
                            // Include thought_signature if present (required by Gemini thinking models)
                            if let Some(sig) = thought_signature {
                                fc["thoughtSignature"] = json!(sig);
                            }
                            Some(fc)
                        }
                        Part::FunctionResponse { name, response } => Some(
                            json!({ "functionResponse": { "name": name, "response": response } }),
                        ),
                    })
                    .collect();
                json!({ "role": c.role, "parts": parts })
            })
            .collect();

        let mut body = json!({
            "contents": contents
        });

        if let Some(tools) = tools {
            if !tools.is_empty() {
                let function_declarations: Vec<serde_json::Value> = tools
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t.name(),
                            "description": t.description(),
                            "parameters": t.schema()
                        })
                    })
                    .collect();

                body["tools"] = json!([{
                    "function_declarations": function_declarations
                }]);

                log::info!(
                    "Sending tools to Gemini: {}",
                    serde_json::to_string_pretty(&body["tools"]).unwrap_or_default()
                );
            }
        }

        log::debug!(
            "Gemini request body: {}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );

        let resp = self.client.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let text = resp.text().await?;
            return Err(format!("Gemini API error: {}", text).into());
        }

        let resp_json: serde_json::Value = resp.json().await?;
        log::info!("Gemini response: {}", resp_json);

        let candidates = resp_json["candidates"]
            .as_array()
            .ok_or("No candidates in response")?;
        let candidate = candidates.first().ok_or("Empty candidates")?;

        // Check for error conditions
        if let Some(finish_reason) = candidate.get("finishReason").and_then(|v| v.as_str()) {
            log::debug!("Gemini finish reason: {}", finish_reason);
            if finish_reason == "UNEXPECTED_TOOL_CALL" {
                return Err(
                    "Gemini returned UNEXPECTED_TOOL_CALL. The tool schema may be incompatible."
                        .into(),
                );
            }
            if finish_reason == "SAFETY" {
                return Err("Gemini blocked response due to safety filters.".into());
            }
            if finish_reason == "MALFORMED_FUNCTION_CALL" {
                // Model tried to call a tool that doesn't exist - return as text
                if let Some(msg) = candidate.get("finishMessage").and_then(|m| m.as_str()) {
                    log::warn!("Gemini malformed function call: {}", msg);
                    return Ok(Content {
                        role: "model".to_string(),
                        parts: vec![Part::Text(format!(
                            "I tried to use a tool that isn't available. {}",
                            msg
                        ))],
                    });
                }
            }
        }

        let content = match candidate.get("content").and_then(|c| c.as_object()) {
            Some(c) => c,
            None => {
                log::error!("No content in candidate. Full response: {}", resp_json);
                return Err(
                    format!("No content in Gemini response. Candidate: {}", candidate).into(),
                );
            }
        };

        let parts_json = match content.get("parts").and_then(|v| v.as_array()) {
            Some(p) => p,
            None => {
                log::error!("No parts in content. Content: {:?}", content);
                return Err(format!("No parts in content. Content: {:?}", content).into());
            }
        };

        let mut parts = Vec::new();
        for p in parts_json {
            // Check for thinking/reasoning content from thinking models
            // Gemini may return this as "thought" or in thinking-specific fields
            if let Some(thought) = p.get("thought").and_then(|t| t.as_str()) {
                if !thought.is_empty() {
                    parts.push(Part::Thinking(thought.to_string()));
                }
            }

            // Regular text content
            if let Some(text) = p["text"].as_str() {
                parts.push(Part::Text(text.to_string()));
            } else if let Some(fc) = p.get("functionCall") {
                let name = fc["name"].as_str().unwrap_or_default().to_string();
                let args = fc["args"].clone();
                // Capture thought_signature (required for Gemini thinking models)
                let thought_signature = p
                    .get("thoughtSignature")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                parts.push(Part::FunctionCall {
                    name,
                    args,
                    thought_signature,
                });
            }
        }

        Ok(Content {
            role: "model".to_string(),
            parts,
        })
    }
}

/// Serialize a Part to Gemini API JSON format
/// Returns None for parts that shouldn't be sent (e.g., Thinking)
pub fn part_to_gemini_json(part: &Part) -> Option<serde_json::Value> {
    match part {
        Part::Text(t) => Some(json!({ "text": t })),
        Part::Thinking(_) => None, // Thinking is internal, not sent to API
        Part::FunctionCall {
            name,
            args,
            thought_signature,
        } => {
            let mut fc = json!({ "functionCall": { "name": name, "args": args } });
            if let Some(sig) = thought_signature {
                fc["thoughtSignature"] = json!(sig);
            }
            Some(fc)
        }
        Part::FunctionResponse { name, response } => {
            Some(json!({ "functionResponse": { "name": name, "response": response } }))
        }
    }
}

/// Parse a Gemini API JSON part into a Part
pub fn parse_gemini_part(p: &serde_json::Value) -> Vec<Part> {
    let mut parts = Vec::new();

    // Check for thinking/reasoning content
    if let Some(thought) = p.get("thought").and_then(|t| t.as_str()) {
        if !thought.is_empty() {
            parts.push(Part::Thinking(thought.to_string()));
        }
    }

    // Regular text content
    if let Some(text) = p["text"].as_str() {
        parts.push(Part::Text(text.to_string()));
    } else if let Some(fc) = p.get("functionCall") {
        let name = fc["name"].as_str().unwrap_or_default().to_string();
        let args = fc["args"].clone();
        let thought_signature = p
            .get("thoughtSignature")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        parts.push(Part::FunctionCall {
            name,
            args,
            thought_signature,
        });
    }

    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // === Serialization Tests ===

    #[test]
    fn test_serialize_text_part() {
        let part = Part::Text("Hello world".to_string());
        let json = part_to_gemini_json(&part).unwrap();
        assert_eq!(json, json!({ "text": "Hello world" }));
    }

    #[test]
    fn test_serialize_thinking_part_returns_none() {
        let part = Part::Thinking("Internal reasoning".to_string());
        assert!(part_to_gemini_json(&part).is_none());
    }

    #[test]
    fn test_serialize_function_call_without_thought_signature() {
        let part = Part::FunctionCall {
            name: "search".to_string(),
            args: json!({"query": "rust"}),
            thought_signature: None,
        };
        let json = part_to_gemini_json(&part).unwrap();

        assert_eq!(json["functionCall"]["name"], "search");
        assert_eq!(json["functionCall"]["args"]["query"], "rust");
        assert!(json.get("thoughtSignature").is_none());
    }

    #[test]
    fn test_serialize_function_call_with_thought_signature() {
        let part = Part::FunctionCall {
            name: "search".to_string(),
            args: json!({"query": "rust"}),
            thought_signature: Some("sig123abc".to_string()),
        };
        let json = part_to_gemini_json(&part).unwrap();

        assert_eq!(json["functionCall"]["name"], "search");
        assert_eq!(json["functionCall"]["args"]["query"], "rust");
        assert_eq!(json["thoughtSignature"], "sig123abc");
    }

    #[test]
    fn test_serialize_function_response() {
        let part = Part::FunctionResponse {
            name: "search".to_string(),
            response: json!({"results": ["a", "b"]}),
        };
        let json = part_to_gemini_json(&part).unwrap();

        assert_eq!(json["functionResponse"]["name"], "search");
        assert_eq!(
            json["functionResponse"]["response"]["results"],
            json!(["a", "b"])
        );
    }

    // === Parsing Tests ===

    #[test]
    fn test_parse_text_part() {
        let json = json!({ "text": "Hello world" });
        let parts = parse_gemini_part(&json);

        assert_eq!(parts.len(), 1);
        match &parts[0] {
            Part::Text(t) => assert_eq!(t, "Hello world"),
            _ => panic!("Expected Text part"),
        }
    }

    #[test]
    fn test_parse_thinking_part() {
        let json = json!({ "thought": "Let me think about this..." });
        let parts = parse_gemini_part(&json);

        assert_eq!(parts.len(), 1);
        match &parts[0] {
            Part::Thinking(t) => assert_eq!(t, "Let me think about this..."),
            _ => panic!("Expected Thinking part"),
        }
    }

    #[test]
    fn test_parse_function_call_without_thought_signature() {
        let json = json!({
            "functionCall": {
                "name": "get_weather",
                "args": {"city": "London"}
            }
        });
        let parts = parse_gemini_part(&json);

        assert_eq!(parts.len(), 1);
        match &parts[0] {
            Part::FunctionCall {
                name,
                args,
                thought_signature,
            } => {
                assert_eq!(name, "get_weather");
                assert_eq!(args["city"], "London");
                assert!(thought_signature.is_none());
            }
            _ => panic!("Expected FunctionCall part"),
        }
    }

    #[test]
    fn test_parse_function_call_with_thought_signature() {
        let json = json!({
            "functionCall": {
                "name": "search",
                "args": {"q": "rust programming"}
            },
            "thoughtSignature": "EvoRCvcRAXLI2nw7..."
        });
        let parts = parse_gemini_part(&json);

        assert_eq!(parts.len(), 1);
        match &parts[0] {
            Part::FunctionCall {
                name,
                args,
                thought_signature,
            } => {
                assert_eq!(name, "search");
                assert_eq!(args["q"], "rust programming");
                assert_eq!(thought_signature.as_ref().unwrap(), "EvoRCvcRAXLI2nw7...");
            }
            _ => panic!("Expected FunctionCall part"),
        }
    }

    #[test]
    fn test_parse_empty_thought_ignored() {
        let json = json!({ "thought": "", "text": "Hello" });
        let parts = parse_gemini_part(&json);

        // Empty thought should be ignored, only text should be parsed
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            Part::Text(t) => assert_eq!(t, "Hello"),
            _ => panic!("Expected Text part"),
        }
    }

    // === Round-trip Tests ===

    #[test]
    fn test_function_call_round_trip_preserves_thought_signature() {
        // Simulate receiving a function call from Gemini
        let received_json = json!({
            "functionCall": {
                "name": "fetch_data",
                "args": {"id": 123}
            },
            "thoughtSignature": "original_signature_xyz"
        });

        // Parse it
        let parts = parse_gemini_part(&received_json);
        assert_eq!(parts.len(), 1);

        // Serialize it back (as would happen when sending history back to Gemini)
        let serialized = part_to_gemini_json(&parts[0]).unwrap();

        // Verify thought_signature is preserved
        assert_eq!(serialized["thoughtSignature"], "original_signature_xyz");
        assert_eq!(serialized["functionCall"]["name"], "fetch_data");
    }

    #[test]
    fn test_multi_turn_conversation_preserves_signatures() {
        // Simulate a multi-turn conversation
        let history = [
            Content {
                role: "user".to_string(),
                parts: vec![Part::Text("Search for Rust".to_string())],
            },
            Content {
                role: "model".to_string(),
                parts: vec![Part::FunctionCall {
                    name: "search".to_string(),
                    args: json!({"q": "Rust"}),
                    thought_signature: Some("turn1_sig".to_string()),
                }],
            },
            Content {
                role: "user".to_string(),
                parts: vec![Part::FunctionResponse {
                    name: "search".to_string(),
                    response: json!({"results": ["Rust lang"]}),
                }],
            },
        ];

        // Serialize the history (as GeminiModel does)
        let serialized: Vec<serde_json::Value> = history
            .iter()
            .map(|c| {
                let parts: Vec<serde_json::Value> =
                    c.parts.iter().filter_map(part_to_gemini_json).collect();
                json!({ "role": c.role, "parts": parts })
            })
            .collect();

        // Verify the function call in turn 2 has the thought_signature
        let turn2_parts = serialized[1]["parts"].as_array().unwrap();
        assert_eq!(turn2_parts[0]["thoughtSignature"], "turn1_sig");
    }
}
