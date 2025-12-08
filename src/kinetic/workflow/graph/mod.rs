// SPDX-License-Identifier: MIT

//! Graph-based workflow execution
//!
//! This module provides the graph workflow executor that runs
//! nodes based on their dependencies and conditions.

pub mod executor;
mod normalizer;
pub mod types;

pub use executor::{CompiledNode, GraphAgent};
pub use normalizer::normalize_to_graph;
pub use types::{DependsOn, GraphWorkflowDef, NodeDefinition, WaitMode};
