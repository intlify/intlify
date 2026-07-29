// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Closed producer failure evidence and control-flow outcomes.
//!
//! This module owns the stable producer stage/reason vocabulary and ensures
//! source spans and limit evidence appear only on the combinations admitted by
//! the message-linker contract. Dependency diagnostics and source excerpts are
//! deliberately not retained.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;

use intlify_contract::{SourceDocumentIdentity, SourceUtf8Span};

/// Stable stage of one source-attributable built-in producer failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum JsProducerFailureStage {
    /// Reference-artifact identity construction.
    Identity,
    /// Selected source suffix and grammar profile.
    Profile,
    /// Exact selected source snapshot admission.
    Snapshot,
    /// Complete-source frontend parsing.
    Parse,
    /// Configured call recognition and argument shape.
    Recognizer,
    /// Static selector conversion and validation.
    Selector,
    /// Checked reference-artifact output.
    Artifact,
}

impl JsProducerFailureStage {
    /// Return the exact machine-facing evidence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Profile => "profile",
            Self::Snapshot => "snapshot",
            Self::Parse => "parse",
            Self::Recognizer => "recognizer",
            Self::Selector => "selector",
            Self::Artifact => "artifact",
        }
    }
}

/// Stable reason for one source-attributable built-in producer failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum JsProducerFailureReason {
    /// The prefixed reference-artifact identity exceeds a checked limit.
    ArtifactIdentityLimit,
    /// The selected logical source path has no supported exact suffix.
    UnsupportedSourceSuffix,
    /// Equal physical bytes were selected through incompatible source profiles.
    ConflictingAliasProfile,
    /// The selected snapshot is not exact valid UTF-8.
    InvalidUtf8,
    /// The invocation contains too many physical source groups.
    SourceCountLimit,
    /// One selected source snapshot exceeds the fixed per-source ceiling.
    SourceBytesLimit,
    /// The canonical source invocation exceeds its aggregate byte ceiling.
    SourceBytesTotalLimit,
    /// The complete selected grammar/source-goal parse has syntax errors.
    SyntaxInvalid,
    /// A recognized call has no first selector argument.
    SelectorArgumentMissing,
    /// A recognized call's first selector argument is a spread element.
    SelectorArgumentSpread,
    /// A statically known lookup selector is invalid under its configured syntax.
    LookupSelectorInvalid,
    /// A configured set call does not have an expression-local static string.
    SetSelectorDynamic,
    /// A statically known set selector is not one valid finite pattern.
    SetSelectorInvalid,
    /// Checked artifact construction rejected source-dependent output.
    OutputContractInvalid,
    /// Checked artifact construction exceeded an effective output limit.
    OutputLimit,
    /// Canonical artifact encoding could not produce one complete output.
    CanonicalEncodingFailed,
}

impl JsProducerFailureReason {
    /// Return the only stage that may carry this reason.
    #[must_use]
    pub const fn stage(self) -> JsProducerFailureStage {
        match self {
            Self::ArtifactIdentityLimit => JsProducerFailureStage::Identity,
            Self::UnsupportedSourceSuffix | Self::ConflictingAliasProfile => {
                JsProducerFailureStage::Profile
            }
            Self::InvalidUtf8
            | Self::SourceCountLimit
            | Self::SourceBytesLimit
            | Self::SourceBytesTotalLimit => JsProducerFailureStage::Snapshot,
            Self::SyntaxInvalid => JsProducerFailureStage::Parse,
            Self::SelectorArgumentMissing | Self::SelectorArgumentSpread => {
                JsProducerFailureStage::Recognizer
            }
            Self::LookupSelectorInvalid | Self::SetSelectorDynamic | Self::SetSelectorInvalid => {
                JsProducerFailureStage::Selector
            }
            Self::OutputContractInvalid | Self::OutputLimit | Self::CanonicalEncodingFailed => {
                JsProducerFailureStage::Artifact
            }
        }
    }

