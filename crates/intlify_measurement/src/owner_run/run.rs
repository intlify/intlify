// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! One owner run, from issuance through capture to its own retained record.
//!
//! Every fixture, expectation, and ordering is fixed before capture begins. A
//! case that produced no measurement stays in the result; a prefix of a failed
//! case never becomes a measured row. The acquired run is retained separately
//! from any submitted document, so revalidation checks a submission against
//! what was captured rather than against the submission itself.

use intlify_shared_json::quantity::Quantity;
use serde::{Deserialize, Serialize};

use super::capture::{Binding, Capture, CaptureFailure, Capturing};
use super::context::{BuildObservation, CaptureContext, ContextObservation};
use super::observation::{Digest, Framing, Observation};
use super::{Case, Owner};
use crate::identity::{OwnerRecordIdentity, RecordIdentity, VersionedIdentity};
use crate::plan::{Issuance, IssuedRunPlan, PlanFailure, RunPlanRecord, Subject, SubjectKind};

// Private reader capacity, not a project-wide resource limit.
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;

/// Which path a fixture is declared to take.
///
/// A fixture states its expected result, so a change that silently moves it
/// onto another path — a fast failure instead of the work it claims to
/// measure — fails a test rather than producing plausible samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Expected {
    /// The operation produces its complete result.
    Complete,
    /// The operation reports why it cannot, which is also measured.
    Blocked,
}

/// A fixture could not be prepared, so its case produced no measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PreparationFailure {
    /// The fixed inputs the harness declares are not themselves admissible.
    Fixture,
    /// The prior stage a measured boundary starts from did not complete.
    PriorStage,
}

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
struct Expectation<W> {
    fixture: String,
    path: Expected,
    observation: Observation,
    work: W,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlannedCase<W> {
    case: Digest,
    requirement: Requirement,
    expectation: Expectation<W>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OwnerPlan<W> {
    codec: String,
    run: Digest,
    result_identity: OwnerRecordIdentity,
    common_run_plan: RecordIdentity,
    context: Digest,
    preparation: Preparation,
    cases: Vec<PlannedCase<W>>,
}

/// Whether the run produced a complete result for its planned inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OwnerOutcome {
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
pub enum AttemptResult<D, W> {
    // Boxing happens after capture, outside every measured interval: a failed
    // attempt need not reserve room for the large successful payload.
    Measured(Box<MeasuredCase<D, W>>),
    PreparationFailed(PreparationFailure),
    CaptureFailed(CaptureFailure),
}

/// One measured case, as the owner recorded it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeasuredCase<D, W> {
    pub descriptors: D,
    pub capture: Capture<W>,
}

/// One attempt at one planned case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaseAttempt<D, W> {
    pub ordinal: Quantity,
    pub binding: Binding,
    pub result: AttemptResult<D, W>,
}

/// The complete owner result of one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerResult<D, W> {
    pub codec: String,
    pub record_identity: OwnerRecordIdentity,
    plan: OwnerPlan<W>,
    pub context: ContextObservation,
    pub attempts: Vec<CaseAttempt<D, W>>,
    pub outcome: OwnerOutcome,
}

/// Serializable acquisition output, not self-authenticating evidence.
///
/// The checksum binds every raw field, but it is neither a signature nor a
/// shared identity: recomputing it over changed content proves nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerRecord<D, W> {
    checksum: Digest,
    result: OwnerResult<D, W>,
}

impl<D, W> OwnerRecord<D, W> {
    /// Borrow the result the checksum covers.
    pub const fn result(&self) -> &OwnerResult<D, W> {
        &self.result
    }
    /// Return the checksum as recorded.
    pub const fn checksum(&self) -> Digest {
        self.checksum
    }
    /// Borrow the common Run Plan this result says it was issued under.
    pub const fn plan_reference(&self) -> &RecordIdentity {
        &self.result.plan.common_run_plan
    }
}

/// The owner record an owner's descriptors and work produce.
pub type RecordOf<O> = OwnerRecord<<O as Owner>::Descriptors, <O as Owner>::Work>;
type AttemptOf<O> = CaseAttempt<<O as Owner>::Descriptors, <O as Owner>::Work>;

/// Complete failure to acquire one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunFailure {
    ClockUnavailable,
    EntropyUnavailable,
    /// A fixture's expectation takes the other path than the one it declares.
    PathMismatch,
    Plan(PlanFailure),
    Encoding,
    SizeLimit,
}

/// Why a submitted document is not this run's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunIssue {
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

fn checksum(framing: Framing, domain: &str, value: &impl Serialize) -> Result<Digest, RunFailure> {
    let value = serde_json::to_value(value).map_err(|_| RunFailure::Encoding)?;
    let mut frame = framing.frame(domain);
    frame.json(&value);
    Ok(frame.finish())
}

fn fresh_run(framing: Framing, context: Digest) -> Result<Digest, RunFailure> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| RunFailure::EntropyUnavailable)?;
        let mut frame = framing.frame("owner-run");
        frame.digest(context);
        frame.bytes(&nonce);
        Ok(frame.finish())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (framing, context);
        Err(RunFailure::EntropyUnavailable)
    }
}

