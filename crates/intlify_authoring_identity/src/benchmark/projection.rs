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

use intlify_authoring::{Token, VersionedIdentity};
use intlify_measurement::execution::Execution;
use intlify_measurement::measurement::{Aggregation, Category, Metric, OperationClass, Surface};
use intlify_measurement::owner_run::run::{subject, Expected};
use intlify_measurement::plan::{CaseProjection as CommonProjection, Subject};
use intlify_shared_json::quantity::Quantity;
use serde::{Deserialize, Serialize};

use super::cases::{Chain, Edge, Input, Limit, Planning, Prepared, CATALOG_DECLARATIONS};
use super::descriptor::{execution_state, Boundary, Method};
use super::operation::LogicalWork;
use super::owner::{AuthoringIdentityReconciliation, LABELS};

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
    limit: Limit,
    edge: Edge,
    value: Quantity,
}

/// What a fixture reads, beyond its name.
///
/// The container's `rename_all` names the variants, not their fields, so a
/// field of more than one word is named here like every other in the case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Reads {
    /// One reconciliation of an inventory against a base.
    Plan {
        planning: Planning,
        #[serde(rename = "catalogDeclarations")]
        catalog_declarations: Quantity,
        #[serde(deserialize_with = "Option::deserialize")]
        bound: Option<Bound>,
    },
    /// One chain verified from its head back to its genesis.
    Replay {
        chain: Chain,
        #[serde(rename = "catalogDeclarations")]
        catalog_declarations: Quantity,
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
            limit: bound.limit,
            edge: bound.edge,
            value: Quantity::new(bound.value),
        });
        let catalog_declarations = Quantity::new(CATALOG_DECLARATIONS);
        let reads = match fixture.input {
            Input::Plan { planning, .. } => Reads::Plan {
                planning,
                catalog_declarations,
                bound,
            },
            Input::Replay { chain, .. } => Reads::Replay {
                chain,
                catalog_declarations,
                bound,
            },
        };
        Self {
            owner_identity: Token::literal(LABELS.owner),
            owner_result_schema_revision: Token::literal(RESULT_SCHEMA_REVISION),
            owner_benchmark_profile_revision: Token::literal(LABELS.profile.revision),
            owner_phase: operation.phase().into(),
            owner_cost: operation.cost().into(),
            category: Category::Value,
            operation_class: OperationClass::Value,
            performance_surface: Surface::Value,
            interval_boundary: Boundary::for_operation(operation),
            fixture: VersionedIdentity::literal("intlify-authoring-identity-minimum-fixtures", "0"),
            variant: Variant {
                fixture: Token::new(fixture.name).expect("registered fixture name"),
                expected: fixture.expected,
                reads,
            },
            scale: Scale::Value,
            verification_subject: subject::<AuthoringIdentityReconciliation>(),
            execution_model: ExecutionModel::Value,
            execution_state: execution_state(operation),
            concurrency: Concurrency::Value,
            workload: prepared.expected().work.clone(),
            metric: Metric::Value,
            measurement_method: Method::monotonic_invocation(),
            sample_aggregation: Aggregation::Value,
            measurement_profile_revision: Token::literal(LABELS.profile.revision),
        }
    }
}

#[cfg(test)]
mod tests {
    use intlify_measurement::plan::case_identity;

