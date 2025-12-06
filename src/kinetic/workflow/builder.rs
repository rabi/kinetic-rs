use crate::adk::agent::{Agent, LLMAgent, LoopAgent, ParallelAgent, SequentialAgent};
use crate::adk::gemini::GeminiModel;
use crate::adk::model::Model;
use crate::adk::tool::Tool;
use crate::kinetic::workflow::loader::{AgentDefinition, WorkflowDefinition, WorkflowLoader};

use crate::kinetic::mcp::manager::McpServiceManager;
use crate::kinetic::workflow::registry::ToolRegistry;

use std::env;
use std::error::Error;
use std::sync::Arc;

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

    /// Infer the provider from the model name prefix
    fn infer_provider_from_model(model_name: &str) -> String {
        let name_lower = model_name.to_lowercase();
        if name_lower.starts_with("gemini") || name_lower.starts_with("models/gemini") {
            "Gemini".to_string()
        } else if name_lower.starts_with("gpt") || name_lower.starts_with("o1") {
            "OpenAI".to_string()
        } else if name_lower.starts_with("claude") {
            "Anthropic".to_string()
        } else if name_lower.starts_with("deepseek") {
            "DeepSeek".to_string()
        } else {
            // Default to Gemini
            "Gemini".to_string()
        }
    }

    pub async fn build_agent(
        &self,
        file_path: &str,
    ) -> Result<Arc<dyn Agent>, Box<dyn Error + Send + Sync>> {
        let def = self.loader.load_workflow(file_path)?;
        self.build_from_def(&def).await
    }

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
            // Initialize MCP services and register tools
            if !def.mcp_servers.is_empty() {
                for server_config in &def.mcp_servers {
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

            match def.kind.as_str() {
                "Direct" => {
                    if let Some(agent_def) = &def.agent {
                        self.build_llm_agent(agent_def).await
                    } else {
                        Err("Direct workflow missing agent definition".into())
                    }
                }
                "Composite" => {
                    if let Some(workflow_def) = &def.workflow {
                        let mut agents = Vec::new();
                        // Handle agents (both inline and references)
                        for agent_config in &workflow_def.agents {
                            match agent_config {
                                crate::kinetic::workflow::loader::AgentConfig::Inline(
                                    agent_def,
                                ) => {
                                    agents.push(self.build_llm_agent(agent_def).await?);
                                }
                                crate::kinetic::workflow::loader::AgentConfig::Reference(
                                    ref_def,
                                ) => {
                                    let sub_agent = self.build_agent(&ref_def.file).await?;
                                    agents.push(sub_agent);
                                }
                            }
                        }

                        match workflow_def.execution.as_str() {
                            "sequential" => Ok(Arc::new(SequentialAgent::new(
                                def.name.clone(),
                                def.description.clone(),
                                agents,
                            )) as Arc<dyn Agent>),
                            "parallel" => Ok(Arc::new(ParallelAgent::new(
                                def.name.clone(),
                                def.description.clone(),
                                agents,
                            )) as Arc<dyn Agent>),
                            "loop" => {
                                if agents.len() != 1 {
                                    return Err("Loop workflow must have exactly one agent".into());
                                }
                                Ok(Arc::new(LoopAgent::new(
                                    def.name.clone(),
                                    def.description.clone(),
                                    agents[0].clone(),
                                    workflow_def.max_iterations.unwrap_or(1),
                                )) as Arc<dyn Agent>)
                            }
                            _ => Err(
                                format!("Unknown execution mode: {}", workflow_def.execution)
                                    .into(),
                            ),
                        }
                    } else {
                        Err("Composite workflow missing workflow definition".into())
                    }
                }
                _ => Err(format!("Unknown workflow kind: {}", def.kind).into()),
            }
        })
    }

    async fn build_llm_agent(
        &self,
        def: &AgentDefinition,
    ) -> Result<Arc<dyn Agent>, Box<dyn Error + Send + Sync>> {
        // Get model name from definition, env var, or default
        let model_name = def.model.model_name.clone().unwrap_or_else(|| {
            env::var("MODEL_NAME")
                .or_else(|_| env::var("GEMINI_MODEL"))
                .unwrap_or_else(|_| "gemini-2.0-flash".to_string())
        });

        // Infer provider from: explicit definition > MODEL_PROVIDER env > model name prefix
        let provider = def
            .model
            .provider
            .clone()
            .or_else(|| env::var("MODEL_PROVIDER").ok())
            .unwrap_or_else(|| Self::infer_provider_from_model(&model_name));

        log::debug!("Using provider '{}' with model '{}'", provider, model_name);

        let model: Arc<dyn Model> = match provider.as_str() {
            "Gemini" | "Google" | "gemini" | "" => Arc::new(GeminiModel::new(model_name)?),
            // "OpenAI" | "openai" => Arc::new(OpenAIModel::new(model_name)?), // TODO: Implement
            // "Anthropic" | "anthropic" => Arc::new(AnthropicModel::new(model_name)?), // TODO
            _ => return Err(format!("Unknown model provider: {}", provider).into()),
        };

        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();
        for tool_name in &def.tools {
            if let Some(tool) = self.registry.get(tool_name).await {
                tools.push(tool.clone());
            } else {
                log::warn!("Tool not found: {}", tool_name);
            }
        }

        Ok(Arc::new(LLMAgent::new(
            def.name.clone(),
            def.description.clone(),
            def.instructions.clone(),
            model,
            tools,
        )))
    }

    async fn initialize_mcp_server(
        &self,
        config: &crate::kinetic::workflow::loader::McpServerConfig,
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
    use crate::kinetic::workflow::loader::CompositeWorkflowDefinition;

    // === Provider Inference Tests ===

    #[test]
    fn test_infer_provider_gemini() {
        assert_eq!(Builder::infer_provider_from_model("gemini-2.0-flash"), "Gemini");
        assert_eq!(Builder::infer_provider_from_model("gemini-1.5-pro"), "Gemini");
        assert_eq!(Builder::infer_provider_from_model("Gemini-2.0-Flash"), "Gemini");
        assert_eq!(Builder::infer_provider_from_model("models/gemini-2.0-flash"), "Gemini");
    }

    #[test]
    fn test_infer_provider_openai() {
        assert_eq!(Builder::infer_provider_from_model("gpt-4"), "OpenAI");
        assert_eq!(Builder::infer_provider_from_model("gpt-4o"), "OpenAI");
        assert_eq!(Builder::infer_provider_from_model("gpt-3.5-turbo"), "OpenAI");
        assert_eq!(Builder::infer_provider_from_model("GPT-4"), "OpenAI");
        assert_eq!(Builder::infer_provider_from_model("o1-preview"), "OpenAI");
        assert_eq!(Builder::infer_provider_from_model("o1-mini"), "OpenAI");
    }

    #[test]
    fn test_infer_provider_anthropic() {
        assert_eq!(Builder::infer_provider_from_model("claude-3-opus"), "Anthropic");
        assert_eq!(Builder::infer_provider_from_model("claude-3-sonnet"), "Anthropic");
        assert_eq!(Builder::infer_provider_from_model("Claude-3.5-Sonnet"), "Anthropic");
    }

    #[test]
    fn test_infer_provider_deepseek() {
        assert_eq!(Builder::infer_provider_from_model("deepseek-chat"), "DeepSeek");
        assert_eq!(Builder::infer_provider_from_model("deepseek-coder"), "DeepSeek");
        assert_eq!(Builder::infer_provider_from_model("DeepSeek-V2"), "DeepSeek");
    }

    #[test]
    fn test_infer_provider_unknown_defaults_to_gemini() {
        assert_eq!(Builder::infer_provider_from_model("unknown-model"), "Gemini");
        assert_eq!(Builder::infer_provider_from_model("my-custom-model"), "Gemini");
        assert_eq!(Builder::infer_provider_from_model(""), "Gemini");
    }

    // === Workflow Definition Validation Tests ===

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

    // Note: Testing build_llm_agent requires mocking GeminiModel which needs an API key.
    // Those are better suited for integration tests with actual YAML files.
}
