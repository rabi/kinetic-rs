//! Graph workflow executor

use crate::adk::agent::Agent;
use crate::kinetic::workflow::condition;
use crate::kinetic::workflow::state::WorkflowState;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Arc;

use super::types::WaitMode;

/// Compiled node ready for execution
pub struct CompiledNode {
    pub id: String,
    pub agent: Arc<dyn Agent>,
    pub depends_on: Vec<String>,
    pub when: Option<String>,
    pub outputs: HashMap<String, String>,
    pub wait_mode: WaitMode,
}

/// Graph-based workflow executor
pub struct GraphAgent {
    name: String,
    #[allow(dead_code)]
    description: String,
    nodes: HashMap<String, CompiledNode>,
    node_order: Vec<String>, // Topological order for deterministic execution
}

impl GraphAgent {
    /// Create a new GraphAgent
    pub fn new(name: String, description: String, nodes: Vec<CompiledNode>) -> Self {
        let node_order: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let nodes_map: HashMap<String, CompiledNode> =
            nodes.into_iter().map(|n| (n.id.clone(), n)).collect();

        Self {
            name,
            description,
            nodes: nodes_map,
            node_order,
        }
    }

    /// Get nodes that are ready to execute
    fn get_ready_nodes(&self, completed: &HashSet<String>, state: &WorkflowState) -> Vec<&str> {
        self.node_order
            .iter()
            .filter(|id| !completed.contains(*id))
            .filter(|id| {
                let node = &self.nodes[*id];
                self.dependencies_satisfied(node, completed) && self.condition_met(node, state)
            })
            .map(|s| s.as_str())
            .collect()
    }

    /// Check if a node's dependencies are satisfied
    fn dependencies_satisfied(&self, node: &CompiledNode, completed: &HashSet<String>) -> bool {
        if node.depends_on.is_empty() {
            return true;
        }

        match node.wait_mode {
            WaitMode::All => node.depends_on.iter().all(|d| completed.contains(d)),
            WaitMode::Any => node.depends_on.iter().any(|d| completed.contains(d)),
        }
    }

    /// Check if a node's condition is met
    fn condition_met(&self, node: &CompiledNode, state: &WorkflowState) -> bool {
        match &node.when {
            None => true,
            Some(condition_str) => match condition::parse(condition_str) {
                Ok(expr) => condition::evaluate(&expr, state),
                Err(e) => {
                    log::error!("Failed to parse condition '{}': {}", condition_str, e);
                    false
                }
            },
        }
    }

    /// Execute a single node and return its output
    async fn execute_node(
        &self,
        node: &CompiledNode,
        input: &str,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        log::info!("Executing node: {}", node.id);
        node.agent.run(input.to_string()).await
    }

    /// Build input for a node based on its dependencies
    fn build_node_input(
        &self,
        original_input: &str,
        node: &CompiledNode,
        state: &WorkflowState,
    ) -> String {
        if node.depends_on.is_empty() {
            // No dependencies - use original input
            return original_input.to_string();
        }

        // Has dependencies - use the last dependency's output
        // For sequential workflows, this is the previous step's output
        let last_dep = node.depends_on.last().unwrap();
        let output_key = format!("output.{}", last_dep);

        if let Some(dep_output) = state.get(&output_key) {
            // Return the dependency's output as text
            match dep_output {
                serde_json::Value::String(s) => s.clone(),
                _ => dep_output.to_string(),
            }
        } else {
            // Dependency output not found, fall back to original input
            original_input.to_string()
        }
    }

    /// Extract output values and update state
    fn apply_outputs(&self, node: &CompiledNode, output: &str, state: &mut WorkflowState) {
        // Try to parse output as JSON first
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(output) {
            // Store as JSON value (not escaped string)
            state.update(&format!("output.{}", node.id), json.clone());

            // Extract mapped outputs
            for (state_key, json_path) in &node.outputs {
                if let Some(value) = extract_json_path(&json, json_path) {
                    state.update(state_key, value);
                }
            }
        } else {
            // Store as string value if not valid JSON
            state.update(
                &format!("output.{}", node.id),
                serde_json::Value::String(output.to_string()),
            );

            if !node.outputs.is_empty() {
                log::warn!(
                    "Node {} output is not JSON, but has output mappings",
                    node.id
                );
            }
        }
    }

