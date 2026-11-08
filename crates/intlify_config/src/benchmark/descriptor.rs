// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite, owner-local descriptors for the currently measured internal calls.
//! These are not 017 artifacts, common Verification Record Envelopes, or a claim
//! that all 015 boundaries or all 026 measurement methods are implemented.

use intlify_measurement::acquisition::memory::ObserverDescription;
use serde::{Deserialize, Serialize};

use super::cases::Engine;
use super::clock::ClockDescription;
use super::instrument::InstrumentDescription;
use super::locale::InputFacts;
use super::locale_icu4x::AdmittedInputFacts;
use super::operation::{Operation, Prepared};
use super::shared::plan::present;

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

    /// The boundary of the peak live memory observed over an operation's
    /// interval: the same markers as its duration's, under its memory pair.
    /// An operation design 015 measures no memory over has none.
    pub(super) fn for_memory(operation: Operation) -> Option<Self> {
        let memory = operation.peak_memory()?;
        Some(Self {
            identity: memory.boundary.into(),
            phase: memory.phase.into(),
            cost: memory.cost.into(),
            metric: "peak_live_bytes".into(),
            excluded_markers: strings(&[
                "fixture-and-input-acquisition",
                "immutable-input-preparation",
                "observer-and-collector-setup",
                "operation-dispatch-and-handle-cloning",
                "input-and-invocation-black-box",
                "peak-selection-and-aggregation",
                "semantic-observation-and-validation",
                "reporting",
                "output-destruction-and-teardown",
            ]),
            ..Self::for_operation(operation)
        })
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

/// A case's Measurement Method Descriptor: how its duration is timed, or how
/// its peak is observed. Each spells its own fields, so neither reads as the
/// other, and a duration case keeps exactly the bytes it had before memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub(super) enum Method {
    Duration(DurationMethod),
    Memory(MemoryMethod),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DurationMethod {
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

/// The method of a peak observed through an allocator observer. What counts,
/// who counts it, and what the counts do not see are the observer's own
/// description; the rest is how this owner reads one interval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MemoryMethod {
    identity: String,
    revision: String,
    metric: String,
    canonical_unit: String,
    provider: String,
    provider_revision: String,
    observer: String,
    observer_revision: String,
    counting: String,
    observation_domain: String,
    excluded_domain: String,
    event_taxonomy: String,
    peak_relation: String,
    net_relation: String,
    sample_aggregation_kind: String,
    repetition_aggregation: String,
    concurrency_coverage: String,
    overflow_behavior: String,
    numeric_decision_eligibility: String,
    limitations: Vec<String>,
    included_instrumentation: Vec<String>,
    estimated_overhead_subtraction: bool,
    optimization_barrier: OptimizationBarrier,
}

impl Method {
    pub(super) fn monotonic_invocation() -> Self {
        Self::Duration(DurationMethod::monotonic_invocation())
    }

    /// The method of a peak the given observer counts.
    pub(super) fn observed_peak(observer: ObserverDescription) -> Self {
        Self::Memory(MemoryMethod {
            identity: "intlify-config-observed-peak-invocation".into(),
            revision: "0".into(),
            metric: "peak_live_bytes".into(),
            canonical_unit: "octet".into(),
            provider: observer.provider.into(),
            provider_revision: observer.provider_revision.into(),
            observer: observer.identity.into(),
            observer_revision: observer.revision.into(),
            counting: observer.counting.into(),
            observation_domain: "owner-boundary-complete-core-invocation".into(),
            excluded_domain: "ordered-owner-boundary-excluded-markers".into(),
            event_taxonomy: "one-observer-mark-pair-per-invocation".into(),
            peak_relation: "largest-net-growth-over-the-start-mark".into(),
            net_relation: "net-below-zero-at-the-end-mark-refused-not-clamped".into(),
            sample_aggregation_kind: "peak_over_batch".into(),
            repetition_aggregation: "largest-of-separate-single-invocation-peaks".into(),
            concurrency_coverage: "calling-thread-synchronous-core-no-worker-summation".into(),
            // Taking the largest peak needs no arithmetic, so it cannot
            // overflow; the provider's unchecked counters are a limitation.
            overflow_behavior: "largest-peak-needs-no-arithmetic-provider-counters-unchecked"
                .into(),
            // 026 fixes this class for memory too (D26:1459).
            numeric_decision_eligibility: "qualified-runner".into(),
            limitations: strings(observer.limitations),
            included_instrumentation: strings(&[
                "preserved-indirect-call",
                "complete-output-black-box",
                "observer-mark-edges",
            ]),
            estimated_overhead_subtraction: false,
            optimization_barrier: OptimizationBarrier::between(
                "intlify-config-observed-core-call-black-box",
                "mark",
            ),
        })
    }
}

impl DurationMethod {
    fn monotonic_invocation() -> Self {
        Self {
            identity: "intlify-config-posix-monotonic-invocation".into(),
            revision: "0".into(),
            metric: "wall_duration".into(),
            canonical_unit: "nanosecond".into(),
            provider: "rustix".into(),
            provider_revision: intlify_measurement::acquisition::PROVIDER_REVISION.into(),
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
            optimization_barrier: OptimizationBarrier::between(
                "intlify-config-core-call-black-box",
                "read",
            ),
        }
    }
}

impl OptimizationBarrier {
    /// The barrier around one invocation between two marks of the given kind:
    /// a clock's reads, or an observer's marks.
    fn between(identity: &str, mark: &str) -> Self {
        Self {
            identity: identity.into(),
            revision: "0".into(),
            applicability: "required".into(),
            input_opacity: "std-hint-black-box-runtime-materialized-input".into(),
            invocation_preservation: "std-hint-black-box-function-pointer-per-repetition".into(),
            output_consumption: "std-hint-black-box-complete-ordinary-output".into(),
            output_lifetime: format!("owned-result-retained-through-end-{mark}-and-observation"),
            input_placement: format!("before-start-{mark}"),
            invocation_placement: format!("after-start-{mark}"),
            output_placement: format!("before-end-{mark}"),
            semantic_validation: format!(
                "after-end-{mark}-compare-full-output-with-independent-fixture-observation"
            ),
        }
    }
}

// The clock observation is one of 026's environment fields, so its shape is
// the common one. What this owner supplies is the acquired values.
pub(super) use intlify_measurement::environment::ClockObservation;

// The execution state is one of 026's common record shapes. What this owner
// supplies is which states its prepared core actually ran in.
pub(super) use intlify_measurement::execution::{Execution, OutputBuffer};

/// The execution state this owner's prepared core actually runs in. `icu4x` is
/// the engine state of a case canonicalizing through the ICU4X provider.
pub(super) fn prepared_core(operation: Operation, icu4x: Option<Engine>) -> Execution {
    if let Some(engine) = icu4x {
        return Execution {
            process_state: "reused-process".into(),
            // A fresh engine is admitted from the artifact bytes before each
            // invocation; a reused one is admitted once, before the first.
            engine_state: match engine {
                Engine::Reused => "reused",
                Engine::Fresh => "fresh",
            }
            .into(),
            initial_preparation_state: "resident".into(),
            cache_state: "disabled".into(),
            runtime_compilation_state: "ahead-of-time".into(),
            managed_heap_state: "not-applicable".into(),
            // The conformance layer and ICU4X work in storage of each call's
            // own, and each call writes a new canonical value.
            scratch_reuse_state: "fresh".into(),
            output_buffer_state: OutputBuffer::Applicable {
                ownership: "caller-owned".into(),
                reuse: "fresh".into(),
            },
        };
    }
    let locale = operation == Operation::LocaleCanonicalization;
    // A construction case builds a fresh resolver in every interval; every
    // other case, the identity's included, reuses what preparation built.
    let engine = if operation == Operation::ConstructionAuthorityAdmission {
        "fresh"
    } else {
        "reused"
    };
    Execution {
        // The harness invokes prepared calls in its existing process. Its
        // compiled core and immutable preparation are retained, not rebuilt
        // between samples. None of these fields claims a warm lookup cache.
        process_state: "reused-process".into(),
        engine_state: engine.into(),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Descriptors {
    pub(super) boundary: Boundary,
    pub(super) method: Method,
    /// The clock a duration case was timed with. A memory case reads none,
    /// so it has none, rather than a null.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "ClockObservation")]
    pub(super) clock_observation: Option<ClockObservation>,
    pub(super) execution: Execution,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) locale_input: Option<InputFacts>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) locale_policy_resolution_input: Option<super::locale_resolution::InputFacts>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) admitted_locale_input: Option<AdmittedInputFacts>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) resource_policy_input: Option<super::resource_policy::InputFacts>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) artifact_collection_input: Option<super::artifact_collection::InputFacts>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) resolved_binding_input: Option<super::resolved_binding::InputFacts>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) target_and_group_input: Option<super::target_group::InputFacts>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) hydration_relation_input: Option<super::hydration_relation::InputFacts>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) construction_input: Option<super::construction::InputFacts>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum DescriptorIssue {
    BoundaryMismatch,
    MethodMismatch,
    ClockBindingMismatch,
    InvalidClockObservation,
    ExecutionMismatch,
    LocaleInputBindingMismatch,
    LocalePolicyResolutionInputBindingMismatch,
    ResourcePolicyInputBindingMismatch,
    ArtifactCollectionInputBindingMismatch,
    ResolvedBindingInputBindingMismatch,
    TargetAndGroupInputBindingMismatch,
    HydrationRelationInputBindingMismatch,
    ConstructionInputBindingMismatch,
}

