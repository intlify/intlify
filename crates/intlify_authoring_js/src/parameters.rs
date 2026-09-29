// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reading the parameter object of one `intent()` use site.
//!
//! The second argument names the external parameters a use site supplies.
//! Only a plain object literal can be read without running code: each property
//! has a static name, and its value is an expression kept by its position and
//! never evaluated. Anything whose names could depend on run time is refused,
//! because a comparison with the message's requirements would then describe
//! names nobody can be sure the object has.
//!
//! Properties are kept in source order, which is the order a lowering has to
//! evaluate them in. Whether the names match what the message requires is
//! decided by `intlify_authoring`: missing, extra and duplicate names are its
//! diagnostics, not this module's.

use intlify_authoring::{OccurrenceRole, ParameterBinding, ReasonFamily};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey, PropertyKind};
use oxc_span::GetSpan;

use crate::cooked::{cook_string, settle};
use crate::detail;
use crate::failure::ProducerFailure;
use crate::limits::{JsAuthoringLimits, JsLimitKind};
use crate::report::Reporter;
use crate::syntax::transparent;

/// Read one use site's parameter object.
///
/// Returns `None` when the object cannot be read. Every reason is reported,
/// not only the first, so an author sees each property that needs changing.
pub(crate) fn read(
    argument: &Argument<'_>,
    text: &str,
    limits: &JsAuthoringLimits,
    reporter: &mut Reporter,
) -> Result<Option<Vec<ParameterBinding>>, ProducerFailure> {
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    let Some(Expression::ObjectExpression(object)) = argument.as_expression().map(transparent)
    else {
        reporter.at(unsupported, detail::parameters_opaque(), argument.span())?;
        return Ok(None);
    };

    let mut bindings = Vec::new();
    let mut readable = true;
    for property in &object.properties {
        let property = match property {
            ObjectPropertyKind::ObjectProperty(property) => property,
            ObjectPropertyKind::SpreadProperty(spread) => {
                reporter.at(unsupported, detail::parameter_spread(), spread.span)?;
                readable = false;
                continue;
            }
        };
        if property.kind != PropertyKind::Init || property.method {
            reporter.at(unsupported, detail::parameter_accessor(), property.span)?;
            readable = false;
            continue;
        }
        let name = if property.computed {
            None
        } else {
            match &property.key {
                PropertyKey::StaticIdentifier(identifier) => {
                    Some(identifier.name.as_str().to_owned())
                }
                PropertyKey::StringLiteral(literal) => {
                    let Some(cooked) = settle(cook_string(text, literal, limits), reporter)? else {
                        // The key spells an escape the profile refuses, and
                        // that has been reported at the escape.
                        readable = false;
                        continue;
                    };
                    Some(cooked.text)
                }
                _ => None,
            }
        };
        let Some(name) = name else {
            reporter.at(unsupported, detail::parameter_key(), property.span)?;
            readable = false;
            continue;
        };
        // `__proto__: value` sets the object's prototype rather than creating
        // a property. The shorthand `{ __proto__ }` is an ordinary property.
        if name == "__proto__" && !property.shorthand {
            reporter.at(unsupported, detail::parameter_prototype(), property.span)?;
            readable = false;
            continue;
        }
        if bindings.len() as u64 >= limits.parameter_bindings {
            return Err(ProducerFailure::Limit(JsLimitKind::ParameterBindings));
        }
        let expression =
            reporter.occurrence(property.value.span(), OccurrenceRole::ParameterExpression)?;
        bindings.push(ParameterBinding::new(&name, expression));
    }
    Ok(readable.then_some(bindings))
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{Diagnostic, Location};
    use oxc_allocator::Allocator;
    use oxc_ast::ast::Statement;

    use super::*;
    use crate::grammar::Grammar;
    use crate::limits::tests::generous;
    use crate::test_support::{at, nth, parse, reporter};

    /// What reading one argument gave: each parameter's name and value range,
    /// or nothing, and every record as a detail and a range.
    type Read = (
        Option<Vec<(String, (u64, u64))>>,
        Vec<(&'static str, (u64, u64))>,
    );

    /// Read the first argument of the call `text` holds.
    fn read_in(
        grammar: Grammar,
        text: &str,
        limits: &JsAuthoringLimits,
    ) -> Result<Read, ProducerFailure> {
        let arena = Allocator::default();
        let parsed = parse(&arena, text, grammar);
        let Some(Statement::ExpressionStatement(statement)) = parsed.program.body.first() else {
            panic!("{text:?} is one call");
        };
        let Expression::CallExpression(call) = &statement.expression else {
            panic!("{text:?} is one call");
        };
        let mut reporter = reporter(text);
        let bindings = read(&call.arguments[0], text, limits, &mut reporter)?;
        let bindings = bindings.map(|bindings| {
            bindings
                .iter()
                .map(|binding| {
                    let expression = binding.expression();
                    assert_eq!(expression.role(), OccurrenceRole::ParameterExpression);
                    let range = expression.range();
                    (binding.name().to_owned(), (range.start(), range.end()))
                })
                .collect()
        });
        let records = reporter.into_diagnostics().iter().map(reduced).collect();
        Ok((bindings, records))
    }

    fn read_as(text: &str) -> Read {
        read_in(Grammar::JsModule, text, &generous()).expect("the object is read")
    }

    fn reduced(record: &Diagnostic) -> (&'static str, (u64, u64)) {
        assert_eq!(record.origin().code(), "authoring-form-unsupported");
        let Location::Region(region) = record.location() else {
            panic!("a record points into the object");
        };
        (
            record.detail().expect("a detail").as_str(),
            (region.range().start(), region.range().end()),
        )
    }

    #[test]
    fn a_plain_object_supplies_its_names_in_source_order_and_its_values_by_position() {
        let text = "f({ total: sum(items), 'na\\x6de': user.name, count })\n";
        assert_eq!(
            read_as(text),
            (
                Some(vec![
                    ("total".to_owned(), at(text, "sum(items)")),
                    // A string key is its decoded text.
                    ("name".to_owned(), at(text, "user.name")),
                    // A shorthand property's value is its identifier.
                    ("count".to_owned(), at(text, "count")),
                ]),
                vec![]
            )
        );
    }

    #[test]
    fn an_empty_or_wrapped_object_is_still_a_plain_object() {
        assert_eq!(read_as("f({})\n"), (Some(vec![]), vec![]));
        let wrapped = "f(({ a } as Params))\n";
        assert_eq!(
            read_in(Grammar::TsModule, wrapped, &generous()).unwrap(),
            (Some(vec![("a".to_owned(), nth(wrapped, "a", 0))]), vec![])
        );
    }

    #[test]
    fn a_shorthand_proto_is_an_ordinary_property() {
        let text = "f({ __proto__ })\n";
        assert_eq!(
            read_as(text),
            (
                Some(vec![("__proto__".to_owned(), at(text, "__proto__"))]),
                vec![]
            )
        );
    }

    #[test]
    fn every_property_whose_name_could_change_at_run_time_is_reported() {
        let text = "f({ ...rest, [key]: v, 1: v, get a() { return 1 }, set b(x) {}, m() {}, \
                    __proto__: p, ok })\n";
        // One record per property, not only the first, and no parameters at
        // all: the names the object supplies are not known.
        assert_eq!(
            read_as(text),
            (
                None,
                vec![
                    ("parameter-spread", at(text, "...rest")),
                    ("parameter-key", at(text, "[key]: v")),
                    ("parameter-key", at(text, "1: v")),
                    ("parameter-accessor", at(text, "get a() { return 1 }")),
                    ("parameter-accessor", at(text, "set b(x) {}")),
                    ("parameter-accessor", at(text, "m() {}")),
                    ("parameter-prototype", at(text, "__proto__: p")),
                ]
            )
        );
        // Two prototype settings in one object are a syntax error, so the
        // string spelling is read on its own.
        let quoted = "f({ '__proto__': q })\n";
        assert_eq!(
            read_as(quoted),
            (
                None,
                vec![("parameter-prototype", at(quoted, "'__proto__': q"))]
            )
        );
    }

    #[test]
    fn an_argument_that_is_not_an_object_literal_is_opaque() {
        for (text, argument) in [
            ("f(params)\n", "params"),
            ("f(...args)\n", "...args"),
            ("f(make({ a }))\n", "make({ a })"),
            ("f(flag ? { a } : { b })\n", "flag ? { a } : { b }"),
        ] {
            assert_eq!(
                read_as(text),
                (None, vec![("parameters-opaque", at(text, argument))]),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_key_spelling_an_escape_the_profile_refuses_is_reported_at_the_escape() {
        let text = "f({ '\x5cuD800': v, ok })\n";
        assert_eq!(
            read_as(text),
            (None, vec![("surrogate-escape", at(text, "\x5cuD800"))])
        );
    }

    #[test]
    fn parameters_one_use_site_supplies_are_bounded_exactly() {
        let text = "f({ a, b })\n";
        let mut limits = generous();
        limits.parameter_bindings = 2;
        assert!(read_in(Grammar::JsModule, text, &limits).is_ok());
        limits.parameter_bindings = 1;
        assert_eq!(
            read_in(Grammar::JsModule, text, &limits),
            Err(ProducerFailure::Limit(JsLimitKind::ParameterBindings))
        );
    }
}
