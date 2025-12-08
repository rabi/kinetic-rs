// SPDX-License-Identifier: MIT

//! Abstract Syntax Tree for condition expressions

/// A condition expression
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    /// Comparison expression: left op right
    Compare {
        left: String,
        op: CompareOp,
        right: Literal,
    },
    /// Logical AND
    And(Box<Expression>, Box<Expression>),
    /// Logical OR
    Or(Box<Expression>, Box<Expression>),
    /// Logical NOT
    Not(Box<Expression>),
    /// Literal true
    True,
    /// Literal false
    False,
}

/// Comparison operators
#[derive(Debug, Clone, PartialEq)]
pub enum CompareOp {
    /// ==
    Eq,
    /// !=
    NotEq,
    /// >
    Gt,
    /// >=
    Gte,
    /// <
    Lt,
    /// <=
    Lte,
    /// contains (for strings and arrays)
    Contains,
}

/// Literal values in expressions
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
}

impl std::fmt::Display for CompareOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompareOp::Eq => write!(f, "=="),
            CompareOp::NotEq => write!(f, "!="),
            CompareOp::Gt => write!(f, ">"),
            CompareOp::Gte => write!(f, ">="),
            CompareOp::Lt => write!(f, "<"),
            CompareOp::Lte => write!(f, "<="),
            CompareOp::Contains => write!(f, "contains"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_op_display() {
        assert_eq!(format!("{}", CompareOp::Eq), "==");
        assert_eq!(format!("{}", CompareOp::NotEq), "!=");
        assert_eq!(format!("{}", CompareOp::Gt), ">");
        assert_eq!(format!("{}", CompareOp::Gte), ">=");
        assert_eq!(format!("{}", CompareOp::Lt), "<");
        assert_eq!(format!("{}", CompareOp::Lte), "<=");
        assert_eq!(format!("{}", CompareOp::Contains), "contains");
    }

    #[test]
    fn test_expression_equality() {
        let expr1 = Expression::Compare {
            left: "a".to_string(),
            op: CompareOp::Eq,
            right: Literal::String("b".to_string()),
        };
        let expr2 = Expression::Compare {
            left: "a".to_string(),
            op: CompareOp::Eq,
            right: Literal::String("b".to_string()),
        };
        assert_eq!(expr1, expr2);
    }
}
