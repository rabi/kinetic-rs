//! Condition expression evaluator

use super::ast::{CompareOp, Expression, Literal};
use crate::kinetic::workflow::state::WorkflowState;
use serde_json::Value;

/// Evaluate a condition expression against workflow state
pub fn evaluate(expr: &Expression, state: &WorkflowState) -> bool {
    match expr {
        Expression::True => true,
        Expression::False => false,
        Expression::Compare { left, op, right } => evaluate_compare(left, op, right, state),
        Expression::And(left, right) => evaluate(left, state) && evaluate(right, state),
        Expression::Or(left, right) => evaluate(left, state) || evaluate(right, state),
        Expression::Not(inner) => !evaluate(inner, state),
    }
}

fn evaluate_compare(left: &str, op: &CompareOp, right: &Literal, state: &WorkflowState) -> bool {
    let left_value = state.get_path(left);

    match op {
        CompareOp::Eq => values_equal(left_value, right),
        CompareOp::NotEq => !values_equal(left_value, right),
        CompareOp::Gt => compare_numbers(left_value, right, |a, b| a > b),
        CompareOp::Gte => compare_numbers(left_value, right, |a, b| a >= b),
        CompareOp::Lt => compare_numbers(left_value, right, |a, b| a < b),
        CompareOp::Lte => compare_numbers(left_value, right, |a, b| a <= b),
        CompareOp::Contains => check_contains(left_value, right),
    }
}

fn values_equal(left: Option<&Value>, right: &Literal) -> bool {
    match (left, right) {
        (None, Literal::Null) => true,
        (None, _) => false,
        (Some(Value::Null), Literal::Null) => true,
        (Some(Value::String(s)), Literal::String(rs)) => s == rs,
        (Some(Value::Number(n)), Literal::Number(rn)) => n
            .as_f64()
            .map(|f| (f - rn).abs() < f64::EPSILON)
            .unwrap_or(false),
        (Some(Value::Bool(b)), Literal::Boolean(rb)) => b == rb,
        _ => false,
    }
}

fn compare_numbers<F>(left: Option<&Value>, right: &Literal, cmp: F) -> bool
where
    F: Fn(f64, f64) -> bool,
{
    match (left, right) {
        (Some(Value::Number(n)), Literal::Number(rn)) => {
            n.as_f64().map(|f| cmp(f, *rn)).unwrap_or(false)
        }
        _ => false,
    }
}

