// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The exact MF2 syntax and semantics this crate pins.
//!
//! 017 requires `mf2Specification` to name the supported syntax together with
//! the 001/002/012 parser-owned semantics, and requires that the binding be
//! pinned with independent fixtures before it is admitted. A package version or
//! a successful JSON decode is not a substitute.
//!
//! The registry snapshot below is that fixture. It records the complete parser
//! and semantic diagnostic catalogues this revision was reviewed against, so a
//! parser upgrade that adds, removes, or renames a code fails the snapshot and
//! forces an explicit decision about the pin instead of silently changing what
//! an already published Intent revision means.

use intlify_shared_json::token::VersionedIdentity;

/// Identity of the pinned MF2 syntax and semantics.
pub const MF2_SEMANTICS_IDENTITY: &str = "intlify-mf2-semantics";

/// Revision of the pinned MF2 syntax and semantics.
pub const MF2_SEMANTICS_REVISION: &str = "0";

/// Return the exact `mf2Specification` pin used by every projection.
#[must_use]
pub fn mf2_specification() -> VersionedIdentity {
    VersionedIdentity::literal(MF2_SEMANTICS_IDENTITY, MF2_SEMANTICS_REVISION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_mf2_parser::{DiagnosticCode, SemanticDiagnosticCode};

    #[test]
    fn the_pin_is_the_exact_registered_pair() {
        assert_eq!(
            serde_json::to_value(mf2_specification()).unwrap(),
            serde_json::json!({"identity": "intlify-mf2-semantics", "revision": "0"})
        );
    }

    #[test]
    fn semantic_diagnostic_catalogue_is_pinned_for_this_revision() {
        let codes: Vec<&str> = SemanticDiagnosticCode::all()
            .iter()
            .map(|code| code.json_code())
            .collect();
        assert_eq!(
            codes,
            [
                "duplicate-declaration",
                "invalid-declaration-dependency",
                "missing-selector-annotation",
                "variant-key-arity-mismatch",
                "missing-fallback-variant",
                "duplicate-variant",
                "duplicate-option-name",
            ],
            "the pinned MF2 semantics changed; review the mf2Specification revision \
             before accepting a parser upgrade, because an unchanged pin claims an \
             unchanged meaning for every already computed Intent revision"
        );
    }

    #[test]
    fn syntax_diagnostic_catalogue_is_pinned_for_this_revision() {
        let codes: Vec<&str> = DiagnosticCode::all()
            .iter()
            .map(|code| code.json_code())
            .collect();
        assert_eq!(
            codes,
            [
                "unspecified",
                "unexpected-end-of-input",
                "unclosed-expression",
                "unclosed-quoted-literal",
                "unclosed-quoted-pattern",
                "invalid-declaration-start",
                "invalid-matcher-syntax",
                "invalid-variant-boundary",
                "invalid-markup-boundary",
                "missing-complex-body",
                "unexpected-token",
                "span-overflow",
                "invalid-escape",
                "ambiguous-message-mode",
                "missing-required-whitespace",
                "missing-identifier-name",
                "invalid-input-declaration",
            ],
            "the pinned MF2 syntax catalogue changed; review the mf2Specification \
             revision before accepting a parser upgrade"
        );
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "duplicate syntax code spelling");
    }
}
