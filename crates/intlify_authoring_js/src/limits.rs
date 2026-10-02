// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Explicit finite limits for one Producer invocation.
//!
//! Every bound is caller-supplied, as in `intlify_authoring`. There is no
//! `Default`, so adding a bound is a compile error at each construction site
//! rather than a hidden value that silently admits unbounded work.
//!
//! The shared crate's own bounds are carried inside, and where a bound means
//! the same thing on both sides it is not repeated: the declarations one unit
//! may hold are the declarations one shared invocation accepts, and one unit's
//! diagnostics are bounded by the shared diagnostic limit, host and shared
//! records together.
//!
//! Exhausting a bound never produces a result from work that stopped short.
//! What it stops follows `intlify_authoring`. A bound on one literal or one
//! use site blocks that declaration or use with an `authoring-resource-limit`
//! diagnostic, and the rest of the unit is still read. A bound on the
//! invocation or on a whole unit is an operational failure.

use intlify_authoring::{AuthoringLimits, LimitScope, LimitsError};

/// Which named bound an invocation exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JsLimitKind {
    /// Units a scope declares, and so units one invocation supplies.
    Units,
    /// Bytes of one supplied unit.
    UnitBytes,
    /// Bytes of every supplied unit together.
    TotalBytes,
    /// Syntax tree nodes of one parsed unit.
    AstNodes,
    /// Input map segments of one decoded literal.
    InputSegments,
    /// References one unit makes.
    References,
    /// Exclusions one unit makes.
    Exclusions,
    /// Parameters one use site supplies.
    ParameterBindings,
    /// Links in one chain of `const` aliases from a DOM receiver origin.
    AliasChain,
    /// DOM receiver origins one function tracks.
    TrackedOrigins,
    /// Steps spent proving receiver evidence in one function.
    ProofSteps,
    /// Bytes of one `@intlify` annotation, its delimiters included.
    AnnotationBytes,
}

impl JsLimitKind {
    /// Return the exact spelling used in diagnostics and ordering.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Units => "units",
            Self::UnitBytes => "unit-bytes",
            Self::TotalBytes => "total-bytes",
            Self::AstNodes => "ast-nodes",
            Self::InputSegments => "input-segments",
            Self::References => "references",
            Self::Exclusions => "exclusions",
            Self::ParameterBindings => "parameter-bindings",
            Self::AliasChain => "alias-chain",
            Self::TrackedOrigins => "tracked-origins",
            Self::ProofSteps => "proof-steps",
            Self::AnnotationBytes => "annotation-bytes",
        }
    }

    /// Return what exhausting this bound stops.
    ///
    /// A literal's input map, one use site's parameters and one annotation
    /// belong to one declaration or one use, and the receiver evidence of one
    /// function belongs to the automatic candidates in it. Everything else
    /// bounds a unit or the whole invocation.
    #[must_use]
    pub const fn scope(self) -> LimitScope {
        match self {
            Self::InputSegments
            | Self::ParameterBindings
            | Self::AliasChain
            | Self::TrackedOrigins
            | Self::ProofSteps
            | Self::AnnotationBytes => LimitScope::Declaration,
            Self::Units
            | Self::UnitBytes
            | Self::TotalBytes
            | Self::AstNodes
            | Self::References
            | Self::Exclusions => LimitScope::Invocation,
        }
    }
}

/// Complete failure of a limit construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsLimitsError {
    /// A bound was supplied that no invocation could satisfy.
    UnsatisfiableBound(JsLimitKind),
    /// The shared crate refused one of its own bounds.
    Authoring(LimitsError),
}

/// Inclusive upper bounds applied to one invocation.
///
/// There is deliberately no `Default`, no builder default, and no "unlimited"
/// spelling. A caller that does not know a bound decides one rather than
/// inheriting a hidden value from this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsAuthoringLimits {
    /// Units a scope declares, and so units one invocation supplies.
    pub units: u64,
    /// Bytes of one supplied unit.
    pub unit_bytes: u64,
    /// Bytes of every supplied unit together.
    pub total_bytes: u64,
    /// Syntax tree nodes of one parsed unit.
    pub ast_nodes: u64,
    /// Input map segments of one decoded literal.
    pub input_segments: u64,
    /// References one unit makes.
    pub references: u64,
    /// Exclusions one unit makes.
    pub exclusions: u64,
    /// Parameters one use site supplies.
    pub parameter_bindings: u64,
    /// Links in one chain of `const` aliases from a DOM receiver origin.
    pub alias_chain: u64,
    /// DOM receiver origins one function tracks.
    pub tracked_origins: u64,
    /// Steps spent proving receiver evidence in one function.
    pub proof_steps: u64,
    /// Bytes of one `@intlify` annotation, its delimiters included.
    pub annotation_bytes: u64,
    /// The shared crate's bounds, applied to each unit's declarations.
    pub authoring: AuthoringLimits,
}

