//! Integration tests for workflow loading and execution
//!
//! These tests verify end-to-end workflow functionality using mock components.

use async_trait::async_trait;
use kinetic_rs::adk::agent::{Agent, LLMAgent, ParallelAgent, SequentialAgent};
use kinetic_rs::adk::model::{Content, GenerationConfig, Model, Part};
use kinetic_rs::adk::tool::Tool;
use kinetic_rs::kinetic::workflow::loader::WorkflowLoader;
use kinetic_rs::kinetic::workflow::registry::ToolRegistry;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::error::Error;
use std::sync::Arc;

// ============================================================================
// Mock Components
// ============================================================================

/// Mock model that returns predefined responses
struct MockModel {
    responses: Vec<Content>,
    response_index: std::sync::atomic::AtomicUsize,
}

impl MockModel {
    fn new(responses: Vec<Content>) -> Self {
        Self {
            responses,
            response_index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn text_response(text: &str) -> Content {
        Content {
            role: "model".to_string(),
            parts: vec![Part::Text(text.to_string())],
        }
    }

    fn tool_call_response(tool_name: &str, args: Value) -> Content {
        Content {
            role: "model".to_string(),
            parts: vec![Part::FunctionCall {
                name: tool_name.to_string(),
                args,
                thought_signature: None,
            }],
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
        let idx = self
            .response_index
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if idx < self.responses.len() {
            Ok(self.responses[idx].clone())
        } else {
            Ok(MockModel::text_response("Max responses reached"))
        }
    }
}

/// Static schema for MockTool
static MOCK_TOOL_SCHEMA: Lazy<Value> = Lazy::new(|| {
    json!({
        "type": "object",
        "properties": {
            "input": {"type": "string"}
        }
    })
});

/// Mock tool that returns predefined response
struct MockTool {
    name: String,
    description: String,
    response: Value,
}

impl MockTool {
    fn new(name: &str, response: Value) -> Self {
        Self {
            name: name.to_string(),
            description: format!("Mock tool: {}", name),
            response,
        }
    }
}

#[async_trait]
impl Tool for MockTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn schema(&self) -> &Value {
        &MOCK_TOOL_SCHEMA
    }

    async fn execute(&self, _input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        Ok(self.response.clone())
    }
}

/// Mock agent for testing
struct MockAgent {
    name: String,
    output: String,
}

impl MockAgent {
    fn new(name: &str, output: &str) -> Self {
        Self {
            name: name.to_string(),
            output: output.to_string(),
        }
    }
}

#[async_trait]
impl Agent for MockAgent {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(&self, _input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
        Ok(self.output.clone())
    }
}

/// Transform agent for testing sequential flows
struct TransformAgent {
    name: String,
    suffix: String,
}

#[async_trait]
impl Agent for TransformAgent {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
        Ok(format!("{}{}", input, self.suffix))
    }
}

// ============================================================================
// Workflow Loading Tests
// ============================================================================

#[test]
fn test_load_direct_workflow_yaml() {
    let yaml = r#"
kind: Direct
name: TestAgent
description: "Test agent"

agent:
  name: TestAgent
  description: "Test"
  instructions: "You are a test agent."
  model:
    kind: llm
  tools: []
"#;

    let def = WorkflowLoader::parse_yaml(yaml).expect("Failed to parse YAML");

    assert_eq!(def.kind, "Direct");
    assert_eq!(def.name, "TestAgent");
    assert!(def.agent.is_some());
}

#[test]
fn test_load_composite_sequential_workflow() {
    let yaml = r#"
kind: Composite
name: SequentialWorkflow
description: "Sequential test workflow"

workflow:
  execution: sequential
  agents:
    - file: agents/step1.yaml
    - file: agents/step2.yaml
"#;

    let def = WorkflowLoader::parse_yaml(yaml).expect("Failed to parse YAML");

    assert_eq!(def.kind, "Composite");
    assert_eq!(def.name, "SequentialWorkflow");
    assert!(def.workflow.is_some());

    let workflow = def.workflow.unwrap();
    assert_eq!(workflow.execution, "sequential");
    assert_eq!(workflow.agents.len(), 2);
}

#[test]
fn test_load_composite_parallel_workflow() {
    let yaml = r#"
kind: Composite
name: ParallelWorkflow
description: "Parallel test workflow"

workflow:
  execution: parallel
  agents:
    - file: agents/agent_a.yaml
    - file: agents/agent_b.yaml
"#;

    let def = WorkflowLoader::parse_yaml(yaml).expect("Failed to parse YAML");

    let workflow = def.workflow.unwrap();
    assert_eq!(workflow.execution, "parallel");
}

#[test]
fn test_load_graph_workflow() {
    let yaml = r#"
kind: Graph
name: IntentRouter
description: "Routes based on intent"

graph:
  state:
    intent:
      type: string
  nodes:
    - id: classifier
      agent:
        name: Classifier
        description: "Classifies intent"
        instructions: "Classify the intent"
        model:
          kind: llm
        tools: []
      outputs:
        intent: "intent"
"#;

    let def = WorkflowLoader::parse_yaml(yaml).expect("Failed to parse YAML");

    assert_eq!(def.kind, "Graph");
    assert!(def.graph.is_some());

    let graph = def.graph.unwrap();
    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.nodes[0].id, "classifier");
}