    use super::*;
    use crate::benchmark::cases::{prepare, testing, FIXTURES};

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
        assert_eq!(value["ownerPhase"], "identity_reconciliation");
        assert_eq!(value["ownerCost"], "association_planning");
        assert_eq!(value["variant"]["fixture"], "first-allocation");
        assert_eq!(value["variant"]["reads"]["kind"], "plan");
        assert_eq!(value["variant"]["reads"]["bound"], serde_json::Value::Null);
    }

    #[test]
    fn every_fixture_names_what_it_reads_and_the_bound_it_is_measured_at() {
        use serde_json::{json, Value};

        let bound = |limit: &str, edge: &str, value: &str| json!({ "limit": limit, "edge": edge, "value": value });
        let plan = |planning: &str, bound: Value| {
            json!({
                "kind": "plan",
                "planning": planning,
                "catalogDeclarations": "128",
                "bound": bound
            })
        };
        let replay = |chain: &str, bound: Value| {
            json!({
                "kind": "replay",
                "chain": chain,
                "catalogDeclarations": "128",
                "bound": bound
            })
        };
        for (name, reads) in [
            ("first-allocation", plan("first-allocation", Value::Null)),
            (
                "catalog-allocation",
                plan("catalog-allocation", Value::Null),
            ),
            ("catalog-retained", plan("catalog-retained", Value::Null)),
            ("equal-text-copy", plan("equal-text-copy", Value::Null)),
            ("ambiguous-copy", plan("ambiguous-copy", Value::Null)),
            ("complete-deletion", plan("complete-deletion", Value::Null)),
            ("partial-deletion", plan("partial-deletion", Value::Null)),
            (
                "candidates-exact",
                plan("first-allocation", bound("candidates", "exact", "3")),
            ),
            (
                "candidates-first-over",
                plan("first-allocation", bound("candidates", "first-over", "2")),
            ),
            (
                "diagnostics-exact",
                plan("ambiguous-copy", bound("diagnostics", "exact", "5")),
            ),
            (
                "diagnostics-first-over",
                plan("ambiguous-copy", bound("diagnostics", "first-over", "4")),
            ),
            ("small-chain", replay("small", Value::Null)),
            ("catalog-chain", replay("catalog", Value::Null)),
            (
                "missing-update",
                replay("small-missing-update", Value::Null),
            ),
            (
                "history-steps-exact",
                replay("small", bound("history-steps", "exact", "2")),
            ),
            (
                "history-steps-first-over",
                replay("small", bound("history-steps", "first-over", "1")),
            ),
        ] {
            let value = serde_json::to_value(CaseProjection::of(&testing::prepared(name))).unwrap();
            assert_eq!(value["variant"]["reads"], reads, "{name}");
        }
        // A bound is part of what a case measures, down to the path it takes.
        let value = serde_json::to_value(CaseProjection::of(&testing::prepared(
            "history-steps-first-over",
        )))
        .unwrap();
        assert_eq!(value["variant"]["expected"], "operational-failure");
        assert_eq!(value["ownerCost"], "registry_replay");
    }

    #[test]
    fn a_case_names_its_owner_its_profile_and_what_it_measured() {
        for name in ["first-allocation", "small-chain"] {
            let prepared = testing::prepared(name);
            let operation = prepared.fixture.operation;
            let projection = CaseProjection::of(&prepared);
            // The report groups its rows under these two.
            assert_eq!(CommonProjection::phase(&projection), operation.phase());
            assert_eq!(CommonProjection::cost(&projection), operation.cost());
            let value = serde_json::to_value(&projection).unwrap();
            assert_eq!(value["ownerIdentity"], LABELS.owner);
            assert_eq!(value["ownerResultSchemaRevision"], RESULT_SCHEMA_REVISION);
            assert_eq!(
                value["ownerBenchmarkProfileRevision"],
                LABELS.profile.revision
            );
            assert_eq!(value["measurementProfileRevision"], LABELS.profile.revision);
            assert_eq!(
                value["fixture"],
                serde_json::json!({
                    "identity": "intlify-authoring-identity-minimum-fixtures",
                    "revision": "0"
                })
            );
            assert_eq!(
                value["intervalBoundary"],
                serde_json::to_value(Boundary::for_operation(operation)).unwrap(),
                "{name}"
            );
            assert_eq!(
                value["executionState"],
                serde_json::to_value(execution_state(operation)).unwrap(),
                "{name}"
            );
            assert_eq!(
                value["workload"],
                serde_json::to_value(&prepared.expected().work).unwrap(),
                "{name}"
            );
        }
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
