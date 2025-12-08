// SPDX-License-Identifier: MIT

//! State schema definitions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Schema defining the workflow state structure
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct StateSchema {
    /// Field definitions
    #[serde(flatten)]
    pub fields: HashMap<String, StateFieldDef>,
}

/// Definition of a single state field
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateFieldDef {
    /// Type of the field
    #[serde(rename = "type")]
    pub field_type: FieldType,
    /// Reducer for merging values
    #[serde(default)]
    pub reducer: ReducerType,
    /// Default value
    pub default: Option<serde_json::Value>,
}

/// Supported field types
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Array,
    Object,
}

/// Reducer types for merging values into state
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReducerType {
    /// Replace the value (default)
    #[default]
    Overwrite,
    /// Append to array
    Append,
    /// Keep maximum value
    Max,
    /// Keep minimum value
    Min,
    /// Deep merge objects
    Merge,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_state_schema_deserialize() {
        let yaml = r#"
            intent:
              type: string
            confidence:
              type: number
              default: 0.0
            findings:
              type: array
              reducer: append
        "#;
        let schema: StateSchema = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(schema.fields.len(), 3);
        assert_eq!(schema.fields["intent"].field_type, FieldType::String);
        assert_eq!(schema.fields["confidence"].field_type, FieldType::Number);
        assert_eq!(schema.fields["confidence"].default, Some(json!(0.0)));
        assert_eq!(schema.fields["findings"].reducer, ReducerType::Append);
    }

    #[test]
    fn test_reducer_default() {
        let def = StateFieldDef {
            field_type: FieldType::String,
            reducer: ReducerType::default(),
            default: None,
        };
        assert_eq!(def.reducer, ReducerType::Overwrite);
    }

    #[test]
    fn test_field_types() {
        let yaml = r#"
            str_field: { type: string }
            num_field: { type: number }
            bool_field: { type: boolean }
            arr_field: { type: array }
            obj_field: { type: object }
        "#;
        let schema: StateSchema = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(schema.fields["str_field"].field_type, FieldType::String);
        assert_eq!(schema.fields["num_field"].field_type, FieldType::Number);
        assert_eq!(schema.fields["bool_field"].field_type, FieldType::Boolean);
        assert_eq!(schema.fields["arr_field"].field_type, FieldType::Array);
        assert_eq!(schema.fields["obj_field"].field_type, FieldType::Object);
    }

    #[test]
    fn test_all_reducers() {
        let yaml = r#"
            f1: { type: string, reducer: overwrite }
            f2: { type: array, reducer: append }
            f3: { type: number, reducer: max }
            f4: { type: number, reducer: min }
            f5: { type: object, reducer: merge }
        "#;
        let schema: StateSchema = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(schema.fields["f1"].reducer, ReducerType::Overwrite);
        assert_eq!(schema.fields["f2"].reducer, ReducerType::Append);
        assert_eq!(schema.fields["f3"].reducer, ReducerType::Max);
        assert_eq!(schema.fields["f4"].reducer, ReducerType::Min);
        assert_eq!(schema.fields["f5"].reducer, ReducerType::Merge);
    }
}