#[test]
fn test_load_workflow_with_mcp_servers() {
    let yaml = r#"
kind: Direct
name: MCPTest
description: "Test MCP"

mcp_servers:
  - name: "myserver"
    command: "npx"
    args: ["-y", "some-package"]

agent:
  name: MCPTest
  description: "Test"
  instructions: "Test"
  model:
    kind: llm
  tools:
    - "myserver:tool1"
"#;

    let def = WorkflowLoader::parse_yaml(yaml).expect("Failed to parse YAML");

    assert_eq!(def.mcp_servers.len(), 1);
    assert_eq!(def.mcp_servers[0].name, "myserver");
    assert_eq!(def.mcp_servers[0].command, "npx");
}

#[test]
fn test_load_workflow_with_model_parameters() {
    let yaml = r#"
kind: Direct
name: ParameterizedModel
description: "Test model with parameters"

agent:
  name: Agent
  description: "Test"
  instructions: "Test"
  model:
    kind: llm
    provider: Gemini
    model_name: gemini-2.0-flash
    parameters:
      temperature: 0.7
      max_tokens: 1000
  tools: []
"#;

    let def = WorkflowLoader::parse_yaml(yaml).expect("Failed to parse YAML");
    let agent = def.agent.unwrap();

    assert_eq!(agent.model.provider, Some("Gemini".to_string()));
    assert_eq!(agent.model.model_name, Some("gemini-2.0-flash".to_string()));

    let params = agent.model.parameters.unwrap();
    assert_eq!(params.get("temperature"), Some(&json!(0.7)));
}

#[test]
fn test_invalid_yaml_returns_error() {
    let yaml = r#"
kind: Direct
name:
  - invalid structure
"#;

    let result = WorkflowLoader::parse_yaml(yaml);
    assert!(result.is_err());
}

// ============================================================================
// Tool Registry Tests
// ============================================================================

