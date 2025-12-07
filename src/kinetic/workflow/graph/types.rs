//! Graph workflow type definitions
//!
//! This module defines the core types for graph-based workflow definitions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::kinetic::workflow::types::AgentConfig;

/// A graph-based workflow definition
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct GraphWorkflowDef {
    /// Name of the workflow
    pub name: String,
    /// Description of the workflow
    #[serde(default)]
    pub description: String,
    /// State schema for the workflow (as raw YAML/JSON)
    pub state: Option<serde_json::Value>,
    /// Nodes in the graph
    #[serde(default)]
    pub nodes: Vec<NodeDefinition>,
}

/// A node in the workflow graph
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeDefinition {
    /// Unique identifier for this node
    pub id: String,
    /// Agent configuration (inline or reference)
    pub agent: AgentConfig,
    /// Dependencies - nodes that must complete before this runs
    #[serde(default)]
    pub depends_on: DependsOn,
    /// Condition for this node to execute
    pub when: Option<String>,
    /// JSON Schema for structured output (enforced by LLM API)
    pub output_schema: Option<serde_json::Value>,
    /// Maps output fields to state fields
    pub outputs: Option<HashMap<String, String>>,
    /// How to wait for dependencies
    #[serde(default)]
    pub wait_for: WaitMode,
}

/// Dependency specification for a node
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum DependsOn {
    /// No dependencies (entry node)
    #[default]
    None,
    /// Single dependency
    Single(String),
    /// Multiple dependencies
    Multiple(Vec<String>),
}

impl DependsOn {
    /// Convert to a vector of dependency IDs
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            DependsOn::None => vec![],
            DependsOn::Single(s) => vec![s.clone()],
            DependsOn::Multiple(v) => v.clone(),
        }
    }

    /// Check if this node has no dependencies (is an entry node)
    pub fn is_empty(&self) -> bool {
        match self {
            DependsOn::None => true,
            DependsOn::Single(_) => false,
            DependsOn::Multiple(v) => v.is_empty(),
        }
    }
}

/// How to wait for dependencies
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WaitMode {
    /// Wait for ALL dependencies to complete (default)
    #[default]
    All,
    /// Run when ANY dependency completes
    Any,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depends_on_none() {
        let deps = DependsOn::None;
        assert!(deps.is_empty());
        assert_eq!(deps.to_vec(), Vec::<String>::new());
    }

    #[test]
    fn test_depends_on_single() {
        let deps = DependsOn::Single("node_a".to_string());
        assert!(!deps.is_empty());
        assert_eq!(deps.to_vec(), vec!["node_a".to_string()]);
    }

    #[test]
    fn test_depends_on_multiple() {
        let deps = DependsOn::Multiple(vec!["a".to_string(), "b".to_string()]);
        assert!(!deps.is_empty());
        assert_eq!(deps.to_vec(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn test_wait_mode_default() {
        let mode = WaitMode::default();
        assert_eq!(mode, WaitMode::All);
    }

    #[test]
    fn test_depends_on_deserialize_none() {
        // When depends_on is not present, should default to None
        let yaml = r#"
            id: test
            agent:
              file: test.yaml
        "#;
        let node: NodeDefinition = serde_yaml::from_str(yaml).unwrap();
        assert!(node.depends_on.is_empty());
    }

    #[test]
    fn test_depends_on_deserialize_single() {
        let yaml = r#"
            id: test
            agent:
              file: test.yaml
            depends_on: node_a
        "#;
        let node: NodeDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(node.depends_on.to_vec(), vec!["node_a".to_string()]);
    }

    #[test]
    fn test_depends_on_deserialize_multiple() {
        let yaml = r#"
            id: test
            agent:
              file: test.yaml
            depends_on:
              - node_a
              - node_b
        "#;
        let node: NodeDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            node.depends_on.to_vec(),
            vec!["node_a".to_string(), "node_b".to_string()]
        );
    }

    #[test]
    fn test_node_with_outputs() {
        let yaml = r#"
            id: classifier
            agent:
              file: classifier.yaml
            outputs:
              intent: "intent"
              confidence: "score"
        "#;
        let node: NodeDefinition = serde_yaml::from_str(yaml).unwrap();
        let outputs = node.outputs.unwrap();
        assert_eq!(outputs.get("intent"), Some(&"intent".to_string()));
        assert_eq!(outputs.get("confidence"), Some(&"score".to_string()));
    }

    #[test]
    fn test_node_with_when_condition() {
        let yaml = r#"
            id: handler
            agent:
              file: handler.yaml
            depends_on: classifier
            when: "intent == 'search'"
        "#;
        let node: NodeDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(node.when, Some("intent == 'search'".to_string()));
    }

    #[test]
    fn test_node_with_output_schema() {
        let yaml = r#"
            id: analyzer
            agent:
              file: analyzer.yaml
            output_schema:
              type: object
              properties:
                severity:
                  type: string
                  enum: [low, medium, high]
              required:
                - severity
        "#;
        let node: NodeDefinition = serde_yaml::from_str(yaml).unwrap();
        let schema = node.output_schema.unwrap();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["severity"]["type"], "string");
    }
}
