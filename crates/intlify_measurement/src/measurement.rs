// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The closed bodies of the common observational records.
//!
//! An owner's semantic and execution identities are explicitly qualified here:
//! who computed them, with which algorithm, under which framing, in which
//! domain. They are never renamed into shared digests, because two equal
//! values under different framings are not the same observation.
//!
//! What this module does not do is acquire anything. Durations, checksums, and
//! descriptors arrive as facts the owner already captured, and a case the owner
//! could not measure stays in the record as an unmeasured case rather than
//! being removed from it.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use intlify_shared_json::quantity::{Quantity, Repetitions};

use crate::build::Build;
use crate::environment::Environment;
use crate::execution::Execution;
use crate::identity::{
    CaseIdentity, IdentityFailure, NativeChecksum, OwnerLabel, OwnerRecordIdentity, RecordIdentity,
    Token, VersionedIdentity,
};
use crate::plan::{BuildIdentity, InventoryEntry, RunPlanRecord, Subject};
use crate::reason::{Reason, Selector};
use crate::record::{EvaluationKind, EvidenceKind, Record, Reference, ReportKind};

macro_rules! literal {
    ($name:ident, $wire:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub enum $name {
            #[serde(rename = $wire)]
            Value,
        }
    };
}

// Only the values this milestone's owners actually use are admitted. 026's
// wider vocabularies are not implemented here, and an unlisted value is
// rejected rather than silently carried.
literal!(
    Aggregation,
    "batch_total",
    "Samples aggregate as a batch total."
);
literal!(NoCalibration, "none", "No calibration was applied.");
literal!(Metric, "wall_duration", "The measured metric.");
literal!(Unit, "nanosecond", "The canonical unit of the metric.");
literal!(Category, "toolchain", "The measured category.");
literal!(OperationClass, "component", "The measured operation class.");
literal!(Surface, "core", "The measured performance surface.");
literal!(Requirement, "required", "A case the inventory cannot omit.");
literal!(
    ObservationalOnly,
    "observational-only-no-numeric-decisions",
    "This profile permits no numeric decision."
);

/// The exact run this record belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunBinding {
    pub measurement_run: RecordIdentity,
    pub measurement_run_plan: RecordIdentity,
    pub measurement_profile: VersionedIdentity,
    pub verification_subject: Subject,
    pub build_identity: BuildIdentity,
}

impl RunBinding {
    /// Bind a record to the Plan that was issued for its run.
    #[must_use]
    pub fn from_plan(plan: &RunPlanRecord) -> Self {
        Self {
            measurement_run: plan.body.measurement_run.clone(),
            measurement_run_plan: plan.identity().clone(),
            measurement_profile: plan.body.measurement_profile.clone(),
            verification_subject: plan.body.verification_subject.clone(),
            build_identity: plan.body.build_identity.clone(),
        }
    }
}

/// How an owner computes one native checksum.
///
/// The framing travels with every value the owner produces, so a consumer
/// never has to assume which one was used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecksumMethod {
    /// Which owner computed the value.
    pub owner_identity: Token,
    /// The digest algorithm, in the owner's registered spelling.
    pub algorithm: OwnerLabel,
    /// The owner's framing of the preimage.
    pub framing: OwnerLabel,
}

/// The owner result one Evidence Set was projected from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerBinding {
    owner_identity: Token,
    result_schema: OwnerLabel,
    benchmark_profile: VersionedIdentity,
    pub result_identity: OwnerRecordIdentity,
    algorithm: OwnerLabel,
    framing: OwnerLabel,
    domain: OwnerLabel,
    checksum: NativeChecksum,
}

/// The exact owner result an Evidence Set binds to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerResultFacts {
    /// How this owner computes its checksums.
    pub method: ChecksumMethod,
    /// The owner's registered result schema.
    pub result_schema: OwnerLabel,
    /// The benchmark profile the run executed under.
    pub benchmark_profile: VersionedIdentity,
    /// The owner result's own instance identity.
    pub result_identity: OwnerRecordIdentity,
    /// The digest domain the result checksum was computed in.
    pub domain: OwnerLabel,
    /// The result checksum itself.
    pub checksum: NativeChecksum,
}