#[tokio::test]
async fn test_tool_registry_register_and_lookup() {
    let registry = ToolRegistry::new();

    let tool1 = Arc::new(MockTool::new("tool_a", json!({"result": "a"})));
    let tool2 = Arc::new(MockTool::new("tool_b", json!({"result": "b"})));

    registry.register(tool1).await;
    registry.register(tool2).await;

    // Lookup works
    let found = registry.get("tool_a").await;
    assert!(found.is_some());
    assert_eq!(found.unwrap().name(), "tool_a");

    // Missing returns None
    let missing = registry.get("nonexistent").await;
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_tool_registry_overwrite() {
    let registry = ToolRegistry::new();

    let tool_v1 = Arc::new(MockTool::new("my_tool", json!({"version": 1})));
    let tool_v2 = Arc::new(MockTool::new("my_tool", json!({"version": 2})));

    registry.register(tool_v1).await;
    registry.register(tool_v2).await;

    // Should have the second version
    let found = registry.get("my_tool").await.unwrap();
    assert_eq!(found.name(), "my_tool");
}

#[tokio::test]
async fn test_tool_registry_clone_shares_state() {
    let registry = ToolRegistry::new();
    let registry_clone = registry.clone();

    let tool = Arc::new(MockTool::new("shared_tool", json!({})));
    registry.register(tool).await;

    // Clone should see the same tool
    let found = registry_clone.get("shared_tool").await;
    assert!(found.is_some());
}

// ============================================================================
// Agent Execution Tests
// ============================================================================

#[tokio::test]
async fn test_llm_agent_returns_text_response() {
    let model = Arc::new(MockModel::new(vec![MockModel::text_response(
        "Hello, world!",
    )]));

    let agent = LLMAgent::new(
        "test".to_string(),
        "test agent".to_string(),
        "You are helpful".to_string(),
        model,
        vec![],
    );

    let result = agent.run("Hi".to_string()).await.expect("Agent failed");
    assert_eq!(result, "Hello, world!");
}

#[tokio::test]
async fn test_llm_agent_executes_tool_call() {
    let tool = Arc::new(MockTool::new(
        "search",
        json!({"results": ["result1", "result2"]}),
    ));

    let model = Arc::new(MockModel::new(vec![
        MockModel::tool_call_response("search", json!({"query": "test"})),
        MockModel::text_response("Found 2 results"),
    ]));

    let agent = LLMAgent::new(
        "test".to_string(),
        "test agent".to_string(),
        "You are helpful".to_string(),
        model,
        vec![tool],
    );

    let result = agent
        .run("Search for test".to_string())
        .await
        .expect("Agent failed");
    assert_eq!(result, "Found 2 results");
}

#[tokio::test]
async fn test_llm_agent_handles_tool_not_found() {
    let model = Arc::new(MockModel::new(vec![
        MockModel::tool_call_response("nonexistent_tool", json!({})),
        MockModel::text_response("Could not find tool"),
    ]));

    let agent = LLMAgent::new(
        "test".to_string(),
        "test agent".to_string(),
        "You are helpful".to_string(),
        model,
        vec![], // No tools!
    );

    // Should not panic, should handle gracefully
    let result = agent.run("Do something".to_string()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_sequential_agent_chains_output() {
    let agent1 = Arc::new(TransformAgent {
        name: "step1".to_string(),
        suffix: "-A".to_string(),
    });
    let agent2 = Arc::new(TransformAgent {
        name: "step2".to_string(),
        suffix: "-B".to_string(),
    });

    let seq = SequentialAgent::new(
        "sequential".to_string(),
        "test".to_string(),
        vec![agent1, agent2],
    );

    let result = seq
        .run("input".to_string())
        .await
        .expect("Sequential failed");
    assert_eq!(result, "input-A-B");
}

#[tokio::test]
async fn test_sequential_agent_empty_returns_input() {
    let seq = SequentialAgent::new("empty".to_string(), "test".to_string(), vec![]);

    let result = seq.run("passthrough".to_string()).await.expect("Failed");
    assert_eq!(result, "passthrough");
}

#[tokio::test]
async fn test_parallel_agent_combines_results() {
    let agent1 = Arc::new(MockAgent::new("a", "result_a"));
    let agent2 = Arc::new(MockAgent::new("b", "result_b"));

    let parallel = ParallelAgent::new(
        "parallel".to_string(),
        "test".to_string(),
        vec![agent1, agent2],
    );

    let result = parallel
        .run("input".to_string())
        .await
        .expect("Parallel failed");

    // Results are joined with separator
    assert!(result.contains("result_a"));
    assert!(result.contains("result_b"));
}

#[tokio::test]
async fn test_parallel_agent_empty_returns_empty() {
    let parallel = ParallelAgent::new("empty".to_string(), "test".to_string(), vec![]);

    let result = parallel.run("input".to_string()).await.expect("Failed");
    assert_eq!(result, "");
}

// ============================================================================
// Error Type Tests
// ============================================================================

#[test]
fn test_kinetic_error_from_str() {
    use kinetic_rs::adk::error::KineticError;

    let err: KineticError = "Something went wrong".into();
    assert_eq!(err.to_string(), "Something went wrong");
}

#[test]
fn test_kinetic_error_api() {
    use kinetic_rs::adk::error::KineticError;

    let err = KineticError::api("GitHub", "Rate limit exceeded");
    assert!(err.to_string().contains("GitHub"));
    assert!(err.to_string().contains("Rate limit"));
}

#[test]
fn test_kinetic_error_tool_not_found() {
    use kinetic_rs::adk::error::KineticError;

    let err = KineticError::tool_not_found("unknown_tool");
    assert!(err.to_string().contains("unknown_tool"));
}

#[test]
fn test_kinetic_error_config() {
    use kinetic_rs::adk::error::KineticError;

    let err = KineticError::config("Missing API key");
    assert!(err.to_string().contains("Missing API key"));
}

#[test]
fn test_workflow_error() {
    use kinetic_rs::adk::error::WorkflowError;

    let err = WorkflowError::UnknownKind("BadKind".to_string());
    assert!(err.to_string().contains("BadKind"));

    let err = WorkflowError::MissingAgent;
    assert!(err.to_string().contains("agent"));
}