impl JsAuthoringLimits {
    /// Check that every bound can be satisfied by some invocation.
    ///
    /// A zero bound is admitted where it has a meaning: a scope with no units,
    /// a unit with no bytes, and a unit that may make no reference are all
    /// real. Every parsed unit has at least its program node, and every
    /// decoded literal has at least one segment, even an empty one, so those
    /// two bounds have to be positive.
    pub fn validate(self) -> Result<Self, JsLimitsError> {
        for (value, kind) in [
            (self.ast_nodes, JsLimitKind::AstNodes),
            (self.input_segments, JsLimitKind::InputSegments),
        ] {
            if value == 0 {
                return Err(JsLimitsError::UnsatisfiableBound(kind));
            }
        }
        self.authoring
            .validate()
            .map_err(JsLimitsError::Authoring)?;
        Ok(self)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use intlify_authoring::LimitKind;

    use super::*;

    pub(crate) fn generous() -> JsAuthoringLimits {
        JsAuthoringLimits {
            units: 64,
            unit_bytes: 64 * 1024,
            total_bytes: 1024 * 1024,
            ast_nodes: 64 * 1024,
            input_segments: 1024,
            references: 1024,
            exclusions: 1024,
            parameter_bindings: 64,
            alias_chain: 16,
            tracked_origins: 64,
            proof_steps: 1 << 20,
            annotation_bytes: 4096,
            authoring: AuthoringLimits {
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
            },
        }
    }

    #[test]
    fn bounds_that_no_invocation_could_satisfy_are_rejected() {
        assert!(generous().validate().is_ok());
        let mut limits = generous();
        limits.ast_nodes = 0;
        assert_eq!(
            limits.validate(),
            Err(JsLimitsError::UnsatisfiableBound(JsLimitKind::AstNodes))
        );
        let mut limits = generous();
        limits.input_segments = 0;
        assert_eq!(
            limits.validate(),
            Err(JsLimitsError::UnsatisfiableBound(
                JsLimitKind::InputSegments
            ))
        );
        // The shared bounds are checked by the crate that owns them.
        let mut limits = generous();
        limits.authoring.projection_depth = 0;
        assert_eq!(
            limits.validate(),
            Err(JsLimitsError::Authoring(LimitsError::UnsatisfiableBound(
                LimitKind::ProjectionDepth
            )))
        );
        // An empty scope, an empty unit, and a unit allowed no reference,
        // exclusion or parameter are meaningful bounds, not defects.
        let mut empty = generous();
        empty.units = 0;
        empty.unit_bytes = 0;
        empty.total_bytes = 0;
        empty.references = 0;
        empty.exclusions = 0;
        empty.parameter_bindings = 0;
        empty.alias_chain = 0;
        empty.tracked_origins = 0;
        empty.proof_steps = 0;
        empty.annotation_bytes = 0;
        assert!(empty.validate().is_ok());
    }

    #[test]
    fn limit_spellings_are_distinct() {
        let kinds = [
            JsLimitKind::Units,
            JsLimitKind::UnitBytes,
            JsLimitKind::TotalBytes,
            JsLimitKind::AstNodes,
            JsLimitKind::InputSegments,
            JsLimitKind::References,
            JsLimitKind::Exclusions,
            JsLimitKind::ParameterBindings,
            JsLimitKind::AliasChain,
            JsLimitKind::TrackedOrigins,
            JsLimitKind::ProofSteps,
            JsLimitKind::AnnotationBytes,
        ];
        let mut spellings: Vec<&str> = kinds.iter().map(|kind| kind.as_str()).collect();
        spellings.sort_unstable();
        spellings.dedup();
        assert_eq!(spellings.len(), kinds.len());
    }

    #[test]
    fn a_bound_on_one_literal_or_use_site_is_scoped_to_it() {
        for kind in [
            JsLimitKind::InputSegments,
            JsLimitKind::ParameterBindings,
            JsLimitKind::AliasChain,
            JsLimitKind::TrackedOrigins,
            JsLimitKind::ProofSteps,
            JsLimitKind::AnnotationBytes,
        ] {
            assert_eq!(kind.scope(), LimitScope::Declaration, "{kind:?}");
        }
        for kind in [
            JsLimitKind::Units,
            JsLimitKind::UnitBytes,
            JsLimitKind::TotalBytes,
            JsLimitKind::AstNodes,
            JsLimitKind::References,
            JsLimitKind::Exclusions,
        ] {
            assert_eq!(kind.scope(), LimitScope::Invocation, "{kind:?}");
        }
    }
}
