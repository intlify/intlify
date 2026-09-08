// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Owner-selected measurement policy for the minimum observational path.
//! This private document is not a 017 artifact, Run Plan, complete Case identity,
//! common Evidence Set, or proof of runner qualification. The enclosing harness
//! must bind it to those records and verify the complete attempted inventory.

use serde::{Deserialize, Serialize};

use super::cases::registry::{AdmittedFixture, Registry, RegistryBinding};
use super::cases::Declaration;
use super::clock::{ClockDescription, MonotonicClock};
use super::collect::{collect_operation, CollectedOperation, CollectionFailure, CollectionIssue};
use super::descriptor::Execution;
use super::operation::Operation;
use super::quantity::{Quantity, Repetitions};
use super::sample::{CaptureBinding, CaptureCapacity, FailureIntegrityIssue, Sampling};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Revision {
    identity: String,
    revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SamplingPolicy {
    warmup_strategy: String,
    warmup_repetitions: Quantity,
    measured_samples: Repetitions,
    repetitions_per_sample: Repetitions,
    calibration: String,
    aggregation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SchedulingPolicy {
    process: String,
    fixture: String,
    allocator: String,
    concurrency: String,
    timeout: String,
    cancellation: String,
    failed_capture: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperationState {
    operation: Operation,
    execution: Execution,
}

/// Decoding is not admission. Every field is compared with the owner-selected
/// profile, not used to infer new acceptance rules for a submitted measurement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MeasurementProfile {
    identity: String,
    revision: String,
    adopted_specification: Revision,
    scope: String,
    fixture_registry: RegistryBinding,
    cases: Vec<Declaration>,
    case_requirement: String,
    sampling: SamplingPolicy,
    scheduling: SchedulingPolicy,
    operation_states: Vec<OperationState>,
    ordering: String,
    clock_resolution_relationship: String,
    raw_samples: String,
    numeric_decisions: String,
    diagnostic_profiling: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProfileIssue {
    Identity,
    Specification,
    Scope,
    FixtureRegistry,
    Inventory,
    Requirement,
    Sampling,
    Scheduling,
    OperationStates,
    Ordering,
    ClockResolution,
    Retention,
    NumericPolicy,
    Profiling,
}

/// No Deserialize, mutable document access, or caller-defined sampling policy.
#[derive(Debug)]
pub(super) struct AdmittedProfile {
    document: MeasurementProfile,
    sampling: Sampling,
}

impl MeasurementProfile {
    fn smoke(registry: &Registry) -> Self {
        let one = Repetitions::new(1).expect("fixed positive smoke count");
        Self {
            identity: "intlify-config-minimum-smoke".into(),
            revision: "0".into(),
            adopted_specification: Revision {
                identity: "intlify-design-026".into(),
                revision: "0".into(),
            },
            scope: "internal-configuration-and-three-project-locale-roles".into(),
            fixture_registry: registry.binding().clone(),
            cases: registry.declarations().cloned().collect(),
            case_requirement: "all-required-unconditional".into(),
            sampling: SamplingPolicy {
                warmup_strategy: "fixed-invocations-before-measured-samples-per-case".into(),
                warmup_repetitions: Quantity::new(1),
                measured_samples: one,
                repetitions_per_sample: one,
                calibration: "none".into(),
                aggregation: "batch_total".into(),
            },
            scheduling: SchedulingPolicy {
                process: "single-calling-process-resident-compiled-core".into(),
                fixture: "prepare-and-admit-before-case-reuse-immutable-input".into(),
                allocator: "ordinary-build-allocator-no-harness-reset".into(),
                concurrency: "sequential-cases-synchronous-calling-thread-no-workers".into(),
                // A finite invocation count is not a wall-clock timeout. A
                // CI timeout terminates this process; it does not publish a
                // successful prefix as a complete run or a zero observation.
                timeout: "no-internal-deadline-external-process-limit-only".into(),
                cancellation: "external-process-termination-no-complete-result".into(),
                failed_capture: "stop-case-retain-diagnostic-prefix-only".into(),
            },
            operation_states: Operation::ALL
                .into_iter()
                .map(|operation| OperationState {
                    operation,
                    execution: Execution::prepared_core(operation),
                })
                .collect(),
            ordering: "fixture-registry-order-no-interleaving".into(),
            // Zero elapsed time can be an honest observation. Smoke checks
            // reported positive granularity but imposes no duration ratio.
            clock_resolution_relationship: "retain-reported-positive-resolution-no-minimum-ratio"
                .into(),
            raw_samples: "retain-all-in-order-no-outlier-deletion".into(),
            numeric_decisions: "prohibited-advisory-and-gating".into(),
            diagnostic_profiling: "disabled".into(),
        }
    }

    pub(super) fn cases(&self) -> &[Declaration] {
        &self.cases
    }

    pub(super) fn validate_smoke(&self, registry: &Registry) -> Vec<ProfileIssue> {
        let expected = Self::smoke(registry);
        [
            (
                ProfileIssue::Identity,
                self.identity == expected.identity && self.revision == expected.revision,
            ),
            (
                ProfileIssue::Specification,
                self.adopted_specification == expected.adopted_specification,
            ),
            (ProfileIssue::Scope, self.scope == expected.scope),
            (
                ProfileIssue::FixtureRegistry,
                self.fixture_registry == expected.fixture_registry,
            ),
            (ProfileIssue::Inventory, self.cases == expected.cases),
            (
                ProfileIssue::Requirement,
                self.case_requirement == expected.case_requirement,
            ),
            (ProfileIssue::Sampling, self.sampling == expected.sampling),
            (
                ProfileIssue::Scheduling,
                self.scheduling == expected.scheduling,
            ),
            (
                ProfileIssue::OperationStates,
                self.operation_states == expected.operation_states,
            ),
            (ProfileIssue::Ordering, self.ordering == expected.ordering),
            (
                ProfileIssue::ClockResolution,
                self.clock_resolution_relationship == expected.clock_resolution_relationship,
            ),
            (
                ProfileIssue::Retention,
                self.raw_samples == expected.raw_samples,
            ),
            (
                ProfileIssue::NumericPolicy,
                self.numeric_decisions == expected.numeric_decisions,
            ),
            (
                ProfileIssue::Profiling,
                self.diagnostic_profiling == expected.diagnostic_profiling,
            ),
        ]
        .into_iter()
        .filter_map(|(issue, matches)| (!matches).then_some(issue))
        .collect()
    }
}

impl AdmittedProfile {
    pub(super) fn smoke(registry: &Registry) -> Self {
        Self::from_checked(MeasurementProfile::smoke(registry))
    }

    pub(super) fn admit_smoke(
        document: MeasurementProfile,
        registry: &Registry,
    ) -> Result<Self, Vec<ProfileIssue>> {
        let issues = document.validate_smoke(registry);
        if issues.is_empty() {
            Ok(Self::from_checked(document))
        } else {
            Err(issues)
        }
    }

    fn from_checked(document: MeasurementProfile) -> Self {
        // These are limits of this finite collector profile, not 015 defaults
        // or a Resource Limit Policy. Admission above pins all submitted counts.
        let one = Repetitions::new(1).expect("fixed positive smoke capacity");
        let sampling = Sampling::admit(
            document.sampling.warmup_repetitions,
            document.sampling.measured_samples,
            document.sampling.repetitions_per_sample,
            CaptureCapacity {
                warmup_repetitions: Quantity::new(1),
                samples: one,
                repetitions_per_sample: one,
                total_invocations: Repetitions::new(2).expect("fixed smoke invocation capacity"),
            },
        )
        .expect("the owner-selected smoke profile fits its finite capture capacity");
        Self { document, sampling }
    }

    pub(super) fn document(&self) -> &MeasurementProfile {
        &self.document
    }

    fn reference(&self) -> Revision {
        Revision {
            identity: self.document.identity.clone(),
            revision: self.document.revision.clone(),
        }
    }

    fn contains(&self, ordinal: Quantity, fixture: &AdmittedFixture) -> bool {
        usize::try_from(ordinal.get())
            .ok()
            .and_then(|index| self.document.cases.get(index))
            == Some(fixture.declaration())
    }

    pub(super) fn collect(
        &self,
        clock: &MonotonicClock,
        ordinal: Quantity,
        fixture: &AdmittedFixture,
        binding: CaptureBinding,
    ) -> Result<ProfiledOperation, ProfileCollectionFailure> {
        if !self.contains(ordinal, fixture) {
            return Err(ProfileCollectionFailure::CaseSelection);
        }
        let operation = collect_operation(clock, fixture, self.sampling, binding)
            .map_err(ProfileCollectionFailure::Collection)?;
        Ok(ProfiledOperation {
            measurement_profile: self.reference(),
            case_ordinal: ordinal,
            operation,
        })
    }

    pub(super) fn validate_failure(
        &self,
        ordinal: Quantity,
        fixture: &AdmittedFixture,
        binding: CaptureBinding,
        failure: &ProfileCollectionFailure,
    ) -> Vec<ProfileCollectionIssue> {
        if !self.contains(ordinal, fixture) {
            return vec![ProfileCollectionIssue::CaseSelection];
        }
        match failure {
            ProfileCollectionFailure::Collection(CollectionFailure::Capture(failure)) => {
                super::sample::validate_failure(
                    failure,
                    self.sampling,
                    binding,
                    fixture.expected(),
                    fixture.expected_work(),
                )
                .into_iter()
                .map(ProfileCollectionIssue::Failure)
                .collect()
            }
            ProfileCollectionFailure::Collection(CollectionFailure::Descriptor(issues))
                if issues.is_empty() =>
            {
                vec![ProfileCollectionIssue::Failure(
                    FailureIntegrityIssue::MissingReason,
                )]
            }
            ProfileCollectionFailure::Collection(CollectionFailure::Integrity(issues))
                if issues.is_empty() =>
            {
                vec![ProfileCollectionIssue::Failure(
                    FailureIntegrityIssue::MissingReason,
                )]
            }
            _ => Vec::new(),
        }
    }
}

/// Retain only the profile reference per case. The enclosing owner result must
/// retain the admitted full profile once, not copy its entire inventory per row.
/// Case ordinal is an owner inventory locator, not a common Measurement Case ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProfiledOperation {
    measurement_profile: Revision,
    case_ordinal: Quantity,
    operation: CollectedOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "detail",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub(super) enum ProfileCollectionFailure {
    CaseSelection,
    Collection(CollectionFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProfileCollectionIssue {
    ProfileBinding,
    CaseSelection,
    Operation(CollectionIssue),
    Failure(FailureIntegrityIssue),
}

impl ProfiledOperation {
    pub(super) fn validate(
        &self,
        profile: &AdmittedProfile,
        ordinal: Quantity,
        fixture: &AdmittedFixture,
        clock: ClockDescription,
        binding: CaptureBinding,
    ) -> Vec<ProfileCollectionIssue> {
        let mut issues = Vec::new();
        if self.measurement_profile != profile.reference() {
            issues.push(ProfileCollectionIssue::ProfileBinding);
        }
        if self.case_ordinal != ordinal || !profile.contains(ordinal, fixture) {
            issues.push(ProfileCollectionIssue::CaseSelection);
        }
        issues.extend(
            self.operation
                .validate(fixture, clock, profile.sampling, binding)
                .into_iter()
                .map(ProfileCollectionIssue::Operation),
        );
        issues
    }
}