impl Descriptors {
    /// Actual preparation and clock acquisition supply the validation inputs.
    /// Decoded record fields must never manufacture their own expected binding.
    pub(super) fn for_acquisition(prepared: &Prepared, clock: ClockDescription) -> Self {
        let operation = prepared.operation();
        Self::assemble(
            prepared,
            Boundary::for_operation(operation),
            Method::monotonic_invocation(),
            Some(clock.observation()),
        )
    }

    /// A memory case's descriptors, from its preparation and the description
    /// its observer gives. An operation without a memory boundary has none.
    pub(super) fn for_observer(prepared: &Prepared, observer: ObserverDescription) -> Option<Self> {
        let boundary = Boundary::for_memory(prepared.operation())?;
        Some(Self::assemble(
            prepared,
            boundary,
            Method::observed_peak(observer),
            None,
        ))
    }

    /// The descriptors the run's instrument gives one case.
    pub(super) fn for_instrument(
        prepared: &Prepared,
        instrument: InstrumentDescription,
    ) -> Option<Self> {
        match instrument {
            InstrumentDescription::Clock(clock) => Some(Self::for_acquisition(prepared, clock)),
            InstrumentDescription::Observer(observer) => Self::for_observer(prepared, observer),
        }
    }

    fn assemble(
        prepared: &Prepared,
        boundary: Boundary,
        method: Method,
        clock_observation: Option<ClockObservation>,
    ) -> Self {
        let operation = prepared.operation();
        Self {
            boundary,
            method,
            clock_observation,
            execution: prepared_core(operation, prepared.icu4x_engine()),
            locale_input: finite(prepared),
            locale_policy_resolution_input: match prepared {
                Prepared::LocalePolicyResolution(prepared) => {
                    Some(super::locale_resolution::InputFacts::observe(prepared))
                }
                _ => None,
            },
            admitted_locale_input: admitted(prepared),
            resource_policy_input: policy(prepared),
            artifact_collection_input: collection(prepared),
            resolved_binding_input: binding(prepared),
            target_and_group_input: targets(prepared),
            hydration_relation_input: hydration(prepared),
            construction_input: construction(prepared),
        }
    }

