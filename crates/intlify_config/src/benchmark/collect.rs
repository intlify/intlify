// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Connect actual owner calls, acquired clock metadata, and bounded capture.
//! The retained fragment is intentionally not a complete Owner Result or common
//! Evidence Set: registry, work vector, build/environment, Run Plan, projection,
//! and report validation are additional mandatory harness responsibilities.

use serde::{Deserialize, Serialize};

use super::clock::{ClockDescription, MonotonicClock};
use super::descriptor::{DescriptorIssue, Descriptors};
use super::observation::Observation;
use super::operation::{Operation, Prepared};
use super::sample::{
    collect_with_work, validate_capture, Capture, CaptureBinding, CaptureFailure,
    SampleIntegrityIssue, Sampling,
};
use super::work::LogicalWork;

/// Raw, serializable owner fragment. Deserialization alone is not admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CollectedOperation {
    descriptors: Descriptors,
    logical_work: LogicalWork,
    capture: Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CollectionIssue {
    Descriptor(DescriptorIssue),
    LogicalWork,
    Sample(SampleIntegrityIssue),
}

#[derive(Debug)]
pub(super) enum CollectionFailure {
    Descriptor(Vec<DescriptorIssue>),
    Capture(Box<CaptureFailure>),
    Integrity(Vec<CollectionIssue>),
}

/// Only the real supported provider enters production capture. Scripted clocks
/// remain test inputs to the lower-level sampler, never a runtime method option.
pub(super) fn collect_operation(
    clock: &MonotonicClock,
    prepared: &Prepared,
    expected: Observation,
    expected_work: &LogicalWork,
    sampling: Sampling,
    binding: CaptureBinding,
) -> Result<CollectedOperation, CollectionFailure> {
    let operation = prepared.operation();
    let descriptors = Descriptors::for_acquisition(operation, clock.description());
    let issues = descriptors.validate(operation, clock.description());
    if !issues.is_empty() {
        return Err(CollectionFailure::Descriptor(issues));
    }
    let capture = collect_with_work(clock, prepared, expected, expected_work, sampling, binding)
        .map_err(CollectionFailure::Capture)?;
    let collected = CollectedOperation {
        descriptors,
        logical_work: expected_work.clone(),
        capture,
    };
    let issues = collected.validate(
        operation,
        clock.description(),
        expected,
        expected_work,
        sampling,
        binding,
    );
    if !issues.is_empty() {
        return Err(CollectionFailure::Integrity(issues));
    }
    Ok(collected)
}

impl CollectedOperation {
    /// Rebind decoded data to separately supplied fixture/run/method inputs.
    /// A record cannot choose its own expected checksum, sample count, operation,
    /// run/case identities, or the clock resolution used to validate itself.
    pub(super) fn validate(
        &self,
        operation: Operation,
        acquisition: ClockDescription,
        expected: Observation,
        expected_work: &LogicalWork,
        sampling: Sampling,
        binding: CaptureBinding,
    ) -> Vec<CollectionIssue> {
        self.descriptors
            .validate(operation, acquisition)
            .into_iter()
            .map(CollectionIssue::Descriptor)
            .chain(
                (!self.logical_work.matches_expected(expected_work))
                    .then_some(CollectionIssue::LogicalWork),
            )
            .chain(
                validate_capture(&self.capture, sampling, binding, expected)
                    .into_iter()
                    .map(CollectionIssue::Sample),
            )
            .collect()
    }
}

#[cfg(test)]
mod tests;
