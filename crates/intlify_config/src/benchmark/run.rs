// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! One-shot owner-run assembly around the six ordinary component operations.
//!
//! All fixtures, expectations, context, and ordering are fixed before capture.
//! Failed cases remain in the result; their prefixes never become measured rows.
//! The acquired run is separate from decoded input, including its original raw
//! observation checksum. A separate 017/026 Run Plan is issued before fixture
//! preparation, and the native result retains its exact reference and its own
//! immutable instance identity. Common Evidence/Run Evaluation, the complete
//! Environment Observation, and report admission remain separate requirements.

use serde::{Deserialize, Serialize};

use super::cases::registry::{
    AdmittedFixture, FixtureExpectation, FixtureFailure, Registry, RegistryFailure,
};
use super::collect::CollectionFailure;
use super::context::{
    CaptureContext, ContextAcquisitionIssue, ContextIssue, ContextObservation, ContextualOperation,
};
use super::measure::MeasurementFailure;
use super::observation::{Digest, Frame};
use super::profile::ProfileCollectionFailure;
use super::quantity::Quantity;
use super::sample::{CaptureBinding, CaptureFailureCause};
use super::shared::identity::{InstanceDomain, RecordIdentity};
use super::shared::plan::{IssuedRunPlan, PlanFailure, RunPlanRecord};
use super::work::WorkFailure;

