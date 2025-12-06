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
