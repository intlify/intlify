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
use crate::memory::{DomainFailure, MemoryObservationDomain};
use crate::plan::{BuildIdentity, CaseProjection, InventoryEntry, RunPlanRecord, Subject};
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

// Only the values the owners measure are admitted: durations of toolchain
// components, and peak live bytes for design 015's memory cases. 026's wider
// vocabularies are not implemented here, and an unlisted value is rejected
// rather than silently carried. The variants have no doc comments, so each
// vocabulary stays one string enumeration in the generated schema.

/// How each sample aggregates its repetition batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Aggregation {
    // The total over the complete batch.
    BatchTotal,
    // The maximum over the batch, never divided by the repetition count.
    PeakOverBatch,
}

/// The measured metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    WallDuration,
    PeakLiveBytes,
}

impl Metric {
    /// The canonical unit, which the metric fixes.
    #[must_use]
    pub const fn unit(self) -> Unit {
        match self {
            Self::WallDuration => Unit::Nanosecond,
            Self::PeakLiveBytes => Unit::Octet,
        }
    }

    /// Whether revision `"0"` admits this metric with this aggregation.
    #[must_use]
    pub const fn admits(self, aggregation: Aggregation) -> bool {
        matches!(
            (self, aggregation),
            (Self::WallDuration, Aggregation::BatchTotal)
                | (Self::PeakLiveBytes, Aggregation::PeakOverBatch)
        )
    }

    /// Whether the metric observes memory, and so needs a Memory Observation
    /// Domain.
    #[must_use]
    pub const fn observes_memory(self) -> bool {
        matches!(self, Self::PeakLiveBytes)
    }
}

/// The canonical unit of the metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Nanosecond,
    Octet,
}

/// The measured category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Toolchain,
    Memory,
}

impl Category {
    /// Whether the operation class is one of this category's.
    #[must_use]
    pub const fn admits(self, class: OperationClass) -> bool {
        matches!(
            (self, class),
            (Self::Toolchain, OperationClass::Component) | (Self::Memory, OperationClass::PeakLive)
        )
    }
}

/// The measured operation class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationClass {
    Component,
    PeakLive,
}

literal!(NoCalibration, "none", "No calibration was applied.");
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

/// The dimensions one case declares in design 026's common vocabulary.
///
/// The report presents them, and they decide whether a measured case can be
/// projected at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommonDimensions<'a> {
    pub category: Category,
    pub operation_class: OperationClass,
    pub surface: Surface,
    pub memory_domain: Option<&'a MemoryObservationDomain>,
    pub metric: Metric,
    pub aggregation: Aggregation,
}

/// Why a measured case is not projected into the common records.
///
/// Each one makes the case `projection-ineligible`: it stays in the
/// evaluation as an unmeasured case, and no sample of it reaches the Evidence
/// Set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ineligibility {
    /// The operation class is not one of the category's.
    OperationClass,
    /// The metric observes memory and the category is not `memory`, or the
    /// other way around.
    Metric,
    /// Revision `"0"` does not admit the metric with the aggregation.
    Aggregation,
    /// A memory case declares no Memory Observation Domain.
    MissingMemoryDomain,
    /// A case that observes no memory declares a Memory Observation Domain.
    UnexpectedMemoryDomain,
    /// The Memory Observation Domain contradicts itself.
    MemoryDomain(DomainFailure),
}

impl CommonDimensions<'_> {
    /// Check the combinations revision `"0"` admits.
    ///
    /// # Errors
    ///
    /// Returns the first failure, in the order the variants are declared.
    pub fn eligibility(&self) -> Result<(), Ineligibility> {
        if !self.category.admits(self.operation_class) {
            return Err(Ineligibility::OperationClass);
        }
        if self.metric.observes_memory() != (self.category == Category::Memory) {
            return Err(Ineligibility::Metric);
        }
        if !self.metric.admits(self.aggregation) {
            return Err(Ineligibility::Aggregation);
        }
        match (self.metric.observes_memory(), self.memory_domain) {
            (true, None) => Err(Ineligibility::MissingMemoryDomain),
            (false, Some(_)) => Err(Ineligibility::UnexpectedMemoryDomain),
            (true, Some(domain)) => domain.check().map_err(Ineligibility::MemoryDomain),
            (false, None) => Ok(()),
        }
    }
}

