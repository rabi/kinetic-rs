// SPDX-License-Identifier: MIT

//! Workflow builder - orchestrates workflow construction
//!
//! This module provides the high-level Builder that loads workflow definitions
//! and constructs executable agent graphs.

use crate::adk::agent::Agent;
use crate::kinetic::mcp::manager::McpServiceManager;
use crate::kinetic::workflow::agent_factory::AgentFactory;
use crate::kinetic::workflow::graph::types::GraphWorkflowDef;
use crate::kinetic::workflow::graph::{normalize_to_graph, CompiledNode, GraphAgent, WaitMode};
use crate::kinetic::workflow::loader::WorkflowLoader;
use crate::kinetic::workflow::registry::ToolRegistry;
use crate::kinetic::workflow::types::{AgentConfig, McpServerConfig, WorkflowDefinition};

use std::error::Error;
use std::sync::Arc;

/// High-level builder for constructing workflows from YAML definitions
pub struct Builder {
    loader: WorkflowLoader,
    registry: ToolRegistry,
    mcp_manager: Arc<McpServiceManager>,
}

impl Builder {
    pub fn new(registry: ToolRegistry, mcp_manager: Arc<McpServiceManager>) -> Self {
        Self {
            loader: WorkflowLoader::new(),
            registry,
            mcp_manager,
        }
    }

    /// Build a workflow agent from a YAML file path
    pub async fn build_agent(
        &self,
        file_path: &str,
    ) -> Result<Arc<dyn Agent>, Box<dyn Error + Send + Sync>> {
        let def = self.loader.load_workflow(file_path)?;
        self.build_from_def(&def).await
    }

    /// Build a workflow agent from a parsed definition
    #[allow(clippy::type_complexity)]
    fn build_from_def<'a>(
        &'a self,
        def: &'a WorkflowDefinition,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Arc<dyn Agent>, Box<dyn Error + Send + Sync>>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            // Initialize MCP services if configured
            self.initialize_mcp_servers(&def.mcp_servers).await;

            // Normalize all workflow kinds to graph format
            let graph_def = normalize_to_graph(def)?;

            log::info!(
                "Normalized '{}' workflow '{}' to graph with {} nodes",
                def.kind,
                def.name,
                graph_def.nodes.len()
            );

