// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The details this crate reports.
//!
//! As in `intlify_authoring`, a detail names which cause inside a reason
//! family one record reports, so that a fixture asserts the cause it means
//! rather than any cause sharing the family. Details are not a stable public
//! code registry; nothing outside this workspace should branch on one.

use intlify_authoring::Detail;

/// The unit's bytes are exactly what its snapshot names, and are not UTF-8.
#[must_use]
pub fn unit_not_text() -> Detail {
    Detail::literal("unit-not-text")
}

/// The host rejected the unit under the grammar its snapshot names.
#[must_use]
pub fn host_syntax_invalid() -> Detail {
    Detail::literal("host-syntax-invalid")
}

/// An intrinsic is referenced other than as the callee of a direct call, or
/// for `mf2`, as the tag of a template.
///
/// Assigning it, passing it, calling it optionally or through `.call`,
/// constructing it, and exporting it all use a known intrinsic in a way this
/// profile does not read, so each is reported rather than treated as an
/// ordinary value.
#[must_use]
pub fn intrinsic_use_unsupported() -> Detail {
    Detail::literal("intrinsic-use-unsupported")
}

/// A registered module is imported other than by a direct named import.
#[must_use]
pub fn import_form_unsupported() -> Detail {
    Detail::literal("import-form-unsupported")
}

/// `intent()` has no source, a spread argument, or more than two arguments.
#[must_use]
pub fn intent_arguments() -> Detail {
    Detail::literal("intent-arguments")
}

/// `intent()` chooses its declaration with a condition.
///
/// 016-010 accepts conditional declaration selection but defers it past the
/// first minimum, so the condition is not evaluated and neither alternative
/// is chosen.
#[must_use]
pub fn conditional_selection() -> Detail {
    Detail::literal("conditional-selection")
}

/// `intent()` names an imported binding, which only a module reference could
/// resolve.
#[must_use]
pub fn module_reference() -> Detail {
    Detail::literal("module-reference")
}

/// `intent()` names a binding that aliases a declaration instead of naming
/// the declaration itself.
#[must_use]
pub fn declaration_alias() -> Detail {
    Detail::literal("declaration-alias")
}

/// `intent()` and `noIntent()` are nested in each other, so one position is
/// both localized and excluded.
#[must_use]
pub fn explicit_forms_nested() -> Detail {
    Detail::literal("explicit-forms-nested")
}

/// A message source is a template with substitutions.
#[must_use]
pub fn template_substitution() -> Detail {
    Detail::literal("template-substitution")
}

/// A message source is computed at run time.
#[must_use]
pub fn message_dynamic() -> Detail {
    Detail::literal("message-dynamic")
}

/// A parameter argument is not an object literal.
#[must_use]
pub fn parameters_opaque() -> Detail {
    Detail::literal("parameters-opaque")
}

/// A parameter object spreads another object.
#[must_use]
pub fn parameter_spread() -> Detail {
    Detail::literal("parameter-spread")
}

/// A parameter's key is computed, or is not an identifier or a string.
#[must_use]
pub fn parameter_key() -> Detail {
    Detail::literal("parameter-key")
}

/// A parameter is a getter, a setter, or a method.
#[must_use]
pub fn parameter_accessor() -> Detail {
    Detail::literal("parameter-accessor")
}

/// A parameter object sets its own prototype with `__proto__: value`.
#[must_use]
pub fn parameter_prototype() -> Detail {
    Detail::literal("parameter-prototype")
}

/// `noIntent()` has a spread argument or more than two arguments.
#[must_use]
pub fn exclusion_arguments() -> Detail {
    Detail::literal("exclusion-arguments")
}

/// `noIntent()` gives no reason.
#[must_use]
pub fn exclusion_reason_missing() -> Detail {
    Detail::literal("exclusion-reason-missing")
}

/// `noIntent()` gives a reason computed at run time.
#[must_use]
pub fn exclusion_reason_dynamic() -> Detail {
    Detail::literal("exclusion-reason-dynamic")
}

/// `noIntent()` gives an empty reason.
#[must_use]
pub fn exclusion_reason_empty() -> Detail {
    Detail::literal("exclusion-reason-empty")
}

/// A literal spells a surrogate the profile does not accept.
#[must_use]
pub fn surrogate_escape() -> Detail {
    Detail::literal("surrogate-escape")
}

/// A tagged template keeps an escape that has no cooked value.
#[must_use]
pub fn template_escape_invalid() -> Detail {
    Detail::literal("template-escape-invalid")
}

/// A literal uses a legacy octal escape, or `\8` or `\9`.
#[must_use]
pub fn legacy_escape() -> Detail {
    Detail::literal("legacy-escape")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_detail_is_spelled_exactly_and_names_one_cause() {
        let details = [
            (unit_not_text(), "unit-not-text"),
            (host_syntax_invalid(), "host-syntax-invalid"),
            (intrinsic_use_unsupported(), "intrinsic-use-unsupported"),
            (import_form_unsupported(), "import-form-unsupported"),
            (intent_arguments(), "intent-arguments"),
            (conditional_selection(), "conditional-selection"),
            (module_reference(), "module-reference"),
            (declaration_alias(), "declaration-alias"),
            (explicit_forms_nested(), "explicit-forms-nested"),
            (template_substitution(), "template-substitution"),
            (message_dynamic(), "message-dynamic"),
            (parameters_opaque(), "parameters-opaque"),
            (parameter_spread(), "parameter-spread"),
            (parameter_key(), "parameter-key"),
            (parameter_accessor(), "parameter-accessor"),
            (parameter_prototype(), "parameter-prototype"),
            (exclusion_arguments(), "exclusion-arguments"),
            (exclusion_reason_missing(), "exclusion-reason-missing"),
            (exclusion_reason_dynamic(), "exclusion-reason-dynamic"),
            (exclusion_reason_empty(), "exclusion-reason-empty"),
            (surrogate_escape(), "surrogate-escape"),
            (template_escape_invalid(), "template-escape-invalid"),
            (legacy_escape(), "legacy-escape"),
        ];
        for (detail, spelling) in details {
            assert_eq!(detail.as_str(), spelling);
        }
        // A fixture tells causes apart by their detail, so no two may share
        // one.
        let mut spellings: Vec<&str> = details.iter().map(|(detail, _)| detail.as_str()).collect();
        spellings.sort_unstable();
        spellings.dedup();
        assert_eq!(spellings.len(), details.len());
    }
}