/// Why one measured case did not become a case of the Evidence Set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionFailure {
    /// The case is outside what revision `"0"` admits.
    Ineligible(Ineligibility),
    /// A local identifier could not be issued.
    Identity(IdentityFailure),
}

impl From<IdentityFailure> for ProjectionFailure {
    fn from(value: IdentityFailure) -> Self {
        Self::Identity(value)
    }
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

impl<P: CaseProjection, D, S> CaseEvidence<P, D, S> {
    /// Project one measured case into the common Evidence Set.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectionFailure::Ineligible`] when the case's dimensions are
    /// outside what revision `"0"` admits, before any identifier is issued.
    pub fn project(
        method: &ChecksumMethod,
        domains: &SampleDomains,
        owner_result: &OwnerRecordIdentity,
        identity: &CaseIdentity,
        projection: P,
        case: MeasuredCase<D, S>,
    ) -> Result<Self, ProjectionFailure> {
        let common = projection.common();
        common
            .eligibility()
            .map_err(ProjectionFailure::Ineligible)?;
        let aggregation = common.aggregation;
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
                aggregation,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Components, Coverage, Inclusion, Reuse, StorageClasses};
    use serde_json::json;

    /// A projection that declares exactly the common dimensions it is given.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Declared {
        category: Category,
        operation_class: OperationClass,
        memory_domain: Option<MemoryObservationDomain>,
        metric: Metric,
        aggregation: Aggregation,
    }