            // Build the graph agent from the normalized definition
            self.build_graph_from_def(&graph_def).await
        })
    }

    /// Build a GraphAgent from a normalized GraphWorkflowDef
    async fn build_graph_from_def(
        &self,
        graph_def: &GraphWorkflowDef,
    ) -> Result<Arc<dyn Agent>, Box<dyn Error + Send + Sync>> {
        let factory = AgentFactory::new(&self.registry);
        let mut compiled_nodes = Vec::new();

        for node_def in &graph_def.nodes {
            // Build the agent for this node
            let agent = match &node_def.agent {
                AgentConfig::Inline(agent_def) => factory.build(agent_def).await?,
                AgentConfig::Reference(ref_def) => self.build_agent(&ref_def.file).await?,
            };

            // Convert depends_on
            let depends_on = node_def.depends_on.to_vec();

            // Convert wait_for
            let wait_mode = match &node_def.wait_for {
                WaitMode::Any => WaitMode::Any,
                WaitMode::All => WaitMode::All,
            };

            // Convert outputs
            let outputs = node_def.outputs.clone().unwrap_or_default();

            compiled_nodes.push(CompiledNode {
                id: node_def.id.clone(),
                agent,
                depends_on,
                when: node_def.when.clone(),
                outputs,
                wait_mode,
            });
        }

        log::info!(
            "Built graph agent '{}' with {} nodes",
            graph_def.name,
            compiled_nodes.len()
        );

        Ok(Arc::new(GraphAgent::new(
            graph_def.name.clone(),
            graph_def.description.clone(),
            compiled_nodes,
        )))
    }

    /// Initialize MCP servers and register their tools
    async fn initialize_mcp_servers(&self, servers: &[McpServerConfig]) {
        for server_config in servers {
            match self.initialize_mcp_server(server_config).await {
                Ok(_) => log::info!("Initialized MCP server: {}", server_config.name),
                Err(e) => log::error!(
                    "Failed to initialize MCP server {}: {}",
                    server_config.name,
                    e
                ),
            }
        }
    }

    async fn initialize_mcp_server(
        &self,
        config: &McpServerConfig,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        use crate::kinetic::mcp::manager::McpServerConfig as ManagerConfig;
        use crate::kinetic::mcp::tool::McpTool;

        // Convert loader config to manager config
        let manager_config = ManagerConfig {
            name: config.name.clone(),
            command: config.command.clone(),
            args: config.args.clone(),
        };

        // Get or create the MCP service
        let service = self
            .mcp_manager
            .get_or_create_service(&manager_config)
            .await?;

        // List all tools from the service
        let tools = {
            let service_lock = service.read().await;
            service_lock.list_all_tools().await?
        };

        // Register each tool in the registry with namespaced name
        for tool in tools {
            let tool_name = format!("{}:{}", config.name, tool.name);
            let mcp_tool = McpTool::new(
                service.clone(),
                tool.name.to_string(),
                tool.description.unwrap_or_default().to_string(),
                serde_json::to_value(&tool.input_schema).unwrap_or_default(),
            );

            self.registry.register(Arc::new(mcp_tool)).await;
            log::info!("Registered MCP tool: {}", tool_name);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinetic::workflow::types::CompositeWorkflowDefinition;

    #[tokio::test]
    async fn test_direct_workflow_missing_agent_returns_error() {
        let registry = ToolRegistry::new();
        let mcp_manager = Arc::new(McpServiceManager::new());
        let builder = Builder::new(registry, mcp_manager);

        let def = WorkflowDefinition {
            name: "Test".to_string(),
            description: "Test".to_string(),
            kind: "Direct".to_string(),
            agent: None, // Missing agent!
            workflow: None,
            graph: None,
            overrides: None,
            mcp_servers: vec![],
        };

        let result = builder.build_from_def(&def).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("missing agent"));
    }

    #[tokio::test]
    async fn test_composite_workflow_missing_workflow_returns_error() {
        let registry = ToolRegistry::new();
        let mcp_manager = Arc::new(McpServiceManager::new());
        let builder = Builder::new(registry, mcp_manager);

        let def = WorkflowDefinition {
            name: "Test".to_string(),
            description: "Test".to_string(),
            kind: "Composite".to_string(),
            agent: None,
            workflow: None, // Missing workflow!
            graph: None,
            overrides: None,
            mcp_servers: vec![],
        };

        let result = builder.build_from_def(&def).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("missing workflow"));
    }

    #[tokio::test]
    async fn test_unknown_workflow_kind_returns_error() {
        let registry = ToolRegistry::new();
        let mcp_manager = Arc::new(McpServiceManager::new());
        let builder = Builder::new(registry, mcp_manager);

        let def = WorkflowDefinition {
            name: "Test".to_string(),
            description: "Test".to_string(),
            kind: "InvalidKind".to_string(),
            agent: None,
            workflow: None,
            graph: None,
            overrides: None,
            mcp_servers: vec![],
        };

        let result = builder.build_from_def(&def).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("Unknown workflow kind"));
    }

    #[tokio::test]
    async fn test_unknown_execution_mode_returns_error() {
        let registry = ToolRegistry::new();
        let mcp_manager = Arc::new(McpServiceManager::new());
        let builder = Builder::new(registry, mcp_manager);

        let def = WorkflowDefinition {
            name: "Test".to_string(),
            description: "Test".to_string(),
            kind: "Composite".to_string(),
            agent: None,
            workflow: Some(CompositeWorkflowDefinition {
                execution: "invalid_mode".to_string(),
                agents: vec![],
                max_iterations: None,
            }),
            graph: None,
            overrides: None,
            mcp_servers: vec![],
        };

        let result = builder.build_from_def(&def).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.to_string().contains("Unknown execution mode"));
    }

    #[test]
    fn test_builder_new() {
        let registry = ToolRegistry::new();
        let mcp_manager = Arc::new(McpServiceManager::new());
        let _builder = Builder::new(registry, mcp_manager);
        // Just verify it doesn't panic
    }
}