impl OwnerBinding {
    /// Bind one Evidence Set to the owner result it was projected from.
    #[must_use]
    pub fn new(facts: OwnerResultFacts) -> Self {
        Self {
            owner_identity: facts.method.owner_identity,
            result_schema: facts.result_schema,
            benchmark_profile: facts.benchmark_profile,
            result_identity: facts.result_identity,
            algorithm: facts.method.algorithm,
            framing: facts.method.framing,
            domain: facts.domain,
            checksum: facts.checksum,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SemanticObservation<S> {
    owner_identity: Token,
    algorithm: OwnerLabel,
    framing: OwnerLabel,
    domain: OwnerLabel,
    identity: NativeChecksum,
    complete_observation: S,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExecutionIdentity {
    owner_result: OwnerRecordIdentity,
    algorithm: OwnerLabel,
    framing: OwnerLabel,
    domain: OwnerLabel,
    value: NativeChecksum,
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

/// One measured sample, as the owner captured it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleFacts<S> {
    /// The sample's position within its case.
    pub ordinal: Quantity,
    /// How many invocations this sample aggregates.
    pub repetition_count: Repetitions,
    /// The aggregate duration, in the metric's canonical unit.
    pub aggregate_quantity: Quantity,
    /// The owner's complete observation of the sample's output.
    pub semantic_observation: S,
    /// The identity of that observation, in the owner's semantic domain.
    pub semantic_identity: NativeChecksum,
    /// The execution identity, in the owner's execution domain.
    pub execution_identity: NativeChecksum,
    /// The owner's own local identity for this sample.
    pub local_identity: NativeChecksum,
}

/// The digest domains one owner uses inside a measured case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleDomains {
    /// The domain the semantic observation's identity is computed in.
    pub semantic: OwnerLabel,
    /// The domain the execution identity is computed in.
    pub execution: OwnerLabel,
}

/// One sample as the common Evidence Set carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sample<S> {
    pub local_record_identity: Token,
    pub ordinal: Quantity,
    pub repetition_count: Repetitions,
    pub aggregate_quantity: Quantity,
    semantic_observation: SemanticObservation<S>,
    execution_identity: ExecutionIdentity,
    determinism_proof: DeterminismProof,
    acquisition: Acquisition,
    owner_sample_identity: NativeChecksum,
}

impl<S> Sample<S> {
    fn project(
        method: &ChecksumMethod,
        domains: &SampleDomains,
        owner_result: &OwnerRecordIdentity,
        attempt: Quantity,
        sample: SampleFacts<S>,
    ) -> Result<Self, IdentityFailure> {
        Ok(Self {
            local_record_identity: Token::new(&format!(
                "measurement-sample-{}-{}",
                attempt.get(),
                sample.ordinal.get()
            ))?,
            ordinal: sample.ordinal,
            repetition_count: sample.repetition_count,
            aggregate_quantity: sample.aggregate_quantity,
            semantic_observation: SemanticObservation {
                owner_identity: method.owner_identity.clone(),
                algorithm: method.algorithm.clone(),
                framing: method.framing.clone(),
                domain: domains.semantic.clone(),
                identity: sample.semantic_identity,
                complete_observation: sample.semantic_observation,
            },
            execution_identity: ExecutionIdentity {
                owner_result: owner_result.clone(),
                algorithm: method.algorithm.clone(),
                framing: method.framing.clone(),
                domain: domains.execution.clone(),
                value: sample.execution_identity,
            },
            determinism_proof: DeterminismProof::NotRequired {},
            acquisition: Acquisition::Unpaired {},
            owner_sample_identity: sample.local_identity,
        })
    }
}

/// The sampling policy a measured case was captured under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SamplingPolicy {
    warmup_strategy: Token,
    warmup_repetitions: Quantity,
    warmup_completed: Quantity,
    measured_samples: Repetitions,
    repetitions_per_sample: Repetitions,
    aggregation: Aggregation,
    calibration: NoCalibration,
}

/// The sampling policy as the owner declared and executed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamplingFacts {
    /// The owner's registered warmup strategy.
    pub warmup_strategy: Token,
    /// How many warmup repetitions were declared.
    pub warmup_repetitions: Quantity,
    /// How many were actually completed.
    pub warmup_completed: Quantity,
    /// How many measured samples were declared.
    pub measured_samples: Repetitions,
    /// How many repetitions each sample aggregates.
    pub repetitions_per_sample: Repetitions,
}