    /// Return the exact machine-facing evidence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ArtifactIdentityLimit => "artifact_identity_limit",
            Self::UnsupportedSourceSuffix => "unsupported_source_suffix",
            Self::ConflictingAliasProfile => "conflicting_alias_profile",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::SourceCountLimit => "source_count_limit",
            Self::SourceBytesLimit => "source_bytes_limit",
            Self::SourceBytesTotalLimit => "source_bytes_total_limit",
            Self::SyntaxInvalid => "syntax_invalid",
            Self::SelectorArgumentMissing => "selector_argument_missing",
            Self::SelectorArgumentSpread => "selector_argument_spread",
            Self::LookupSelectorInvalid => "lookup_selector_invalid",
            Self::SetSelectorDynamic => "set_selector_dynamic",
            Self::SetSelectorInvalid => "set_selector_invalid",
            Self::OutputContractInvalid => "output_contract_invalid",
            Self::OutputLimit => "output_limit",
            Self::CanonicalEncodingFailed => "canonical_encoding_failed",
        }
    }

    const fn permits_span(self) -> bool {
        matches!(
            self.stage(),
            JsProducerFailureStage::Parse
                | JsProducerFailureStage::Recognizer
                | JsProducerFailureStage::Selector
        )
    }

    const fn requires_limit(self) -> bool {
        matches!(
            self,
            Self::ArtifactIdentityLimit
                | Self::SourceCountLimit
                | Self::SourceBytesLimit
                | Self::SourceBytesTotalLimit
                | Self::OutputLimit
        )
    }
}

/// Canonical bounded evidence for one failed JS/TS source participant.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsProducerFailure {
    reason: JsProducerFailureReason,
    source: SourceDocumentIdentity,
    span: Option<SourceUtf8Span>,
    limit: Option<u64>,
    observed: Option<u64>,
}

impl JsProducerFailure {
    pub(crate) fn without_optional(
        reason: JsProducerFailureReason,
        source: SourceDocumentIdentity,
    ) -> Self {
        debug_assert!(!reason.requires_limit());
        Self {
            reason,
            source,
            span: None,
            limit: None,
            observed: None,
        }
    }

    pub(crate) fn with_span(
        reason: JsProducerFailureReason,
        source: SourceDocumentIdentity,
        span: Option<SourceUtf8Span>,
    ) -> Self {
        debug_assert!(reason.permits_span());
        debug_assert!(!reason.requires_limit());
        Self {
            reason,
            source,
            span,
            limit: None,
            observed: None,
        }
    }

    pub(crate) fn with_limit(
        reason: JsProducerFailureReason,
        source: SourceDocumentIdentity,
        limit: u64,
        observed: u64,
    ) -> Self {
        debug_assert!(reason.requires_limit());
        Self {
            reason,
            source,
            span: None,
            limit: Some(limit),
            observed: Some(observed),
        }
    }

    /// Return the derived closed producer stage.
    #[must_use]
    pub const fn stage(&self) -> JsProducerFailureStage {
        self.reason.stage()
    }

    /// Return the stage-owned closed reason.
    #[must_use]
    pub const fn reason(&self) -> JsProducerFailureReason {
        self.reason
    }

    /// Return the primary portable source identity.
    #[must_use]
    pub const fn source(&self) -> &SourceDocumentIdentity {
        &self.source
    }

    /// Return an exact safe source range when the stage admits one.
    #[must_use]
    pub const fn span(&self) -> Option<SourceUtf8Span> {
        self.span
    }

    /// Return the checked inclusive limit when this is a limit failure.
    #[must_use]
    pub const fn limit(&self) -> Option<u64> {
        self.limit
    }

    /// Return the deterministic observed evidence paired with `limit`.
    ///
    /// Most limit failures retain the exact attempted observation. A bounded
    /// streaming-style admission may instead retain the first rejected value
    /// (`limit + 1`) when that sentinel is fixed by its public contract.
    #[must_use]
    pub const fn observed(&self) -> Option<u64> {
        self.observed
    }

    pub(crate) fn cmp_within_source(&self, other: &Self) -> Ordering {
        self.stage()
            .cmp(&other.stage())
            .then_with(|| compare_optional_spans(self.span, other.span))
            .then_with(|| self.reason.cmp(&other.reason))
    }
}

fn compare_optional_spans(left: Option<SourceUtf8Span>, right: Option<SourceUtf8Span>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Complete outcome error for one synchronous source scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsProducerError {
    /// The selected source failed with stable source-attributable evidence.
    Failed(JsProducerFailure),
    /// The caller-owned cancellation probe stopped the complete operation.
    Cancelled,
    /// An impossible frontend or checked-boundary contradiction was detected.
    InternalInvariant,
}

impl fmt::Display for JsProducerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed(failure) => write!(
                formatter,
                "JavaScript producer failed at {}/{}",
                failure.stage().as_str(),
                failure.reason().as_str()
            ),
            Self::Cancelled => formatter.write_str("JavaScript producer operation was cancelled"),
            Self::InternalInvariant => formatter.write_str("JavaScript producer invariant failed"),
        }
    }
}

impl Error for JsProducerError {}

impl From<JsProducerFailure> for JsProducerError {
    fn from(value: JsProducerFailure) -> Self {
        Self::Failed(value)
    }
}
