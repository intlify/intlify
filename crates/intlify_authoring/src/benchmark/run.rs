// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! One owner run, from issuance through capture to its own retained record.
//!
//! Every fixture, expectation, and ordering is fixed before capture begins. A
//! case that produced no measurement stays in the result; a prefix of a failed
//! case never becomes a measured row. The acquired run is retained separately
//! from any submitted document, so revalidation checks a submission against
//! what was captured rather than against the submission itself.

use intlify_measurement::identity::{OwnerRecordIdentity, RecordIdentity, VersionedIdentity};
use intlify_measurement::plan::{Issuance, IssuedRunPlan, PlanFailure, RunPlanRecord};
use intlify_shared_json::quantity::{Quantity, Repetitions};
use serde::{Deserialize, Serialize};

use super::capture::{capture, Binding, Capture, CaptureFailure, Sampling};
use super::cases::{prepare, Expected, PreparationFailure, Prepared, FIXTURES};
use super::context::{harness, CaptureContext, ContextObservation, SamplingPolicy};
use super::descriptor::Descriptors;
use super::observation::{Digest, Frame};
use super::operation::LogicalWork;
use super::projection::{profile, subject, CaseProjection};

/// The registered spellings this owner produces.
pub(super) const RESULT_CODEC: &str = "intlify-authoring-owner-run-result/1";
pub(super) const RESULT_DOMAIN: &str = "intlify-authoring-owner-result-v1";
const RUNNER_DOMAIN: &str = "intlify-authoring-local-runner-instance-v0";
const PLAN_CODEC: &str = "intlify-authoring-owner-run-plan/1";
// Private reader capacity, not a project-wide resource limit.
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Requirement {
    RequiredUnconditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Preparation {
    AllFixturesBeforeCapture,
}

/// What one case was expected to produce, fixed before it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Expectation {
    fixture: String,
    path: Expected,
    observation: super::observation::Observation,
    work: LogicalWork,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlannedCase {
    case: Digest,
    requirement: Requirement,
    expectation: Expectation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OwnerPlan {
    codec: String,
    run: Digest,
    result_identity: OwnerRecordIdentity,
    common_run_plan: RecordIdentity,
    context: Digest,
    preparation: Preparation,
    cases: Vec<PlannedCase>,
}

/// Whether the run produced a complete result for its planned inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum OwnerOutcome {
    Complete,
    Incomplete,
    Invalid,
}

/// What became of one planned case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "detail",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub(super) enum AttemptResult {
    // Boxing happens after capture, outside every measured interval: a failed
    // attempt need not reserve room for the large successful payload.
    Measured(Box<MeasuredCase>),
    PreparationFailed(PreparationFailure),
    CaptureFailed(CaptureFailure),
}

/// One measured case, as the owner recorded it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MeasuredCase {
    pub(super) descriptors: Descriptors,
    pub(super) capture: Capture,
}

/// One attempt at one planned case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CaseAttempt {
    pub(super) ordinal: Quantity,
    pub(super) binding: Binding,
    pub(super) result: AttemptResult,
}

/// The complete owner result of one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OwnerResult {
    pub(super) codec: String,
    pub(super) record_identity: OwnerRecordIdentity,
    plan: OwnerPlan,
    pub(super) context: ContextObservation,
    pub(super) attempts: Vec<CaseAttempt>,
    pub(super) outcome: OwnerOutcome,
}

/// Serializable acquisition output, not self-authenticating evidence.
///
/// The checksum binds every raw field, but it is neither a signature nor a
/// shared identity: recomputing it over changed content proves nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OwnerRecord {
    checksum: Digest,
    result: OwnerResult,
}

impl OwnerRecord {
    pub(super) const fn result(&self) -> &OwnerResult {
        &self.result
    }
    pub(super) const fn checksum(&self) -> Digest {
        self.checksum
    }
    pub(super) const fn plan_reference(&self) -> &RecordIdentity {
        &self.result.plan.common_run_plan
    }
}

/// Complete failure to acquire one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunFailure {
    ClockUnavailable,
    EntropyUnavailable,
    Plan(PlanFailure),
    Encoding,
    SizeLimit,
}