/// One measured case, as the owner captured it.
pub struct MeasuredCase<D, S> {
    /// The attempt's position in the owner's run.
    pub ordinal: Quantity,
    /// The descriptors observed while measuring.
    pub descriptors: D,
    /// The sampling policy this case ran under.
    pub sampling: SamplingFacts,
    /// The captured samples, in order.
    pub samples: Vec<SampleFacts<S>>,
}

/// One measured case as the common Evidence Set carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaseEvidence<P, D, S> {
    pub local_record_identity: Token,
    pub case_identity: CaseIdentity,
    pub identity_projection: P,
    pub owner_attempt: Reference,
    pub observed_descriptors: D,
    sampling_policy: SamplingPolicy,
    #[schemars(length(min = 1))]
    pub samples: Vec<Sample<S>>,
}

/// Reference one attempt inside an owner's result record.
pub fn owner_attempt_reference(
    owner_result: &OwnerRecordIdentity,
    ordinal: Quantity,
) -> Result<Reference, IdentityFailure> {
    Ok(Reference::nested(
        owner_result,
        Token::new(&format!("attempt-{}", ordinal.get()))?,
    ))
}

impl<P, D, S> CaseEvidence<P, D, S> {
    /// Project one measured case into the common Evidence Set.
    pub fn project(
        method: &ChecksumMethod,
        domains: &SampleDomains,
        owner_result: &OwnerRecordIdentity,
        identity: &CaseIdentity,
        projection: P,
        case: MeasuredCase<D, S>,
    ) -> Result<Self, IdentityFailure> {
        let ordinal = case.ordinal;
        Ok(Self {
            local_record_identity: Token::new(&format!("measurement-case-{}", ordinal.get()))?,
            case_identity: identity.clone(),
            identity_projection: projection,
            owner_attempt: owner_attempt_reference(owner_result, ordinal)?,
            observed_descriptors: case.descriptors,
            sampling_policy: SamplingPolicy {
                warmup_strategy: case.sampling.warmup_strategy,
                warmup_repetitions: case.sampling.warmup_repetitions,
                warmup_completed: case.sampling.warmup_completed,
                measured_samples: case.sampling.measured_samples,
                repetitions_per_sample: case.sampling.repetitions_per_sample,
                aggregation: Aggregation::Value,
                calibration: NoCalibration::Value,
            },
            samples: case
                .samples
                .into_iter()
                .map(|sample| Sample::project(method, domains, owner_result, ordinal, sample))
                .collect::<Result<_, _>>()?,
        })
    }
}

/// The complete Evidence Set body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceBody<P, D, S> {
    pub binding: RunBinding,
    pub projection: VersionedIdentity,
    pub owner_result: OwnerBinding,
    pub build: Build,
    pub environment: Environment,
    #[schemars(length(min = 1))]
    pub cases: Vec<CaseEvidence<P, D, S>>,
}

/// One complete Measurement Evidence Set.
pub type Evidence<P, D, S> = Record<EvidenceKind, EvidenceBody<P, D, S>>;

/// The owner result a Run Evaluation expected to resolve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpectedOwner {
    pub record_identity: OwnerRecordIdentity,
    result_schema: OwnerLabel,
    benchmark_profile: VersionedIdentity,
}

impl ExpectedOwner {
    /// Name the owner result this run's Plan was issued for.
    #[must_use]
    pub fn new(
        record_identity: OwnerRecordIdentity,
        result_schema: OwnerLabel,
        benchmark_profile: VersionedIdentity,
    ) -> Self {
        Self {
            record_identity,
            result_schema,
            benchmark_profile,
        }
    }
}

