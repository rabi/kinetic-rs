// SPDX-License-Identifier: MIT

//! Runtime state storage for workflow execution

use serde_json::{Map, Value};
use std::collections::HashMap;

use super::schema::{ReducerType, StateSchema};

/// Runtime workflow state with reducer support
#[derive(Debug, Clone)]
pub struct WorkflowState {
    /// Current state values
    fields: HashMap<String, Value>,
    /// Reducers for each field
    reducers: HashMap<String, ReducerType>,
}

impl WorkflowState {
    /// Create a new WorkflowState from a schema
    pub fn new(schema: &StateSchema) -> Self {
        let mut fields = HashMap::new();
        let mut reducers = HashMap::new();

        for (name, def) in &schema.fields {
            if let Some(default) = &def.default {
                fields.insert(name.clone(), default.clone());
            }
            reducers.insert(name.clone(), def.reducer.clone());
        }

        Self { fields, reducers }
    }

    /// Create an empty WorkflowState
    pub fn empty() -> Self {
        Self {
            fields: HashMap::new(),
            reducers: HashMap::new(),
        }
    }

    /// Update a field using the appropriate reducer
    pub fn update(&mut self, key: &str, value: Value) {
        let reducer = self
            .reducers
            .get(key)
            .cloned()
            .unwrap_or(ReducerType::Overwrite);

        match reducer {
            ReducerType::Overwrite => {
                self.fields.insert(key.to_string(), value);
            }
            ReducerType::Append => {
                let arr = self
                    .fields
                    .entry(key.to_string())
                    .or_insert(Value::Array(vec![]));
                if let Value::Array(a) = arr {
                    match value {
                        Value::Array(new_items) => a.extend(new_items),
                        other => a.push(other),
                    }
                }
            }
            ReducerType::Max => {
                let current = self.fields.get(key).and_then(|v| v.as_f64());
                if let Some(new) = value.as_f64() {
                    if current.is_none() || new > current.unwrap() {
                        self.fields.insert(key.to_string(), value);
                    }
                }
            }
            ReducerType::Min => {
                let current = self.fields.get(key).and_then(|v| v.as_f64());
                if let Some(new) = value.as_f64() {
                    if current.is_none() || new < current.unwrap() {
                        self.fields.insert(key.to_string(), value);
                    }
                }
            }
            ReducerType::Merge => {
                let current = self
                    .fields
                    .entry(key.to_string())
                    .or_insert(Value::Object(Map::new()));
                if let (Value::Object(current_obj), Value::Object(new_obj)) = (current, value) {
                    for (k, v) in new_obj {
                        current_obj.insert(k, v);
                    }
                }
            }
        }
    }

    /// Get a field value
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
    }

    /// Get a nested field value using dot notation (e.g., "result.intent")
    pub fn get_path(&self, path: &str) -> Option<&Value> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return None;
        }

        let mut current = self.fields.get(parts[0])?;
        for part in &parts[1..] {
            current = current.get(part)?;
        }
        Some(current)
    }

    /// Convert state to JSON object
    pub fn to_json(&self) -> Value {
        Value::Object(
            self.fields
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
    }

    /// Get all field names
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.fields.keys()
    }
}

impl Default for WorkflowState {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinetic::workflow::state::schema::{FieldType, StateFieldDef};
    use serde_json::json;

    fn make_schema(fields: Vec<(&str, FieldType, ReducerType, Option<Value>)>) -> StateSchema {
        let mut schema = StateSchema::default();
        for (name, field_type, reducer, default) in fields {
            schema.fields.insert(
                name.to_string(),
                StateFieldDef {
                    field_type,
                    reducer,
                    default,
                },
            );
        }
        schema
    }

    #[test]
    fn test_empty_state() {
        let state = WorkflowState::empty();
        assert!(state.get("anything").is_none());
    }

    #[test]
    fn test_state_with_defaults() {
        let schema = make_schema(vec![
            (
                "count",
                FieldType::Number,
                ReducerType::Overwrite,
                Some(json!(0)),
            ),
            (
                "name",
                FieldType::String,
                ReducerType::Overwrite,
                Some(json!("default")),
            ),
        ]);
        let state = WorkflowState::new(&schema);

        assert_eq!(state.get("count"), Some(&json!(0)));
        assert_eq!(state.get("name"), Some(&json!("default")));
    }