    /// Check a duration case's descriptors against the acquired clock.
    pub(super) fn validate(
        &self,
        prepared: &Prepared,
        acquisition: ClockDescription,
    ) -> Vec<DescriptorIssue> {
        self.validate_with(prepared, InstrumentDescription::Clock(acquisition))
    }

    /// Check one case's descriptors against the run's acquired instrument.
    pub(super) fn validate_with(
        &self,
        prepared: &Prepared,
        acquisition: InstrumentDescription,
    ) -> Vec<DescriptorIssue> {
        let operation = prepared.operation();
        let mut issues = match acquisition {
            InstrumentDescription::Clock(clock) => self.validate_clock(operation, clock),
            InstrumentDescription::Observer(observer) => {
                self.validate_observer(operation, observer)
            }
        };
        if self.execution != prepared_core(operation, prepared.icu4x_engine()) {
            issues.push(DescriptorIssue::ExecutionMismatch);
        }
        if self.locale_input != finite(prepared) || self.admitted_locale_input != admitted(prepared)
        {
            issues.push(DescriptorIssue::LocaleInputBindingMismatch);
        }
        let expected_resolution = match prepared {
            Prepared::LocalePolicyResolution(prepared) => {
                Some(super::locale_resolution::InputFacts::observe(prepared))
            }
            _ => None,
        };
        if self.locale_policy_resolution_input != expected_resolution {
            issues.push(DescriptorIssue::LocalePolicyResolutionInputBindingMismatch);
        }
        if self.resource_policy_input != policy(prepared) {
            issues.push(DescriptorIssue::ResourcePolicyInputBindingMismatch);
        }
        if self.artifact_collection_input != collection(prepared) {
            issues.push(DescriptorIssue::ArtifactCollectionInputBindingMismatch);
        }
        if self.resolved_binding_input != binding(prepared) {
            issues.push(DescriptorIssue::ResolvedBindingInputBindingMismatch);
        }
        if self.target_and_group_input != targets(prepared) {
            issues.push(DescriptorIssue::TargetAndGroupInputBindingMismatch);
        }
        if self.hydration_relation_input != hydration(prepared) {
            issues.push(DescriptorIssue::HydrationRelationInputBindingMismatch);
        }
        if self.construction_input != construction(prepared) {
            issues.push(DescriptorIssue::ConstructionInputBindingMismatch);
        }
        issues
    }