const PLAN_CODEC: &str = "intlify-config-owner-run-plan/1";
const RESULT_CODEC: &str = "intlify-config-owner-run-result/1";
// Private, bounded developer input; not a project Resource Limit Policy default.
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Requirement {
    RequiredUnconditional,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlannedCase {
    case: Digest,
    requirement: Requirement,
    expectation: FixtureExpectation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Preparation {
    AllFixturesBeforeCapture,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OwnerPlan {
    codec: String,
    run: Digest,
    result_identity: RecordIdentity,
    common_run_plan: RecordIdentity,
    context: Digest,
    preparation: Preparation,
    cases: Vec<PlannedCase>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum OwnerOutcome {
    Complete,
    Incomplete,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "detail",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub(super) enum AttemptResult {
    // Boxing is after capture, outside every measured component interval. A
    // failed attempt need not reserve space for the large successful payload.
    Measured(Box<ContextualOperation>),
    PreparationFailed(FixtureFailure),
    CollectionFailed(ProfileCollectionFailure),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CaseAttempt {
    pub(super) ordinal: Quantity,
    pub(super) binding: CaptureBinding,
    pub(super) result: AttemptResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OwnerResult {
    pub(super) codec: String,
    pub(super) record_identity: RecordIdentity,
    plan: OwnerPlan,
    pub(super) context: ContextObservation,
    pub(super) attempts: Vec<CaseAttempt>,
    pub(super) outcome: OwnerOutcome,
}

/// Serializable acquisition output, not self-authenticating evidence. Its
/// checksum binds every raw field but is neither a signature nor a shared ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OwnerRecord {
    checksum: Digest,
    result: OwnerResult,
}

impl OwnerRecord {
    pub(super) fn result(&self) -> &OwnerResult {
        &self.result
    }
    pub(super) const fn checksum(&self) -> Digest {
        self.checksum
    }
    pub(super) fn plan_reference(&self) -> &RecordIdentity {
        &self.result.plan.common_run_plan
    }
}

/// Only the separately retained acquisition can issue this borrow. Raw owner
/// documents remain inspectable but cannot enter the common projection alone.
pub(super) struct ProjectionSource<'a>(&'a OwnerRecord);
impl ProjectionSource<'_> {
    pub(super) fn document(&self) -> &OwnerRecord {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RunFailure {
    Registry(RegistryFailure),
    Context(ContextAcquisitionIssue),
    EntropyUnavailable,
    SharedPlan(PlanFailure),
    Addressability,
    Allocation,
    PreparedInventory,
    Encoding,
    SizeLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RunIssue {
    Codec,
    Integrity,
    RecordedObservation,
    RecordIdentity,
    Plan,
    Context(ContextIssue),
    AttemptCount,
    CaseBinding(Quantity),
    Preparation(Quantity),
    Operation {
        ordinal: Quantity,
        issue: ContextIssue,
    },
    Outcome,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum DecodeFailure {
    SizeLimit,
    Malformed,
    Invalid(Vec<RunIssue>),
}

/// No Clone/Deserialize/mutable getter. Consuming capture issues exactly one
/// result for this fresh run. Context/preparation failures issue no run result.
pub(super) struct PreparedRun {
    context: CaptureContext,
    plan: OwnerPlan,
    common_plan: IssuedRunPlan,
    fixtures: Vec<Result<AdmittedFixture, FixtureFailure>>,
}

/// Retains acquisition authority independently from any submitted document.
/// Revalidation never executes another timed run or learns new expected values.
pub(super) struct RecordedRun {
    inputs: PreparedRun,
    record: OwnerRecord,
}

/// Checked serialization of this owner run, not common measurement admission.
/// Incomplete/invalid benchmark outcomes are valid diagnostic owner documents.
pub(super) struct CheckedOwnerRecord(OwnerRecord);

impl CheckedOwnerRecord {
    pub(super) fn document(&self) -> &OwnerRecord {
        &self.0
    }

    pub(super) fn projection_source(&self) -> ProjectionSource<'_> {
        ProjectionSource(&self.0)
    }
}

fn checksum(domain: &str, value: &impl Serialize) -> Result<Digest, RunFailure> {
    let value = serde_json::to_value(value).map_err(|_| RunFailure::Encoding)?;
    let mut frame = Frame::new(domain);
    frame.json(&value);
    Ok(frame.finish())
}

fn fresh_run(context: Digest) -> Result<Digest, RunFailure> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| RunFailure::EntropyUnavailable)?;
        let mut frame = Frame::new("owner-run");
        frame.digest(context);
        frame.bytes(&nonce);
        Ok(frame.finish())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = context;
        Err(RunFailure::EntropyUnavailable)
    }
}

impl PreparedRun {
    pub(super) fn acquire() -> Result<Self, RunFailure> {
        let registry = Registry::load().map_err(RunFailure::Registry)?;
        let context = CaptureContext::acquire(&registry).map_err(RunFailure::Context)?;
        let context_binding = checksum("owner-run-context", &context.observation())?;
        let run = fresh_run(context_binding)?;
        let common_plan =
            IssuedRunPlan::issue(&registry, &context).map_err(RunFailure::SharedPlan)?;
        let result_identity = RecordIdentity::fresh(InstanceDomain::NativeOwnerResult)
            .map_err(|_| RunFailure::EntropyUnavailable)?;
        let count = registry.expectations().len();
        u64::try_from(count).map_err(|_| RunFailure::Addressability)?;
        let mut cases = Vec::new();
        let mut fixtures = Vec::new();
        cases
            .try_reserve_exact(count)
            .map_err(|_| RunFailure::Allocation)?;
        fixtures
            .try_reserve_exact(count)
            .map_err(|_| RunFailure::Allocation)?;
        for (declaration, expectation) in context
            .profile()
            .cases()
            .iter()
            .zip(registry.expectations())
        {
            // Stable case binding has no run nonce, ordinal, clock, or build.
            // Exact expectations include the input, operation, and owner scope.
            let case = checksum("owner-planned-case", &(registry.binding(), expectation))?;
            cases.push(PlannedCase {
                case,
                requirement: Requirement::RequiredUnconditional,
                expectation: expectation.clone(),
            });
            fixtures.push(registry.prepare(declaration));
        }
        Ok(Self {
            context,
            plan: OwnerPlan {
                codec: PLAN_CODEC.into(),
                run,
                result_identity,
                common_run_plan: common_plan.document().identity().clone(),
                context: context_binding,
                preparation: Preparation::AllFixturesBeforeCapture,
                cases,
            },
            common_plan,
            fixtures,
        })
    }

    pub(super) fn collect(self) -> Result<RecordedRun, RunFailure> {
        self.collect_with(CaptureContext::collect)
    }

    pub(super) fn common_plan(&self) -> &IssuedRunPlan {
        &self.common_plan
    }

    // Private injection seam tests orchestration failures; the ordinary entry
    // always calls the real context-owned collector, never another algorithm.
    fn collect_with(
        self,
        mut collect: impl FnMut(
            &CaptureContext,
            Quantity,
            &AdmittedFixture,
            CaptureBinding,
        ) -> Result<ContextualOperation, ProfileCollectionFailure>,
    ) -> Result<RecordedRun, RunFailure> {
        if self.fixtures.len() != self.plan.cases.len() {
            return Err(RunFailure::PreparedInventory);
        }
        let mut attempts = Vec::new();
        attempts
            .try_reserve_exact(self.plan.cases.len())
            .map_err(|_| RunFailure::Allocation)?;
        for (index, (planned, fixture)) in self.plan.cases.iter().zip(&self.fixtures).enumerate() {
            let ordinal = Quantity::new(u64::try_from(index).expect("pre-admitted case count"));
            let binding = CaptureBinding {
                run: self.plan.run,
                case: planned.case,
            };
            let result = match fixture {
                Ok(fixture) => match collect(&self.context, ordinal, fixture, binding) {
                    Ok(operation) => AttemptResult::Measured(Box::new(operation)),
                    Err(failure) => AttemptResult::CollectionFailed(failure),
                },
                Err(failure) => AttemptResult::PreparationFailed(*failure),
            };
            attempts.push(CaseAttempt {
                ordinal,
                binding,
                result,
            });
        }
        let result = OwnerResult {
            codec: RESULT_CODEC.into(),
            record_identity: self.plan.result_identity.clone(),
            plan: self.plan.clone(),
            context: self.context.observation(),
            outcome: outcome(&attempts),
            attempts,
        };
        let record = OwnerRecord {
            checksum: checksum("owner-run-result", &result)?,
            result,
        };
        Ok(RecordedRun {
            inputs: self,
            record,
        })
    }
}

impl RecordedRun {
    pub(super) fn expected_owner_identity(&self) -> &RecordIdentity {
        &self.inputs.plan.result_identity
    }

    pub(super) fn admit_owned(
        &self,
        submitted: OwnerRecord,
    ) -> Result<CheckedOwnerRecord, Vec<RunIssue>> {
        let issues = self.validate(&submitted);
        if issues.is_empty() {
            Ok(CheckedOwnerRecord(submitted))
        } else {
            Err(issues)
        }
    }
    pub(super) fn projection_source<'a>(
        &self,
        submitted: &'a OwnerRecord,
    ) -> Result<ProjectionSource<'a>, Vec<RunIssue>> {
        let issues = self.validate(submitted);
        if issues.is_empty() {
            Ok(ProjectionSource(submitted))
        } else {
            Err(issues)
        }
    }

    pub(super) fn document(&self) -> &OwnerRecord {
        &self.record
    }
    pub(super) fn common_plan(&self) -> &IssuedRunPlan {
        &self.inputs.common_plan
    }

    pub(super) fn plan_record(&self) -> &RunPlanRecord {
        self.inputs.common_plan.document()
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, RunFailure> {
        let bytes = serde_json::to_vec(&self.record).map_err(|_| RunFailure::Encoding)?;
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(RunFailure::SizeLimit);
        }
        Ok(bytes)
    }

    pub(super) fn decode_checked(&self, bytes: &[u8]) -> Result<CheckedOwnerRecord, DecodeFailure> {
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(DecodeFailure::SizeLimit);
        }
        let record: OwnerRecord =
            serde_json::from_slice(bytes).map_err(|_| DecodeFailure::Malformed)?;
        let issues = self.validate(&record);
        if issues.is_empty() {
            Ok(CheckedOwnerRecord(record))
        } else {
            Err(DecodeFailure::Invalid(issues))
        }
    }

    pub(super) fn validate(&self, submitted: &OwnerRecord) -> Vec<RunIssue> {
        let mut issues = Vec::new();
        let result = &submitted.result;
        if result.codec != RESULT_CODEC {
            issues.push(RunIssue::Codec);
        }
        if result.record_identity != self.inputs.plan.result_identity {
            issues.push(RunIssue::RecordIdentity);
        }
        let actual = checksum("owner-run-result", result);
        if actual != Ok(submitted.checksum) {
            issues.push(RunIssue::Integrity);
        }
        // Recomputing a changed payload's checksum does not prove acquisition.
        if actual != Ok(self.record.checksum) {
            issues.push(RunIssue::RecordedObservation);
        }
        if result.plan != self.inputs.plan {
            issues.push(RunIssue::Plan);
        }
        issues.extend(
            self.inputs
                .context
                .validate(&result.context)
                .into_iter()
                .map(RunIssue::Context),
        );
        if result.attempts.len() != self.inputs.plan.cases.len() {
            issues.push(RunIssue::AttemptCount);
        }
        for (index, ((attempt, planned), fixture)) in result
            .attempts
            .iter()
            .zip(&self.inputs.plan.cases)
            .zip(&self.inputs.fixtures)
            .enumerate()
        {
            let ordinal = Quantity::new(u64::try_from(index).expect("pre-admitted case count"));
            let binding = CaptureBinding {
                run: self.inputs.plan.run,
                case: planned.case,
            };
            if attempt.ordinal != ordinal || attempt.binding != binding {
                issues.push(RunIssue::CaseBinding(ordinal));
            }
            match (&attempt.result, fixture) {
                (AttemptResult::Measured(operation), Ok(fixture)) => issues.extend(
                    operation
                        .validate(&self.inputs.context, ordinal, fixture, binding)
                        .into_iter()
                        .map(|issue| RunIssue::Operation { ordinal, issue }),
                ),
                (AttemptResult::PreparationFailed(actual), Err(expected)) if actual == expected => {
                }
                (AttemptResult::CollectionFailed(failure), Ok(fixture)) => issues.extend(
                    self.inputs
                        .context
                        .validate_failure(ordinal, fixture, binding, failure)
                        .into_iter()
                        .map(|issue| RunIssue::Operation { ordinal, issue }),
                ),
                _ => issues.push(RunIssue::Preparation(ordinal)),
            }
        }
        if result.outcome != outcome(&result.attempts) {
            issues.push(RunIssue::Outcome);
        }
        issues
    }
}

fn outcome(attempts: &[CaseAttempt]) -> OwnerOutcome {
    attempts
        .iter()
        .map(|attempt| match &attempt.result {
            AttemptResult::Measured(_) => OwnerOutcome::Complete,
            AttemptResult::CollectionFailed(ProfileCollectionFailure::Collection(
                CollectionFailure::Capture(failure),
            )) => match &failure.cause {
                CaptureFailureCause::SemanticObservationMismatch(_)
                | CaptureFailureCause::LogicalWorkMismatch(_)
                | CaptureFailureCause::Output(_)
                | CaptureFailureCause::PrerequisiteUnavailable
                | CaptureFailureCause::Measurement(MeasurementFailure::PrerequisiteUnavailable)
                | CaptureFailureCause::LogicalWorkObservation(
                    WorkFailure::OperationMismatch | WorkFailure::InvalidOrdinaryResult,
                ) => OwnerOutcome::Invalid,
                CaptureFailureCause::CollectorAllocation
                | CaptureFailureCause::Measurement(_)
                | CaptureFailureCause::ObservationPanicked
                | CaptureFailureCause::MeasurementOverflow
                | CaptureFailureCause::LogicalWorkObservation(
                    WorkFailure::UnrepresentableCounter,
                ) => OwnerOutcome::Incomplete,
            },
            AttemptResult::PreparationFailed(_) | AttemptResult::CollectionFailed(_) => {
                OwnerOutcome::Invalid
            }
        })
        .max()
        .unwrap_or(OwnerOutcome::Invalid)
}

pub(super) fn attempt_outcome(attempt: &CaseAttempt) -> OwnerOutcome {
    outcome(std::slice::from_ref(attempt))
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
