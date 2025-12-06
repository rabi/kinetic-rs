use crate::adk::model::{Content, GenerationConfig, Model, Part};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::error::Error;

pub struct GeminiModel {
    client: Client,
    api_key: String,
    model_name: String,
}

use std::env;

impl GeminiModel {
    pub fn new(model_name: String) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let api_key = env::var("GOOGLE_API_KEY").map_err(|_| "GOOGLE_API_KEY must be set")?;
        Ok(Self {
            client: Client::new(),
            api_key,
            model_name,
        })
    }
}

use crate::adk::tool::Tool;
use std::sync::Arc;

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
                    .map(|p| match p {
                        Part::Text(t) => json!({ "text": t }),
                        Part::FunctionCall {
                            name,
                            args,
                            thought_signature,
                        } => {
                            let mut json =
                                json!({ "functionCall": { "name": name, "args": args } });
                            if let Some(sig) = thought_signature {
                                if let Some(obj) = json.as_object_mut() {
                                    obj.insert("thought_signature".to_string(), json!(sig));
                                }
                            }
                            json
                        }
                        Part::FunctionResponse { name, response } => {
                            json!({ "functionResponse": { "name": name, "response": response } })
                        }
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
            if finish_reason == "UNEXPECTED_TOOL_CALL" {
                return Err(
                    "Gemini returned UNEXPECTED_TOOL_CALL. The tool schema may be incompatible."
                        .into(),
                );
            }
        }

        let content = candidate["content"]
            .as_object()
            .ok_or("No content in candidate")?;
        let parts_json = content
            .get("parts")
            .and_then(|v| v.as_array())
            .ok_or("No parts in content")?;

        let mut parts = Vec::new();
        for p in parts_json {
            if let Some(text) = p["text"].as_str() {
                parts.push(Part::Text(text.to_string()));
            } else if let Some(fc) = p.get("functionCall") {
                let name = fc["name"].as_str().unwrap_or_default().to_string();
                let args = fc["args"].clone();
                // thoughtSignature is a sibling of functionCall in the part object, NOT inside functionCall
                // Wait, the log showed: {"functionCall":{...}, "thoughtSignature":"..."} inside the part object.
                // Yes, p is the part object.
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
