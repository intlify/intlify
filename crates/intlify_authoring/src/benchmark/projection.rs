// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Measurement Case projection this owner produces.
//!
//! Exactly the applicable 026 case dimensions, and nothing else. No source
//! path, build revision, clock observation, run or record identity, expected
//! checksum, or sample value appears here: the identity of a case must not
//! change because it ran on another machine, at another time, or after a
//! rebuild.
//!
//! Artifact, Target, Release, service-profile and memory-domain dimensions do
//! not apply to these four in-process boundaries, so they are absent rather
//! than guessed.

use intlify_measurement::execution::Execution;
use intlify_measurement::measurement::{Aggregation, Category, Metric, OperationClass, Surface};
use intlify_measurement::plan::{CaseProjection as CommonProjection, Subject, SubjectKind};
use intlify_shared_json::token::{Token, VersionedIdentity};
use serde::{Deserialize, Serialize};

use super::cases::{Expected, Prepared};
use super::descriptor::{execution_state, Boundary, Method};
use super::operation::LogicalWork;

macro_rules! literal {
    ($name:ident, $value:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
        )]
        pub(super) enum $name {
            #[serde(rename = $value)]
            Value,
        }
    };
}

literal!(
    Scale,
    "fixed-owner-recipe",
    "The workload size is a fixed recipe, not a parameter."
);
literal!(
    ExecutionModel,
    "same-process-synchronous-ordinary-core",
    "The operation runs in this process, synchronously."
);
literal!(
    Concurrency,
    "sequential-cases-calling-thread-no-workers",
    "Cases run one after another on the calling thread."
);

/// The owner fragment that names which fixture a case ran.
///
/// The fixture name and its declared path are retained rather than reduced to
/// a checksum, so two cases that differ only in which path they exercise are
/// visibly different rather than merely unequal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Variant {
    fixture: Token,
    expected: Expected,
}

/// What exactly this owner measures.
pub(super) fn subject() -> Subject {
    Subject {
        kind: SubjectKind::Value,
        identity: Token::literal("intlify-authoring-phase1-semantics"),
    }
}

/// The measurement profile this owner runs under.
pub(super) fn profile() -> VersionedIdentity {
    VersionedIdentity::literal("intlify-authoring-minimum-smoke", "0")
}

/// The complete projection of one measured case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CaseProjection {
    owner_identity: Token,
    owner_result_schema_revision: Token,
    owner_benchmark_profile_revision: Token,
    owner_phase: String,
    owner_cost: String,
    category: Category,
    operation_class: OperationClass,
    performance_surface: Surface,
    interval_boundary: Boundary,
    fixture: VersionedIdentity,
    variant: Variant,
    scale: Scale,
    verification_subject: Subject,
    execution_model: ExecutionModel,
    execution_state: Execution,
    concurrency: Concurrency,
    // Complete, closed, versioned owner workload fragment; not its checksum.
    workload: LogicalWork,
    metric: Metric,
    measurement_method: Method,
    sample_aggregation: Aggregation,
    measurement_profile_revision: Token,
}

impl CommonProjection for CaseProjection {
    fn phase(&self) -> &str {
        &self.owner_phase
    }
    fn cost(&self) -> &str {
        &self.owner_cost
    }
}

impl CaseProjection {
    /// Project one prepared case.
    ///
    /// The workload is the logical work the expectation established, so the
    /// identity of a case is fixed before any sample is taken.
    pub(super) fn of(prepared: &Prepared) -> Self {
        let operation = prepared.fixture.operation;
        Self {
            owner_identity: Token::literal("intlify-authoring"),
            owner_result_schema_revision: Token::literal("1"),
            owner_benchmark_profile_revision: Token::literal("0"),
            owner_phase: operation.phase().into(),
            owner_cost: operation.cost().into(),
            category: Category::Value,
            operation_class: OperationClass::Value,
            performance_surface: Surface::Value,
            interval_boundary: Boundary::for_operation(operation),
            fixture: VersionedIdentity::literal("intlify-authoring-minimum-fixtures", "0"),
            variant: Variant {
                fixture: Token::new(prepared.fixture.name).expect("registered fixture name"),
                expected: prepared.fixture.expected,
            },
            scale: Scale::Value,
            verification_subject: subject(),
            execution_model: ExecutionModel::Value,
            execution_state: execution_state(operation),
            concurrency: Concurrency::Value,
            workload: prepared.expected().work.clone(),
            metric: Metric::Value,
            measurement_method: Method::monotonic_invocation(),
            sample_aggregation: Aggregation::Value,
            measurement_profile_revision: Token::literal("0"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::cases::{prepare, FIXTURES};
    use intlify_measurement::plan::case_identity;

    #[test]
    fn every_fixture_projects_to_a_distinct_case_identity() {
        let mut seen = std::collections::BTreeSet::new();
        for fixture in FIXTURES {
            let prepared = prepare(fixture).unwrap();
            let identity = case_identity(&CaseProjection::of(&prepared)).unwrap();
            assert!(seen.insert(identity), "{} collides", fixture.name);
        }
        assert_eq!(seen.len(), FIXTURES.len());
    }

    #[test]
    fn a_case_identity_does_not_change_between_two_preparations() {
        // Nothing in a projection comes from the machine, the clock, or the
        // run, so preparing the same fixture twice yields the same case.
        for fixture in FIXTURES {
            let first = case_identity(&CaseProjection::of(&prepare(fixture).unwrap())).unwrap();
            let second = case_identity(&CaseProjection::of(&prepare(fixture).unwrap())).unwrap();
            assert_eq!(first, second, "{}", fixture.name);
        }
    }

    #[test]
    fn a_projection_carries_no_run_clock_or_sample_dimension() {
        let prepared = prepare(FIXTURES[0]).unwrap();
        let value = serde_json::to_value(CaseProjection::of(&prepared)).unwrap();
        let text = value.to_string();
        for forbidden in [
            "clockObservation",
            "resolutionNanoseconds",
            "measurementRun",
            "recordIdentity",
            "aggregateQuantity",
            "semanticObservation",
        ] {
            assert!(
                !text.contains(forbidden),
                "{forbidden} is in the projection"
            );
        }
        // The dimensions that do belong are all present.
        assert_eq!(value["ownerPhase"], "message_analysis");
        assert_eq!(value["variant"]["fixture"], "plain");
    }
}
