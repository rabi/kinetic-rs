//! Workflow loader - YAML file loading and parsing
//!
//! This module handles loading workflow definitions from YAML files.

use super::types::WorkflowDefinition;
use std::error::Error;
use std::fs;
use std::path::Path;

/// Loads workflow definitions from YAML files
pub struct WorkflowLoader;

impl WorkflowLoader {
    pub fn new() -> Self {
        Self
    }

    /// Load a workflow definition from a YAML file
    pub fn load_workflow<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<WorkflowDefinition, Box<dyn Error + Send + Sync>> {
        let content = fs::read_to_string(path)?;
        Self::parse_yaml(&content)
    }

    /// Parse a workflow definition from a YAML string
    pub fn parse_yaml(content: &str) -> Result<WorkflowDefinition, Box<dyn Error + Send + Sync>> {
        let def: WorkflowDefinition = serde_yaml::from_str(content)?;
        Ok(def)
    }
}

impl Default for WorkflowLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinetic::workflow::types::AgentConfig;

    #[test]
    fn test_parse_direct_workflow() {
        let yaml = r#"
kind: Direct
name: TestAgent
description: "A test agent"

agent:
  name: TestAgent
  description: "Test description"
  instructions: "You are a test agent."
  model:
    kind: llm
  tools: []
"#;
        let def = WorkflowLoader::parse_yaml(yaml).unwrap();
        assert_eq!(def.name, "TestAgent");
        assert_eq!(def.kind, "Direct");
        assert!(def.agent.is_some());
        assert!(def.workflow.is_none());

        let agent = def.agent.unwrap();
        assert_eq!(agent.name, "TestAgent");
        assert_eq!(agent.instructions.trim(), "You are a test agent.");
        assert!(agent.tools.is_empty());
    }

    #[test]
    fn test_parse_composite_workflow() {
        let yaml = r#"
kind: Composite
name: TestWorkflow
description: "A test workflow"

workflow:
  execution: sequential
  agents:
    - file: agents/step1.yaml
    - file: agents/step2.yaml
"#;
        let def = WorkflowLoader::parse_yaml(yaml).unwrap();
        assert_eq!(def.name, "TestWorkflow");
        assert_eq!(def.kind, "Composite");
        assert!(def.agent.is_none());
        assert!(def.workflow.is_some());

        let workflow = def.workflow.unwrap();
        assert_eq!(workflow.execution, "sequential");
        assert_eq!(workflow.agents.len(), 2);
    }

    #[test]
    fn test_parse_loop_workflow() {
        let yaml = r#"
kind: Composite
name: LoopWorkflow
description: "A loop workflow"

workflow:
  execution: loop
  max_iterations: 5
  agents:
    - file: agents/worker.yaml
"#;
        let def = WorkflowLoader::parse_yaml(yaml).unwrap();
        let workflow = def.workflow.unwrap();
        assert_eq!(workflow.execution, "loop");
        assert_eq!(workflow.max_iterations, Some(5));
    }

    #[test]
    fn test_parse_model_with_provider() {
        let yaml = r#"
kind: Direct
name: TestAgent
description: "Test"

agent:
  name: TestAgent
  description: "Test"
  instructions: "Test"
  model:
    kind: llm
    provider: OpenAI
    model_name: gpt-4o
    parameters:
      temperature: 0.5
  tools: []
"#;
        let def = WorkflowLoader::parse_yaml(yaml).unwrap();
        let agent = def.agent.unwrap();
        assert_eq!(agent.model.provider, Some("OpenAI".to_string()));
        assert_eq!(agent.model.model_name, Some("gpt-4o".to_string()));

        let params = agent.model.parameters.unwrap();
        assert_eq!(params.get("temperature").unwrap(), &serde_json::json!(0.5));
    }

    #[test]
    fn test_parse_model_without_provider() {
        let yaml = r#"
kind: Direct
name: TestAgent
description: "Test"

agent:
  name: TestAgent
  description: "Test"
  instructions: "Test"
  model:
    kind: llm
  tools: []
"#;
        let def = WorkflowLoader::parse_yaml(yaml).unwrap();
        let agent = def.agent.unwrap();
        assert!(agent.model.provider.is_none());
        assert!(agent.model.model_name.is_none());
    }

    #[test]
    fn test_parse_mcp_servers() {
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
        let def = WorkflowLoader::parse_yaml(yaml).unwrap();
        assert_eq!(def.mcp_servers.len(), 1);
        assert_eq!(def.mcp_servers[0].name, "myserver");
        assert_eq!(def.mcp_servers[0].command, "npx");
        assert_eq!(def.mcp_servers[0].args, vec!["-y", "some-package"]);
    }

    #[test]
    fn test_parse_workflow_reference() {
        let yaml = r#"
kind: Composite
name: RefWorkflow
description: "Test references"

workflow:
  execution: sequential
  agents:
    - file: agents/step1.yaml
"#;
        let def = WorkflowLoader::parse_yaml(yaml).unwrap();
        let workflow = def.workflow.unwrap();

        match &workflow.agents[0] {
            AgentConfig::Reference(ref_def) => {
                assert_eq!(ref_def.file, "agents/step1.yaml");
            }
            AgentConfig::Inline(_) => panic!("Expected Reference, got Inline"),
        }
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
}
