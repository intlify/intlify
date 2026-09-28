// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! How the recognizers read an expression.

use oxc_ast::ast::Expression;

/// Return the expression transparent wrappers enclose.
///
/// Parentheses and TypeScript's `as`, `satisfies`, `!` and angle-bracket
/// assertions change nothing at run time, so what an expression is seen
/// through them is what its value is. They preserve what is already known;
/// they never establish anything the enclosed expression does not.
pub(crate) fn transparent<'x, 'a>(expression: &'x Expression<'a>) -> &'x Expression<'a> {
    let mut current = expression;
    loop {
        current = match current {
            Expression::ParenthesizedExpression(inner) => &inner.expression,
            Expression::TSAsExpression(inner) => &inner.expression,
            Expression::TSSatisfiesExpression(inner) => &inner.expression,
            Expression::TSNonNullExpression(inner) => &inner.expression,
            Expression::TSTypeAssertion(inner) => &inner.expression,
            _ => return current,
        };
    }
}

/// Return the string an expression spells statically, when it spells one.
///
/// A string literal and a template without substitutions spell their cooked
/// value. This is for values where only the text matters, such as a module
/// specifier; message text is decoded with its source positions instead.
pub(crate) fn static_string<'x>(expression: &'x Expression<'_>) -> Option<&'x str> {
    match transparent(expression) {
        Expression::StringLiteral(literal) if !literal.lone_surrogates => {
            Some(literal.value.as_str())
        }
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
            let element = template.quasis.first()?;
            if element.lone_surrogates {
                return None;
            }
            element.value.cooked.as_deref()
        }
        _ => None,
    }
}
