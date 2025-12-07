use async_trait::async_trait;
use serde_json::Value;
use std::error::Error;

/// Trait for tools that can be called by agents.
///
/// # Optimization Notes
/// - `name()` and `description()` return `&str` to avoid allocation on every call
/// - `schema()` returns `&Value` to avoid cloning the schema on every access
/// - Implementations should store these values in struct fields
#[async_trait]
pub trait Tool: Send + Sync {
    /// Returns the tool name (must be unique within an agent's tool set)
    fn name(&self) -> &str;

    /// Returns a human-readable description of what the tool does
    fn description(&self) -> &str;

    /// Returns the JSON schema for the tool's input parameters
    fn schema(&self) -> &Value;

    /// Execute the tool with the given input and return the result
    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>>;
}
