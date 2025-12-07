//! Simple condition expression parser
//!
//! Parses expressions like:
//! - `field == 'value'`
//! - `score > 0.8`
//! - `a == 'x' and b > 5`

use super::ast::{CompareOp, Expression, Literal};
use std::error::Error;

/// Parse a condition expression string into an AST
pub fn parse(input: &str) -> Result<Expression, Box<dyn Error + Send + Sync>> {
    let input = input.trim();

    // Handle special cases
    if input == "true" {
        return Ok(Expression::True);
    }
    if input == "false" {
        return Ok(Expression::False);
    }

    // Try to parse as compound expression (and/or)
    if let Some(expr) = try_parse_compound(input)? {
        return Ok(expr);
    }

    // Parse as simple comparison
    parse_comparison(input)
}

fn try_parse_compound(input: &str) -> Result<Option<Expression>, Box<dyn Error + Send + Sync>> {
    // Look for " and " or " or " at top level (not inside quotes)
    let mut depth = 0;
    let mut in_string = false;
    let chars: Vec<char> = input.chars().collect();

    for i in 0..chars.len() {
        let c = chars[i];
        if c == '\'' || c == '"' {
            in_string = !in_string;
        } else if !in_string {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
            } else if depth == 0 {
                // Check for " and "
                if i + 5 <= chars.len() {
                    let word: String = chars[i..i + 5].iter().collect();
                    if word == " and " {
                        let left = parse(&input[..i])?;
                        let right = parse(&input[i + 5..])?;
                        return Ok(Some(Expression::And(Box::new(left), Box::new(right))));
                    }
                }
                // Check for " or "
                if i + 4 <= chars.len() {
                    let word: String = chars[i..i + 4].iter().collect();
                    if word == " or " {
                        let left = parse(&input[..i])?;
                        let right = parse(&input[i + 4..])?;
                        return Ok(Some(Expression::Or(Box::new(left), Box::new(right))));
                    }
                }
            }
        }
    }

    Ok(None)
}

fn parse_comparison(input: &str) -> Result<Expression, Box<dyn Error + Send + Sync>> {
    // Try operators in order of length (longest first)
    let operators = [
        ("!=", CompareOp::NotEq),
        (">=", CompareOp::Gte),
        ("<=", CompareOp::Lte),
        ("==", CompareOp::Eq),
        (">", CompareOp::Gt),
        ("<", CompareOp::Lt),
        (" contains ", CompareOp::Contains),
    ];

    for (op_str, op) in operators {
        if let Some(pos) = find_operator(input, op_str) {
            let left = input[..pos].trim().to_string();
            let right_str = input[pos + op_str.len()..].trim();
            let right = parse_literal(right_str)?;
            return Ok(Expression::Compare { left, op, right });
        }
    }

    Err(format!("Could not parse condition: {}", input).into())
}

fn find_operator(input: &str, op: &str) -> Option<usize> {
    let mut in_string = false;
    let chars: Vec<char> = input.chars().collect();

    for i in 0..chars.len() {
        let c = chars[i];
        if c == '\'' || c == '"' {
            in_string = !in_string;
        } else if !in_string && input[i..].starts_with(op) {
            return Some(i);
        }
    }
    None
}

