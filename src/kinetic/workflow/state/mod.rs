// SPDX-License-Identifier: MIT

//! State management for graph workflows
//!
//! This module provides:
//! - `StateSchema` - defines the shape and types of workflow state
//! - `WorkflowState` - runtime state storage with reducer support
//! - `Reducer` - strategies for merging values into state

mod schema;
mod store;

pub use schema::{FieldType, ReducerType, StateFieldDef, StateSchema};
pub use store::WorkflowState;
