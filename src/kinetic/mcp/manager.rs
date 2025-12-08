// SPDX-License-Identifier: MIT

use crate::kinetic::mcp::{create_mcp_service, BasicClientHandler};
use rmcp::service::{RoleClient, RunningService};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration for an MCP server
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

/// Type alias for MCP service map to reduce complexity
type ServiceMap = HashMap<String, Arc<RwLock<RunningService<RoleClient, BasicClientHandler>>>>;

/// Manages the lifecycle of MCP services
pub struct McpServiceManager {
    services: Arc<RwLock<ServiceMap>>,
}

impl McpServiceManager {
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create an MCP service for the given server configuration
    pub async fn get_or_create_service(
        &self,
        config: &McpServerConfig,
    ) -> Result<
        Arc<RwLock<RunningService<RoleClient, BasicClientHandler>>>,
        Box<dyn Error + Send + Sync>,
    > {
        // Check if service already exists
        {
            let services = self.services.read().await;
            if let Some(service) = services.get(&config.name) {
                return Ok(service.clone());
            }
        }

        // Create new service
        log::info!(
            "Creating MCP service '{}' with command: {} {:?}",
            config.name,
            config.command,
            config.args
        );

        let service = create_mcp_service(&config.command, &config.args).await?;
        let service = Arc::new(RwLock::new(service));

        // Store in map
        {
            let mut services = self.services.write().await;
            services.insert(config.name.clone(), service.clone());
        }

        Ok(service)
    }

    /// Get an existing service by name
    pub async fn get_service(
        &self,
        name: &str,
    ) -> Option<Arc<RwLock<RunningService<RoleClient, BasicClientHandler>>>> {
        let services = self.services.read().await;
        services.get(name).cloned()
    }
}

impl Default for McpServiceManager {
    fn default() -> Self {
        Self::new()
    }
}