/// Name the tool that produced an owner's records.
///
/// It is the owner's harness at the owner's own package version: the records
/// describe that owner's run, not this crate.
#[must_use]
pub fn producing_tool<O: Owner>() -> VersionedIdentity {
    VersionedIdentity::new(O::LABELS.harness.identity, O::PACKAGE.version)
        .expect("registered producing tool")
}

/// What an owner measures, as the Plan and its projections name it.
#[must_use]
pub fn subject<O: Owner>() -> Subject {
    Subject {
        kind: SubjectKind::Value,
        identity: crate::identity::Token::literal(O::LABELS.subject),
    }
}

/// A run whose inputs are fixed and whose Plan is issued, before any capture.
///
/// No Clone and no Deserialize: consuming the capture issues exactly one
/// result for this one run.
pub struct PreparedRun<O: Owner> {
    context: CaptureContext,
    plan: OwnerPlan<O::Work>,
    common_plan: IssuedRunPlan<O::Projection>,
    prepared: Vec<Result<O::Prepared, PreparationFailure>>,
}

/// A run that has been captured, retaining its own acquisition authority.
pub struct RecordedRun<O: Owner> {
    inputs: PreparedRun<O>,
    record: RecordOf<O>,
}

/// A submitted document a run has admitted as its own result.
pub struct CheckedOwnerRecord<O: Owner>(RecordOf<O>);

impl<O: Owner> CheckedOwnerRecord<O> {
    /// Borrow the admitted document.
    pub const fn document(&self) -> &RecordOf<O> {
        &self.0
    }
}

