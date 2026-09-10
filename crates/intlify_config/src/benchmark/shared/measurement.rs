// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Closed bodies for the initial 026 observation path. Native semantic and
//! execution identities are explicitly qualified, not renamed shared digests.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use super::build::Build;
use super::environment::{projection_identity, Environment};
use super::identity::{CaseIdentity, IdentityFailure, RecordIdentity, Token, VersionedIdentity};
use super::plan::{BuildIdentity, CaseProjection, InventoryEntry, RunPlanRecord, Subject};
use super::reason::{Reason, Selector};
use super::record::{EvaluationKind, EvidenceKind, Record, Reference, ReportKind};
use crate::benchmark::descriptor::{Descriptors, Execution};
use crate::benchmark::observation::{Digest, Observation};
use crate::benchmark::quantity::{Quantity, Repetitions};
use crate::benchmark::run::{CaseAttempt, OwnerRecord, ProjectionSource};
use crate::benchmark::sample::CapturedSample;

pub(in crate::benchmark) fn evidence_schema() -> Result<serde_json::Value, serde_json::Error> {
    crate::schema::draft7_schema::<Evidence>()
}

pub(in crate::benchmark) fn evaluation_schema() -> Result<serde_json::Value, serde_json::Error> {
    crate::schema::draft7_schema::<Evaluation>()
}

pub(in crate::benchmark) fn report_schema() -> Result<serde_json::Value, serde_json::Error> {
    crate::schema::draft7_schema::<Report>()
}

