// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Structured authoring diagnostics with deterministic ordering.
//!
//! These are component-owned reason families, not a frozen global registry and
//! not 019's common envelope. MF2 parser and semantic diagnostics keep their
//! owning codes rather than being relabelled into an authoring family, so a
//! reader can still tell a malformed message from a rejected authoring form.
//!
//! A diagnostic is distinct from the overall outcome. Retaining diagnostics
//! never turns a blocked result into a checked one, and any truncation is
//! explicit.

use crate::limits::LimitKind;
use crate::primitives::{ByteRange, Occurrence};

/// Reason families owned by design 016.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReasonFamily {
    /// Source, profile, inventory, or binding inputs are malformed,
    /// incompatible, or inconsistently attached.
    AuthoringInputInvalid,
    /// A recognized intrinsic or known UI surface uses an unsupported form.
    AuthoringFormUnsupported,
    /// Message source or reference identity cannot be enumerated finitely.
    AuthoringSourceDynamic,
    /// Metadata is malformed, multiply attached, ambiguously placed, or
    /// attempts to redefine an existing declaration.
    AuthoringMetadataInvalid,
    /// No permitted source-locale basis exists.
    AuthoringSourceLocaleMissing,
    /// The supplied locale fails the admitted canonicalization rules.
    AuthoringSourceLocaleInvalid,
    /// A required surface-class assignment is missing or outside the exact
    /// admitted vocabulary.
    AuthoringSurfaceClassInvalid,
    /// Missing, extra, or duplicate parameter names, or incompatible
    /// finite-alternative requirements.
    AuthoringParameterMismatch,
    /// New or moved source needs accepted identity associations.
    AuthoringIdentityUpdateRequired,
    /// Several lineages compete, or plan and base inputs conflict.
    AuthoringIdentityConflict,
    /// A named limit was exceeded.
    AuthoringResourceLimit,
}

impl ReasonFamily {
    /// Return the exact wire spelling used for reporting and ordering.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AuthoringInputInvalid => "authoring-input-invalid",
            Self::AuthoringFormUnsupported => "authoring-form-unsupported",
            Self::AuthoringSourceDynamic => "authoring-source-dynamic",
            Self::AuthoringMetadataInvalid => "authoring-metadata-invalid",
            Self::AuthoringSourceLocaleMissing => "authoring-source-locale-missing",
            Self::AuthoringSourceLocaleInvalid => "authoring-source-locale-invalid",
            Self::AuthoringSurfaceClassInvalid => "authoring-surface-class-invalid",
            Self::AuthoringParameterMismatch => "authoring-parameter-mismatch",
            Self::AuthoringIdentityUpdateRequired => "authoring-identity-update-required",
            Self::AuthoringIdentityConflict => "authoring-identity-conflict",
            Self::AuthoringResourceLimit => "authoring-resource-limit",
        }
    }
}

/// The 016 operation that produced a diagnostic.
///
/// Phase 1 emits the three middle stages. Host discovery belongs to a Producer
/// and identity resolution to the registry operation; both are listed so the
/// ordering rule stays stable as those phases are implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    HostDiscovery,
    MessageAnalysis,
    ContextResolution,
    IdentityResolution,
    ResultConstruction,
}

impl Stage {
    /// Return the exact wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HostDiscovery => "host-discovery",
            Self::MessageAnalysis => "message-analysis",
            Self::ContextResolution => "context-resolution",
            Self::IdentityResolution => "identity-resolution",
            Self::ResultConstruction => "result-construction",
        }
    }
}

/// Severity of one structured record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Information,
}

/// Which owner defined the reported reason.
///
/// Parser syntax and semantic failures keep their owning code. They are not
/// rewritten into an authoring family, because "this message is malformed" and
/// "this authoring form is unsupported" are different facts with different
/// remediation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticOrigin {
    /// A reason family owned by 016.
    Authoring(ReasonFamily),
    /// One MF2 syntax diagnostic, by its parser-owned stable code.
    Mf2Syntax(&'static str),
    /// One MF2 semantic diagnostic, by its parser-owned stable code.
    Mf2Semantic(&'static str),
}

impl DiagnosticOrigin {
    /// Return the reason spelling used for reporting and ordering.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Authoring(family) => family.as_str(),
            Self::Mf2Syntax(code) | Self::Mf2Semantic(code) => code,
        }
    }

    // Ordering groups authoring reasons before parser-owned ones so a reader
    // sees the authoring decision before the message-level detail it caused.
    const fn rank(self) -> u8 {
        match self {
            Self::Authoring(_) => 0,
            Self::Mf2Syntax(_) => 1,
            Self::Mf2Semantic(_) => 2,
        }
    }
}

