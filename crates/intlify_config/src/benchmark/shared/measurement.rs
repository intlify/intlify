// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! This owner's projection into the common observational records.
//!
//! The record bodies belong to `intlify_measurement`. What this module supplies
//! is what only this owner knows: which labels qualify its checksums, which
//! domains it computes them in, and which of its attempts actually produced
//! samples. A non-measured attempt is reported as such rather than omitted.

use intlify_measurement::measurement::{
    self as common, ChecksumMethod, MeasuredCase, OwnerResultFacts, SampleDomains, SampleFacts,
    SamplingFacts,
};

pub(in crate::benchmark) use intlify_measurement::measurement::{
    owner_attempt_reference as native_attempt_reference, CaseEvaluation, CaseResult,
    EvaluationBody, ExpectedOwner, InputResolution, InputState, MissingCase, Outcome, ReportBody,
    ReportSample, Row, RunBinding, Section, Truncation, UnavailableKind,
};

pub(super) use intlify_measurement::measurement::{
    Aggregation, Category, Metric, ObservationalOnly, OperationClass, Requirement, Surface, Unit,
};

use super::build;
use super::environment::{self, projection_identity};
use super::identity::{
    IdentityFailure, NativeChecksum, OwnerLabel, OwnerRecordIdentity, Token, VersionedIdentity,
};
use super::plan::{CaseProjection, RunPlanRecord};
use crate::benchmark::descriptor::Descriptors;
use crate::benchmark::observation::Observation;
use crate::benchmark::quantity::Quantity;
use crate::benchmark::run::{AttemptResult, CaseAttempt, OwnerRecord, ProjectionSource};
use crate::benchmark::sample::CapturedSample;

/// The Evidence Set this owner produces, over its own closed fragments.
pub(in crate::benchmark) type Evidence = common::Evidence<CaseProjection, Descriptors, Observation>;
pub(in crate::benchmark) type EvidenceBody =
    common::EvidenceBody<CaseProjection, Descriptors, Observation>;
pub(in crate::benchmark) type CaseEvidence =
    common::CaseEvidence<CaseProjection, Descriptors, Observation>;
pub(in crate::benchmark) type Evaluation = common::Evaluation;
pub(in crate::benchmark) type Report = common::Report;

const RESULT_SCHEMA: &str = "intlify-config-owner-run-result/1";

pub(in crate::benchmark) fn evidence_schema() -> Result<serde_json::Value, serde_json::Error> {
    common::evidence_schema::<CaseProjection, Descriptors, Observation>()
}

pub(in crate::benchmark) fn evaluation_schema() -> Result<serde_json::Value, serde_json::Error> {
    common::evaluation_schema()
}

pub(in crate::benchmark) fn report_schema() -> Result<serde_json::Value, serde_json::Error> {
    common::report_schema()
}

/// How this owner computes every native checksum it records.
fn method() -> ChecksumMethod {
    ChecksumMethod {
        owner_identity: Token::literal("intlify-config"),
        algorithm: OwnerLabel::literal("blake3-256"),
        framing: OwnerLabel::literal("intlify-config-minimum-observation/0"),
    }
}

/// The digest domains this owner separates inside one measured case.
fn domains() -> SampleDomains {
    SampleDomains {
        semantic: OwnerLabel::literal("observation"),
        execution: OwnerLabel::literal("sample-execution"),
    }
}

fn result_schema() -> OwnerLabel {
    OwnerLabel::literal(RESULT_SCHEMA)
}

fn benchmark_profile(owner: &OwnerRecord) -> Result<VersionedIdentity, IdentityFailure> {
    let (profile, revision) = owner.result().context.profile.identity_revision();
    VersionedIdentity::new(profile, revision)
}

