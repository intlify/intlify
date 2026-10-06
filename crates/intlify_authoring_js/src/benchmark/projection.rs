// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Measurement Case projection this owner produces.
//!
//! Exactly the applicable 026 case dimensions, and nothing else. No source
//! path, build revision, clock observation, run or record identity, expected
//! checksum, or sample value appears here: the identity of a case must not
//! change because it ran on another machine, at another time, or after a
//! rebuild. A bound the fixture is measured at is part of what it measures,
//! so its kind, its edge and its value are.

use intlify_authoring::Completeness;
use intlify_measurement::execution::Execution;
use intlify_measurement::measurement::{Aggregation, Category, Metric, OperationClass, Surface};
use intlify_measurement::owner_run::run::{subject, Expected};
use intlify_measurement::plan::{CaseProjection as CommonProjection, Subject};
use intlify_shared_json::quantity::Quantity;
use intlify_shared_json::token::{Token, VersionedIdentity};
use serde::{Deserialize, Serialize};

use super::cases::{Edge, Input, Prepared, Profile};
use super::descriptor::{execution_state, Boundary, Method};
use super::operation::LogicalWork;
use super::owner::AuthoringJsDiscovery;

macro_rules! literal {
    ($name:ident, $value:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
        )]
        enum $name {
            #[serde(rename = $value)]
            Value,
        }
    };
}

literal!(
    Scale,
    "fixed-owner-recipe",
    "The workload size is a fixed recipe, named by its fixture."
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

/// The revision of the owner result schema a case is recorded under.
///
/// It is the revision the result codec names, so a case recorded in another
/// result schema is a different case.
pub(super) const RESULT_SCHEMA_REVISION: &str = "1";

/// A bound a fixture is measured at, as the case names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Bound {
    limit: Token,
    edge: Edge,
    value: Quantity,
}

/// What a fixture reads, beyond its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Reads {
    /// One module-goal unit, read under a profile.
    Unit {
        profile: Profile,
        grammar: VersionedIdentity,
        #[serde(deserialize_with = "Option::deserialize")]
        bound: Option<Bound>,
    },
    /// The fixed unit set, assembled under a declared completeness.
    Inventory {
        completeness: Completeness,
        units: Quantity,
        #[serde(deserialize_with = "Option::deserialize")]
        bound: Option<Bound>,
    },
}

/// The owner fragment that names which fixture a case ran.
///
/// The fixture name and its declared path are retained rather than reduced to
/// a checksum, so two cases that differ only in which path they exercise are
/// visibly different rather than merely unequal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Variant {
    fixture: Token,
    expected: Expected,
    reads: Reads,
}

/// The complete projection of one measured case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaseProjection {
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
        let fixture = prepared.fixture;
        let operation = fixture.operation;
        let bound = prepared.bound.map(|bound| Bound {
            limit: Token::literal(bound.kind.as_str()),
            edge: bound.edge,
            value: Quantity::new(bound.value),
        });
        let reads = match fixture.input {
            Input::Unit { profile, .. } => Reads::Unit {
                profile,
                grammar: crate::Grammar::JsModule.identity(),
                bound,
            },
            Input::Inventory { completeness, .. } => Reads::Inventory {
                completeness,
                units: Quantity::new(super::cases::INVENTORY_UNIT_COUNT),
                bound,
            },
        };
        Self {
            owner_identity: Token::literal("intlify-authoring-js"),
            owner_result_schema_revision: Token::literal(RESULT_SCHEMA_REVISION),
            owner_benchmark_profile_revision: Token::literal("0"),
            owner_phase: operation.phase().into(),
            owner_cost: operation.cost().into(),
            category: Category::Value,
            operation_class: OperationClass::Value,
            performance_surface: Surface::Value,
            interval_boundary: Boundary::for_operation(operation),
            fixture: VersionedIdentity::literal("intlify-authoring-js-minimum-fixtures", "0"),
            variant: Variant {
                fixture: Token::new(fixture.name).expect("registered fixture name"),
                expected: fixture.expected,
                reads,
            },
            scale: Scale::Value,
            verification_subject: subject::<AuthoringJsDiscovery>(),
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
    use intlify_measurement::plan::case_identity;

    use super::*;
    use crate::benchmark::cases::{prepare, FIXTURES};

    #[test]
    fn every_fixture_projects_to_a_distinct_case_identity() {
        let mut seen = std::collections::BTreeSet::new();
        for fixture in FIXTURES {
            let identity = case_identity(&CaseProjection::of(&prepare(fixture).unwrap())).unwrap();
            assert!(seen.insert(identity), "{} collides", fixture.name);
        }
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
        let value =
            serde_json::to_value(CaseProjection::of(&prepare(FIXTURES[0]).unwrap())).unwrap();
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
        assert_eq!(value["ownerPhase"], "source_discovery");
        assert_eq!(value["variant"]["fixture"], "no-candidates");
        assert_eq!(value["variant"]["reads"]["kind"], "unit");
        assert_eq!(value["variant"]["reads"]["bound"], serde_json::Value::Null);
    }

    #[test]
    fn a_bound_is_part_of_the_case_it_measures() {
        let fixture = FIXTURES
            .into_iter()
            .find(|fixture| fixture.name == "ast-nodes-first-over")
            .unwrap();
        let value = serde_json::to_value(CaseProjection::of(&prepare(fixture).unwrap())).unwrap();
        let bound = &value["variant"]["reads"]["bound"];
        assert_eq!(bound["limit"], "ast-nodes");
        assert_eq!(bound["edge"], "first-over");
        assert_eq!(value["variant"]["expected"], "operational-failure");
    }

    #[test]
    fn the_projection_names_the_result_schema_revision_the_run_records() {
        let codec = super::super::owner::LABELS.result_codec;
        assert_eq!(
            codec.rsplit_once('/').map(|(_, revision)| revision),
            Some(RESULT_SCHEMA_REVISION)
        );
    }
}
