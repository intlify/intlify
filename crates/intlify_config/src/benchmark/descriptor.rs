// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite, owner-local descriptors for the currently measured internal calls.
//! These are not 017 artifacts, common Verification Record Envelopes, or a claim
//! that all 015 boundaries or all 026 measurement methods are implemented.

use serde::{Deserialize, Serialize};

use super::clock::ClockDescription;
use super::locale::InputFacts;
use super::operation::{Operation, Prepared};
use super::quantity::Quantity;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        Self {
            identity: operation.boundary().into(),
            revision: "0".into(),
            operation,
            phase: operation.phase().into(),
            cost: operation.cost().into(),
            metric: "wall_duration".into(),
            occurrence_policy: "single".into(),
            first_included_marker: "preserved-core-invocation".into(),
            final_included_marker: "complete-output-black-box".into(),
            included_markers: strings(&[
                "preserved-core-invocation",
                "ordinary-complete-result-construction",
                "complete-output-black-box",
            ]),
            excluded_markers: strings(&[
                "fixture-and-input-acquisition",
                "immutable-input-preparation",
                "clock-and-collector-setup",
                "operation-dispatch-and-handle-cloning",
                "input-and-invocation-black-box",
                "duration-subtraction-conversion-and-aggregation",
                "semantic-observation-and-validation",
                "reporting",
                "output-destruction-and-teardown",
            ]),
            // No workflow interval or profiler span is active in this profile.
            direct_parents: Vec::new(),
            direct_children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
            identity: "intlify-config-posix-monotonic-invocation".into(),
            revision: "0".into(),
            metric: "wall_duration".into(),
            canonical_unit: "nanosecond".into(),
            provider: "rustix".into(),
            provider_revision: "1.1.4".into(),
            clock: "posix-clock-monotonic".into(),
            observation_domain: "owner-boundary-complete-core-invocation".into(),
            excluded_domain: "ordered-owner-boundary-excluded-markers".into(),
            event_taxonomy: "one-start-and-one-end-read-per-invocation".into(),
            duration_conversion: "subtract-u128-nanosecond-timestamps-then-checked-u64".into(),
            sample_aggregation_kind: "batch_total".into(),
            repetition_aggregation: "checked-sum-of-separate-single-invocation-intervals".into(),
            concurrency_coverage: "calling-thread-synchronous-core-no-worker-summation".into(),
            overflow_behavior: "fail-complete-case-with-diagnostic-only-prefix".into(),
            // 026 fixes this class for duration. The observational profile adds
            // a prohibition of numeric decisions; it does not weaken the method.
            numeric_decision_eligibility: "qualified-runner".into(),
            clock_resolution_source: "clock-getres-reported-granularity".into(),
            included_instrumentation: strings(&[
                "preserved-indirect-call",
                "complete-output-black-box",
                "clock-read-edge-overhead",
            ]),
            estimated_overhead_subtraction: false,
            optimization_barrier: OptimizationBarrier {
                identity: "intlify-config-core-call-black-box".into(),
                revision: "0".into(),
                applicability: "required".into(),
                input_opacity: "std-hint-black-box-runtime-materialized-input".into(),
                invocation_preservation: "std-hint-black-box-function-pointer-per-repetition"
                    .into(),
                output_consumption: "std-hint-black-box-complete-ordinary-output".into(),
                output_lifetime: "owned-result-retained-through-end-read-and-observation".into(),
                input_placement: "before-start-read".into(),
                invocation_placement: "after-start-read".into(),
                output_placement: "before-end-read".into(),
                semantic_validation:
                    "after-end-read-compare-full-output-with-independent-fixture-observation".into(),
            },
        }
    }
}

/// An actual acquisition fact, not a guessed precision or part of semantic output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ClockObservation {
    provider: String,
    provider_revision: String,
    clock: String,
    resolution_nanoseconds: Quantity,
    resolution_source: String,
    conversion: String,
}

