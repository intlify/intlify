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

/// Producer-owned expression view retained after the parser arena is released.
///
/// Building this view is parser-to-producer conversion. Interpreting it as a
/// known or dynamic selector remains the recognizer's static-evaluation stage.
pub(crate) enum StaticExpressionView {
    StringLiteral {
        value: Box<str>,
        lone_surrogates: bool,
    },
    TemplateLiteral {
        cooked: Option<Box<str>>,
        lone_surrogates: bool,
    },
    Dynamic,
}

pub(crate) fn capture_static_expression(expression: &Expression<'_>) -> StaticExpressionView {
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
        Expression::StringLiteral(literal) => StaticExpressionView::StringLiteral {
            value: literal.value.as_str().into(),
            lone_surrogates: literal.lone_surrogates,
        },
        Expression::TemplateLiteral(template)
            if template.expressions.is_empty() && template.quasis.len() == 1 =>
        {
            let quasi = &template.quasis[0];
            StaticExpressionView::TemplateLiteral {
                cooked: quasi
                    .value
                    .cooked
                    .as_ref()
                    .map(|value| Box::<str>::from(value.as_str())),
                lone_surrogates: quasi.lone_surrogates,
            }
        }
        _ => StaticExpressionView::Dynamic,
    }
}

pub(crate) fn evaluate_static_expression(view: &StaticExpressionView) -> StaticString<'_> {
    match view {
        StaticExpressionView::StringLiteral {
            value,
            lone_surrogates,
        } => {
            if *lone_surrogates {
                StaticString::KnownInvalid
            } else {
                StaticString::Known(value)
            }
        }
        StaticExpressionView::TemplateLiteral {
            cooked,
            lone_surrogates,
        } => {
            if *lone_surrogates {
                StaticString::KnownInvalid
            } else if let Some(cooked) = cooked {
                StaticString::Known(cooked)
            } else {
                StaticString::KnownInvalid
            }
        }
        StaticExpressionView::Dynamic => StaticString::Dynamic,
    }
}