    fn validate_clock(
        &self,
        operation: Operation,
        acquisition: ClockDescription,
    ) -> Vec<DescriptorIssue> {
        let mut issues = Vec::new();
        if self.boundary != Boundary::for_operation(operation) {
            issues.push(DescriptorIssue::BoundaryMismatch);
        }
        let method = DurationMethod::monotonic_invocation();
        if self.method != Method::Duration(method.clone()) {
            issues.push(DescriptorIssue::MethodMismatch);
        }
        let Some(clock) = &self.clock_observation else {
            issues.push(DescriptorIssue::ClockBindingMismatch);
            return issues;
        };
        if *clock != acquisition.observation() {
            issues.push(DescriptorIssue::ClockBindingMismatch);
        }
        if clock.provider != method.provider
            || clock.provider_revision != method.provider_revision
            || clock.clock != method.clock
            || clock.resolution_nanoseconds.get() == 0
            || clock.resolution_source != method.clock_resolution_source
            || clock.conversion != "exact-integer-nanoseconds"
        {
            issues.push(DescriptorIssue::InvalidClockObservation);
        }
        issues
    }

    fn validate_observer(
        &self,
        operation: Operation,
        observer: ObserverDescription,
    ) -> Vec<DescriptorIssue> {
        let mut issues = Vec::new();
        if Boundary::for_memory(operation).as_ref() != Some(&self.boundary) {
            issues.push(DescriptorIssue::BoundaryMismatch);
        }
        if self.method != Method::observed_peak(observer) {
            issues.push(DescriptorIssue::MethodMismatch);
        }
        // A memory interval reads no clock, so a clock binding is foreign.
        if self.clock_observation.is_some() {
            issues.push(DescriptorIssue::ClockBindingMismatch);
        }
        issues
    }
}

/// The finite provider's facts, for a case canonicalizing through it.
fn finite(prepared: &Prepared) -> Option<InputFacts> {
    match prepared {
        Prepared::Locale { core, .. } => Some(InputFacts::observe(core)),
        Prepared::LocalePolicyResolution(prepared) => {
            prepared.finite().map(|core| InputFacts::observe(core))
        }
        _ => None,
    }
}

/// The admitted artifact's facts, for a case canonicalizing through the
/// ICU4X provider.
fn admitted(prepared: &Prepared) -> Option<AdmittedInputFacts> {
    match prepared {
        Prepared::Icu4xLocale { engine, .. } => Some(AdmittedInputFacts::observe(engine)),
        Prepared::LocalePolicyResolution(prepared) => {
            prepared.icu4x().map(AdmittedInputFacts::observe)
        }
        _ => None,
    }
}

/// The capability, selection, and collection facts of a Phase 4 case.
fn policy(prepared: &Prepared) -> Option<super::resource_policy::InputFacts> {
    match prepared {
        Prepared::ResourcePolicy(prepared) => {
            Some(super::resource_policy::InputFacts::observe(prepared))
        }
        _ => None,
    }
}

/// The policy and collection facts of a Phase 9 case.
fn collection(prepared: &Prepared) -> Option<super::artifact_collection::InputFacts> {
    match prepared {
        Prepared::ArtifactCollection(prepared) => {
            Some(super::artifact_collection::InputFacts::observe(prepared))
        }
        _ => None,
    }
}

/// The configuration and collection facts of a Phase 9 exact-resolution case.
fn binding(prepared: &Prepared) -> Option<super::resolved_binding::InputFacts> {
    match prepared {
        Prepared::ResolvedBinding(prepared) => {
            Some(super::resolved_binding::InputFacts::observe(prepared))
        }
        _ => None,
    }
}

/// The configuration and collection facts of a Phase 11 and 12 case.
fn targets(prepared: &Prepared) -> Option<super::target_group::InputFacts> {
    match prepared {
        Prepared::TargetAndGroup(prepared) => {
            Some(super::target_group::InputFacts::observe(prepared))
        }
        _ => None,
    }
}

/// The configuration and collection facts of a Phase 13 case, whose Target
/// Profiles declare the roles.
fn hydration(prepared: &Prepared) -> Option<super::hydration_relation::InputFacts> {
    match prepared {
        Prepared::Hydration(prepared) => {
            Some(super::hydration_relation::InputFacts::observe(prepared))
        }
        _ => None,
    }
}

/// The input facts of a construction case, or of an identity case and the
/// resolver it reuses.
fn construction(prepared: &Prepared) -> Option<super::construction::InputFacts> {
    match prepared {
        Prepared::Construction(prepared) => {
            Some(super::construction::InputFacts::construction(prepared))
        }
        Prepared::ConstructionIdentity(prepared) => {
            Some(super::construction::InputFacts::identity(prepared))
        }
        _ => None,
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[cfg(test)]
mod tests;
