// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Bounded expression-local static string evaluation.
//!
//! Evaluation removes only the value-preserving wrappers admitted by the
//! producer contract. It performs no binding lookup, constant propagation,
//! property access, call execution, or expression folding.

use oxc_ast::ast::Expression;

pub(crate) enum StaticString<'a> {
    Known(&'a str),
    KnownInvalid,
    Dynamic,
}

pub(crate) fn evaluate_static_string<'expression>(
    expression: &'expression Expression<'_>,
) -> StaticString<'expression> {
    let mut current = expression;
    loop {
        current = match current {
            Expression::ParenthesizedExpression(wrapper) => &wrapper.expression,
            Expression::TSAsExpression(wrapper) => &wrapper.expression,
            Expression::TSSatisfiesExpression(wrapper) => &wrapper.expression,
            Expression::TSNonNullExpression(wrapper) => &wrapper.expression,
            Expression::TSTypeAssertion(wrapper) => &wrapper.expression,
            _ => break,
        };
    }

    match current {
        Expression::StringLiteral(literal) => {
            if literal.lone_surrogates {
                StaticString::KnownInvalid
            } else {
                StaticString::Known(literal.value.as_str())
            }
        }
        Expression::TemplateLiteral(template)
            if template.expressions.is_empty() && template.quasis.len() == 1 =>
        {
            let quasi = &template.quasis[0];
            if quasi.lone_surrogates {
                StaticString::KnownInvalid
            } else if let Some(cooked) = &quasi.value.cooked {
                StaticString::Known(cooked.as_str())
            } else {
                StaticString::KnownInvalid
            }
        }
        _ => StaticString::Dynamic,
    }
}