/// Bind one Evidence Set to the owner result it was projected from.
fn owner_binding(owner: &OwnerRecord) -> Result<common::OwnerBinding, IdentityFailure> {
    Ok(common::OwnerBinding::new(OwnerResultFacts {
        method: method(),
        result_schema: result_schema(),
        benchmark_profile: benchmark_profile(owner)?,
        result_identity: owner.result().record_identity.clone(),
        domain: OwnerLabel::literal("owner-run-result"),
        checksum: NativeChecksum::from_bytes(owner.checksum().bytes()),
    }))
}

/// Name the owner result the issued Plan expects.
pub(super) fn expected_from_plan(
    plan: &RunPlanRecord,
    native_identity: &OwnerRecordIdentity,
) -> ExpectedOwner {
    ExpectedOwner::new(
        native_identity.clone(),
        result_schema(),
        plan.body.measurement_profile.clone(),
    )
}

/// Name the owner result a submitted document claims to be.
pub(super) fn expected_from_owner(owner: &OwnerRecord) -> Result<ExpectedOwner, IdentityFailure> {
    Ok(ExpectedOwner::new(
        owner.result().record_identity.clone(),
        result_schema(),
        benchmark_profile(owner)?,
    ))
}

fn sample_facts(sample: &CapturedSample) -> SampleFacts<Observation> {
    SampleFacts {
        ordinal: sample.ordinal,
        repetition_count: sample.repetition_count,
        aggregate_quantity: sample.aggregate_nanoseconds,
        semantic_observation: sample.semantic_observation,
        semantic_identity: NativeChecksum::from_bytes(
            sample.semantic_observation.identity().bytes(),
        ),
        execution_identity: NativeChecksum::from_bytes(sample.execution_identity.bytes()),
        local_identity: NativeChecksum::from_bytes(sample.local_identity.bytes()),
    }
}

/// Project one attempt, or report that it produced no measurement.
pub(super) fn project_case(
    source: &ProjectionSource<'_>,
    attempt: &CaseAttempt,
    identity: &intlify_measurement::identity::CaseIdentity,
    projection: &CaseProjection,
) -> Result<Option<CaseEvidence>, IdentityFailure> {
    let AttemptResult::Measured(measured) = &attempt.result else {
        return Ok(None);
    };
    let owner = source.document();
    let operation = &measured.operation.operation;
    let policy = owner.result().context.profile.sampling_policy();
    CaseEvidence::project(
        &method(),
        &domains(),
        &owner.result().record_identity,
        identity,
        projection.clone(),
        MeasuredCase {
            ordinal: attempt.ordinal,
            descriptors: operation.descriptors.clone(),
            sampling: SamplingFacts {
                warmup_strategy: Token::literal(
                    "fixed-invocations-before-measured-samples-per-case",
                ),
                warmup_repetitions: policy.warmup_repetitions,
                warmup_completed: operation.capture.warmup_completed,
                measured_samples: policy.measured_samples,
                repetitions_per_sample: policy.repetitions_per_sample,
            },
            samples: operation.capture.samples.iter().map(sample_facts).collect(),
        },
    )
    .map(Some)
}

/// Assemble the complete Evidence Set body for one admitted owner result.
pub(super) fn evidence_body(
    source: &ProjectionSource<'_>,
    plan: &RunPlanRecord,
    parent: &intlify_measurement::identity::RecordIdentity,
    cases: Vec<CaseEvidence>,
) -> Result<EvidenceBody, IdentityFailure> {
    Ok(EvidenceBody {
        binding: RunBinding::from_plan(plan),
        projection: projection_identity(),
        owner_result: owner_binding(source.document())?,
        build: build::project(source, parent)?,
        environment: environment::project(source, parent)?,
        cases,
    })
}

/// Reference one attempt by its ordinal inside the owner result.
pub(super) fn attempt_reference(
    owner: &OwnerRecord,
    ordinal: Quantity,
) -> Result<intlify_measurement::record::Reference, IdentityFailure> {
    native_attempt_reference(&owner.result().record_identity, ordinal)
}