fn present_reference<'de, D: Deserializer<'de>>(decoder: D) -> Result<Option<Reference>, D::Error> {
    Reference::deserialize(decoder).map(Some)
}

/// Whether the owner result resolved, and if not, exactly why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum InputState {
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

/// The owner result this evaluation expected, and what it got.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputResolution {
    pub expected: ExpectedOwner,
    pub result: InputState,
}

/// Why a planned case produced no measurement.
///
/// The kinds are separate because they call for different responses, and none
/// of them is a measured result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum UnavailableKind {
    Missing,
    Skipped,
    Unsupported,
    Failed,
    ProjectionIneligible,
    Stale,
}

/// What became of one planned case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CaseResult {
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

/// One planned case's evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaseEvaluation {
    pub local_record_identity: Token,
    pub case_identity: CaseIdentity,
    pub result: CaseResult,
}

impl CaseEvaluation {
    /// Select one planned case inside its Run Plan.
    #[must_use]
    pub fn selector(plan: &RunPlanRecord, case: &CaseIdentity) -> Selector {
        Selector::PlannedCase {
            run_plan: plan.identity().clone(),
            case_identity: case.clone(),
        }
    }
}

/// Whether a run produced a complete result for its planned inventory.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Complete,
    Incomplete,
    Invalid,
}

/// The complete Run Evaluation body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationBody {
    pub binding: RunBinding,
    pub owner_result_input: InputResolution,
    pub case_inventory: Vec<InventoryEntry>,
    pub cases: Vec<CaseEvaluation>,
    pub outcome: Outcome,
    pub reasons: Vec<Reason>,
}

/// One complete Measurement Run Evaluation.
pub type Evaluation = Record<EvaluationKind, EvaluationBody>;

/// One raw sample, as a report row presents it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportSample {
    pub local_record_identity: Token,
    pub raw_sample: Reference,
    pub ordinal: Quantity,
    pub repetition_count: Repetitions,
    pub aggregate_quantity: Quantity,
}

/// One measured case, as a report row presents it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Row {
    pub local_record_identity: Token,
    pub case_identity: CaseIdentity,
    pub case_evaluation: Reference,
    pub evidence: Reference,
    pub owner_phase: String,
    pub owner_cost: String,
    pub category: Category,
    pub operation_class: OperationClass,
    pub performance_surface: Surface,
    pub execution_state: Execution,
    pub metric: Metric,
    pub unit: Unit,
    pub sample_aggregation: Aggregation,
    pub samples: Vec<ReportSample>,
}

/// One planned case that produced no row, retained rather than dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MissingCase {
    pub local_record_identity: Token,
    pub case_identity: CaseIdentity,
    pub requirement: Requirement,
    pub evaluation: Reference,
    pub result: CaseResult,
}

/// Whether a report presents everything it evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Truncation {
    Complete {},
}

/// One report section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Section {
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

/// The complete structured report body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportBody {
    pub report_specification: VersionedIdentity,
    pub sections: Vec<Section>,
}

/// One complete structured report.
pub type Report = Record<ReportKind, ReportBody>;

/// Generate the complete Evidence Set representation.
pub fn evidence_schema<P, D, S>() -> Result<serde_json::Value, serde_json::Error>
where
    P: JsonSchema + 'static,
    D: JsonSchema + 'static,
    S: JsonSchema + 'static,
{
    crate::schema::draft7_record_schema::<Evidence<P, D, S>>("Evidence")
}

/// Generate the complete Run Evaluation representation.
pub fn evaluation_schema() -> Result<serde_json::Value, serde_json::Error> {
    crate::schema::draft7_record_schema::<Evaluation>("Evaluation")
}

/// Generate the complete structured report representation.
pub fn report_schema() -> Result<serde_json::Value, serde_json::Error> {
    crate::schema::draft7_record_schema::<Report>("Report")
}
