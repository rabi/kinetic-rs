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

/// ReAct (Reasoning + Acting) Agent
/// 
/// Implements the ReAct pattern where the agent explicitly reasons about
/// what to do, takes actions (tool calls), and observes the results in
/// a structured Thought → Action → Observation loop.
pub struct ReActAgent {
    pub name: String,
    pub description: String,
    pub instruction: String,
    pub model: Arc<dyn Model>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub max_iterations: u32,
}

/// ReAct step types
#[derive(Debug)]
enum ReActStep {
    /// Model is thinking/reasoning
    Thought(String),
    /// Model wants to call a tool
    Action { tool: String, args: serde_json::Value },
    /// Model has a final answer
    FinalAnswer(String),
}

impl ReActAgent {
    pub fn new(
        name: String,
        description: String,
        instruction: String,
        model: Arc<dyn Model>,
        tools: Vec<Arc<dyn Tool>>,
        max_iterations: u32,
    ) -> Self {
        Self {
            name,
            description,
            instruction,
            model,
            tools,
            max_iterations,
        }
    }

    /// Build the ReAct system prompt with tool descriptions
    fn build_react_system_prompt(&self) -> String {
        let tool_section = if self.tools.is_empty() {
            "No tools are available. You must answer based on your knowledge.".to_string()
        } else {
            let tool_descriptions: Vec<String> = self
                .tools
                .iter()
                .map(|t| format!("- {}: {}", t.name(), t.description()))
                .collect();
            format!("Available tools:\n{}", tool_descriptions.join("\n"))
        };

        format!(
            r#"{}

You are using the ReAct (Reasoning + Acting) pattern. For each step:

1. **Thought**: Reason about what you know and what you need to do next
2. **Action**: Either call a tool OR provide a final answer

{}

Response format:
- To use a tool, respond with a function call (only use tools listed above)
- To provide a final answer, respond with text starting with "Final Answer:" followed by your answer

Always think step by step. After receiving tool results (Observations), continue reasoning until you can provide a final answer."#,
            self.instruction,
            tool_section
        )
    }

    /// Build the current prompt including scratchpad history
    fn build_prompt_with_scratchpad(&self, input: &str, scratchpad: &[String]) -> String {
        if scratchpad.is_empty() {
            input.to_string()
        } else {
            format!(
                "{}\n\n--- Previous Steps ---\n{}\n\nContinue from where you left off.",
                input,
                scratchpad.join("\n")
            )
        }
    }

    /// Parse the model response to determine the ReAct step type
    fn parse_response(&self, response: &Content) -> ReActStep {
        for part in &response.parts {
            match part {
                Part::Thinking(thought) => {
                    return ReActStep::Thought(thought.clone());
                }
                Part::FunctionCall { name, args, .. } => {
                    return ReActStep::Action {
                        tool: name.clone(),
                        args: args.clone(),
                    };
                }
                Part::Text(text) => {
                    let text_trimmed = text.trim();
                    // Check if this is a final answer
                    if text_trimmed.to_lowercase().starts_with("final answer:") {
                        let answer = text_trimmed
                            .strip_prefix("Final Answer:")
                            .or_else(|| text_trimmed.strip_prefix("final answer:"))
                            .or_else(|| text_trimmed.strip_prefix("FINAL ANSWER:"))
                            .unwrap_or(text_trimmed)
                            .trim()
                            .to_string();
                        return ReActStep::FinalAnswer(answer);
                    }
                    // Otherwise treat as thought/reasoning
                    if !text_trimmed.is_empty() {
                        return ReActStep::Thought(text_trimmed.to_string());
                    }
                }
                _ => {}
            }
        }
        // Default to empty thought if nothing parsed
        ReActStep::Thought(String::new())
    }

    /// Execute a tool and return the result
    async fn execute_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        let tool = self.tools.iter().find(|t| t.name() == tool_name);
        
        if let Some(t) = tool {
            match t.execute(args).await {
                Ok(result) => Ok(serde_json::to_string_pretty(&result).unwrap_or_default()),
                Err(e) => Ok(format!("Error: {}", e)),
            }
        } else {
            Ok(format!("Error: Tool '{}' not found", tool_name))
        }
    }
}

#[async_trait]
impl Agent for ReActAgent {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
        let system_prompt = self.build_react_system_prompt();
        let mut scratchpad: Vec<String> = Vec::new();