    /// Format the final response from state as human-readable text
    /// Only returns output from "terminal" nodes (nodes that aren't dependencies of others)
    fn format_response(&self, state: &WorkflowState) -> String {
        // Find terminal nodes - nodes that are NOT dependencies of any other node
        let all_deps: HashSet<String> = self
            .nodes
            .values()
            .flat_map(|n| n.depends_on.iter().cloned())
            .collect();

        let terminal_nodes: Vec<&str> = self
            .nodes
            .values()
            .filter(|n| !all_deps.contains(&n.id))
            .map(|n| n.id.as_str())
            .collect();

        let state_json = state.to_json();

        // Collect outputs only from terminal nodes
        let outputs: Vec<_> = state_json
            .as_object()
            .map(|obj| {
                obj.iter()
                    .filter(|(k, _)| {
                        if let Some(node_id) = k.strip_prefix("output.") {
                            terminal_nodes.contains(&node_id)
                        } else {
                            false
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // If there's only one terminal output, return just that value
        if outputs.len() == 1 {
            let (_, value) = outputs[0];
            return self.value_to_text(value);
        }

        // Multiple terminal outputs (parallel execution) - combine them
        let mut result = String::new();
        for (i, (_key, value)) in outputs.iter().enumerate() {
            if i > 0 {
                result.push_str("\n\n---\n\n");
            }

            let text = self.value_to_text(value);
            result.push_str(&text);
        }

        result
    }

    /// Convert a JSON value to readable text
    fn value_to_text(&self, value: &serde_json::Value) -> String {
        value_to_text(value)
    }
}

/// Convert a JSON value to readable text (free function to avoid clippy recursion warning)
fn value_to_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(obj) => {
            // If object has a single "result" or "answer" key, extract it
            if obj.len() == 1 {
                if let Some(v) = obj
                    .get("result")
                    .or(obj.get("answer"))
                    .or(obj.get("response"))
                {
                    return value_to_text(v);
                }
            }
            // Otherwise, format key-value pairs
            obj.iter()
                .map(|(k, v)| format!("**{}**: {}", k, value_to_text(v)))
                .collect::<Vec<_>>()
                .join("\n")
        }
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(|v| format!("- {}", value_to_text(v)))
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "".to_string(),
    }
}

/// Extract a value from JSON using a simple dot-notation path
fn extract_json_path(json: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = json;

    for part in parts {
        current = current.get(part)?;
    }

    Some(current.clone())
}

#[async_trait]
impl Agent for GraphAgent {
    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut state = WorkflowState::empty();
        state.update("input", serde_json::Value::String(input.clone()));

        let mut completed: HashSet<String> = HashSet::new();
        let mut iteration = 0;
        let max_iterations = 100; // Safety limit

        loop {
            iteration += 1;
            if iteration > max_iterations {
                log::error!("Graph execution exceeded max iterations");
                break;
            }

            let ready = self.get_ready_nodes(&completed, &state);

            if ready.is_empty() {
                // No more nodes to run
                break;
            }

            log::info!(
                "Graph iteration {}: executing {} nodes: {:?}",
                iteration,
                ready.len(),
                ready
            );

            // Execute ready nodes (could be parallelized with futures::join_all)
            // For now, execute sequentially for simplicity
            for node_id in ready {
                let node = &self.nodes[node_id];

                // Build input for this node:
                // - If node has dependencies, pass the last dependency's output
                // - Otherwise, pass the original input
                let node_input = self.build_node_input(&input, node, &state);

                match self.execute_node(node, &node_input).await {
                    Ok(output) => {
                        self.apply_outputs(node, &output, &mut state);
                        completed.insert(node_id.to_string());
                        log::info!("Node {} completed", node_id);
                    }
                    Err(e) => {
                        log::error!("Node {} failed: {}", node_id, e);
                        state.update(
                            &format!("{}.error", node_id),
                            serde_json::Value::String(e.to_string()),
                        );
                        // Continue with other nodes (don't fail entire workflow)
                        completed.insert(node_id.to_string());
                    }
                }
            }
        }

        // Return formatted response
        Ok(self.format_response(&state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    // Mock agent for testing - returns fixed response
    struct MockNodeAgent {
        name: String,
        response: String,
    }

    impl MockNodeAgent {
        fn new(name: &str, response: &str) -> Self {
            Self {
                name: name.to_string(),
                response: response.to_string(),
            }
        }
    }

    #[async_trait]
    impl Agent for MockNodeAgent {
        fn name(&self) -> String {
            self.name.clone()
        }

        async fn run(&self, _input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
            Ok(self.response.clone())
        }
    }

    // Mock agent that captures input for verification
    struct InputCapturingAgent {
        name: String,
        response: String,
        captured_input: Arc<Mutex<Option<String>>>,
    }

    impl InputCapturingAgent {
        fn new(name: &str, response: &str) -> (Self, Arc<Mutex<Option<String>>>) {
            let captured = Arc::new(Mutex::new(None));
            (
                Self {
                    name: name.to_string(),
                    response: response.to_string(),
                    captured_input: captured.clone(),
                },
                captured,
            )
        }
    }

    #[async_trait]
    impl Agent for InputCapturingAgent {
        fn name(&self) -> String {
            self.name.clone()
        }

        async fn run(&self, input: String) -> Result<String, Box<dyn Error + Send + Sync>> {
            *self.captured_input.lock().unwrap() = Some(input);
            Ok(self.response.clone())
        }
    }

    fn make_node(id: &str, agent: Arc<dyn Agent>, depends_on: Vec<&str>) -> CompiledNode {
        CompiledNode {
            id: id.to_string(),
            agent,
            depends_on: depends_on.into_iter().map(|s| s.to_string()).collect(),
            when: None,
            outputs: HashMap::new(),
            wait_mode: WaitMode::All,
        }
    }

    #[tokio::test]
    async fn test_single_node_execution() {
        let agent = Arc::new(MockNodeAgent::new("test", r#"{"result": "done"}"#));
        let node = make_node("main", agent, vec![]);

        let graph = GraphAgent::new("test".to_string(), "test".to_string(), vec![node]);

        let result = graph.run("input".to_string()).await.unwrap();

        // Single node with result key - should extract "done"
        assert_eq!(result, "done");
    }

    #[tokio::test]
    async fn test_sequential_execution() {
        let agent_a = Arc::new(MockNodeAgent::new("A", "step a complete"));
        let agent_b = Arc::new(MockNodeAgent::new("B", "step b complete"));

        let node_a = make_node("a", agent_a, vec![]);
        let node_b = make_node("b", agent_b, vec!["a"]);

        let graph = GraphAgent::new(
            "seq".to_string(),
            "sequential".to_string(),
            vec![node_a, node_b],
        );

        let result = graph.run("start".to_string()).await.unwrap();

        // Sequential workflow - only terminal node's output should be returned
        assert!(result.contains("step b complete"));
        assert!(
            !result.contains("step a complete"),
            "Intermediate outputs should not be shown"
        );
    }

    #[tokio::test]
    async fn test_parallel_execution() {
        let agent_a = Arc::new(MockNodeAgent::new("A", "result_a"));
        let agent_b = Arc::new(MockNodeAgent::new("B", "result_b"));

        let node_a = make_node("a", agent_a, vec![]);
        let node_b = make_node("b", agent_b, vec![]);

        let graph = GraphAgent::new(
            "par".to_string(),
            "parallel".to_string(),
            vec![node_a, node_b],
        );

        let result = graph.run("start".to_string()).await.unwrap();

        // Multiple outputs - should contain both
        assert!(result.contains("result_a"));
        assert!(result.contains("result_b"));
    }

    #[tokio::test]
    async fn test_conditional_execution() {
        let agent_a = Arc::new(MockNodeAgent::new("A", r#"{"intent": "search"}"#));
        let agent_b = Arc::new(MockNodeAgent::new("B", "search_result"));
        let agent_c = Arc::new(MockNodeAgent::new("C", "code_result"));

        let mut node_a = make_node("a", agent_a, vec![]);
        node_a
            .outputs
            .insert("intent".to_string(), "intent".to_string());

        let mut node_b = make_node("b", agent_b, vec!["a"]);
        node_b.when = Some("intent == 'search'".to_string());

        let mut node_c = make_node("c", agent_c, vec!["a"]);
        node_c.when = Some("intent == 'code'".to_string());

        let graph = GraphAgent::new(
            "cond".to_string(),
            "conditional".to_string(),
            vec![node_a, node_b, node_c],
        );

        let result = graph.run("test".to_string()).await.unwrap();

        // Node A and B should execute, C should not (wrong intent)
        assert!(result.contains("search_result"));
        assert!(!result.contains("code_result"));
    }

    #[tokio::test]
    async fn test_sequential_data_flow() {
        // This test verifies that node B receives node A's output as input
        // This is critical for sequential workflows where data flows between steps

        let agent_a = Arc::new(MockNodeAgent::new("A", "data from step A"));
        let (agent_b, captured_b) = InputCapturingAgent::new("B", "processed by B");

        let node_a = make_node("a", agent_a, vec![]);
        let node_b = make_node("b", Arc::new(agent_b), vec!["a"]);

        let graph = GraphAgent::new(
            "seq".to_string(),
            "sequential data flow".to_string(),
            vec![node_a, node_b],
        );

        let _ = graph.run("original input".to_string()).await.unwrap();

        // Verify node B received node A's output, NOT the original input
        let b_input = captured_b.lock().unwrap().clone().unwrap();
        assert_eq!(
            b_input, "data from step A",
            "Node B should receive A's output"
        );
        assert_ne!(
            b_input, "original input",
            "Node B should NOT receive original input"
        );
    }

    #[tokio::test]
    async fn test_first_node_receives_original_input() {
        // Verify that nodes with no dependencies receive the original input

        let (agent_a, captured_a) = InputCapturingAgent::new("A", "response");

        let node_a = make_node("a", Arc::new(agent_a), vec![]);

        let graph = GraphAgent::new("single".to_string(), "test".to_string(), vec![node_a]);

        let _ = graph.run("my original input".to_string()).await.unwrap();

        let a_input = captured_a.lock().unwrap().clone().unwrap();
        assert_eq!(a_input, "my original input");
    }

    #[test]
    fn test_extract_json_path() {
        let json = json!({
            "result": {
                "data": {
                    "value": 42
                }
            }
        });

        assert_eq!(
            extract_json_path(&json, "result"),
            Some(json!({"data": {"value": 42}}))
        );
        assert_eq!(
            extract_json_path(&json, "result.data"),
            Some(json!({"value": 42}))
        );
        assert_eq!(
            extract_json_path(&json, "result.data.value"),
            Some(json!(42))
        );
        assert_eq!(extract_json_path(&json, "nonexistent"), None);
    }

    #[test]
    fn test_dependencies_satisfied_all() {
        let agent = Arc::new(MockNodeAgent::new("test", ""));
        let node = CompiledNode {
            id: "test".to_string(),
            agent,
            depends_on: vec!["a".to_string(), "b".to_string()],
            when: None,
            outputs: HashMap::new(),
            wait_mode: WaitMode::All,
        };

        let graph = GraphAgent::new("test".to_string(), "".to_string(), vec![]);

        let mut completed = HashSet::new();
        assert!(!graph.dependencies_satisfied(&node, &completed));

        completed.insert("a".to_string());
        assert!(!graph.dependencies_satisfied(&node, &completed));

        completed.insert("b".to_string());
        assert!(graph.dependencies_satisfied(&node, &completed));
    }

    #[test]
    fn test_dependencies_satisfied_any() {
        let agent = Arc::new(MockNodeAgent::new("test", ""));
        let node = CompiledNode {
            id: "test".to_string(),
            agent,
            depends_on: vec!["a".to_string(), "b".to_string()],
            when: None,
            outputs: HashMap::new(),
            wait_mode: WaitMode::Any,
        };

        let graph = GraphAgent::new("test".to_string(), "".to_string(), vec![]);

        let mut completed = HashSet::new();
        assert!(!graph.dependencies_satisfied(&node, &completed));

        completed.insert("a".to_string());
        assert!(graph.dependencies_satisfied(&node, &completed)); // Any mode: one is enough
    }
}
