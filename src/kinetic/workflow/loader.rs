use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WorkflowDefinition {
    pub name: String,
    pub description: String,
    pub kind: String, // "Direct" or "Composite"
    pub agent: Option<AgentDefinition>,
    pub workflow: Option<CompositeWorkflowDefinition>,
    pub overrides: Option<HashMap<String, serde_json::Value>>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub model: ModelDefinition,
    pub tools: Vec<String>,
    pub memory: Option<MemoryDefinition>,
    pub workflow: Option<WorkflowReference>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CompositeWorkflowDefinition {
    pub execution: String, // "sequential", "parallel", "loop"
    #[serde(default)]
    pub agents: Vec<AgentConfig>,
    pub max_iterations: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum AgentConfig {
    Inline(AgentDefinition),
    Reference(WorkflowReference),
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelDefinition {
    /// Provider is optional - can be inferred from model_name or MODEL_PROVIDER env var
    pub provider: Option<String>,
    pub model_name: Option<String>,
    pub kind: Option<String>,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MemoryDefinition {
    pub kind: String,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WorkflowReference {
    pub file: String,
    pub overrides: Option<HashMap<String, serde_json::Value>>,
}

pub struct WorkflowLoader;

impl WorkflowLoader {
    pub fn new() -> Self {
        Self
    }

    pub fn load_workflow<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<WorkflowDefinition, Box<dyn Error + Send + Sync>> {
        let content = fs::read_to_string(path)?;
        let def: WorkflowDefinition = serde_yaml::from_str(&content)?;
        Ok(def)
    }
}
