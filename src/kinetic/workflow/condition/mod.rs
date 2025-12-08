// SPDX-License-Identifier: MIT

//! Condition evaluation for graph workflows
//!
//! This module provides parsing and evaluation of `when` conditions.
//! Conditions are simple expressions like:
//! - `intent == 'search'`
//! - `confidence > 0.8`
//! - `intent == 'bug' and priority > 3`

mod ast;
mod evaluator;
mod parser;

pub use ast::{CompareOp, Expression, Literal};
pub use evaluator::evaluate;
pub use parser::parse;
