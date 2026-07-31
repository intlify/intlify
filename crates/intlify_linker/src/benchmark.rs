// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Non-product observation surface for the dedicated message benchmark.
//!
//! This module exists only with the non-default `benchmark` Cargo feature.
//! It exposes no partial-link constructor and accepts only an ordinary checked
//! [`LinkRequest`]. The measured execution shares the exact semantic machinery
//! used by [`crate::link`].

use std::time::Duration;

use crate::{LinkOperationalError, LinkOutcome, LinkRequest};

/// One semantic-link stage observable by the dedicated benchmark runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BenchmarkLinkStage {
    /// Canonical definition, candidate, and reference index construction.
    SemanticIndexConstruction,
    /// Selector expansion and locale-independent reference resolution.
    SelectorExpansionAndReferenceResolution,
    /// Root reachability and duplicate placement.
    ReachabilityAndPlacement,
    /// Finding, plan, and immutable outcome materialization.
    FindingAndPlanMaterialization,
}

impl BenchmarkLinkStage {
    /// Return the exact design-owned benchmark cost token.
    #[must_use]
    pub const fn cost(self) -> &'static str {
        match self {
            Self::SemanticIndexConstruction => "semantic_index_construction",
            Self::SelectorExpansionAndReferenceResolution => {
                "selector_expansion_and_reference_resolution"
            }
            Self::ReachabilityAndPlacement => "reachability_and_placement",
            Self::FindingAndPlanMaterialization => "finding_and_plan_materialization",
        }
    }

    /// Return the exact stable measurement-boundary identifier.
    #[must_use]
    pub const fn boundary_id(self) -> &'static str {
        match self {
            Self::SemanticIndexConstruction => "link_semantic_index_construction",
            Self::SelectorExpansionAndReferenceResolution => "link_selector_resolution",
            Self::ReachabilityAndPlacement => "link_reachability_placement",
            Self::FindingAndPlanMaterialization => "link_finding_plan_materialization",
        }
    }
}

/// One completed non-overlapping semantic-stage observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkLinkStageMeasurement {
    stage: BenchmarkLinkStage,
    elapsed: Duration,
    checksum: u32,
}

impl BenchmarkLinkStageMeasurement {
    pub(crate) const fn new(stage: BenchmarkLinkStage, elapsed: Duration, checksum: u32) -> Self {
        Self {
            stage,
            elapsed,
            checksum,
        }
    }

    /// Return the exact observed semantic stage.
    #[must_use]
    pub const fn stage(&self) -> BenchmarkLinkStage {
        self.stage
    }

    /// Return the unadjusted monotonic-clock duration.
    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Return the deterministic stage observation checksum.
    #[must_use]
    pub const fn checksum(&self) -> u32 {
        self.checksum
    }
}

/// Complete result of one benchmark-observed ordinary semantic link.
#[derive(Debug)]
pub struct BenchmarkLinkExecution {
    outcome: LinkOutcome,
    stages: Box<[BenchmarkLinkStageMeasurement]>,
    observation_overhead: Duration,
}

impl BenchmarkLinkExecution {
    pub(crate) fn new(
        outcome: LinkOutcome,
        stages: Vec<BenchmarkLinkStageMeasurement>,
        observation_overhead: Duration,
    ) -> Self {
        Self {
            outcome,
            stages: stages.into_boxed_slice(),
            observation_overhead,
        }
    }

    /// Return the ordinary immutable link result.
    #[must_use]
    pub const fn outcome(&self) -> &LinkOutcome {
        &self.outcome
    }

    /// Return the four semantic stages in their exact execution order.
    #[must_use]
    pub const fn stages(&self) -> &[BenchmarkLinkStageMeasurement] {
        &self.stages
    }

    /// Return benchmark-only checksum and recorder time outside stage intervals.
    #[must_use]
    pub const fn observation_overhead(&self) -> Duration {
        self.observation_overhead
    }

    /// Consume the observation and return its ordinary link result.
    #[must_use]
    pub fn into_outcome(self) -> LinkOutcome {
        self.outcome
    }
}

/// Observe one ordinary semantic link through the dedicated benchmark path.
///
/// This function neither accepts nor returns a caller-authored intermediate
/// state. Request admission remains the ordinary [`LinkRequest`] boundary.
#[doc(hidden)]
pub fn benchmark_link(
    request: &LinkRequest<'_>,
) -> Result<BenchmarkLinkExecution, LinkOperationalError> {
    crate::link::benchmark_link(request)
}
