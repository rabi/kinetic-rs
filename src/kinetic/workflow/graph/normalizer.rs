//! Workflow normalization - converts Direct/Composite to Graph format

use super::types::{DependsOn, GraphWorkflowDef, NodeDefinition, WaitMode};
use crate::kinetic::workflow::types::{
    AgentConfig, GraphDependsOn as LoaderDependsOn, WorkflowDefinition,
};
use std::error::Error;

/// Normalize any workflow definition to graph format
pub fn normalize_to_graph(
    def: &WorkflowDefinition,
) -> Result<GraphWorkflowDef, Box<dyn Error + Send + Sync>> {
    match def.kind.as_str() {
        "Direct" => normalize_direct(def),
        "Composite" => normalize_composite(def),
        "Graph" => normalize_graph(def),
        other => Err(format!("Unknown workflow kind: {}", other).into()),
    }
}

fn normalize_direct(
    def: &WorkflowDefinition,
) -> Result<GraphWorkflowDef, Box<dyn Error + Send + Sync>> {
    let agent_def = def
        .agent
        .as_ref()
        .ok_or("Direct workflow missing agent definition")?;

    let node = NodeDefinition {
        id: "main".to_string(),
        agent: AgentConfig::Inline(Box::new(agent_def.clone())),
        depends_on: DependsOn::None,
        when: None,
        output_schema: None,
        outputs: None,
        wait_for: WaitMode::All,
    };

    Ok(GraphWorkflowDef {
        name: def.name.clone(),
        description: def.description.clone(),
        state: None,
        nodes: vec![node],
    })
}

fn normalize_composite(
    def: &WorkflowDefinition,
) -> Result<GraphWorkflowDef, Box<dyn Error + Send + Sync>> {
    let workflow_def = def
        .workflow
        .as_ref()
        .ok_or("Composite workflow missing workflow definition")?;

    let mut nodes = Vec::new();
    let agent_configs = &workflow_def.agents;

    match workflow_def.execution.as_str() {
        "sequential" => {
            // Each node depends on the previous one
            let mut prev_id: Option<String> = None;
            for (i, agent_config) in agent_configs.iter().enumerate() {
                let id = format!("step_{}", i);
                let depends_on = match &prev_id {
                    Some(prev) => DependsOn::Single(prev.clone()),
                    None => DependsOn::None,
                };

                nodes.push(NodeDefinition {
                    id: id.clone(),
                    agent: agent_config.clone(),
                    depends_on,
                    when: None,
                    output_schema: None,
                    outputs: None,
                    wait_for: WaitMode::All,
                });

                prev_id = Some(id);
            }
        }
        "parallel" => {
            // All nodes run in parallel (no dependencies)
            for (i, agent_config) in agent_configs.iter().enumerate() {
                nodes.push(NodeDefinition {
                    id: format!("parallel_{}", i),
                    agent: agent_config.clone(),
                    depends_on: DependsOn::None,
                    when: None,
                    output_schema: None,
                    outputs: None,
                    wait_for: WaitMode::All,
                });
            }
        }
        "loop" => {
            // For loop, we create nodes that can be iterated
            // The actual iteration is handled by the executor based on max_iterations
            for (i, agent_config) in agent_configs.iter().enumerate() {
                let depends_on = if i == 0 {
                    DependsOn::None
                } else {
                    DependsOn::Single(format!("loop_{}", i - 1))
                };

                nodes.push(NodeDefinition {
                    id: format!("loop_{}", i),
                    agent: agent_config.clone(),
                    depends_on,
                    when: None,
                    output_schema: None,
                    outputs: None,
                    wait_for: WaitMode::All,
                });
            }
        }
        other => {
            return Err(format!("Unknown execution mode: {}", other).into());
        }
    }

    Ok(GraphWorkflowDef {
        name: def.name.clone(),
        description: def.description.clone(),
        state: None,
        nodes,
    })
}