/// One structured authoring record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    stage: Stage,
    origin: DiagnosticOrigin,
    severity: Severity,
    occurrence: Occurrence,
    message_range: Option<ByteRange>,
    limit: Option<LimitKind>,
    related: Box<[Occurrence]>,
}

impl Diagnostic {
    /// Record one diagnostic against a declaration occurrence.
    #[must_use]
    pub fn new(
        stage: Stage,
        origin: DiagnosticOrigin,
        severity: Severity,
        occurrence: Occurrence,
    ) -> Self {
        Self {
            stage,
            origin,
            severity,
            occurrence,
            message_range: None,
            limit: None,
            related: Box::new([]),
        }
    }

    /// Attach the range inside the extracted MF2 message that this concerns.
    ///
    /// This addresses the emitted MF2 bytes, not the host source. Callers map
    /// it back through the extraction segments.
    #[must_use]
    pub fn with_message_range(mut self, range: ByteRange) -> Self {
        self.message_range = Some(range);
        self
    }

    /// Name the exhausted bound for a resource-limit reason.
    #[must_use]
    pub fn with_limit(mut self, limit: LimitKind) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Attach related occurrences in the same admitted source domain.
    #[must_use]
    pub fn with_related(mut self, related: impl Into<Box<[Occurrence]>>) -> Self {
        self.related = related.into();
        self
    }

    /// Return the producing stage.
    #[must_use]
    pub const fn stage(&self) -> Stage {
        self.stage
    }

    /// Return the reason and its owner.
    #[must_use]
    pub const fn origin(&self) -> DiagnosticOrigin {
        self.origin
    }