    #[test]
    fn test_overwrite_reducer() {
        let schema = make_schema(vec![(
            "value",
            FieldType::String,
            ReducerType::Overwrite,
            None,
        )]);
        let mut state = WorkflowState::new(&schema);

        state.update("value", json!("first"));
        assert_eq!(state.get("value"), Some(&json!("first")));

        state.update("value", json!("second"));
        assert_eq!(state.get("value"), Some(&json!("second")));
    }

    #[test]
    fn test_append_reducer() {
        let schema = make_schema(vec![("items", FieldType::Array, ReducerType::Append, None)]);
        let mut state = WorkflowState::new(&schema);

        state.update("items", json!("item1"));
        assert_eq!(state.get("items"), Some(&json!(["item1"])));

        state.update("items", json!("item2"));
        assert_eq!(state.get("items"), Some(&json!(["item1", "item2"])));

        // Append array
        state.update("items", json!(["item3", "item4"]));
        assert_eq!(
            state.get("items"),
            Some(&json!(["item1", "item2", "item3", "item4"]))
        );
    }

    #[test]
    fn test_max_reducer() {
        let schema = make_schema(vec![("score", FieldType::Number, ReducerType::Max, None)]);
        let mut state = WorkflowState::new(&schema);

        state.update("score", json!(5.0));
        assert_eq!(state.get("score"), Some(&json!(5.0)));

        state.update("score", json!(3.0)); // Lower, should not update
        assert_eq!(state.get("score"), Some(&json!(5.0)));

        state.update("score", json!(8.0)); // Higher, should update
        assert_eq!(state.get("score"), Some(&json!(8.0)));
    }

    #[test]
    fn test_min_reducer() {
        let schema = make_schema(vec![("cost", FieldType::Number, ReducerType::Min, None)]);
        let mut state = WorkflowState::new(&schema);

        state.update("cost", json!(10.0));
        assert_eq!(state.get("cost"), Some(&json!(10.0)));

        state.update("cost", json!(15.0)); // Higher, should not update
        assert_eq!(state.get("cost"), Some(&json!(10.0)));

        state.update("cost", json!(5.0)); // Lower, should update
        assert_eq!(state.get("cost"), Some(&json!(5.0)));
    }

    #[test]
    fn test_merge_reducer() {
        let schema = make_schema(vec![("meta", FieldType::Object, ReducerType::Merge, None)]);
        let mut state = WorkflowState::new(&schema);

        state.update("meta", json!({"a": 1}));
        assert_eq!(state.get("meta"), Some(&json!({"a": 1})));

        state.update("meta", json!({"b": 2}));
        assert_eq!(state.get("meta"), Some(&json!({"a": 1, "b": 2})));

        state.update("meta", json!({"a": 10})); // Overwrite existing key
        assert_eq!(state.get("meta"), Some(&json!({"a": 10, "b": 2})));
    }

    #[test]
    fn test_get_path() {
        let mut state = WorkflowState::empty();
        state.update("result", json!({"data": {"value": 42}}));

        assert_eq!(
            state.get_path("result"),
            Some(&json!({"data": {"value": 42}}))
        );
        assert_eq!(state.get_path("result.data"), Some(&json!({"value": 42})));
        assert_eq!(state.get_path("result.data.value"), Some(&json!(42)));
        assert_eq!(state.get_path("result.nonexistent"), None);
    }

    #[test]
    fn test_to_json() {
        let mut state = WorkflowState::empty();
        state.update("a", json!(1));
        state.update("b", json!("hello"));

        let json = state.to_json();
        assert_eq!(json["a"], 1);
        assert_eq!(json["b"], "hello");
    }

    #[test]
    fn test_undefined_field_uses_overwrite() {
        let state_schema = StateSchema::default();
        let mut state = WorkflowState::new(&state_schema);

        // Field not defined in schema should use overwrite
        state.update("unknown", json!("first"));
        assert_eq!(state.get("unknown"), Some(&json!("first")));

        state.update("unknown", json!("second"));
        assert_eq!(state.get("unknown"), Some(&json!("second")));
    }
}