/// Why a submitted document is not this run's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RunIssue {
    Codec,
    Integrity,
    RecordedObservation,
    RecordIdentity,
    Plan,
    Context,
    AttemptCount,
    CaseBinding,
    Attempt,
    Outcome,
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

/// The sampling the first smoke profile performs.
fn sampling() -> (SamplingPolicy, Sampling) {
    let warmup = Quantity::new(2);
    let samples = Repetitions::new(1).expect("one measured sample");
    let repetitions = Repetitions::new(1).expect("one repetition");
    (
        SamplingPolicy {
            warmup_repetitions: warmup,
            measured_samples: samples,
            repetitions_per_sample: repetitions,
        },
        Sampling {
            warmup,
            samples,
            repetitions,
        },
    )
}

/// A run whose inputs are fixed and whose Plan is issued, before any capture.
///
/// No Clone and no Deserialize: consuming the capture issues exactly one
/// result for this one run.
pub(super) struct PreparedRun {
    context: CaptureContext,
    plan: OwnerPlan,
    common_plan: IssuedRunPlan<CaseProjection>,
    prepared: Vec<Result<Prepared, PreparationFailure>>,
    sampling: Sampling,
}

/// A run that has been captured, retaining its own acquisition authority.
pub(super) struct RecordedRun {
    inputs: PreparedRun,
    record: OwnerRecord,
}

/// A submitted document this run has admitted as its own result.
pub(super) struct CheckedOwnerRecord(OwnerRecord);

impl CheckedOwnerRecord {
    pub(super) const fn document(&self) -> &OwnerRecord {
        &self.0
    }
}

impl PreparedRun {
    /// Fix every input and issue the Plan, before any interval opens.
    pub(super) fn acquire() -> Result<Self, RunFailure> {
        let (policy, sampling) = sampling();
        let context =
            CaptureContext::acquire(profile(), policy).map_err(|_| RunFailure::ClockUnavailable)?;
        let context_binding = checksum("owner-run-context", context.observation())?;
        let run = fresh_run(context_binding)?;
        let result_identity = OwnerRecordIdentity::fresh(RESULT_DOMAIN)
            .map_err(|_| RunFailure::EntropyUnavailable)?;

        let prepared = FIXTURES.map(prepare).into_iter().collect::<Vec<_>>();
        let mut projections = Vec::with_capacity(prepared.len());
        let mut cases = Vec::with_capacity(prepared.len());
        for (fixture, prepared) in FIXTURES.iter().zip(&prepared) {
            let Ok(ready) = prepared else {
                // A fixture that will not prepare still has a planned case, so
                // its absence from the results is visible rather than silent.
                projections.push(None);
                cases.push(None);
                continue;
            };
            let projection = CaseProjection::of(ready);
            let expectation = Expectation {
                fixture: fixture.name.into(),
                path: fixture.expected,
                observation: ready.expected().observation,
                work: ready.expected().work.clone(),
            };
            // The case binding has no run nonce, ordinal, clock, or build: two
            // runs of the same fixture bind to the same case.
            cases.push(Some(PlannedCase {
                case: checksum("owner-planned-case", &(&projection, &expectation))?,
                requirement: Requirement::RequiredUnconditional,
                expectation,
            }));
            projections.push(Some(projection));
        }
        if cases.iter().any(Option::is_none) {
            // Phase 1's fixtures are fixed literals; one that cannot prepare is
            // a defect in this harness rather than an input this run can plan.
            return Err(RunFailure::Plan(PlanFailure::InvalidRecord));
        }
        let cases = cases.into_iter().flatten().collect::<Vec<_>>();
        let projections = projections.into_iter().flatten().collect::<Vec<_>>();

        let common_plan = IssuedRunPlan::issue(Issuance {
            projections,
            measurement_profile: profile(),
            verification_subject: subject(),
            build_identity: context.observation().build.identity(),
            planned_runner_class: None,
            runner_instance_identity: OwnerRecordIdentity::fresh(RUNNER_DOMAIN)
                .map_err(|_| RunFailure::EntropyUnavailable)?,
            producing_tool: producing_tool(),
        })
        .map_err(RunFailure::Plan)?;

        Ok(Self {
            plan: OwnerPlan {
                codec: PLAN_CODEC.into(),
                run,
                result_identity,
                common_run_plan: common_plan.document().identity().clone(),
                context: context_binding,
                preparation: Preparation::AllFixturesBeforeCapture,
                cases,
            },
            context,
            common_plan,
            prepared,
            sampling,
        })
    }

