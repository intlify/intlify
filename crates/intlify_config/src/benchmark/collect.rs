// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Connect actual owner calls, the acquired instrument, and bounded capture.
//! The retained fragment is intentionally not a complete Owner Result or common
//! Evidence Set: common case identities, build/environment, Run Plan, projection,
//! and report validation are additional mandatory harness responsibilities.

use serde::{Deserialize, Serialize};

use super::cases::registry::AdmittedFixture;
use super::descriptor::{DescriptorIssue, Descriptors};
use super::instrument::{InstrumentDescription, Peaked, RunInstrument, Timed};
use super::observation::{Digest, Observation};
use super::operation::Prepared;
use super::sample::{
    collect_with_work, validate_capture, Capture, CaptureBinding, CaptureFailure,
    SampleIntegrityIssue, Sampling,
};
use super::work::LogicalWork;

/// Raw, serializable owner fragment. Deserialization alone is not admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CollectedOperation {
    pub(super) fixture_input_context: Digest,
    pub(super) descriptors: Descriptors,
    pub(super) logical_work: LogicalWork,
    pub(super) capture: Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "detail",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub(super) enum CollectionIssue {
    FixtureInputContext,
    Descriptor(DescriptorIssue),
    LogicalWork,
    Sample(SampleIntegrityIssue),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "detail",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub(super) enum CollectionFailure {
    Descriptor(Vec<DescriptorIssue>),
    Capture(Box<CaptureFailure>),
    Integrity(Vec<CollectionIssue>),
}

/// Only the run's acquired instrument enters production capture: the real
/// clock, or the observer the run was handed. Scripted clocks remain test
/// inputs to the lower-level sampler, never a runtime method option.
pub(super) fn collect_operation(
    instrument: &RunInstrument,
    fixture: &AdmittedFixture,
    sampling: Sampling,
    binding: CaptureBinding,
) -> Result<CollectedOperation, CollectionFailure> {
    collect_prepared(
        instrument,
        fixture.prepared(),
        fixture.expected(),
        fixture.expected_work(),
        fixture.input_context(),
        sampling,
        binding,
    )
}

// Free-form prepared calls and expectations are an implementation detail, used
// directly only by this module's negative tests. Normal collection needs the
// immutable fixture token constructed by the checked-in expectation gate.
fn collect_prepared(
    instrument: &RunInstrument,
    prepared: &Prepared,
    expected: Observation,
    expected_work: &LogicalWork,
    fixture_input_context: Digest,
    sampling: Sampling,
    binding: CaptureBinding,
) -> Result<CollectedOperation, CollectionFailure> {
    let description = instrument.description();
    // An operation without a memory boundary has no descriptors to observe
    // its peak under, so it is refused before any interval.
    let descriptors = Descriptors::for_instrument(prepared, description)
        .ok_or_else(|| CollectionFailure::Descriptor(vec![DescriptorIssue::BoundaryMismatch]))?;
    let issues = descriptors.validate_with(prepared, description);
    if !issues.is_empty() {
        return Err(CollectionFailure::Descriptor(issues));
    }
    let capture = match instrument {
        RunInstrument::Clock(clock) => collect_with_work(
            &Timed(clock),
            prepared,
            expected,
            expected_work,
            sampling,
            binding,
        ),
        RunInstrument::Observer { observer, .. } => collect_with_work(
            &Peaked(observer.as_ref()),
            prepared,
            expected,
            expected_work,
            sampling,
            binding,
        ),
    }
    .map_err(CollectionFailure::Capture)?;
    let collected = CollectedOperation {
        fixture_input_context,
        descriptors,
        logical_work: expected_work.clone(),
        capture,
    };
    let issues = collected.validate_against(
        prepared,
        description,
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
    pub(super) fn validate(
        &self,
        fixture: &AdmittedFixture,
        acquisition: InstrumentDescription,
        sampling: Sampling,
        binding: CaptureBinding,
    ) -> Vec<CollectionIssue> {
        let mut issues = Vec::new();
        if self.fixture_input_context != fixture.input_context() {
            issues.push(CollectionIssue::FixtureInputContext);
        }
        issues.extend(self.validate_against(
            fixture.prepared(),
            acquisition,
            fixture.expected(),
            fixture.expected_work(),
            sampling,
            binding,
        ));
        issues
    }

    /// Rebind decoded data to separately supplied fixture/run/method inputs.
    /// A record cannot choose its own expected checksum, sample count, operation,
    /// run/case identities, or the instrument used to validate itself.
    fn validate_against(
        &self,
        prepared: &Prepared,
        acquisition: InstrumentDescription,
        expected: Observation,
        expected_work: &LogicalWork,
        sampling: Sampling,
        binding: CaptureBinding,
    ) -> Vec<CollectionIssue> {
        self.descriptors
            .validate_with(prepared, acquisition)
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