macro_rules! literal {
    ($name:ident, $wire:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub(super) enum $name {
            #[serde(rename = $wire)]
            Value,
        }
    };
}
literal!(OwnerCodec, "intlify-config-owner-run-result/1");
literal!(Algorithm, "blake3-256");
literal!(Framing, "intlify-config-minimum-observation/0");
literal!(OwnerDomain, "owner-run-result");
literal!(SemanticDomain, "observation");
literal!(ExecutionDomain, "sample-execution");
literal!(
    WarmupStrategy,
    "fixed-invocations-before-measured-samples-per-case"
);
literal!(Aggregation, "batch_total");
literal!(NoCalibration, "none");
literal!(Metric, "wall_duration");
literal!(Unit, "nanosecond");
literal!(Category, "toolchain");
literal!(OperationClass, "component");
literal!(Surface, "core");
literal!(Requirement, "required");
literal!(ObservationalOnly, "observational-only-no-numeric-decisions");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RunBinding {
    pub(super) measurement_run: RecordIdentity,
    pub(super) measurement_run_plan: RecordIdentity,
    pub(super) measurement_profile: VersionedIdentity,
    pub(super) verification_subject: Subject,
    pub(super) build_identity: BuildIdentity,
}
impl RunBinding {
    pub(super) fn from_plan(plan: &RunPlanRecord) -> Self {
        Self {
            measurement_run: plan.body.measurement_run.clone(),
            measurement_run_plan: plan.identity().clone(),
            measurement_profile: plan.body.measurement_profile.clone(),
            verification_subject: plan.body.verification_subject.clone(),
            build_identity: plan.body.build_identity.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OwnerBinding {
    owner_identity: Token,
    result_schema: OwnerCodec,
    benchmark_profile: VersionedIdentity,
    pub(super) result_identity: RecordIdentity,
    algorithm: Algorithm,
    framing: Framing,
    domain: OwnerDomain,
    checksum: Digest,
}
impl OwnerBinding {
    pub(super) fn from_owner(owner: &OwnerRecord) -> Result<Self, IdentityFailure> {
        let result = owner.result();
        let (profile, revision) = result.context.profile.identity_revision();
        Ok(Self {
            owner_identity: Token::literal("intlify-config"),
            result_schema: OwnerCodec::Value,
            benchmark_profile: VersionedIdentity::new(profile, revision)?,
            result_identity: result.record_identity.clone(),
            algorithm: Algorithm::Value,
            framing: Framing::Value,
            domain: OwnerDomain::Value,
            checksum: owner.checksum(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SemanticObservation {
    owner_identity: Token,
    algorithm: Algorithm,
    framing: Framing,
    domain: SemanticDomain,
    identity: Digest,
    complete_observation: Observation,
}
impl From<Observation> for SemanticObservation {
    fn from(observation: Observation) -> Self {
        Self {
            owner_identity: Token::literal("intlify-config"),
            algorithm: Algorithm::Value,
            framing: Framing::Value,
            domain: SemanticDomain::Value,
            identity: observation.identity(),
            complete_observation: observation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExecutionIdentity {
    owner_result: RecordIdentity,
    algorithm: Algorithm,
    framing: Framing,
    domain: ExecutionDomain,
    value: Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum DeterminismProof {
    NotRequired {},
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Acquisition {
    Unpaired {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Sample {
    pub(super) local_record_identity: Token,
    pub(super) ordinal: Quantity,
    pub(super) repetition_count: Repetitions,
    pub(super) aggregate_quantity: Quantity,
    semantic_observation: SemanticObservation,
    execution_identity: ExecutionIdentity,
    determinism_proof: DeterminismProof,
    acquisition: Acquisition,
    owner_sample_identity: Digest,
}
impl Sample {
    fn from_owner(
        owner: &OwnerRecord,
        attempt: Quantity,
        sample: &CapturedSample,
    ) -> Result<Self, IdentityFailure> {
        Ok(Self {
            local_record_identity: Token::new(&format!(
                "measurement-sample-{}-{}",
                attempt.get(),
                sample.ordinal.get()
            ))?,
            ordinal: sample.ordinal,
            repetition_count: sample.repetition_count,
            aggregate_quantity: sample.aggregate_nanoseconds,
            semantic_observation: sample.semantic_observation.into(),
            execution_identity: ExecutionIdentity {
                owner_result: owner.result().record_identity.clone(),
                algorithm: Algorithm::Value,
                framing: Framing::Value,
                domain: ExecutionDomain::Value,
                value: sample.execution_identity,
            },
            determinism_proof: DeterminismProof::NotRequired {},
            acquisition: Acquisition::Unpaired {},
            owner_sample_identity: sample.local_identity,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SamplingPolicy {
    warmup_strategy: WarmupStrategy,
    warmup_repetitions: Quantity,
    warmup_completed: Quantity,
    measured_samples: Repetitions,
    repetitions_per_sample: Repetitions,
    aggregation: Aggregation,
    calibration: NoCalibration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CaseEvidence {
    pub(super) local_record_identity: Token,
    pub(super) case_identity: CaseIdentity,
    pub(super) identity_projection: CaseProjection,
    pub(super) owner_attempt: Reference,
    pub(super) observed_descriptors: Descriptors,
    sampling_policy: SamplingPolicy,
    #[schemars(length(min = 1))]
    pub(super) samples: Vec<Sample>,
}

pub(super) fn native_attempt_reference(
    owner: &OwnerRecord,
    ordinal: Quantity,
) -> Result<Reference, IdentityFailure> {
    Ok(Reference::nested(
        &owner.result().record_identity,
        Token::new(&format!("attempt-{}", ordinal.get()))?,
    ))
}

impl CaseEvidence {
    pub(super) fn project(
        source: &ProjectionSource<'_>,
        attempt: &CaseAttempt,
        identity: &CaseIdentity,
        projection: &CaseProjection,
    ) -> Result<Option<Self>, IdentityFailure> {
        let crate::benchmark::run::AttemptResult::Measured(measured) = &attempt.result else {
            return Ok(None);
        };
        let owner = source.document();
        let operation = &measured.operation.operation;
        let policy = owner.result().context.profile.sampling_policy();
        Ok(Some(Self {
            local_record_identity: Token::new(&format!(
                "measurement-case-{}",
                attempt.ordinal.get()
            ))?,
            case_identity: identity.clone(),
            identity_projection: projection.clone(),
            owner_attempt: native_attempt_reference(owner, attempt.ordinal)?,
            observed_descriptors: operation.descriptors.clone(),
            sampling_policy: SamplingPolicy {
                warmup_strategy: WarmupStrategy::Value,
                warmup_repetitions: policy.warmup_repetitions,
                warmup_completed: operation.capture.warmup_completed,
                measured_samples: policy.measured_samples,
                repetitions_per_sample: policy.repetitions_per_sample,
                aggregation: Aggregation::Value,
                calibration: NoCalibration::Value,
            },
            samples: operation
                .capture
                .samples
                .iter()
                .map(|sample| Sample::from_owner(owner, attempt.ordinal, sample))
                .collect::<Result<_, _>>()?,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct EvidenceBody {
    pub(super) binding: RunBinding,
    pub(super) projection: VersionedIdentity,
    pub(super) owner_result: OwnerBinding,
    pub(super) build: Build,
    pub(super) environment: Environment,
    #[schemars(length(min = 1))]
    pub(super) cases: Vec<CaseEvidence>,
}
pub(super) type Evidence = Record<EvidenceKind, EvidenceBody>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ExpectedOwner {
    pub(super) record_identity: RecordIdentity,
    result_schema: OwnerCodec,
    benchmark_profile: VersionedIdentity,
}
impl ExpectedOwner {
    pub(super) fn from_plan(plan: &RunPlanRecord, native_identity: &RecordIdentity) -> Self {
        Self {
            record_identity: native_identity.clone(),
            result_schema: OwnerCodec::Value,
            benchmark_profile: plan.body.measurement_profile.clone(),
        }
    }
    pub(super) fn from_owner(owner: &OwnerRecord) -> Result<Self, IdentityFailure> {
        let (profile, revision) = owner.result().context.profile.identity_revision();
        Ok(Self {
            record_identity: owner.result().record_identity.clone(),
            result_schema: OwnerCodec::Value,
            benchmark_profile: VersionedIdentity::new(profile, revision)?,
        })
    }
}

fn present_reference<'de, D: Deserializer<'de>>(decoder: D) -> Result<Option<Reference>, D::Error> {
    Reference::deserialize(decoder).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum InputState {
    Resolved {
        reference: Reference,
    },
    Unavailable {
        source_evaluations: Vec<Reference>,
        #[schemars(length(min = 1))]
        reasons: Vec<Reason>,
    },
    Invalid {
        #[serde(
            default,
            deserialize_with = "present_reference",
            skip_serializing_if = "Option::is_none"
        )]
        #[schemars(with = "Reference")]
        submitted_input: Option<Reference>,
        #[schemars(length(min = 1))]
        reasons: Vec<Reason>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InputResolution {
    pub(super) expected: ExpectedOwner,
    pub(super) result: InputState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(super) enum UnavailableKind {
    Missing,
    Skipped,
    Unsupported,
    Failed,
    ProjectionIneligible,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum CaseResult {
    Measured {
        evidence: Reference,
    },
    NotApplicable {
        applicability_rule: VersionedIdentity,
    },
    Unavailable {
        unavailable_kind: UnavailableKind,
        #[schemars(length(min = 1))]
        reasons: Vec<Reason>,
        diagnostic_partial_observations: Vec<Reference>,
    },
    Invalid {
        #[schemars(length(min = 1))]
        reasons: Vec<Reason>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CaseEvaluation {
    pub(super) local_record_identity: Token,
    pub(super) case_identity: CaseIdentity,
    pub(super) result: CaseResult,
}
impl CaseEvaluation {
    pub(super) fn selector(plan: &RunPlanRecord, case: &CaseIdentity) -> Selector {
        Selector::PlannedCase {
            run_plan: plan.identity().clone(),
            case_identity: case.clone(),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum Outcome {
    Complete,
    Incomplete,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct EvaluationBody {
    pub(super) binding: RunBinding,
    pub(super) owner_result_input: InputResolution,
    pub(super) case_inventory: Vec<InventoryEntry>,
    pub(super) cases: Vec<CaseEvaluation>,
    pub(super) outcome: Outcome,
    pub(super) reasons: Vec<Reason>,
}
pub(super) type Evaluation = Record<EvaluationKind, EvaluationBody>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReportSample {
    pub(super) local_record_identity: Token,
    pub(super) raw_sample: Reference,
    pub(super) ordinal: Quantity,
    pub(super) repetition_count: Repetitions,
    pub(super) aggregate_quantity: Quantity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Row {
    pub(super) local_record_identity: Token,
    pub(super) case_identity: CaseIdentity,
    pub(super) case_evaluation: Reference,
    pub(super) evidence: Reference,
    pub(super) owner_phase: String,
    pub(super) owner_cost: String,
    pub(super) category: Category,
    pub(super) operation_class: OperationClass,
    pub(super) performance_surface: Surface,
    pub(super) execution_state: Execution,
    pub(super) metric: Metric,
    pub(super) unit: Unit,
    pub(super) sample_aggregation: Aggregation,
    pub(super) samples: Vec<ReportSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MissingCase {
    pub(super) local_record_identity: Token,
    pub(super) case_identity: CaseIdentity,
    pub(super) requirement: Requirement,
    pub(super) evaluation: Reference,
    pub(super) result: CaseResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum Truncation {
    Complete {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum Section {
    MeasurementObservation {
        local_record_identity: Token,
        run_evaluation: Reference,
        evidence_sets: Vec<Reference>,
        outcome: Outcome,
        reasons: Vec<Reason>,
        numeric_policy: ObservationalOnly,
        rows: Vec<Row>,
        missing_case_inventory: Vec<MissingCase>,
        truncation: Truncation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReportBody {
    pub(super) report_specification: VersionedIdentity,
    pub(super) sections: Vec<Section>,
}
pub(super) type Report = Record<ReportKind, ReportBody>;

impl EvidenceBody {
    pub(super) fn from_source(
        source: &ProjectionSource<'_>,
        plan: &RunPlanRecord,
        parent: &RecordIdentity,
        cases: Vec<CaseEvidence>,
    ) -> Result<Self, IdentityFailure> {
        Ok(Self {
            binding: RunBinding::from_plan(plan),
            projection: projection_identity(),
            owner_result: OwnerBinding::from_owner(source.document())?,
            build: Build::project(source, parent)?,
            environment: Environment::project(source, parent)?,
            cases,
        })
    }
}
