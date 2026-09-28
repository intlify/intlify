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
