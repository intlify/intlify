// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Bounded owner sample capture, not a common Measurement Evidence record.
//! A caller supplies a preissued run/case binding and independently admitted
//! fixture observation. Run Plan, method, environment, and projection admission
//! remain separate requirements; successful capture is not evidence eligibility.

use serde::{Deserialize, Serialize};
use std::panic::{catch_unwind, AssertUnwindSafe};

use super::clock::Clock;
use super::measure::MeasurementFailure;
use super::observation::{Digest, Frame, Observation};
use super::operation::{OutputFailure, Prepared};
use super::quantity::{Quantity, Repetitions};
use super::work::{LogicalWork, WorkFailure};

#[derive(Debug, Clone, Copy)]
pub(super) struct CaptureCapacity {
    pub(super) warmup_repetitions: Quantity,
    pub(super) samples: Repetitions,
    pub(super) repetitions_per_sample: Repetitions,
    pub(super) total_invocations: Repetitions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SamplingError {
    CapacityExceeded,
    InvocationCountOverflow,
    Addressability,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Sampling {
    warmup: Quantity,
    samples: Repetitions,
    repetitions: Repetitions,
    sample_capacity: usize,
}

impl Sampling {
    pub(super) fn admit(
        warmup: Quantity,
        samples: Repetitions,
        repetitions: Repetitions,
        capacity: CaptureCapacity,
    ) -> Result<Self, SamplingError> {
        let total = samples
            .get()
            .checked_mul(repetitions.get())
            .and_then(|value| value.checked_add(warmup.get()))
            .ok_or(SamplingError::InvocationCountOverflow)?;
        if warmup > capacity.warmup_repetitions
            || samples > capacity.samples
            || repetitions > capacity.repetitions_per_sample
            || total > capacity.total_invocations.get()
        {
            return Err(SamplingError::CapacityExceeded);
        }
        let sample_capacity =
            usize::try_from(samples.get()).map_err(|_| SamplingError::Addressability)?;
        Ok(Self {
            warmup,
            samples,
            repetitions,
            sample_capacity,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CaptureBinding {
    pub(super) run: Digest,
    pub(super) case: Digest,
}

impl CaptureBinding {
    fn local_identity(self, domain: &str, ordinal: u64) -> Digest {
        let mut frame = Frame::new(domain);
        frame.digest(self.run);
        frame.digest(self.case);
        frame.uint(ordinal);
        frame.finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum DurationProof {
    NotRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Acquisition {
    Unpaired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CapturedSample {
    pub(super) local_identity: Digest,
    pub(super) ordinal: Quantity,
    pub(super) repetition_count: Repetitions,
    pub(super) aggregate_nanoseconds: Quantity,
    pub(super) semantic_observation: Observation,
    pub(super) execution_identity: Digest,
    pub(super) determinism_proof: DurationProof,
    pub(super) acquisition: Acquisition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Capture {
    pub(super) warmup_completed: Quantity,
    pub(super) samples: Vec<CapturedSample>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum CaptureStage {
    Preparation,
    Warmup,
    Measured,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "detail",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub(super) enum CaptureFailureCause {
    PrerequisiteUnavailable,
    CollectorAllocation,
    Measurement(MeasurementFailure),
    Output(OutputFailure),
    ObservationPanicked,
    SemanticObservationMismatch(Box<ObservationMismatch>),
    LogicalWorkObservation(WorkFailure),
    LogicalWorkMismatch(Box<WorkMismatch>),
    MeasurementOverflow,
}

/// Preserve both complete observations. The allocation exists only on failure,
/// after the measured interval; ordinary calls do not carry the large payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ObservationMismatch {
    pub(super) expected: Observation,
    pub(super) actual: Observation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WorkMismatch {
    pub(super) expected: LogicalWork,
    pub(super) actual: LogicalWork,
}

/// Values here are diagnostic prefixes only, never admitted successful samples.
/// On any failure the complete capture stays unavailable to numeric consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CaptureFailure {
    pub(super) cause: CaptureFailureCause,
    pub(super) stage: CaptureStage,
    pub(super) warmup_completed: Quantity,
    pub(super) complete_sample_prefix: Vec<CapturedSample>,
    pub(super) attempted_repetitions_in_sample: Quantity,
    pub(super) completed_repetitions_in_sample: Quantity,
    pub(super) accumulated_nanoseconds: Quantity,
}

/// Lower-level sampler tests may isolate semantic observations from work facts.
/// Ordinary collection uses `collect_with_work` and cannot skip work validation.
#[cfg(test)]
pub(super) fn collect(
    clock: &impl Clock,
    operation: &Prepared,
    expected: Observation,
    sampling: Sampling,
    binding: CaptureBinding,
) -> Result<Capture, Box<CaptureFailure>> {
    collect_inner(
        operation.prerequisites_admitted(),
        sampling,
        binding,
        expected,
        || observe_invocation(clock, operation, expected, None),
    )
}

pub(super) fn collect_with_work(
    clock: &impl Clock,
    operation: &Prepared,
    expected: Observation,
    expected_work: &LogicalWork,
    sampling: Sampling,
    binding: CaptureBinding,
) -> Result<Capture, Box<CaptureFailure>> {
    collect_inner(
        operation.prerequisites_admitted(),
        sampling,
        binding,
        expected,
        || observe_invocation(clock, operation, expected, Some(expected_work)),
    )
}

fn collect_inner(
    prerequisites: bool,
    sampling: Sampling,
    binding: CaptureBinding,
    expected: Observation,
    mut invoke: impl FnMut() -> Result<Quantity, CaptureFailureCause>,
) -> Result<Capture, Box<CaptureFailure>> {
    let mut progress = CaptureFailure {
        cause: CaptureFailureCause::PrerequisiteUnavailable,
        stage: CaptureStage::Preparation,
        warmup_completed: Quantity::new(0),
        complete_sample_prefix: Vec::new(),
        attempted_repetitions_in_sample: Quantity::new(0),
        completed_repetitions_in_sample: Quantity::new(0),
        accumulated_nanoseconds: Quantity::new(0),
    };
    if !prerequisites {
        return Err(Box::new(progress));
    }
    if progress
        .complete_sample_prefix
        .try_reserve_exact(sampling.sample_capacity)
        .is_err()
    {
        progress.cause = CaptureFailureCause::CollectorAllocation;
        return Err(Box::new(progress));
    }
    progress.stage = CaptureStage::Warmup;
    for index in 0..sampling.warmup.get() {
        if let Err(cause) = invoke() {
            progress.cause = cause;
            return Err(Box::new(progress));
        }
        // index is strictly below the admitted finite bound, so +1 is exact.
        progress.warmup_completed = Quantity::new(index + 1);
    }
    progress.stage = CaptureStage::Measured;
    for ordinal in 0..sampling.samples.get() {
        // Immutable input, fresh output and scratch for every ordinary call.
        // Identity construction and resetting collector state precede intervals.
        let local_identity = binding.local_identity("sample", ordinal);
        let execution_identity = binding.local_identity("sample-execution", ordinal);
        progress.accumulated_nanoseconds = Quantity::new(0);
        progress.completed_repetitions_in_sample = Quantity::new(0);
        progress.attempted_repetitions_in_sample = Quantity::new(0);
        for repetition in 0..sampling.repetitions.get() {
            progress.attempted_repetitions_in_sample = Quantity::new(repetition + 1);
            let duration = match invoke() {
                Ok(duration) => duration,
                Err(cause) => {
                    progress.cause = cause;
                    return Err(Box::new(progress));
                }
            };
            let Ok(total) = progress.accumulated_nanoseconds.checked_add(duration) else {
                progress.cause = CaptureFailureCause::MeasurementOverflow;
                return Err(Box::new(progress));
            };
            progress.accumulated_nanoseconds = total;
            progress.completed_repetitions_in_sample = Quantity::new(repetition + 1);
        }
        progress.complete_sample_prefix.push(CapturedSample {
            local_identity,
            ordinal: Quantity::new(ordinal),
            repetition_count: sampling.repetitions,
            aggregate_nanoseconds: progress.accumulated_nanoseconds,
            semantic_observation: expected,
            execution_identity,
            determinism_proof: DurationProof::NotRequired,
            acquisition: Acquisition::Unpaired,
        });
    }
    Ok(Capture {
        warmup_completed: progress.warmup_completed,
        samples: progress.complete_sample_prefix,
    })
}

fn observe_invocation(
    clock: &impl Clock,
    operation: &Prepared,
    expected: Observation,
    expected_work: Option<&LogicalWork>,
) -> Result<Quantity, CaptureFailureCause> {
    let measured = operation
        .once(clock)
        .map_err(CaptureFailureCause::Measurement)?;
    let actual = catch_unwind(AssertUnwindSafe(|| measured.output.observe()))
        .map_err(|_| CaptureFailureCause::ObservationPanicked)?
        .map_err(CaptureFailureCause::Output)?;
    if actual != expected {
        return Err(CaptureFailureCause::SemanticObservationMismatch(Box::new(
            ObservationMismatch { expected, actual },
        )));
    }
    if let Some(expected) = expected_work {
        let actual = catch_unwind(AssertUnwindSafe(|| {
            LogicalWork::observe(operation, &measured.output)
        }))
        .map_err(|_| CaptureFailureCause::ObservationPanicked)?
        .map_err(CaptureFailureCause::LogicalWorkObservation)?;
        if !actual.matches_expected(expected) {
            return Err(CaptureFailureCause::LogicalWorkMismatch(Box::new(
                WorkMismatch {
                    expected: expected.clone(),
                    actual,
                },
            )));
        }
    }
    // Observation and destruction occur after the end marker; no estimated
    // checksum/teardown cost is subtracted from the recorded component duration.
    Ok(measured.duration)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum SampleIntegrityKind {
    WarmupCount,
    SampleCount,
    Ordinal,
    Repetitions,
    SampleIdentity,
    ExecutionIdentity,
    SemanticObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SampleIntegrityIssue {
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) sample_index: Option<Quantity>,
    pub(super) kind: SampleIntegrityKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FailureIntegrityIssue {
    MissingReason,
    StageProgress,
    DiagnosticSamplePrefix,
    RepetitionProgress,
    SemanticObservation,
    LogicalWork,
}

/// Check the failure's progress against the separately admitted sampling and
/// fixture, without promoting any diagnostic prefix into a successful capture.
pub(super) fn validate_failure(
    failure: &CaptureFailure,
    sampling: Sampling,
    binding: CaptureBinding,
    expected: Observation,
    expected_work: &LogicalWork,
) -> Vec<FailureIntegrityIssue> {
    let mut issues = Vec::new();
    let zero_progress = failure.attempted_repetitions_in_sample.get() == 0
        && failure.completed_repetitions_in_sample.get() == 0
        && failure.accumulated_nanoseconds.get() == 0;
    let preparation_cause = matches!(
        failure.cause,
        CaptureFailureCause::PrerequisiteUnavailable | CaptureFailureCause::CollectorAllocation
    );
    let stage_valid = match failure.stage {
        CaptureStage::Preparation => {
            preparation_cause
                && failure.warmup_completed.get() == 0
                && failure.complete_sample_prefix.is_empty()
                && zero_progress
        }
        CaptureStage::Warmup => {
            !preparation_cause
                && failure.warmup_completed < sampling.warmup
                && failure.complete_sample_prefix.is_empty()
                && zero_progress
        }
        CaptureStage::Measured => !preparation_cause && failure.warmup_completed == sampling.warmup,
    };
    if !stage_valid {
        issues.push(FailureIntegrityIssue::StageProgress);
    }
    if failure.stage == CaptureStage::Measured {
        let attempted = failure.attempted_repetitions_in_sample.get();
        let completed = failure.completed_repetitions_in_sample.get();
        if attempted == 0
            || attempted > sampling.repetitions.get()
            || completed.checked_add(1) != Some(attempted)
            || (completed == 0 && failure.accumulated_nanoseconds.get() != 0)
        {
            issues.push(FailureIntegrityIssue::RepetitionProgress);
        }
    }
    // A complete successful prefix must still leave a sample that failed.
    if failure.complete_sample_prefix.len() >= sampling.sample_capacity {
        issues.push(FailureIntegrityIssue::DiagnosticSamplePrefix);
    } else {
        let prefix = Capture {
            warmup_completed: sampling.warmup,
            samples: failure.complete_sample_prefix.clone(),
        };
        let prefix_sampling = Sampling {
            sample_capacity: prefix.samples.len(),
            ..sampling
        };
        if !validate_capture(&prefix, prefix_sampling, binding, expected).is_empty() {
            issues.push(FailureIntegrityIssue::DiagnosticSamplePrefix);
        }
    }
    match &failure.cause {
        CaptureFailureCause::SemanticObservationMismatch(mismatch)
            if mismatch.expected != expected || mismatch.actual == expected =>
        {
            issues.push(FailureIntegrityIssue::SemanticObservation);
        }
        CaptureFailureCause::LogicalWorkMismatch(mismatch)
            if !mismatch.expected.matches_expected(expected_work)
                || mismatch.actual.matches_expected(expected_work) =>
        {
            issues.push(FailureIntegrityIssue::LogicalWork);
        }
        CaptureFailureCause::MeasurementOverflow
            if failure.stage != CaptureStage::Measured
                || failure.completed_repetitions_in_sample.get() == 0 =>
        {
            issues.push(FailureIntegrityIssue::RepetitionProgress);
        }
        _ => {}
    }
    issues
}

/// Independent revalidation for a decoded owner record. It proves only these
/// sample relations, not method/environment/run-plan/projection eligibility.
pub(super) fn validate_capture(
    capture: &Capture,
    sampling: Sampling,
    binding: CaptureBinding,
    expected: Observation,
) -> Vec<SampleIntegrityIssue> {
    let mut issues = Vec::new();
    if capture.warmup_completed != sampling.warmup {
        issues.push(SampleIntegrityIssue {
            sample_index: None,
            kind: SampleIntegrityKind::WarmupCount,
        });
    }
    if capture.samples.len() != sampling.sample_capacity {
        issues.push(SampleIntegrityIssue {
            sample_index: None,
            kind: SampleIntegrityKind::SampleCount,
        });
    }
    for (index, sample) in capture.samples.iter().enumerate() {
        let ordinal = u64::try_from(index).expect("addressable capture sample count");
        let mut reject = |kind| {
            issues.push(SampleIntegrityIssue {
                sample_index: Some(Quantity::new(ordinal)),
                kind,
            });
        };
        if sample.ordinal.get() != ordinal {
            reject(SampleIntegrityKind::Ordinal);
        }
        if sample.repetition_count != sampling.repetitions {
            reject(SampleIntegrityKind::Repetitions);
        }
        if sample.local_identity != binding.local_identity("sample", ordinal) {
            reject(SampleIntegrityKind::SampleIdentity);
        }
        if sample.execution_identity != binding.local_identity("sample-execution", ordinal) {
            reject(SampleIntegrityKind::ExecutionIdentity);
        }
        if sample.semantic_observation != expected {
            reject(SampleIntegrityKind::SemanticObservation);
        }
    }
    issues
}

#[cfg(test)]
mod tests;
