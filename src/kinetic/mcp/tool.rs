// SPDX-License-Identifier: MIT

use crate::adk::tool::Tool;
use async_trait::async_trait;
use rmcp::model::CallToolRequestParam;
use rmcp::service::RoleClient;
use serde_json::Value;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Wrapper around an MCP tool that implements the kinetic-rs Tool trait
pub struct McpTool {
    service: Arc<
        RwLock<rmcp::service::RunningService<RoleClient, crate::kinetic::mcp::BasicClientHandler>>,
    >,
    name: String,
    description: String,
    schema: Value,
}

impl McpTool {
    pub fn new(
        service: Arc<
            RwLock<
                rmcp::service::RunningService<RoleClient, crate::kinetic::mcp::BasicClientHandler>,
            >,
        >,
        name: String,
        description: String,
        schema: Value,
    ) -> Self {
        Self {
            service,
            name,
            description,
            schema,
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn schema(&self) -> &Value {
        &self.schema
    }

    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let service = self.service.read().await;

        // Convert Value to Map if it's an object
        let arguments = match input {
            Value::Object(map) => Some(map),
            _ => None,
        };

        let result = service
            .call_tool(CallToolRequestParam {
                name: self.name.clone().into(),
                arguments,
            })
            .await?;

        Ok(serde_json::to_value(result)?)
    }
}