    /// Capture every planned case, in the planned order.
    pub(super) fn collect(self) -> Result<RecordedRun, RunFailure> {
        let mut attempts = Vec::with_capacity(self.plan.cases.len());
        for (index, (planned, prepared)) in self.plan.cases.iter().zip(&self.prepared).enumerate() {
            let ordinal = Quantity::new(u64::try_from(index).expect("pre-admitted case count"));
            let binding = Binding {
                run: self.plan.run,
                case: planned.case,
            };
            let result = match prepared {
                Err(failure) => AttemptResult::PreparationFailed(*failure),
                Ok(prepared) => {
                    match capture(self.context.clock(), prepared, self.sampling, binding) {
                        Ok(capture) => AttemptResult::Measured(Box::new(MeasuredCase {
                            descriptors: Descriptors::for_acquisition(
                                prepared.fixture.operation,
                                self.context.observation().clock.clone(),
                            ),
                            capture,
                        })),
                        Err(failure) => AttemptResult::CaptureFailed(failure),
                    }
                }
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
            context: self.context.observation().clone(),
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
    pub(super) const fn expected_owner_identity(&self) -> &OwnerRecordIdentity {
        &self.inputs.plan.result_identity
    }

    pub(super) fn plan_record(&self) -> &RunPlanRecord {
        self.inputs.common_plan.document()
    }

    pub(super) const fn common_plan(&self) -> &IssuedRunPlan<CaseProjection> {
        &self.inputs.common_plan
    }

    pub(super) const fn context(&self) -> &ContextObservation {
        self.inputs.context.observation()
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, RunFailure> {
        let bytes = serde_json::to_vec(&self.record).map_err(|_| RunFailure::Encoding)?;
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(RunFailure::SizeLimit);
        }
        Ok(bytes)
    }

    /// Admit a submitted document only when it is what this run captured.
    pub(super) fn admit(
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
        if result.context != *self.inputs.context.observation() {
            issues.push(RunIssue::Context);
        }
        if result.attempts.len() != self.record.result.attempts.len() {
            issues.push(RunIssue::AttemptCount);
            return issues;
        }
        for (submitted, captured) in result.attempts.iter().zip(&self.record.result.attempts) {
            if submitted.ordinal != captured.ordinal || submitted.binding != captured.binding {
                issues.push(RunIssue::CaseBinding);
            }
            if submitted.result != captured.result {
                issues.push(RunIssue::Attempt);
            }
        }
        if result.outcome != outcome(&result.attempts) {
            issues.push(RunIssue::Outcome);
        }
        issues
    }
}

pub(super) fn producing_tool() -> VersionedIdentity {
    VersionedIdentity::new(harness().identity().as_str(), env!("CARGO_PKG_VERSION"))
        .expect("registered producing tool")
}

fn outcome(attempts: &[CaseAttempt]) -> OwnerOutcome {
    attempts
        .iter()
        .map(|attempt| match &attempt.result {
            AttemptResult::Measured(_) => OwnerOutcome::Complete,
            // A failure to read the clock or to sum an interval leaves the run
            // incomplete. A wrong result, or a fixture that will not prepare,
            // invalidates it: neither is a missing measurement.
            AttemptResult::CaptureFailed(
                CaptureFailure::Measurement(_) | CaptureFailure::Overflow,
            ) => OwnerOutcome::Incomplete,
            AttemptResult::CaptureFailed(_) | AttemptResult::PreparationFailed(_) => {
                OwnerOutcome::Invalid
            }
        })
        .max()
        .unwrap_or(OwnerOutcome::Invalid)
}
