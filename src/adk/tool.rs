use async_trait::async_trait;
use serde_json::Value;
use std::error::Error;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> String;
    fn description(&self) -> String;
    fn schema(&self) -> Value;
    async fn execute(&self, input: Value) -> Result<Value, Box<dyn Error + Send + Sync>>;
}
