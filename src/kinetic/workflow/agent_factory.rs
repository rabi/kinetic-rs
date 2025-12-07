//! Agent factory - constructs agents from definitions
//!
//! This module handles the creation of Agent instances from AgentDefinition
//! configurations, including model instantiation and tool binding.

use crate::adk::agent::{Agent, LLMAgent, ReActAgent};
use crate::adk::gemini::GeminiModel;
use crate::adk::model::Model;
use crate::adk::tool::Tool;
use crate::kinetic::workflow::registry::ToolRegistry;
use crate::kinetic::workflow::types::AgentDefinition;

use std::env;
use std::error::Error;
use std::sync::Arc;

/// Factory for creating Agent instances from definitions
pub struct AgentFactory<'a> {
    registry: &'a ToolRegistry,
}

impl<'a> AgentFactory<'a> {
    pub fn new(registry: &'a ToolRegistry) -> Self {
        Self { registry }
    }

    /// Build an agent from an AgentDefinition
    pub async fn build(
        &self,
        def: &AgentDefinition,
    ) -> Result<Arc<dyn Agent>, Box<dyn Error + Send + Sync>> {
        let model = self.create_model(def)?;
        let tools = self.collect_tools(def).await;

        let executor = def.executor.as_deref().unwrap_or("default");
        log::info!("Building agent '{}' with executor '{}'", def.name, executor);

        match executor {
            "react" => self.build_react_agent(def, model, tools),
            "cot" => self.build_cot_agent(def, model, tools),
            _ => self.build_default_agent(def, model, tools),
        }
    }

    /// Create the model instance for an agent
    fn create_model(
        &self,
        def: &AgentDefinition,
    ) -> Result<Arc<dyn Model>, Box<dyn Error + Send + Sync>> {
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
            .unwrap_or_else(|| infer_provider_from_model(&model_name));

        log::debug!("Using provider '{}' with model '{}'", provider, model_name);

        match provider.as_str() {
            "Gemini" | "Google" | "gemini" | "" => Ok(Arc::new(GeminiModel::new(model_name)?)),
            // "OpenAI" | "openai" => Ok(Arc::new(OpenAIModel::new(model_name)?)), // TODO
            // "Anthropic" | "anthropic" => Ok(Arc::new(AnthropicModel::new(model_name)?)), // TODO
            _ => Err(format!("Unknown model provider: {}", provider).into()),
        }
    }

    /// Collect tools for an agent from the registry
    async fn collect_tools(&self, def: &AgentDefinition) -> Vec<Arc<dyn Tool>> {
        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();
        for tool_name in &def.tools {
            if let Some(tool) = self.registry.get(tool_name).await {
                tools.push(tool.clone());
            } else {
                log::warn!("Tool not found: {}", tool_name);
            }
        }
        tools
    }

    fn build_default_agent(
        &self,
        def: &AgentDefinition,
        model: Arc<dyn Model>,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Result<Arc<dyn Agent>, Box<dyn Error + Send + Sync>> {
        Ok(Arc::new(LLMAgent::new(
            def.name.clone(),
            def.description.clone(),
            def.instructions.clone(),
            model,
            tools,
        )))
    }

    fn build_react_agent(
        &self,
        def: &AgentDefinition,
        model: Arc<dyn Model>,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Result<Arc<dyn Agent>, Box<dyn Error + Send + Sync>> {
        let max_iterations = def.max_iterations.unwrap_or(10);
        Ok(Arc::new(ReActAgent::new(
            def.name.clone(),
            def.description.clone(),
            def.instructions.clone(),
            model,
            tools,
            max_iterations,
        )))
    }

    fn build_cot_agent(
        &self,
        def: &AgentDefinition,
        model: Arc<dyn Model>,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Result<Arc<dyn Agent>, Box<dyn Error + Send + Sync>> {
        // Chain-of-Thought: Use standard LLMAgent with CoT-specific instructions
        // The user should include CoT prompting in their instructions
        log::info!("Using Chain-of-Thought executor (standard agent with CoT prompting)");
        Ok(Arc::new(LLMAgent::new(
            def.name.clone(),
            def.description.clone(),
            def.instructions.clone(),
            model,
            tools,
        )))
    }
}

/// Infer the provider from the model name prefix
pub fn infer_provider_from_model(model_name: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_provider_gemini() {
        assert_eq!(infer_provider_from_model("gemini-2.0-flash"), "Gemini");
        assert_eq!(infer_provider_from_model("gemini-1.5-pro"), "Gemini");
        assert_eq!(infer_provider_from_model("Gemini-2.0-Flash"), "Gemini");
        assert_eq!(
            infer_provider_from_model("models/gemini-2.0-flash"),
            "Gemini"
        );
    }

    #[test]
    fn test_infer_provider_openai() {
        assert_eq!(infer_provider_from_model("gpt-4"), "OpenAI");
        assert_eq!(infer_provider_from_model("gpt-4o"), "OpenAI");
        assert_eq!(infer_provider_from_model("gpt-3.5-turbo"), "OpenAI");
        assert_eq!(infer_provider_from_model("GPT-4"), "OpenAI");
        assert_eq!(infer_provider_from_model("o1-preview"), "OpenAI");
        assert_eq!(infer_provider_from_model("o1-mini"), "OpenAI");
    }

    #[test]
    fn test_infer_provider_anthropic() {
        assert_eq!(infer_provider_from_model("claude-3-opus"), "Anthropic");
        assert_eq!(infer_provider_from_model("claude-3-sonnet"), "Anthropic");
        assert_eq!(infer_provider_from_model("Claude-3.5-Sonnet"), "Anthropic");
    }

    #[test]
    fn test_infer_provider_deepseek() {
        assert_eq!(infer_provider_from_model("deepseek-chat"), "DeepSeek");
        assert_eq!(infer_provider_from_model("deepseek-coder"), "DeepSeek");
        assert_eq!(infer_provider_from_model("DeepSeek-V2"), "DeepSeek");
    }

    #[test]
    fn test_infer_provider_unknown_defaults_to_gemini() {
        assert_eq!(infer_provider_from_model("unknown-model"), "Gemini");
        assert_eq!(infer_provider_from_model("my-custom-model"), "Gemini");
        assert_eq!(infer_provider_from_model(""), "Gemini");
    }
}
