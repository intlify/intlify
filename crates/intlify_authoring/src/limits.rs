// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Explicit finite limits for one authoring invocation.
//!
//! Every bound is caller-supplied. This type deliberately has no `Default`, so
//! adding a bound is a compile error at each construction site rather than a
//! hidden value that silently admits unbounded work.
//!
//! Exhausting a bound is reported separately from an ordinary semantic
//! mismatch: it is a resource-limit outcome, never an incomplete result
//! promoted to success.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which named bound an invocation exhausted.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum LimitKind {
    /// Declarations accepted in one invocation.
    Declarations,
    /// Bytes of one supplied message text, before MF2 encoding.
    MessageTextBytes,
    /// Bytes of one emitted MF2 message, after literal encoding.
    EmittedMf2Bytes,
    /// Extraction segments retained for one message.
    ExtractionSegments,
    /// Distinct external parameter names required by one message.
    ParameterNames,
    /// Bytes of one external parameter name.
    ParameterNameBytes,
    /// Bytes of one supplied metadata value.
    MetadataValueBytes,
    /// Members admitted in one surface-class vocabulary.
    VocabularyMembers,
    /// Nodes retained in one semantic projection.
    ProjectionNodes,
    /// Nesting depth of one semantic projection.
    ProjectionDepth,
    /// Diagnostics retained by one invocation.
    Diagnostics,
}

impl LimitKind {
    /// Return the exact wire spelling used in diagnostics and ordering.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Declarations => "declarations",
            Self::MessageTextBytes => "message-text-bytes",
            Self::EmittedMf2Bytes => "emitted-mf2-bytes",
            Self::ExtractionSegments => "extraction-segments",
            Self::ParameterNames => "parameter-names",
            Self::ParameterNameBytes => "parameter-name-bytes",
            Self::MetadataValueBytes => "metadata-value-bytes",
            Self::VocabularyMembers => "vocabulary-members",
            Self::ProjectionNodes => "projection-nodes",
            Self::ProjectionDepth => "projection-depth",
            Self::Diagnostics => "diagnostics",
        }
    }
}

/// Complete failure of a limit construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitsError {
    /// A bound was supplied that no invocation could satisfy.
    UnsatisfiableBound(LimitKind),
}

/// Inclusive upper bounds applied to one invocation.
///
/// There is deliberately no `Default`, no builder default, and no "unlimited"
/// spelling. A caller that does not know a bound must decide one rather than
/// inherit a hidden value from this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthoringLimits {
    /// Declarations accepted in one invocation.
    pub declarations: u64,
    /// Bytes of one supplied message text, before MF2 encoding.
    pub message_text_bytes: u64,
    /// Bytes of one emitted MF2 message, after literal encoding.
    pub emitted_mf2_bytes: u64,
    /// Extraction segments retained for one message.
    pub extraction_segments: u64,
    /// Distinct external parameter names required by one message.
    pub parameter_names: u64,
    /// Bytes of one external parameter name.
    pub parameter_name_bytes: u64,
    /// Bytes of one supplied metadata value.
    pub metadata_value_bytes: u64,
    /// Members admitted in one surface-class vocabulary.
    pub vocabulary_members: u64,
    /// Nodes retained in one semantic projection.
    pub projection_nodes: u64,
    /// Nesting depth of one semantic projection.
    pub projection_depth: u64,
    /// Diagnostics retained by one invocation.
    pub diagnostics: u64,
}

impl AuthoringLimits {
    /// Check that every bound can be satisfied by some invocation.
    ///
    /// A zero bound is admitted where it has a meaning, such as accepting no
    /// declarations. Bounds that must be positive for any message to succeed
    /// are rejected here instead of failing every later message.
    pub fn validate(self) -> Result<Self, LimitsError> {
        for (value, kind) in [
            (self.emitted_mf2_bytes, LimitKind::EmittedMf2Bytes),
            (self.extraction_segments, LimitKind::ExtractionSegments),
            (self.projection_depth, LimitKind::ProjectionDepth),
            (self.projection_nodes, LimitKind::ProjectionNodes),
        ] {
            if value == 0 {
                return Err(LimitsError::UnsatisfiableBound(kind));
            }
        }
        Ok(self)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn generous() -> AuthoringLimits {
        AuthoringLimits {
            declarations: 1024,
            message_text_bytes: 64 * 1024,
            emitted_mf2_bytes: 128 * 1024,
            extraction_segments: 4096,
            parameter_names: 64,
            parameter_name_bytes: 256,
            metadata_value_bytes: 4096,
            vocabulary_members: 256,
            projection_nodes: 4096,
            projection_depth: 32,
            diagnostics: 256,
        }
    }

    #[test]
    fn bounds_that_no_invocation_could_satisfy_are_rejected() {
        assert!(generous().validate().is_ok());
        for (mutate, kind) in [
            (
                (|limits: &mut AuthoringLimits| limits.emitted_mf2_bytes = 0)
                    as fn(&mut AuthoringLimits),
                LimitKind::EmittedMf2Bytes,
            ),
            (
                |limits: &mut AuthoringLimits| limits.extraction_segments = 0,
                LimitKind::ExtractionSegments,
            ),
            (
                |limits: &mut AuthoringLimits| limits.projection_depth = 0,
                LimitKind::ProjectionDepth,
            ),
            (
                |limits: &mut AuthoringLimits| limits.projection_nodes = 0,
                LimitKind::ProjectionNodes,
            ),
        ] {
            let mut limits = generous();
            mutate(&mut limits);
            assert_eq!(
                limits.validate(),
                Err(LimitsError::UnsatisfiableBound(kind))
            );
        }
        // Accepting no declarations is a meaningful bound, not a defect.
        let mut none = generous();
        none.declarations = 0;
        assert!(none.validate().is_ok());
    }

    #[test]
    fn limit_spellings_are_distinct_and_stable() {
        let kinds = [
            LimitKind::Declarations,
            LimitKind::MessageTextBytes,
            LimitKind::EmittedMf2Bytes,
            LimitKind::ExtractionSegments,
            LimitKind::ParameterNames,
            LimitKind::ParameterNameBytes,
            LimitKind::MetadataValueBytes,
            LimitKind::VocabularyMembers,
            LimitKind::ProjectionNodes,
            LimitKind::ProjectionDepth,
            LimitKind::Diagnostics,
        ];
        let mut spellings: Vec<&str> = kinds.iter().map(|kind| kind.as_str()).collect();
        spellings.sort_unstable();
        let total = spellings.len();
        spellings.dedup();
        assert_eq!(spellings.len(), total);
        for kind in kinds {
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::json!(kind.as_str())
            );
        }
    }
}