fn normalize_graph(
    def: &WorkflowDefinition,
) -> Result<GraphWorkflowDef, Box<dyn Error + Send + Sync>> {
    let graph_def = def
        .graph
        .as_ref()
        .ok_or("Graph workflow missing graph definition")?;

    let mut nodes = Vec::new();

    for node_def in &graph_def.nodes {
        let depends_on = match &node_def.depends_on {
            LoaderDependsOn::None => DependsOn::None,
            LoaderDependsOn::Single(s) => DependsOn::Single(s.clone()),
            LoaderDependsOn::Multiple(v) => DependsOn::Multiple(v.clone()),
        };

        let wait_mode = match node_def.wait_for.as_str() {
            "any" => WaitMode::Any,
            _ => WaitMode::All,
        };

        nodes.push(NodeDefinition {
            id: node_def.id.clone(),
            agent: node_def.agent.clone(),
            depends_on,
            when: node_def.when.clone(),
            output_schema: node_def.output_schema.clone(),
            outputs: node_def.outputs.clone(),
            wait_for: wait_mode,
        });
    }

    Ok(GraphWorkflowDef {
        name: def.name.clone(),
        description: def.description.clone(),
        state: None, // State schema could be parsed here if needed
        nodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinetic::workflow::types::{
        AgentDefinition, CompositeWorkflowDefinition, ModelDefinition, WorkflowReference,
    };

    fn make_agent_def(name: &str) -> AgentDefinition {
        AgentDefinition {
            name: name.to_string(),
            description: format!("{} description", name),
            instructions: format!("{} instructions", name),
            executor: None,
            model: ModelDefinition {
                provider: None,
                model_name: None,
                kind: Some("llm".to_string()),
                parameters: None,
            },
            tools: vec![],
            memory: None,
            workflow: None,
            max_iterations: None,
        }
    }

    #[test]
    fn test_normalize_direct() {
        let def = WorkflowDefinition {
            name: "TestDirect".to_string(),
            description: "Test".to_string(),
            kind: "Direct".to_string(),
            agent: Some(make_agent_def("TestAgent")),
            workflow: None,
            graph: None,
            overrides: None,
            mcp_servers: vec![],
        };

        let graph = normalize_to_graph(&def).unwrap();
        assert_eq!(graph.name, "TestDirect");
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].id, "main");
        assert!(graph.nodes[0].depends_on.is_empty());
    }

    #[test]
    fn test_normalize_sequential() {
        let def = WorkflowDefinition {
            name: "TestSeq".to_string(),
            description: "Test".to_string(),
            kind: "Composite".to_string(),
            agent: None,
            workflow: Some(CompositeWorkflowDefinition {
                execution: "sequential".to_string(),
                agents: vec![
                    AgentConfig::Inline(Box::new(make_agent_def("A"))),
                    AgentConfig::Inline(Box::new(make_agent_def("B"))),
                    AgentConfig::Inline(Box::new(make_agent_def("C"))),
                ],
                max_iterations: None,
            }),
            graph: None,
            overrides: None,
            mcp_servers: vec![],
        };

        let graph = normalize_to_graph(&def).unwrap();
        assert_eq!(graph.nodes.len(), 3);

        // First node has no dependencies
        assert!(graph.nodes[0].depends_on.is_empty());

        // Second depends on first
        assert_eq!(graph.nodes[1].depends_on.to_vec(), vec!["step_0"]);

        // Third depends on second
        assert_eq!(graph.nodes[2].depends_on.to_vec(), vec!["step_1"]);
    }

    #[test]
    fn test_normalize_parallel() {
        let def = WorkflowDefinition {
            name: "TestPar".to_string(),
            description: "Test".to_string(),
            kind: "Composite".to_string(),
            agent: None,
            workflow: Some(CompositeWorkflowDefinition {
                execution: "parallel".to_string(),
                agents: vec![
                    AgentConfig::Inline(Box::new(make_agent_def("A"))),
                    AgentConfig::Inline(Box::new(make_agent_def("B"))),
                ],
                max_iterations: None,
            }),
            graph: None,
            overrides: None,
            mcp_servers: vec![],
        };

        let graph = normalize_to_graph(&def).unwrap();
        assert_eq!(graph.nodes.len(), 2);

        // All nodes have no dependencies (parallel)
        assert!(graph.nodes[0].depends_on.is_empty());
        assert!(graph.nodes[1].depends_on.is_empty());
    }

    #[test]
    fn test_normalize_with_references() {
        let def = WorkflowDefinition {
            name: "TestRef".to_string(),
            description: "Test".to_string(),
            kind: "Composite".to_string(),
            agent: None,
            workflow: Some(CompositeWorkflowDefinition {
                execution: "sequential".to_string(),
                agents: vec![
                    AgentConfig::Reference(WorkflowReference {
                        file: "agents/a.yaml".to_string(),
                        overrides: None,
                    }),
                    AgentConfig::Reference(WorkflowReference {
                        file: "agents/b.yaml".to_string(),
                        overrides: None,
                    }),
                ],
                max_iterations: None,
            }),
            graph: None,
            overrides: None,
            mcp_servers: vec![],
        };

        let graph = normalize_to_graph(&def).unwrap();
        assert_eq!(graph.nodes.len(), 2);
    }

    #[test]
    fn test_unknown_kind_error() {
        let def = WorkflowDefinition {
            name: "Test".to_string(),
            description: "Test".to_string(),
            kind: "Unknown".to_string(),
            agent: None,
            workflow: None,
            graph: None,
            overrides: None,
            mcp_servers: vec![],
        };

        let result = normalize_to_graph(&def);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown workflow kind"));
    }

    #[test]
    fn test_unknown_execution_error() {
        let def = WorkflowDefinition {
            name: "Test".to_string(),
            description: "Test".to_string(),
            kind: "Composite".to_string(),
            agent: None,
            workflow: Some(CompositeWorkflowDefinition {
                execution: "unknown".to_string(),
                agents: vec![],
                max_iterations: None,
            }),
            graph: None,
            overrides: None,
            mcp_servers: vec![],
        };

        let result = normalize_to_graph(&def);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown execution mode"));
    }
}
