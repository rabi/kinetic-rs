// SPDX-License-Identifier: MIT

//! Typed error handling for kinetic-rs
//!
//! This module provides a proper error type hierarchy using thiserror,
//! replacing the previous Box<dyn Error + Send + Sync> pattern.

use thiserror::Error;

/// Top-level error type for kinetic-rs
#[derive(Debug, Error)]
pub enum KineticError {
    /// API errors from external services (Gemini, GitHub, Jira, etc.)
    #[error("API error from {provider}: {message}")]
    Api { provider: String, message: String },

    /// Tool not found during execution
    #[error("Tool '{name}' not found")]
    ToolNotFound { name: String },

    /// Configuration errors (missing env vars, invalid config)
    #[error("Configuration error: {0}")]
    Config(String),

    /// Workflow-specific errors
    #[error("Workflow error: {0}")]
    Workflow(#[from] WorkflowError),

    /// I/O errors
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization errors
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// YAML parsing errors
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),

    /// HTTP request errors
    #[error(transparent)]
    Http(#[from] reqwest::Error),

    /// Max iterations/turns reached
    #[error("Max {kind} reached: {limit}")]
    MaxIterations { kind: String, limit: u32 },

    /// Generic error wrapper for compatibility
    #[error("{0}")]
    Other(String),
}

/// Workflow-specific errors
#[derive(Debug, Error)]
pub enum WorkflowError {
    /// Missing agent in Direct workflow
    #[error("Missing agent definition in Direct workflow")]
    MissingAgent,

    /// Missing workflow definition in Composite workflow
    #[error("Missing workflow definition in Composite workflow")]
    MissingWorkflow,

    /// Unknown workflow kind
    #[error("Unknown workflow kind: {0}")]
    UnknownKind(String),

    /// Circular dependency detected in graph workflow
    #[error("Circular dependency detected: {0:?}")]
    CircularDependency(Vec<String>),

    /// File not found when loading workflow
    #[error("Workflow file not found: {0}")]
    FileNotFound(String),

    /// Invalid execution mode
    #[error("Invalid execution mode: {0}")]
    InvalidExecutionMode(String),
}

/// Model/LLM-specific errors
#[derive(Debug, Error)]
pub enum ModelError {
    /// API key not configured
    #[error("API key not configured for provider: {0}")]
    ApiKeyMissing(String),

    /// Model not supported
    #[error("Model not supported: {0}")]
    UnsupportedModel(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded, retry after {retry_after_secs:?} seconds")]
    RateLimited { retry_after_secs: Option<u64> },

    /// Invalid response from model
    #[error("Invalid response from model: {0}")]
    InvalidResponse(String),
}

impl KineticError {
    /// Create an API error
    pub fn api(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Api {
            provider: provider.into(),
            message: message.into(),
        }
    }

    /// Create a tool not found error
    pub fn tool_not_found(name: impl Into<String>) -> Self {
        Self::ToolNotFound { name: name.into() }
    }

    /// Create a config error
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }

    /// Create from a generic error
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}

// Allow conversion from &str for backward compatibility
impl From<&str> for KineticError {
    fn from(s: &str) -> Self {
        Self::Other(s.to_string())
    }
}

impl From<String> for KineticError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}

// Convert from Box<dyn Error> for compatibility
impl From<Box<dyn std::error::Error + Send + Sync>> for KineticError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self::Other(err.to_string())
    }
}
