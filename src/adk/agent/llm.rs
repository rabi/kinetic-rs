// SPDX-License-Identifier: MIT

//! LLM Agent - Standard LLM agent with tool calling
//!
//! This agent sends prompts to an LLM and handles tool calls in a loop
//! until a text response is received.

use super::{Agent, AgentEvent};
use crate::adk::model::{Content, Model, Part};
use crate::adk::tool::Tool;
use async_trait::async_trait;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

/// Standard LLM agent with tool calling support
pub struct LLMAgent {
    pub name: String,
    pub description: String,
    pub instruction: String,
    pub model: Arc<dyn Model>,
    pub tools: Vec<Arc<dyn Tool>>,
    /// HashMap for O(1) tool lookups
    tool_map: HashMap<String, usize>,
}

impl LLMAgent {
    pub fn new(
        name: String,
        description: String,
        instruction: String,
        model: Arc<dyn Model>,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Self {
        // Build tool lookup map
        let tool_map: HashMap<String, usize> = tools
            .iter()
            .enumerate()
            .map(|(i, t)| (t.name().to_string(), i))
            .collect();

        Self {
            name,
            description,
            instruction,
            model,
            tools,
            tool_map,
        }
    }

    /// O(1) tool lookup by name
    fn get_tool(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tool_map.get(name).map(|&i| &self.tools[i])
    }
}

#[async_trait]
impl Agent for LLMAgent {
    fn name(&self) -> &str {
        &self.name
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

            // Check for text response BEFORE adding to history
            // This avoids cloning when we can return early
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

            // Collect function calls using references
            let function_calls: Vec<(&str, &serde_json::Value)> = response
                .parts
                .iter()
                .filter_map(|part| {
                    if let Part::FunctionCall { name, args, .. } = part {
                        Some((name.as_str(), args))
                    } else {
                        None
                    }
                })
                .collect();

            if function_calls.is_empty() {
                log::warn!(
                    "Agent {} received empty response with no function calls",
                    self.name
                );
                return Ok(String::new());
            }

            // Execute function calls and build responses
            let mut function_responses = Vec::with_capacity(function_calls.len());
            for (name, args) in function_calls {
                log::info!("Tool call: {} {:?}", name, args);

                // Use O(1) HashMap lookup
                let tool_response = if let Some(t) = self.get_tool(name) {
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
                    name: name.to_string(),
                    response: tool_response,
                });
            }

            // Add model response to history
            history.push(response);

            // Add tool responses to history
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
    async fn run_stream(
        &self,
        input: String,
        tx: tokio::sync::mpsc::Sender<AgentEvent>,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
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

            // Analyze response parts
            let mut text_content = String::new();
            let mut function_calls = Vec::new();

            for part in &response.parts {
                match part {
                    Part::Text(text) => text_content.push_str(text),
                    Part::FunctionCall { name, args, .. } => {
                        function_calls.push((name.as_str(), args))
                    }
                    _ => {}
                }
            }

            if function_calls.is_empty() {
                // No function calls, treat text as final answer
                if !text_content.is_empty() {
                    let _ = tx.send(AgentEvent::Answer(text_content.clone())).await;
                    return Ok(text_content);
                }
                // If both empty, it's weird, but we continue or return empty?
                // Usually retry or return empty string? Original code returned empty string.
                return Ok(String::new());
            }

            // Has function calls, treat text as Thought
            if !text_content.is_empty() {
                let _ = tx.send(AgentEvent::Thought(text_content.clone())).await;
            }

            // Execute function calls
            let mut function_responses = Vec::with_capacity(function_calls.len());
            for (name, args) in function_calls {
                // Emit ToolCall event
                let _ = tx
                    .send(AgentEvent::ToolCall {
                        name: name.to_string(),
                        args: args.clone(),
                    })
                    .await;

                let tool_response = if let Some(t) = self.get_tool(name) {
                    match t.execute(args.clone()).await {
                        Ok(res) => res,
                        Err(e) => {
                            let _ = tx
                                .send(AgentEvent::Error(format!("Tool {} failed: {}", name, e)))
                                .await;
                            serde_json::json!({ "error": e.to_string() })
                        }
                    }
                } else {
                    let _ = tx
                        .send(AgentEvent::Error(format!("Tool {} not found", name)))
                        .await;
                    serde_json::json!({ "error": format!("Tool {} not found", name) })
                };

                // Emit ToolResult event
                let _ = tx
                    .send(AgentEvent::ToolResult {
                        name: name.to_string(),
                        result: tool_response.clone(),
                    })
                    .await;

                function_responses.push(Part::FunctionResponse {
                    name: name.to_string(),
                    response: tool_response,
                });
            }

            history.push(response);
            history.push(Content {
                role: "user".to_string(),
                parts: function_responses,
            });
        }

        Err("Max turns reached".into())
    }
}
