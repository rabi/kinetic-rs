pub mod manager;
pub mod tool;

use rmcp::model::{ClientCapabilities, ClientInfo, Implementation};
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::{ClientHandler, ServiceExt};
use std::error::Error;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct BasicClientHandler;

impl ClientHandler for BasicClientHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo {
            protocol_version: Default::default(),
            capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "kinetic-rs".to_string(),
                version: "0.1.0".to_string(),
                ..Default::default()
            },
        }
    }
}

/// Creates an MCP service by connecting to an MCP server via stdio.
///
/// Returns the service which can be used to call `list_all_tools()`, `call_tool()`, etc.
///
/// # Example
/// ```rust,no_run
/// use kinetic_rs::kinetic::mcp::create_mcp_service;
///
/// let service = create_mcp_service("npx", &["-y", "@modelcontextprotocol/server-everything"]).await?;
/// let tools = service.list_all_tools().await?;
/// ```
pub async fn create_mcp_service(
    command: &str,
    args: &[String],
) -> Result<
    rmcp::service::RunningService<rmcp::service::RoleClient, BasicClientHandler>,
    Box<dyn Error + Send + Sync>,
> {
    let mut server_cmd = Command::new(command);
    for arg in args {
        server_cmd.arg(arg);
    }

    let transport = TokioChildProcess::new(server_cmd)?;
    let client_handler = BasicClientHandler;
    let service = client_handler.serve(transport).await?;

    Ok(service)
}
