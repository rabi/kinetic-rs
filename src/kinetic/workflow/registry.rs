use crate::adk::tool::Tool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, Arc<dyn Tool>>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, tool: Arc<dyn Tool>) {
        let mut tools = self.tools.write().await;
        tools.insert(tool.name(), tool);
    }

    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let tools = self.tools.read().await;
        tools.get(name).cloned()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use std::error::Error;

    /// A mock tool for testing
    struct MockTool {
        name: String,
    }

    impl MockTool {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait]
    impl Tool for MockTool {
        fn name(&self) -> String {
            self.name.clone()
        }

        fn description(&self) -> String {
            format!("Mock tool: {}", self.name)
        }

        fn schema(&self) -> Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        async fn execute(&self, _input: Value) -> Result<Value, Box<dyn Error + Send + Sync>> {
            Ok(json!({"result": "mock"}))
        }
    }

    #[tokio::test]
    async fn test_register_and_get_tool() {
        let registry = ToolRegistry::new();
        let tool = Arc::new(MockTool::new("test_tool"));

        registry.register(tool).await;

        let retrieved = registry.get("test_tool").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "test_tool");
    }

    #[tokio::test]
    async fn test_get_nonexistent_tool() {
        let registry = ToolRegistry::new();

        let retrieved = registry.get("nonexistent").await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_register_multiple_tools() {
        let registry = ToolRegistry::new();

        registry.register(Arc::new(MockTool::new("tool1"))).await;
        registry.register(Arc::new(MockTool::new("tool2"))).await;
        registry.register(Arc::new(MockTool::new("tool3"))).await;

        assert!(registry.get("tool1").await.is_some());
        assert!(registry.get("tool2").await.is_some());
        assert!(registry.get("tool3").await.is_some());
        assert!(registry.get("tool4").await.is_none());
    }

    #[tokio::test]
    async fn test_register_overwrites_existing() {
        let registry = ToolRegistry::new();

        registry
            .register(Arc::new(MockTool::new("same_name")))
            .await;
        registry
            .register(Arc::new(MockTool::new("same_name")))
            .await;

        // Should still work, just overwrites
        let retrieved = registry.get("same_name").await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_registry_is_clone() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(MockTool::new("tool1"))).await;

        let cloned = registry.clone();

        // Both should see the same tools
        assert!(cloned.get("tool1").await.is_some());

        // Registering on clone should be visible to original
        cloned.register(Arc::new(MockTool::new("tool2"))).await;
        assert!(registry.get("tool2").await.is_some());
    }
}
