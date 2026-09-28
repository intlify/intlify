// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Explicit finite limits for one Producer invocation.
//!
//! Every bound is caller-supplied, as in `intlify_authoring`. There is no
//! `Default`, so adding a bound is a compile error at each construction site
//! rather than a hidden value that silently admits unbounded work.
//!
//! Exhausting a bound is an operational failure, as it is in
//! `intlify_authoring`: a result is never produced from work that stopped
//! short.

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
        }
    }
}

/// Complete failure of a limit construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsLimitsError {
    /// A bound was supplied that no invocation could satisfy.
    UnsatisfiableBound(JsLimitKind),
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
}

impl JsAuthoringLimits {
    /// Check that every bound can be satisfied by some invocation.
    ///
    /// A zero bound is admitted where it has a meaning: a scope with no units
    /// and a unit with no bytes are both real. Every parsed unit has at least
    /// its program node, and every decoded literal has at least one segment,
    /// even an empty one, so those two bounds have to be positive.
    pub fn validate(self) -> Result<Self, JsLimitsError> {
        for (value, kind) in [
            (self.ast_nodes, JsLimitKind::AstNodes),
            (self.input_segments, JsLimitKind::InputSegments),
        ] {
            if value == 0 {
                return Err(JsLimitsError::UnsatisfiableBound(kind));
            }
        }
        Ok(self)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn generous() -> JsAuthoringLimits {
        JsAuthoringLimits {
            units: 64,
            unit_bytes: 64 * 1024,
            total_bytes: 1024 * 1024,
            ast_nodes: 64 * 1024,
            input_segments: 1024,
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
        // An empty scope and an empty unit are meaningful bounds, not defects.
        let mut empty = generous();
        empty.units = 0;
        empty.unit_bytes = 0;
        empty.total_bytes = 0;
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
        ];
        let mut spellings: Vec<&str> = kinds.iter().map(|kind| kind.as_str()).collect();
        spellings.sort_unstable();
        spellings.dedup();
        assert_eq!(spellings.len(), kinds.len());
    }
}
