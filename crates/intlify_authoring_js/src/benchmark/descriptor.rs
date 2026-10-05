// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What this owner declares about how it measured.
//!
//! These descriptors are not 017 artifacts or common record envelopes. They
//! state which interval was opened, what it contained, and what it excluded,
//! so a reader can tell what a duration is a duration *of*.
//!
//! Anything the harness does inside the interval is declared here rather than
//! subtracted from the result. That includes the workspace it lends to source
//! discovery and the serialization of the sealed inventory: both are inside,
//! because a caller pays for both.

use intlify_measurement::environment::ClockObservation;
use intlify_measurement::execution::{Execution, OutputBuffer};
use intlify_measurement::owner::ObservedDescriptors;
use serde::{Deserialize, Serialize};

use super::operation::Operation;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

/// The exact interval one case opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Boundary {
    identity: String,
    revision: String,
    operation: Operation,
    phase: String,
    cost: String,
    metric: String,
    occurrence_policy: String,
    first_included_marker: String,
    final_included_marker: String,
    included_markers: Vec<String>,
    excluded_markers: Vec<String>,
    direct_parents: Vec<String>,
    direct_children: Vec<String>,
}

impl Boundary {
    pub(super) fn for_operation(operation: Operation) -> Self {
        let (included, excluded): (&[&str], &[&str]) = match operation {
            Operation::ParseAndClassify => (
                &[
                    "preserved-operation-invocation",
                    "lent-scratch-borrow",
                    "ordinary-complete-result-construction",
                    "complete-output-black-box",
                ],
                &[
                    "fixture-and-input-preparation",
                    "unit-admission-and-snapshot-digest-check",
                    "phase-1-declaration-resolution",
                    "expectation-establishment",
                    "clock-acquisition",
                    "warmup-invocations",
                    "duration-conversion-and-aggregation",
                    "observation-encoding-and-comparison",
                    "logical-work-counting",
                    "record-assembly-and-reporting",
                    "output-destruction-and-teardown",
                ],
            ),
            Operation::InventoryAssembly => (
                &[
                    "preserved-operation-invocation",
                    "ordinary-complete-result-construction",
                    "artifact-serialization",
                    "complete-output-black-box",
                ],
                &[
                    "fixture-and-input-preparation",
                    "unit-admission-and-snapshot-digest-check",
                    "prior-stage-establishment",
                    "expectation-establishment",
                    "clock-acquisition",
                    "warmup-invocations",
                    "duration-conversion-and-aggregation",
                    "observation-encoding-and-comparison",
                    "logical-work-counting",
                    "record-assembly-and-reporting",
                    "output-destruction-and-teardown",
                ],
            ),
        };
        Self {
            identity: operation.boundary().into(),
            revision: "0".into(),
            operation,
            phase: operation.phase().into(),
            cost: operation.cost().into(),
            metric: "wall_duration".into(),
            occurrence_policy: "single".into(),
            first_included_marker: "preserved-operation-invocation".into(),
            final_included_marker: "complete-output-black-box".into(),
            included_markers: strings(included),
            excluded_markers: strings(excluded),
            // No workflow interval or profiler span is active in this profile.
            direct_parents: Vec::new(),
            direct_children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OptimizationBarrier {
    identity: String,
    revision: String,
    applicability: String,
    input_opacity: String,
    invocation_preservation: String,
    output_consumption: String,
    output_lifetime: String,
    input_placement: String,
    invocation_placement: String,
    output_placement: String,
    semantic_validation: String,
}

/// How a duration is read, converted, and aggregated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Method {
    identity: String,
    revision: String,
    metric: String,
    canonical_unit: String,
    provider: String,
    provider_revision: String,
    clock: String,
    observation_domain: String,
    excluded_domain: String,
    event_taxonomy: String,
    duration_conversion: String,
    sample_aggregation_kind: String,
    repetition_aggregation: String,
    concurrency_coverage: String,
    overflow_behavior: String,
    numeric_decision_eligibility: String,
    clock_resolution_source: String,
    included_instrumentation: Vec<String>,
    estimated_overhead_subtraction: bool,
    optimization_barrier: OptimizationBarrier,
}

impl Method {
    pub(super) fn monotonic_invocation() -> Self {
        Self {
            identity: "intlify-authoring-js-posix-monotonic-invocation".into(),
            revision: "0".into(),
            metric: "wall_duration".into(),
            canonical_unit: "nanosecond".into(),
            provider: "rustix".into(),
            // The clock description the shared acquisition records names the
            // same revision, and the two have to agree.
            provider_revision: "1.1.4".into(),
            clock: "posix-clock-monotonic".into(),
            observation_domain: "owner-boundary-complete-operation-invocation".into(),
            excluded_domain: "ordered-owner-boundary-excluded-markers".into(),
            event_taxonomy: "one-start-and-one-end-read-per-invocation".into(),
            duration_conversion: "subtract-u128-nanosecond-timestamps-then-checked-u64".into(),
            sample_aggregation_kind: "batch_total".into(),
            repetition_aggregation: "checked-sum-of-separate-single-invocation-intervals".into(),
            concurrency_coverage: "calling-thread-synchronous-operation-no-worker-summation".into(),
            overflow_behavior: "fail-complete-case-without-partial-sample".into(),
            // 026 fixes this class for duration. The observational profile adds
            // a prohibition of numeric decisions; it does not weaken the method.
            numeric_decision_eligibility: "qualified-runner".into(),
            clock_resolution_source: "clock-getres-reported-granularity".into(),
            included_instrumentation: strings(&[
                "preserved-indirect-call",
                "complete-output-black-box",
                "clock-read-edge-overhead",
                "lent-scratch-borrow-check",
            ]),
            estimated_overhead_subtraction: false,
            optimization_barrier: OptimizationBarrier {
                identity: "intlify-authoring-js-operation-black-box".into(),
                revision: "0".into(),
                applicability: "required".into(),
                input_opacity: "std-hint-black-box-prepared-input".into(),
                invocation_preservation: "std-hint-black-box-function-pointer-per-repetition"
                    .into(),
                output_consumption: "std-hint-black-box-complete-ordinary-output".into(),
                output_lifetime: "owned-result-retained-through-end-read".into(),
                input_placement: "before-start-read".into(),
                invocation_placement: "after-start-read".into(),
                output_placement: "before-end-read".into(),
                semantic_validation:
                    "after-end-read-compare-with-independently-established-expectation".into(),
            },
        }
    }
}

/// The execution state one operation's prepared core runs in.
pub(super) fn execution_state(operation: Operation) -> Execution {
    // Source discovery borrows the case's workspace, whose arena keeps its
    // capacity between invocations. Assembly lends nothing. Neither writes
    // into a caller-owned output buffer: both return owned results.
    let lends_scratch = matches!(operation, Operation::ParseAndClassify);
    Execution {
        // The harness invokes prepared operations in its existing process. Its
        // compiled core and immutable preparation are retained, not rebuilt
        // between samples. None of these fields claims a warm lookup cache.
        process_state: "reused-process".into(),
        engine_state: "reused".into(),
        initial_preparation_state: "resident".into(),
        cache_state: "disabled".into(),
        runtime_compilation_state: "ahead-of-time".into(),
        managed_heap_state: "not-applicable".into(),
        scratch_reuse_state: if lends_scratch {
            "reused"
        } else {
            "not-applicable"
        }
        .into(),
        output_buffer_state: OutputBuffer::NotApplicable {},
    }
}

/// Everything this owner observed about how one case was measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Descriptors {
    boundary: Boundary,
    method: Method,
    clock_observation: ClockObservation,
    execution: Execution,
}

impl Descriptors {
    /// Record the descriptors of one acquisition.
    ///
    /// The clock observation comes from the provider that was actually
    /// acquired, never from the method description that names it.
    pub(super) fn for_acquisition(operation: Operation, clock: ClockObservation) -> Self {
        Self {
            boundary: Boundary::for_operation(operation),
            method: Method::monotonic_invocation(),
            clock_observation: clock,
            execution: execution_state(operation),
        }
    }
}

impl ObservedDescriptors for Descriptors {
    fn execution(&self) -> &Execution {
        &self.execution
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boundary_names_its_operation_and_keeps_the_two_marker_sets_disjoint() {
        for operation in Operation::ALL {
            let boundary = Boundary::for_operation(operation);
            assert_eq!(boundary.phase, operation.phase());
            assert_eq!(boundary.cost, operation.cost());
            assert!(boundary
                .included_markers
                .contains(&boundary.first_included_marker));
            assert!(boundary
                .included_markers
                .contains(&boundary.final_included_marker));
            for marker in &boundary.excluded_markers {
                assert!(
                    !boundary.included_markers.contains(marker),
                    "{marker} is both included and excluded"
                );
            }
        }
    }

    #[test]
    fn source_discovery_excludes_the_phase_1_semantics_it_stops_before() {
        // The interval ends where declarations reach the shared crate, and
        // admission's digest check happens before it opens.
        let boundary = Boundary::for_operation(Operation::ParseAndClassify);
        for excluded in [
            "phase-1-declaration-resolution",
            "unit-admission-and-snapshot-digest-check",
            "expectation-establishment",
            "warmup-invocations",
        ] {
            assert!(
                boundary.excluded_markers.contains(&excluded.to_owned()),
                "{excluded} is not declared excluded"
            );
        }
        assert!(boundary
            .included_markers
            .contains(&"lent-scratch-borrow".to_owned()));
    }

    #[test]
    fn assembly_includes_the_bytes_it_hands_over_and_lends_no_scratch() {
        let boundary = Boundary::for_operation(Operation::InventoryAssembly);
        assert!(boundary
            .included_markers
            .contains(&"artifact-serialization".to_owned()));
        assert!(boundary
            .excluded_markers
            .contains(&"prior-stage-establishment".to_owned()));
        assert_eq!(
            execution_state(Operation::InventoryAssembly).scratch_reuse_state,
            "not-applicable"
        );
        assert_eq!(
            execution_state(Operation::ParseAndClassify).scratch_reuse_state,
            "reused"
        );
    }
}
