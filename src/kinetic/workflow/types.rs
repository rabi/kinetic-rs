// SPDX-License-Identifier: MIT

//! YAML schema types for workflow and agent definitions
//!
//! This module contains all the data structures used for parsing
//! workflow and agent YAML configuration files.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level workflow definition
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WorkflowDefinition {
    pub name: String,
    pub description: String,
    /// Workflow kind: "Direct", "Composite", or "Graph"
    pub kind: String,
    /// Agent definition (for Direct workflows)
    pub agent: Option<AgentDefinition>,
    /// Composite workflow definition (for Composite workflows)
    pub workflow: Option<CompositeWorkflowDefinition>,
    /// Graph workflow definition (for Graph workflows)
    pub graph: Option<GraphDefinition>,
    /// Override values for referenced workflows
    pub overrides: Option<HashMap<String, serde_json::Value>>,
    /// MCP server configurations
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

/// Graph workflow definition with nodes and state
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GraphDefinition {
    /// State schema for the workflow
    pub state: Option<HashMap<String, StateFieldDef>>,
    /// Nodes in the graph
    pub nodes: Vec<GraphNodeDefinition>,
}

/// State field definition for graph workflows
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StateFieldDef {
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub reducer: Option<String>,
    pub default: Option<serde_json::Value>,
}

/// A node in a graph workflow
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GraphNodeDefinition {
    /// Unique node identifier
    pub id: String,
    /// Agent configuration (inline or file reference)
    pub agent: AgentConfig,
    /// Dependencies - nodes that must complete before this runs
    #[serde(default)]
    pub depends_on: GraphDependsOn,
    /// Condition for this node to execute
    pub when: Option<String>,
    /// JSON Schema for structured output
    pub output_schema: Option<serde_json::Value>,
    /// Maps output fields to state fields
    pub outputs: Option<HashMap<String, String>>,
    /// How to wait for dependencies (all or any)
    #[serde(default)]
    pub wait_for: String,
}

/// Dependency specification (single string or array)
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum GraphDependsOn {
    #[default]
    None,
    Single(String),
    Multiple(Vec<String>),
}

impl GraphDependsOn {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            GraphDependsOn::None => vec![],
            GraphDependsOn::Single(s) => vec![s.clone()],
            GraphDependsOn::Multiple(v) => v.clone(),
        }
    }
}

/// MCP server configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

/// Agent definition
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    pub instructions: String,
    /// Executor type: "default" (turn-based), "react" (Thought-Action-Observation), "cot" (Chain-of-Thought)
    pub executor: Option<String>,
    #[serde(default)]
    pub model: ModelDefinition,
    pub tools: Vec<String>,
    pub memory: Option<MemoryDefinition>,
    pub workflow: Option<WorkflowReference>,
    /// Maximum iterations for ReAct executor (default: 10)
    pub max_iterations: Option<u32>,
}

/// Composite workflow definition
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CompositeWorkflowDefinition {
    /// Execution mode: "sequential", "parallel", "loop"
    pub execution: String,
    #[serde(default)]
    pub agents: Vec<AgentConfig>,
    pub max_iterations: Option<u32>,
}

/// Agent configuration - either inline definition or file reference
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum AgentConfig {
    Inline(Box<AgentDefinition>),
    Reference(WorkflowReference),
}

/// Model configuration
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ModelDefinition {
    /// Provider is optional - can be inferred from model_name or MODEL_PROVIDER env var
    pub provider: Option<String>,
    pub model_name: Option<String>,
    pub kind: Option<String>,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
}

/// Memory configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MemoryDefinition {
    pub kind: String,
    pub parameters: Option<HashMap<String, serde_json::Value>>,
}

/// Reference to an external workflow file
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WorkflowReference {
    pub file: String,
    pub overrides: Option<HashMap<String, serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_depends_on_none() {
        let dep = GraphDependsOn::None;
        assert!(dep.to_vec().is_empty());
    }

    #[test]
    fn test_graph_depends_on_single() {
        let dep = GraphDependsOn::Single("node_a".to_string());
        assert_eq!(dep.to_vec(), vec!["node_a"]);
    }

    #[test]
    fn test_graph_depends_on_multiple() {
        let dep = GraphDependsOn::Multiple(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(dep.to_vec(), vec!["a", "b"]);
    }
}