    /// Return the severity.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.severity
    }

    /// Borrow the reported occurrence.
    #[must_use]
    pub const fn occurrence(&self) -> &Occurrence {
        &self.occurrence
    }

    /// Return the exhausted bound, when this is a resource-limit reason.
    #[must_use]
    pub const fn limit(&self) -> Option<LimitKind> {
        self.limit
    }

    /// Return whether this record blocks a checked result.
    #[must_use]
    pub const fn is_blocking(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }

    /// Compare two records in 016's deterministic reporting order.
    ///
    /// The order is stage, admitted source-unit identity and range, component
    /// reason, then a stable related-occurrence discriminator. It never depends
    /// on worker scheduling or hash-map iteration.
    #[must_use]
    pub fn reporting_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.stage
            .cmp(&other.stage)
            .then_with(|| self.occurrence.canonical_cmp(&other.occurrence))
            .then_with(|| self.origin.rank().cmp(&other.origin.rank()))
            .then_with(|| {
                self.origin
                    .code()
                    .as_bytes()
                    .cmp(other.origin.code().as_bytes())
            })
            .then_with(|| self.message_range.cmp(&other.message_range))
            .then_with(|| self.severity.cmp(&other.severity))
            .then_with(|| self.limit.cmp(&other.limit))
            .then_with(|| self.related.len().cmp(&other.related.len()))
            .then_with(|| {
                for (left, right) in self.related.iter().zip(other.related.iter()) {
                    let ordering = left.canonical_cmp(right);
                    if ordering != std::cmp::Ordering::Equal {
                        return ordering;
                    }
                }
                std::cmp::Ordering::Equal
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{OccurrenceRole, OwnerIdentity, OwnerKind, SourceSnapshot};
    use intlify_shared_json::token::VersionedIdentity;

    fn occurrence(start: u64, end: u64, role: OccurrenceRole) -> Occurrence {
        let source = SourceSnapshot::new(
            OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
            "checkout",
            "1",
            VersionedIdentity::literal("intlify-js-grammar", "0"),
            512,
            &format!("sha256:{}", "0".repeat(64)),
        )
        .unwrap();
        Occurrence::new(source, ByteRange::new(start, end).unwrap(), role).unwrap()
    }

    #[test]
    fn reason_and_stage_spellings_are_distinct() {
        let families = [
            ReasonFamily::AuthoringInputInvalid,
            ReasonFamily::AuthoringFormUnsupported,
            ReasonFamily::AuthoringSourceDynamic,
            ReasonFamily::AuthoringMetadataInvalid,
            ReasonFamily::AuthoringSourceLocaleMissing,
            ReasonFamily::AuthoringSourceLocaleInvalid,
            ReasonFamily::AuthoringSurfaceClassInvalid,
            ReasonFamily::AuthoringParameterMismatch,
            ReasonFamily::AuthoringIdentityUpdateRequired,
            ReasonFamily::AuthoringIdentityConflict,
            ReasonFamily::AuthoringResourceLimit,
        ];
        let mut spellings: Vec<&str> = families.iter().map(|family| family.as_str()).collect();
        let total = spellings.len();
        spellings.sort_unstable();
        spellings.dedup();
        assert_eq!(spellings.len(), total);
        assert_eq!(Stage::MessageAnalysis.as_str(), "message-analysis");
    }

    #[test]
    fn parser_owned_codes_are_preserved_rather_than_relabelled() {
        let syntax = DiagnosticOrigin::Mf2Syntax("unclosed-expression");
        let semantic = DiagnosticOrigin::Mf2Semantic("duplicate-declaration");
        let authoring = DiagnosticOrigin::Authoring(ReasonFamily::AuthoringFormUnsupported);
        assert_eq!(syntax.code(), "unclosed-expression");
        assert_eq!(semantic.code(), "duplicate-declaration");
        assert_eq!(authoring.code(), "authoring-form-unsupported");
        assert_ne!(syntax, semantic);
    }

    #[test]
    fn reporting_order_is_deterministic_and_independent_of_insertion() {
        let build = |stage, origin, start, end| {
            Diagnostic::new(
                stage,
                origin,
                Severity::Error,
                occurrence(start, end, OccurrenceRole::UiLiteral),
            )
        };
        let mut records = vec![
            build(
                Stage::ContextResolution,
                DiagnosticOrigin::Authoring(ReasonFamily::AuthoringSourceLocaleMissing),
                0,
                4,
            ),
            build(
                Stage::MessageAnalysis,
                DiagnosticOrigin::Mf2Syntax("unclosed-expression"),
                10,
                14,
            ),
            build(
                Stage::MessageAnalysis,
                DiagnosticOrigin::Authoring(ReasonFamily::AuthoringParameterMismatch),
                10,
                14,
            ),
            build(
                Stage::MessageAnalysis,
                DiagnosticOrigin::Mf2Syntax("unclosed-expression"),
                2,
                6,
            ),
        ];
        let mut reversed = records.clone();
        reversed.reverse();
        records.sort_by(Diagnostic::reporting_cmp);
        reversed.sort_by(Diagnostic::reporting_cmp);
        assert_eq!(records, reversed);
        let order: Vec<(&str, &str, u64)> = records
            .iter()
            .map(|record| {
                (
                    record.stage().as_str(),
                    record.origin().code(),
                    record.occurrence().range().start(),
                )
            })
            .collect();
        assert_eq!(
            order,
            [
                ("message-analysis", "unclosed-expression", 2),
                ("message-analysis", "authoring-parameter-mismatch", 10),
                ("message-analysis", "unclosed-expression", 10),
                ("context-resolution", "authoring-source-locale-missing", 0),
            ]
        );
    }

    #[test]
    fn only_errors_block_a_checked_result() {
        let base = occurrence(0, 4, OccurrenceRole::UiLiteral);
        let error = Diagnostic::new(
            Stage::MessageAnalysis,
            DiagnosticOrigin::Authoring(ReasonFamily::AuthoringResourceLimit),
            Severity::Error,
            base.clone(),
        )
        .with_limit(LimitKind::EmittedMf2Bytes);
        let warning = Diagnostic::new(
            Stage::MessageAnalysis,
            DiagnosticOrigin::Mf2Syntax("ambiguous-message-mode"),
            Severity::Warning,
            base,
        );
        assert!(error.is_blocking());
        assert_eq!(error.limit(), Some(LimitKind::EmittedMf2Bytes));
        assert!(!warning.is_blocking());
        assert_eq!(warning.limit(), None);
    }
}
