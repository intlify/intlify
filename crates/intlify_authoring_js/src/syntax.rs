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

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::GetSpan;

    use super::*;
    use crate::grammar::Grammar;

    /// Parse `text` as one TypeScript expression and hand it to `check`.
    fn with_expression(text: &str, check: impl FnOnce(&Expression<'_>)) {
        let allocator = Allocator::default();
        let expression = Parser::new(&allocator, text, Grammar::TsModule.source_type())
            .parse_expression()
            .unwrap_or_else(|errors| panic!("{text:?} is an expression: {errors:?}"));
        check(&expression);
    }

    /// The source an expression covers once its wrappers are seen through.
    fn seen_through(text: &str) -> String {
        let mut seen = String::new();
        with_expression(text, |expression| {
            let span = transparent(expression).span();
            seen = text[span.start as usize..span.end as usize].to_owned();
        });
        seen
    }

    fn spelled(text: &str) -> Option<String> {
        let mut spelled = None;
        with_expression(text, |expression| {
            spelled = static_string(expression).map(str::to_owned);
        });
        spelled
    }

    #[test]
    fn every_transparent_wrapper_is_seen_through_however_deeply_nested() {
        assert_eq!(seen_through("('Pay')"), "'Pay'");
        assert_eq!(seen_through("'Pay' as string"), "'Pay'");
        assert_eq!(seen_through("'Pay' satisfies string"), "'Pay'");
        assert_eq!(seen_through("value!"), "value");
        assert_eq!(seen_through("<string>'Pay'"), "'Pay'");
        assert_eq!(
            seen_through("((<string>('Pay' as string))!) satisfies string"),
            "'Pay'"
        );
    }

    #[test]
    fn an_expression_that_changes_its_value_is_not_a_wrapper() {
        // Each of these computes something, so what it holds is not what the
        // expression inside it holds.
        for text in [
            "-('Pay')",
            "`${'Pay'}`",
            "void 'Pay'",
            "f('Pay')",
            "flag ? 'Pay' : 'Save'",
        ] {
            assert_eq!(seen_through(text), text);
        }
    }

    #[test]
    fn only_a_string_or_a_template_without_substitutions_spells_a_static_string() {
        assert_eq!(
            spelled("'fixture-authoring'"),
            Some("fixture-authoring".to_owned())
        );
        assert_eq!(
            spelled("`fixture-authoring`"),
            Some("fixture-authoring".to_owned())
        );
        assert_eq!(spelled("('fixture' as string)"), Some("fixture".to_owned()));
        // The cooked value, not the spelling.
        assert_eq!(
            spelled("'fixture\\x2dauthoring'"),
            Some("fixture-authoring".to_owned())
        );
        assert_eq!(spelled("``"), Some(String::new()));
        assert_eq!(spelled("`fixture-${name}`"), None);
        assert_eq!(spelled("name"), None);
        assert_eq!(spelled("'fixture' + '-authoring'"), None);
        assert_eq!(spelled("42"), None);
    }

    #[test]
    fn a_lone_surrogate_spells_no_string() {
        // The value is not a Rust string, so it is not a module specifier.
        assert_eq!(spelled("'\x5cuD800'"), None);
        assert_eq!(spelled("`\x5cuDC00`"), None);
    }
}