    impl CaseProjection for Declared {
        fn phase(&self) -> &'static str {
            "test_phase"
        }
        fn cost(&self) -> &'static str {
            "test_cost"
        }
        fn common(&self) -> CommonDimensions<'_> {
            CommonDimensions {
                category: self.category,
                operation_class: self.operation_class,
                surface: Surface::Value,
                memory_domain: self.memory_domain.as_ref(),
                metric: self.metric,
                aggregation: self.aggregation,
            }
        }
    }

    fn domain() -> MemoryObservationDomain {
        MemoryObservationDomain {
            identity: VersionedIdentity::literal("intlify-measurement-test-memory", "0"),
            observer: Token::literal("test-allocator"),
            storage: StorageClasses {
                included: vec![Token::literal("invocation-scratch")],
                excluded: Vec::new(),
            },
            components: Components {
                harness: Inclusion::Excluded,
                language_runtime: Inclusion::Included,
                jit: Inclusion::NotApplicable,
                shared_libraries: Inclusion::Excluded,
                locale_data: Inclusion::NotApplicable,
            },
            coverage: Coverage {
                process: Token::literal("calling-process"),
                thread: Token::literal("calling-thread-only"),
                workers: Inclusion::NotApplicable,
                child_processes: Inclusion::NotApplicable,
            },
            reuse: Reuse {
                ownership: Token::literal("allocated-in-interval"),
                state: Token::literal("fresh"),
            },
        }
    }

    fn duration() -> Declared {
        Declared {
            category: Category::Toolchain,
            operation_class: OperationClass::Component,
            memory_domain: None,
            metric: Metric::WallDuration,
            aggregation: Aggregation::BatchTotal,
        }
    }

    fn memory() -> Declared {
        Declared {
            category: Category::Memory,
            operation_class: OperationClass::PeakLive,
            memory_domain: Some(domain()),
            metric: Metric::PeakLiveBytes,
            aggregation: Aggregation::PeakOverBatch,
        }
    }

    fn rejects<T: for<'de> Deserialize<'de>>(spelling: &str) -> bool {
        serde_json::from_value::<T>(json!(spelling)).is_err()
    }

    #[test]
    fn each_vocabulary_spells_the_026_values_its_owners_measure() {
        assert_eq!(
            serde_json::to_value([Aggregation::BatchTotal, Aggregation::PeakOverBatch]).unwrap(),
            json!(["batch_total", "peak_over_batch"])
        );
        assert_eq!(
            serde_json::to_value([Metric::WallDuration, Metric::PeakLiveBytes]).unwrap(),
            json!(["wall_duration", "peak_live_bytes"])
        );
        assert_eq!(
            serde_json::to_value([Unit::Nanosecond, Unit::Octet]).unwrap(),
            json!(["nanosecond", "octet"])
        );
        assert_eq!(
            serde_json::to_value([Category::Toolchain, Category::Memory]).unwrap(),
            json!(["toolchain", "memory"])
        );
        assert_eq!(
            serde_json::to_value([OperationClass::Component, OperationClass::PeakLive]).unwrap(),
            json!(["component", "peak_live"])
        );
    }

    #[test]
    fn a_wider_026_value_that_no_owner_measures_is_refused() {
        assert!(rejects::<Aggregation>("terminal_value"));
        assert!(rejects::<Aggregation>("deterministic_value"));
        assert!(rejects::<Metric>("peak_rss_bytes"));
        assert!(rejects::<Metric>("allocation_count"));
        assert!(rejects::<Unit>("count"));
        assert!(rejects::<Unit>("millisecond"));
        assert!(rejects::<Category>("artifact_size"));
        assert!(rejects::<OperationClass>("workflow"));
        assert!(rejects::<OperationClass>("allocation"));
        // Spellings are exact: no case folding and no other separator.
        assert!(rejects::<Metric>("peak-live-bytes"));
        assert!(rejects::<OperationClass>("peakLive"));
    }

    #[test]
    fn the_metric_fixes_the_unit() {
        assert_eq!(Metric::WallDuration.unit(), Unit::Nanosecond);
        assert_eq!(Metric::PeakLiveBytes.unit(), Unit::Octet);
        assert!(!Metric::WallDuration.observes_memory());
        assert!(Metric::PeakLiveBytes.observes_memory());
    }

    #[test]
    fn only_the_combinations_in_026s_matrix_are_admitted() {
        assert!(Metric::WallDuration.admits(Aggregation::BatchTotal));
        assert!(!Metric::WallDuration.admits(Aggregation::PeakOverBatch));
        assert!(!Metric::PeakLiveBytes.admits(Aggregation::BatchTotal));
        assert!(Metric::PeakLiveBytes.admits(Aggregation::PeakOverBatch));
    }

    #[test]
    fn a_category_admits_only_its_own_operation_classes() {
        assert!(Category::Toolchain.admits(OperationClass::Component));
        assert!(!Category::Toolchain.admits(OperationClass::PeakLive));
        assert!(!Category::Memory.admits(OperationClass::Component));
        assert!(Category::Memory.admits(OperationClass::PeakLive));
    }

    #[test]
    fn a_duration_case_and_a_memory_case_with_its_domain_are_eligible() {
        assert_eq!(duration().common().eligibility(), Ok(()));
        assert_eq!(memory().common().eligibility(), Ok(()));
    }

    #[test]
    fn each_combination_outside_revision_zero_is_named() {
        let changed = |base: fn() -> Declared, change: fn(&mut Declared)| {
            let mut declared = base();
            change(&mut declared);
            declared.common().eligibility()
        };
        for (base, change, expected) in [
            (
                duration as fn() -> Declared,
                (|declared: &mut Declared| declared.operation_class = OperationClass::PeakLive)
                    as fn(&mut Declared),
                Ineligibility::OperationClass,
            ),
            (
                memory,
                |declared| declared.operation_class = OperationClass::Component,
                Ineligibility::OperationClass,
            ),
            (
                memory,
                |declared| {
                    declared.metric = Metric::WallDuration;
                    declared.aggregation = Aggregation::BatchTotal;
                },
                Ineligibility::Metric,
            ),
            (
                duration,
                |declared| {
                    declared.metric = Metric::PeakLiveBytes;
                    declared.aggregation = Aggregation::PeakOverBatch;
                },
                Ineligibility::Metric,
            ),
            (
                duration,
                |declared| declared.aggregation = Aggregation::PeakOverBatch,
                Ineligibility::Aggregation,
            ),
            (
                memory,
                |declared| declared.aggregation = Aggregation::BatchTotal,
                Ineligibility::Aggregation,
            ),
            (
                memory,
                |declared| declared.memory_domain = None,
                Ineligibility::MissingMemoryDomain,
            ),
            (
                duration,
                |declared| declared.memory_domain = Some(domain()),
                Ineligibility::UnexpectedMemoryDomain,
            ),
            (
                memory,
                |declared| {
                    if let Some(domain) = &mut declared.memory_domain {
                        domain.storage.included.clear();
                    }
                },
                Ineligibility::MemoryDomain(DomainFailure::NothingIncluded),
            ),
        ] {
            assert_eq!(changed(base, change), Err(expected), "{expected:?}");
        }
    }

    #[test]
    fn the_first_combination_that_fails_is_the_one_named() {
        // A memory class under toolchain, a memory metric, and no domain: the
        // class is checked first.
        let mut declared = duration();
        declared.operation_class = OperationClass::PeakLive;
        declared.metric = Metric::PeakLiveBytes;
        assert_eq!(
            declared.common().eligibility(),
            Err(Ineligibility::OperationClass)
        );
        // A duration metric under memory with a peak aggregation: the metric
        // is checked before the aggregation.
        let mut declared = memory();
        declared.metric = Metric::WallDuration;
        assert_eq!(declared.common().eligibility(), Err(Ineligibility::Metric));
        // Outside the matrix and without a domain: the matrix is checked first.
        let mut declared = memory();
        declared.aggregation = Aggregation::BatchTotal;
        declared.memory_domain = None;
        assert_eq!(
            declared.common().eligibility(),
            Err(Ineligibility::Aggregation)
        );
    }

    fn measured(quantity: u64) -> MeasuredCase<(), ()> {
        let one = Repetitions::new(1).unwrap();
        MeasuredCase {
            ordinal: Quantity::new(0),
            descriptors: (),
            sampling: SamplingFacts {
                warmup_strategy: Token::literal("no-warmup"),
                warmup_repetitions: Quantity::new(0),
                warmup_completed: Quantity::new(0),
                measured_samples: one,
                repetitions_per_sample: one,
            },
            samples: vec![SampleFacts {
                ordinal: Quantity::new(0),
                repetition_count: one,
                aggregate_quantity: Quantity::new(quantity),
                semantic_observation: (),
                semantic_identity: NativeChecksum::from_bytes([1; 32]),
                execution_identity: NativeChecksum::from_bytes([2; 32]),
                local_identity: NativeChecksum::from_bytes([3; 32]),
            }],
        }
    }

    fn project(declared: Declared) -> Result<CaseEvidence<Declared, (), ()>, ProjectionFailure> {
        let method = ChecksumMethod {
            owner_identity: Token::literal("intlify-measurement-test"),
            algorithm: OwnerLabel::literal("blake3-256"),
            framing: OwnerLabel::literal("intlify-measurement-test-observation/0"),
        };
        let domains = SampleDomains {
            semantic: OwnerLabel::literal("observation"),
            execution: OwnerLabel::literal("sample-execution"),
        };
        let owner = OwnerRecordIdentity::fresh("intlify-measurement-test-result-v0").unwrap();
        let identity = crate::plan::case_identity(&declared).unwrap();
        CaseEvidence::project(
            &method,
            &domains,
            &owner,
            &identity,
            declared,
            measured(4096),
        )
    }

    #[test]
    fn a_case_samples_under_the_aggregation_its_metric_takes() {
        let case = project(duration()).unwrap();
        assert_eq!(case.sampling_policy.aggregation, Aggregation::BatchTotal);
        let case = project(memory()).unwrap();
        assert_eq!(case.sampling_policy.aggregation, Aggregation::PeakOverBatch);
        // The peak is the value observed, never divided by the repetitions.
        assert_eq!(case.samples[0].aggregate_quantity, Quantity::new(4096));
        assert_eq!(
            serde_json::to_value(&case).unwrap()["samplingPolicy"]["aggregation"],
            "peak_over_batch"
        );
    }

    #[test]
    fn an_ineligible_case_is_never_projected() {
        let mut declared = memory();
        declared.aggregation = Aggregation::BatchTotal;
        assert_eq!(
            project(declared).map(|_| ()),
            Err(ProjectionFailure::Ineligible(Ineligibility::Aggregation))
        );
        let mut declared = memory();
        declared.memory_domain = None;
        assert_eq!(
            project(declared).map(|_| ()),
            Err(ProjectionFailure::Ineligible(
                Ineligibility::MissingMemoryDomain
            ))
        );
    }
}