fn check_contains(left: Option<&Value>, right: &Literal) -> bool {
    match (left, right) {
        // String contains substring
        (Some(Value::String(s)), Literal::String(substr)) => s.contains(substr),
        // Array contains value
        (Some(Value::Array(arr)), Literal::String(val)) => {
            arr.iter().any(|v| v.as_str() == Some(val.as_str()))
        }
        (Some(Value::Array(arr)), Literal::Number(val)) => arr.iter().any(|v| {
            v.as_f64()
                .map(|f| (f - val).abs() < f64::EPSILON)
                .unwrap_or(false)
        }),
        (Some(Value::Array(arr)), Literal::Boolean(val)) => {
            arr.iter().any(|v| v.as_bool() == Some(*val))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinetic::workflow::condition::parser::parse;
    use serde_json::json;

    fn state_with(pairs: Vec<(&str, Value)>) -> WorkflowState {
        let mut state = WorkflowState::empty();
        for (k, v) in pairs {
            state.update(k, v);
        }
        state
    }

    #[test]
    fn test_string_equality() {
        let state = state_with(vec![("intent", json!("search"))]);
        let expr = parse("intent == 'search'").unwrap();
        assert!(evaluate(&expr, &state));

        let expr = parse("intent == 'code'").unwrap();
        assert!(!evaluate(&expr, &state));
    }

    #[test]
    fn test_string_not_equal() {
        let state = state_with(vec![("status", json!("pending"))]);
        let expr = parse("status != 'done'").unwrap();
        assert!(evaluate(&expr, &state));

        let expr = parse("status != 'pending'").unwrap();
        assert!(!evaluate(&expr, &state));
    }

    #[test]
    fn test_number_comparison() {
        let state = state_with(vec![("score", json!(7.5))]);

        assert!(evaluate(&parse("score > 5").unwrap(), &state));
        assert!(!evaluate(&parse("score > 10").unwrap(), &state));

        assert!(evaluate(&parse("score >= 7.5").unwrap(), &state));
        assert!(!evaluate(&parse("score >= 8").unwrap(), &state));

        assert!(evaluate(&parse("score < 10").unwrap(), &state));
        assert!(!evaluate(&parse("score < 5").unwrap(), &state));

        assert!(evaluate(&parse("score <= 7.5").unwrap(), &state));
        assert!(!evaluate(&parse("score <= 7").unwrap(), &state));
    }

    #[test]
    fn test_boolean_comparison() {
        let state = state_with(vec![("is_draft", json!(true))]);

        assert!(evaluate(&parse("is_draft == true").unwrap(), &state));
        assert!(!evaluate(&parse("is_draft == false").unwrap(), &state));
    }

    #[test]
    fn test_null_check() {
        let state = state_with(vec![("result", json!(null))]);

        assert!(evaluate(&parse("result == null").unwrap(), &state));
        assert!(!evaluate(&parse("result != null").unwrap(), &state));

        // Non-existent field is also null
        assert!(evaluate(&parse("nonexistent == null").unwrap(), &state));
    }

    #[test]
    fn test_missing_field() {
        let state = WorkflowState::empty();

        assert!(evaluate(&parse("missing == null").unwrap(), &state));
        assert!(!evaluate(&parse("missing == 'value'").unwrap(), &state));
    }

    #[test]
    fn test_contains_string() {
        let state = state_with(vec![("message", json!("hello world"))]);

        assert!(evaluate(
            &parse("message contains 'world'").unwrap(),
            &state
        ));
        assert!(!evaluate(&parse("message contains 'foo'").unwrap(), &state));
    }

    #[test]
    fn test_contains_array() {
        let state = state_with(vec![("tags", json!(["bug", "urgent", "backend"]))]);

        assert!(evaluate(&parse("tags contains 'bug'").unwrap(), &state));
        assert!(evaluate(&parse("tags contains 'urgent'").unwrap(), &state));
        assert!(!evaluate(
            &parse("tags contains 'frontend'").unwrap(),
            &state
        ));
    }

    #[test]
    fn test_and_expression() {
        let state = state_with(vec![("intent", json!("code")), ("confidence", json!(0.9))]);

        assert!(evaluate(
            &parse("intent == 'code' and confidence > 0.8").unwrap(),
            &state
        ));
        assert!(!evaluate(
            &parse("intent == 'code' and confidence > 0.95").unwrap(),
            &state
        ));
        assert!(!evaluate(
            &parse("intent == 'search' and confidence > 0.8").unwrap(),
            &state
        ));
    }

    #[test]
    fn test_or_expression() {
        let state = state_with(vec![("type", json!("feature")), ("priority", json!(5))]);

        assert!(evaluate(
            &parse("type == 'bug' or priority > 3").unwrap(),
            &state
        ));
        assert!(evaluate(
            &parse("type == 'feature' or priority > 10").unwrap(),
            &state
        ));
        assert!(!evaluate(
            &parse("type == 'bug' or priority > 10").unwrap(),
            &state
        ));
    }

    #[test]
    fn test_literal_true_false() {
        let state = WorkflowState::empty();

        assert!(evaluate(&parse("true").unwrap(), &state));
        assert!(!evaluate(&parse("false").unwrap(), &state));
    }

    #[test]
    fn test_nested_path() {
        let state = state_with(vec![("result", json!({"data": {"intent": "search"}}))]);

        assert!(evaluate(
            &parse("result.data.intent == 'search'").unwrap(),
            &state
        ));
        assert!(!evaluate(
            &parse("result.data.intent == 'code'").unwrap(),
            &state
        ));
    }

    #[test]
    fn test_complex_expression() {
        let state = state_with(vec![
            ("intent", json!("search")),
            ("confidence", json!(0.85)),
            ("tags", json!(["important"])),
        ]);

        // This tests: (intent == 'search' and confidence > 0.8)
        // Note: Our parser handles left-to-right, so complex nesting would need parentheses
        assert!(evaluate(
            &parse("intent == 'search' and confidence > 0.8").unwrap(),
            &state
        ));
    }
}