        for iteration in 0..self.max_iterations {
            log::info!(
                "ReActAgent {} iteration {}/{}",
                self.name,
                iteration + 1,
                self.max_iterations
            );

            // Build conversation with current scratchpad
            let current_prompt = self.build_prompt_with_scratchpad(&input, &scratchpad);
            
            let history = vec![
                Content {
                    role: "system".to_string(),
                    parts: vec![Part::Text(system_prompt.clone())],
                },
                Content {
                    role: "user".to_string(),
                    parts: vec![Part::Text(current_prompt)],
                },
            ];

            // Get model response
            let response = self
                .model
                .generate_content(&history, None, Some(&self.tools))
                .await?;

            // Parse the response
            let step = self.parse_response(&response);
            log::debug!("ReActAgent step: {:?}", step);

            match step {
                ReActStep::Thought(thought) => {
                    if !thought.is_empty() {
                        scratchpad.push(format!("Thought: {}", thought));
                        log::info!("Thought: {}", thought);
                    }
                }
                ReActStep::Action { tool, args } => {
                    scratchpad.push(format!("Action: {}({})", tool, args));
                    log::info!("Action: {}({})", tool, args);
                    
                    // Execute the tool
                    let observation = self.execute_tool(&tool, args).await?;
                    scratchpad.push(format!("Observation: {}", observation));
                    log::info!("Observation: {}", observation);
                }
                ReActStep::FinalAnswer(answer) => {
                    log::info!("Final Answer: {}", answer);
                    return Ok(answer);
                }
            }
        }

        // Max iterations reached - compile final answer from scratchpad
        log::warn!(
            "ReActAgent {} reached max iterations ({})",
            self.name,
            self.max_iterations
        );
        
        // Return the last meaningful content from scratchpad
        let summary = format!(
            "Reached maximum iterations. Here's what I found:\n\n{}",
            scratchpad.join("\n")
        );
        Ok(summary)
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

    // === ReActAgent Tests ===

    use crate::adk::model::{Content, GenerationConfig, Model, Part};
    use crate::adk::tool::Tool;
    use serde_json::json;

    /// Mock model for testing ReActAgent
    struct MockModel {
        responses: std::sync::Mutex<Vec<Content>>,
    }

    impl MockModel {
        fn new(responses: Vec<Content>) -> Self {
            Self {
                responses: std::sync::Mutex::new(responses),
            }
        }
    }

