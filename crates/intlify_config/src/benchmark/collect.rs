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
    collect, validate_capture, Capture, CaptureBinding, CaptureFailure, SampleIntegrityIssue,
    Sampling,
};

/// Raw, serializable owner fragment. Deserialization alone is not admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CollectedOperation {
    descriptors: Descriptors,
    capture: Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CollectionIssue {
    Descriptor(DescriptorIssue),
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
    sampling: Sampling,
    binding: CaptureBinding,
) -> Result<CollectedOperation, CollectionFailure> {
    let operation = prepared.operation();
    let descriptors = Descriptors::for_acquisition(operation, clock.description());
    let issues = descriptors.validate(operation, clock.description());
    if !issues.is_empty() {
        return Err(CollectionFailure::Descriptor(issues));
    }
    let capture = collect(clock, prepared, expected, sampling, binding)
        .map_err(CollectionFailure::Capture)?;
    let collected = CollectedOperation {
        descriptors,
        capture,
    };
    let issues = collected.validate(operation, clock.description(), expected, sampling, binding);
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
        sampling: Sampling,
        binding: CaptureBinding,
    ) -> Vec<CollectionIssue> {
        self.descriptors
            .validate(operation, acquisition)
            .into_iter()
            .map(CollectionIssue::Descriptor)
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