impl From<ClockDescription> for ClockObservation {
    fn from(value: ClockDescription) -> Self {
        Self {
            provider: value.provider.into(),
            provider_revision: value.provider_revision.into(),
            clock: value.clock.into(),
            resolution_nanoseconds: value.resolution_nanoseconds,
            resolution_source: value.resolution_source.into(),
            conversion: value.conversion.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[expect(
    clippy::struct_field_names,
    reason = "mirror 026's independent execution-state fields"
)]
pub(super) struct Execution {
    process_state: String,
    engine_state: String,
    initial_preparation_state: String,
    cache_state: String,
    runtime_compilation_state: String,
    managed_heap_state: String,
    scratch_reuse_state: String,
    output_buffer_state: OutputBuffer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
enum OutputBuffer {
    NotApplicable {},
    Applicable { ownership: String, reuse: String },
}

impl Execution {
    pub(super) fn prepared_core(operation: Operation) -> Self {
        let locale = operation == Operation::LocaleCanonicalization;
        Self {
            // The harness invokes prepared calls in its existing process. Its
            // compiled core and immutable preparation are retained, not rebuilt
            // between samples. None of these fields claims a warm lookup cache.
            process_state: "reused-process".into(),
            engine_state: "reused".into(),
            initial_preparation_state: "resident".into(),
            cache_state: "disabled".into(),
            runtime_compilation_state: "ahead-of-time".into(),
            managed_heap_state: "not-applicable".into(),
            scratch_reuse_state: if locale { "not-applicable" } else { "fresh" }.into(),
            output_buffer_state: if locale {
                OutputBuffer::NotApplicable {}
            } else {
                OutputBuffer::Applicable {
                    ownership: "caller-owned".into(),
                    reuse: "fresh".into(),
                }
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Descriptors {
    boundary: Boundary,
    method: Method,
    clock_observation: ClockObservation,
    execution: Execution,
    #[serde(deserialize_with = "Option::deserialize")]
    locale_input: Option<InputFacts>,
    #[serde(deserialize_with = "Option::deserialize")]
    locale_core_input: Option<super::locale_core::InputFacts>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DescriptorIssue {
    BoundaryMismatch,
    MethodMismatch,
    ClockBindingMismatch,
    InvalidClockObservation,
    ExecutionMismatch,
    LocaleInputBindingMismatch,
    LocaleCoreInputBindingMismatch,
}

impl Descriptors {
    /// Actual preparation and clock acquisition supply the validation inputs.
    /// Decoded record fields must never manufacture their own expected binding.
    pub(super) fn for_acquisition(prepared: &Prepared, clock: ClockDescription) -> Self {
        let operation = prepared.operation();
        Self {
            boundary: Boundary::for_operation(operation),
            method: Method::monotonic_invocation(),
            clock_observation: clock.into(),
            execution: Execution::prepared_core(operation),
            locale_input: match prepared {
                Prepared::Locale { core, .. } => Some(InputFacts::observe(core)),
                Prepared::LocaleCore(core) => Some(InputFacts::observe(&core.provider)),
                _ => None,
            },
            locale_core_input: match prepared {
                Prepared::LocaleCore(core) => Some(super::locale_core::InputFacts::observe(core)),
                _ => None,
            },
        }
    }

    pub(super) fn validate(
        &self,
        prepared: &Prepared,
        acquisition: ClockDescription,
    ) -> Vec<DescriptorIssue> {
        let mut issues = Vec::new();
        let operation = prepared.operation();
        if self.boundary != Boundary::for_operation(operation) {
            issues.push(DescriptorIssue::BoundaryMismatch);
        }
        let method = Method::monotonic_invocation();
        if self.method != method {
            issues.push(DescriptorIssue::MethodMismatch);
        }
        if self.clock_observation != ClockObservation::from(acquisition) {
            issues.push(DescriptorIssue::ClockBindingMismatch);
        }
        let clock = &self.clock_observation;
        if clock.provider != method.provider
            || clock.provider_revision != method.provider_revision
            || clock.clock != method.clock
            || clock.resolution_nanoseconds.get() == 0
            || clock.resolution_source != method.clock_resolution_source
            || clock.conversion != "exact-integer-nanoseconds"
        {
            issues.push(DescriptorIssue::InvalidClockObservation);
        }
        if self.execution != Execution::prepared_core(operation) {
            issues.push(DescriptorIssue::ExecutionMismatch);
        }
        let expected_locale = match prepared {
            Prepared::Locale { core, .. } => Some(InputFacts::observe(core)),
            Prepared::LocaleCore(core) => Some(InputFacts::observe(&core.provider)),
            _ => None,
        };
        if self.locale_input != expected_locale {
            issues.push(DescriptorIssue::LocaleInputBindingMismatch);
        }
        let expected_core = match prepared {
            Prepared::LocaleCore(core) => Some(super::locale_core::InputFacts::observe(core)),
            _ => None,
        };
        if self.locale_core_input != expected_core {
            issues.push(DescriptorIssue::LocaleCoreInputBindingMismatch);
        }
        issues
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[cfg(test)]
mod tests;