    #[async_trait]
    impl Model for MockModel {
        async fn generate_content(
            &self,
            _history: &[Content],
            _config: Option<&GenerationConfig>,
            _tools: Option<&[Arc<dyn Tool>]>,
        ) -> Result<Content, Box<dyn Error + Send + Sync>> {
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                Ok(Content {
                    role: "model".to_string(),
                    parts: vec![Part::Text("Final Answer: Done".to_string())],
                })
            } else {
                Ok(responses.remove(0))
            }
        }
    }

    /// Mock tool for testing
    struct MockTool {
        name: String,
    }

    impl MockTool {
        fn new(name: &str) -> Self {
            Self { name: name.to_string() }
        }
    }

    #[async_trait]
    impl Tool for MockTool {
        fn name(&self) -> String { self.name.clone() }
        fn description(&self) -> String { format!("Mock tool: {}", self.name) }
        fn schema(&self) -> serde_json::Value {
            json!({"type": "object", "properties": {"query": {"type": "string"}}})
        }
        async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
            Ok(json!({"result": format!("Mock result for {} with args: {}", self.name, args)}))
        }
    }

    #[test]
    fn test_react_parse_final_answer() {
        let model = Arc::new(MockModel::new(vec![]));
        let agent = ReActAgent::new("test".to_string(), "test".to_string(), "test".to_string(), model, vec![], 10);

        let response = Content {
            role: "model".to_string(),
            parts: vec![Part::Text("Final Answer: The result is 42".to_string())],
        };
        match agent.parse_response(&response) {
            ReActStep::FinalAnswer(answer) => assert_eq!(answer, "The result is 42"),
            _ => panic!("Expected FinalAnswer"),
        }
    }

    #[test]
    fn test_react_parse_thought() {
        let model = Arc::new(MockModel::new(vec![]));
        let agent = ReActAgent::new("test".to_string(), "test".to_string(), "test".to_string(), model, vec![], 10);

        let response = Content {
            role: "model".to_string(),
            parts: vec![Part::Text("I need to search for more information".to_string())],
        };
        match agent.parse_response(&response) {
            ReActStep::Thought(thought) => assert_eq!(thought, "I need to search for more information"),
            _ => panic!("Expected Thought"),
        }
    }

    #[test]
    fn test_react_parse_thinking_part() {
        let model = Arc::new(MockModel::new(vec![]));
        let agent = ReActAgent::new("test".to_string(), "test".to_string(), "test".to_string(), model, vec![], 10);

        let response = Content {
            role: "model".to_string(),
            parts: vec![Part::Thinking("Deep reasoning about the problem".to_string())],
        };
        match agent.parse_response(&response) {
            ReActStep::Thought(thought) => assert_eq!(thought, "Deep reasoning about the problem"),
            _ => panic!("Expected Thought from Thinking part"),
        }
    }

    #[test]
    fn test_react_parse_function_call() {
        let model = Arc::new(MockModel::new(vec![]));
        let agent = ReActAgent::new("test".to_string(), "test".to_string(), "test".to_string(), model, vec![], 10);

        let response = Content {
            role: "model".to_string(),
            parts: vec![Part::FunctionCall { name: "search".to_string(), args: json!({"query": "rust"}), thought_signature: None }],
        };
        match agent.parse_response(&response) {
            ReActStep::Action { tool, args } => {
                assert_eq!(tool, "search");
                assert_eq!(args, json!({"query": "rust"}));
            }
            _ => panic!("Expected Action"),
        }
    }

    #[test]
    fn test_react_build_scratchpad_prompt() {
        let model = Arc::new(MockModel::new(vec![]));
        let agent = ReActAgent::new("test".to_string(), "test".to_string(), "test".to_string(), model, vec![], 10);

        // Empty scratchpad
        assert_eq!(agent.build_prompt_with_scratchpad("What is 2+2?", &[]), "What is 2+2?");

        // With scratchpad
        let scratchpad = vec!["Thought: I need to calculate".to_string(), "Observation: 4".to_string()];
        let prompt = agent.build_prompt_with_scratchpad("What is 2+2?", &scratchpad);
        assert!(prompt.contains("Previous Steps"));
        assert!(prompt.contains("Thought: I need to calculate"));
    }

    #[test]
    fn test_react_system_prompt_includes_tools() {
        let model = Arc::new(MockModel::new(vec![]));
        let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(MockTool::new("search")), Arc::new(MockTool::new("calc"))];
        let agent = ReActAgent::new("test".to_string(), "test".to_string(), "You are helpful".to_string(), model, tools, 10);

        let prompt = agent.build_react_system_prompt();
        assert!(prompt.contains("ReAct"));
        assert!(prompt.contains("search"));
        assert!(prompt.contains("calc"));
    }

    #[tokio::test]
    async fn test_react_agent_final_answer_first() {
        let model = Arc::new(MockModel::new(vec![Content {
            role: "model".to_string(),
            parts: vec![Part::Text("Final Answer: 42".to_string())],
        }]));
        let agent = ReActAgent::new("test".to_string(), "test".to_string(), "test".to_string(), model, vec![], 10);

        let result = agent.run("What?".to_string()).await.unwrap();
        assert_eq!(result, "42");
    }

    #[tokio::test]
    async fn test_react_agent_with_tool_call() {
        let responses = vec![
            Content { role: "model".to_string(), parts: vec![Part::FunctionCall { name: "search".to_string(), args: json!({"q": "test"}), thought_signature: None }] },
            Content { role: "model".to_string(), parts: vec![Part::Text("Final Answer: Found it".to_string())] },
        ];
        let model = Arc::new(MockModel::new(responses));
        let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(MockTool::new("search"))];
        let agent = ReActAgent::new("test".to_string(), "test".to_string(), "test".to_string(), model, tools, 10);

        let result = agent.run("Search".to_string()).await.unwrap();
        assert_eq!(result, "Found it");
    }

    #[tokio::test]
    async fn test_react_agent_max_iterations() {
        let model = Arc::new(MockModel::new(vec![
            Content { role: "model".to_string(), parts: vec![Part::Text("Thinking...".to_string())] },
            Content { role: "model".to_string(), parts: vec![Part::Text("Still thinking...".to_string())] },
            Content { role: "model".to_string(), parts: vec![Part::Text("More...".to_string())] },
        ]));
        let agent = ReActAgent::new("test".to_string(), "test".to_string(), "test".to_string(), model, vec![], 3);

        let result = agent.run("Think".to_string()).await.unwrap();
        assert!(result.contains("Reached maximum iterations"));
    }

    #[tokio::test]
    async fn test_react_execute_tool() {
        let model = Arc::new(MockModel::new(vec![]));
        let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(MockTool::new("test_tool"))];
        let agent = ReActAgent::new("test".to_string(), "test".to_string(), "test".to_string(), model, tools, 10);

        let result = agent.execute_tool("test_tool", json!({"x": 1})).await.unwrap();
        assert!(result.contains("Mock result"));

        let result = agent.execute_tool("nonexistent", json!({})).await.unwrap();
        assert!(result.contains("not found"));
    }
}
