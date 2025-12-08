use clap::{Parser, Subcommand};
use dotenv::dotenv;
use kinetic_rs::adk::agent::{Agent, LLMAgent};

use kinetic_rs::kinetic::tools::{github, jira, search};
use kinetic_rs::kinetic::workflow::builder::Builder;
use kinetic_rs::kinetic::workflow::registry::ToolRegistry;

use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a simple prompt directly
    Run {
        /// The prompt to send
        #[arg(short, long)]
        prompt: String,

        /// The model to use
        #[arg(short, long, default_value = "gemini-1.5-flash")]
        model: String,
    },
    /// Run a workflow from a file
    Workflow {
        /// Path to the workflow file
        #[arg(short, long)]
        file: String,

        /// Input to the workflow
        #[arg(short, long)]
        input: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenv().ok();
    env_logger::init();

    let args = Args::parse();

    match args.command {
        Commands::Run {
            prompt,
            model: model_name,
        } => {
            // Infer provider
            let provider = std::env::var("MODEL_PROVIDER")
                .ok()
                .or_else(|| {
                    if model_name.starts_with("gpt") {
                        Some("OpenAI".to_string())
                    } else if model_name.starts_with("claude") {
                        Some("Anthropic".to_string())
                    } else {
                        Some("Gemini".to_string())
                    }
                })
                .unwrap();

            log::info!("Using provider: {} with model: {}", provider, model_name);

            let model: Arc<dyn kinetic_rs::adk::model::Model> = match provider.as_str() {
                "OpenAI" | "openai" => Arc::new(kinetic_rs::adk::model::openai::OpenAIModel::new(
                    model_name,
                )?),
                "Anthropic" | "anthropic" => Arc::new(
                    kinetic_rs::adk::model::anthropic::AnthropicModel::new(model_name)?,
                ),
                _ => Arc::new(kinetic_rs::adk::model::gemini::GeminiModel::new(
                    model_name,
                )?),
            };

            let agent = LLMAgent::new(
                "simple-agent".to_string(),
                "A simple agent".to_string(),
                "You are a helpful assistant.".to_string(),
                model,
                vec![],
            );

            println!("Sending prompt: {}", prompt);
            let response = agent.run(prompt).await?;
            println!("Response: {}", response);
        }
        Commands::Workflow { file, input } => {
            let registry = ToolRegistry::new();

            // Register native tools
            match search::BraveSearchTool::new() {
                Ok(search_tool) => {
                    log::info!("Registered tool: brave_search");
                    registry.register(Arc::new(search_tool)).await;
                }
                Err(e) => log::warn!("Failed to load search tools: {}", e),
            }

            match github::create_tools() {
                Ok(github_tools) => {
                    for tool in github_tools {
                        log::info!("Registered tool: {}", tool.name());
                        registry.register(tool).await;
                    }
                }
                Err(e) => log::warn!("Failed to load GitHub tools: {}", e),
            }

            match jira::create_tools() {
                Ok(jira_tools) => {
                    for tool in jira_tools {
                        log::info!("Registered tool: {}", tool.name());
                        registry.register(tool).await;
                    }
                }
                Err(e) => log::warn!("Failed to load Jira tools: {}", e),
            }

            // Create MCP service manager
            let mcp_manager = Arc::new(kinetic_rs::kinetic::mcp::manager::McpServiceManager::new());

            // Build workflow
            let builder = Builder::new(registry, mcp_manager);
            let agent = builder.build_agent(&file).await?;

            println!("Running workflow: {}", agent.name());
            let response = agent.run(input).await?;
            println!("Response: {}", response);
        }
    }

    Ok(())
}