fn parse_literal(input: &str) -> Result<Literal, Box<dyn Error + Send + Sync>> {
    let input = input.trim();

    // Null
    if input == "null" {
        return Ok(Literal::Null);
    }

    // Boolean
    if input == "true" {
        return Ok(Literal::Boolean(true));
    }
    if input == "false" {
        return Ok(Literal::Boolean(false));
    }

    // String (single or double quotes)
    if (input.starts_with('\'') && input.ends_with('\''))
        || (input.starts_with('"') && input.ends_with('"'))
    {
        let s = &input[1..input.len() - 1];
        return Ok(Literal::String(s.to_string()));
    }

    // Number
    if let Ok(n) = input.parse::<f64>() {
        return Ok(Literal::Number(n));
    }

    Err(format!("Could not parse literal: {}", input).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_equality() {
        let expr = parse("intent == 'search'").unwrap();
        assert_eq!(
            expr,
            Expression::Compare {
                left: "intent".to_string(),
                op: CompareOp::Eq,
                right: Literal::String("search".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_not_equal() {
        let expr = parse("status != 'done'").unwrap();
        assert_eq!(
            expr,
            Expression::Compare {
                left: "status".to_string(),
                op: CompareOp::NotEq,
                right: Literal::String("done".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_numeric_comparison() {
        let expr = parse("confidence > 0.8").unwrap();
        assert_eq!(
            expr,
            Expression::Compare {
                left: "confidence".to_string(),
                op: CompareOp::Gt,
                right: Literal::Number(0.8),
            }
        );
    }

    #[test]
    fn test_parse_gte() {
        let expr = parse("score >= 5").unwrap();
        assert_eq!(
            expr,
            Expression::Compare {
                left: "score".to_string(),
                op: CompareOp::Gte,
                right: Literal::Number(5.0),
            }
        );
    }

    #[test]
    fn test_parse_lte() {
        let expr = parse("count <= 10").unwrap();
        assert_eq!(
            expr,
            Expression::Compare {
                left: "count".to_string(),
                op: CompareOp::Lte,
                right: Literal::Number(10.0),
            }
        );
    }

    #[test]
    fn test_parse_lt() {
        let expr = parse("priority < 3").unwrap();
        assert_eq!(
            expr,
            Expression::Compare {
                left: "priority".to_string(),
                op: CompareOp::Lt,
                right: Literal::Number(3.0),
            }
        );
    }

    #[test]
    fn test_parse_boolean_literal() {
        let expr = parse("is_draft == false").unwrap();
        assert_eq!(
            expr,
            Expression::Compare {
                left: "is_draft".to_string(),
                op: CompareOp::Eq,
                right: Literal::Boolean(false),
            }
        );
    }

    #[test]
    fn test_parse_null_check() {
        let expr = parse("error == null").unwrap();
        assert_eq!(
            expr,
            Expression::Compare {
                left: "error".to_string(),
                op: CompareOp::Eq,
                right: Literal::Null,
            }
        );
    }

    #[test]
    fn test_parse_contains() {
        let expr = parse("tags contains 'bug'").unwrap();
        assert_eq!(
            expr,
            Expression::Compare {
                left: "tags".to_string(),
                op: CompareOp::Contains,
                right: Literal::String("bug".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_and() {
        let expr = parse("a == 'x' and b > 5").unwrap();
        match expr {
            Expression::And(left, right) => {
                assert_eq!(
                    *left,
                    Expression::Compare {
                        left: "a".to_string(),
                        op: CompareOp::Eq,
                        right: Literal::String("x".to_string()),
                    }
                );
                assert_eq!(
                    *right,
                    Expression::Compare {
                        left: "b".to_string(),
                        op: CompareOp::Gt,
                        right: Literal::Number(5.0),
                    }
                );
            }
            _ => panic!("Expected And expression"),
        }
    }

    #[test]
    fn test_parse_or() {
        let expr = parse("type == 'bug' or priority > 3").unwrap();
        match expr {
            Expression::Or(left, right) => {
                assert_eq!(
                    *left,
                    Expression::Compare {
                        left: "type".to_string(),
                        op: CompareOp::Eq,
                        right: Literal::String("bug".to_string()),
                    }
                );
                assert_eq!(
                    *right,
                    Expression::Compare {
                        left: "priority".to_string(),
                        op: CompareOp::Gt,
                        right: Literal::Number(3.0),
                    }
                );
            }
            _ => panic!("Expected Or expression"),
        }
    }

    #[test]
    fn test_parse_true() {
        let expr = parse("true").unwrap();
        assert_eq!(expr, Expression::True);
    }

    #[test]
    fn test_parse_false() {
        let expr = parse("false").unwrap();
        assert_eq!(expr, Expression::False);
    }

    #[test]
    fn test_parse_double_quotes() {
        let expr = parse(r#"name == "hello""#).unwrap();
        assert_eq!(
            expr,
            Expression::Compare {
                left: "name".to_string(),
                op: CompareOp::Eq,
                right: Literal::String("hello".to_string()),
            }
        );
    }

    #[test]
    fn test_parse_invalid() {
        let result = parse("this is not valid");
        assert!(result.is_err());
    }
}