impl<O: Owner> PreparedRun<O> {
    /// Fix every input and issue the Plan, before any interval opens.
    ///
    /// A fixture that will not prepare fails the run here: the fixtures are
    /// fixed literals, so one that cannot prepare is a defect rather than an
    /// input this run can plan.
    pub fn acquire() -> Result<Self, RunFailure> {
        let labels = O::LABELS;
        let framing = labels.framing;
        let context = CaptureContext::acquire(
            labels.profile.versioned(),
            O::sampling(),
            BuildObservation::acquire(framing, O::PACKAGE),
        )
        .map_err(|_| RunFailure::ClockUnavailable)?;
        let context_binding = checksum(framing, "owner-run-context", context.observation())?;
        let run = fresh_run(framing, context_binding)?;
        let result_identity = OwnerRecordIdentity::fresh(labels.result_domain)
            .map_err(|_| RunFailure::EntropyUnavailable)?;

        let fixtures = O::fixtures();
        let prepared = fixtures
            .iter()
            .map(|fixture| O::prepare(*fixture))
            .collect::<Vec<_>>();
        let mut projections = Vec::with_capacity(prepared.len());
        let mut cases = Vec::with_capacity(prepared.len());
        for (fixture, prepared) in fixtures.iter().zip(&prepared) {
            let Ok(ready) = prepared else {
                return Err(RunFailure::Plan(PlanFailure::InvalidRecord));
            };
            let projection = O::project(ready);
            let expected = O::expected(ready);
            // A fixture that quietly takes the other path would be measured as
            // the work it names while doing different work, and its samples
            // would still agree with each other. Only its declaration shows it.
            if expected.complete != (fixture.path() == Expected::Complete) {
                return Err(RunFailure::PathMismatch);
            }
            let expectation = Expectation {
                fixture: fixture.name().into(),
                path: fixture.path(),
                observation: expected.observation,
                work: expected.work.clone(),
            };
            // The case binding has no run nonce, ordinal, clock, or build: two
            // runs of the same fixture bind to the same case.
            cases.push(PlannedCase {
                case: checksum(framing, "owner-planned-case", &(&projection, &expectation))?,
                requirement: Requirement::RequiredUnconditional,
                expectation,
            });
            projections.push(projection);
        }

        let common_plan = IssuedRunPlan::issue(Issuance {
            projections,
            measurement_profile: labels.profile.versioned(),
            verification_subject: subject::<O>(),
            build_identity: context.observation().build.identity(&labels),
            planned_runner_class: None,
            runner_instance_identity: OwnerRecordIdentity::fresh(labels.runner_domain)
                .map_err(|_| RunFailure::EntropyUnavailable)?,
            producing_tool: producing_tool::<O>(),
        })
        .map_err(RunFailure::Plan)?;

        Ok(Self {
            plan: OwnerPlan {
                codec: labels.plan_codec.into(),
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
        })
    }

    /// Capture every planned case, in the planned order.
    ///
    /// A case that fails to capture is not an error here: it stays in the
    /// result as an attempt.
    pub fn collect(self) -> Result<RecordedRun<O>, RunFailure> {
        let labels = O::LABELS;
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
                    let capturing = Capturing {
                        clock: self.context.clock(),
                        framing: labels.framing,
                        sampling: self.context.observation().sampling,
                        binding,
                    };
                    match O::capture(&capturing, prepared) {
                        Ok(capture) => AttemptResult::Measured(Box::new(MeasuredCase {
                            descriptors: O::descriptors(
                                prepared,
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
            codec: labels.result_codec.into(),
            record_identity: self.plan.result_identity.clone(),
            plan: self.plan.clone(),
            context: self.context.observation().clone(),
            outcome: outcome::<O>(&attempts),
            attempts,
        };
        let record = OwnerRecord {
            checksum: checksum(labels.framing, "owner-run-result", &result)?,
            result,
        };
        Ok(RecordedRun {
            inputs: self,
            record,
        })
    }
}

impl<O: Owner> RecordedRun<O> {
    /// Borrow the instance identity the Plan was issued for.
    pub const fn expected_owner_identity(&self) -> &OwnerRecordIdentity {
        &self.inputs.plan.result_identity
    }

    /// Borrow the issued common Run Plan.
    pub fn plan_record(&self) -> &RunPlanRecord {
        self.inputs.common_plan.document()
    }

    /// Borrow the issued Plan with its projections.
    pub const fn common_plan(&self) -> &IssuedRunPlan<O::Projection> {
        &self.inputs.common_plan
    }

    /// Borrow what was observed at acquisition.
    pub const fn context(&self) -> &ContextObservation {
        self.inputs.context.observation()
    }

    /// Serialize the record this run captured.
    pub fn encode(&self) -> Result<Vec<u8>, RunFailure> {
        let bytes = serde_json::to_vec(&self.record).map_err(|_| RunFailure::Encoding)?;
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(RunFailure::SizeLimit);
        }
        Ok(bytes)
    }

    /// Admit a submitted document only when it is what this run captured.
    pub fn admit(&self, submitted: RecordOf<O>) -> Result<CheckedOwnerRecord<O>, Vec<RunIssue>> {
        let issues = self.validate(&submitted);
        if issues.is_empty() {
            Ok(CheckedOwnerRecord(submitted))
        } else {
            Err(issues)
        }
    }

    /// Report every way a submitted document differs from this run's result.
    pub fn validate(&self, submitted: &RecordOf<O>) -> Vec<RunIssue> {
        let labels = O::LABELS;
        let mut issues = Vec::new();
        let result = &submitted.result;
        if result.codec != labels.result_codec {
            issues.push(RunIssue::Codec);
        }
        if result.record_identity != self.inputs.plan.result_identity {
            issues.push(RunIssue::RecordIdentity);
        }
        let actual = checksum(labels.framing, "owner-run-result", result);
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
        if result.outcome != outcome::<O>(&result.attempts) {
            issues.push(RunIssue::Outcome);
        }
        issues
    }
}

fn outcome<O: Owner>(attempts: &[AttemptOf<O>]) -> OwnerOutcome {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acquisition::{ClockFailure, MeasurementFailure};
    use crate::owner_run::test_owner::{TestOwner, LABELS, SOUND};

    type Sound = TestOwner<SOUND>;

    fn attempt(
        result: AttemptResult<<Sound as Owner>::Descriptors, <Sound as Owner>::Work>,
    ) -> AttemptOf<Sound> {
        let digest = |value: &str| {
            let mut frame = LABELS.framing.frame("test-binding");
            frame.text(value);
            frame.finish()
        };
        CaseAttempt {
            ordinal: Quantity::new(0),
            binding: Binding {
                run: digest("run"),
                case: digest("case"),
            },
            result,
        }
    }

    #[test]
    fn a_missing_measurement_is_incomplete_and_a_wrong_result_is_invalid() {
        let failed = |failure| attempt(AttemptResult::CaptureFailed(failure));
        for missing in [
            CaptureFailure::Overflow,
            CaptureFailure::Measurement(MeasurementFailure::InvocationPanicked),
            CaptureFailure::Measurement(MeasurementFailure::Clock(ClockFailure::ReversedClock)),
        ] {
            assert_eq!(
                outcome::<Sound>(&[failed(missing)]),
                OwnerOutcome::Incomplete
            );
        }
        for wrong in [
            CaptureFailure::SemanticMismatch,
            CaptureFailure::Preparation(PreparationFailure::PriorStage),
        ] {
            assert_eq!(outcome::<Sound>(&[failed(wrong)]), OwnerOutcome::Invalid);
        }
        assert_eq!(
            outcome::<Sound>(&[attempt(AttemptResult::PreparationFailed(
                PreparationFailure::Fixture
            ))]),
            OwnerOutcome::Invalid
        );
        // The worst attempt decides, and a run with no attempt at all is not a
        // complete one.
        assert_eq!(
            outcome::<Sound>(&[
                failed(CaptureFailure::Overflow),
                failed(CaptureFailure::SemanticMismatch),
            ]),
            OwnerOutcome::Invalid
        );
        assert_eq!(outcome::<Sound>(&[]), OwnerOutcome::Invalid);
    }
}
