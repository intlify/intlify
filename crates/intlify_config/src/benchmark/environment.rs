// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Acquisition inputs for the eventual complete 026 Environment Observation.
//! This owner-local snapshot is not the 27-field common record or its admission.
//! The kernel, compiled-target and parallelism views are acquired by the shared
//! `intlify_measurement` acquisition and remain distinct from physical-host
//! claims. Missing OS/build/device/qualification/projection knowledge is not
//! invented.

use serde::{Deserialize, Serialize};

use intlify_measurement::acquisition::host::HostView;
pub(super) use intlify_measurement::acquisition::host::{
    KernelView, LibraryTarget, ParallelismHint,
};
pub(super) use intlify_measurement::acquisition::Acquired;

use super::clock::MonotonicClock;
use super::descriptor::ClockObservation;
use super::observation::{Digest, Frame};

/// This path has no qualification/preflight admission, so it cannot construct
/// either controlled or qualified contexts from CI flags or caller strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum RunnerContext {
    LocalUncontrolled,
}

/// A source of acquired facts, not the common environment-field inventory.
/// Additional required fields and admitted harness/projection references belong
/// to the enclosing complete record. No absent field here proves non-applicability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct EnvironmentInputs {
    pub(super) codec: String,
    pub(super) library_target: LibraryTarget,
    pub(super) kernel_view: Acquired<KernelView>,
    pub(super) available_parallelism_hint: Acquired<ParallelismHint>,
    pub(super) runner: RunnerContext,
    pub(super) clock: ClockObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnvironmentIssue {
    Encoding,
    ObservationMismatch,
}

/// No Deserialize or mutable access. Acquisition, not a submitted document,
/// defines the observation used to bind every operation in this capture context.
pub(super) struct ObservedEnvironment {
    document: EnvironmentInputs,
    checksum: Digest,
}

fn checksum(document: &EnvironmentInputs) -> Result<Digest, EnvironmentIssue> {
    let mut frame = Frame::new("environment-inputs");
    frame.json(&serde_json::to_value(document).map_err(|_| EnvironmentIssue::Encoding)?);
    Ok(frame.finish())
}

impl ObservedEnvironment {
    pub(super) fn acquire(clock: &MonotonicClock) -> Result<Self, EnvironmentIssue> {
        // The host views are the shared acquisition's, so every owner reads
        // the same machine the same way. This owner's codec frames them.
        let host = HostView::acquire();
        let document = EnvironmentInputs {
            codec: "intlify-config-environment-inputs/0".into(),
            library_target: host.library_target,
            kernel_view: host.kernel_view,
            available_parallelism_hint: host.available_parallelism_hint,
            runner: RunnerContext::LocalUncontrolled,
            clock: clock.description().observation(),
        };
        let checksum = checksum(&document)?;
        Ok(Self { document, checksum })
    }

    pub(super) fn document(&self) -> &EnvironmentInputs {
        &self.document
    }

    pub(super) const fn checksum(&self) -> Digest {
        self.checksum
    }

    pub(super) fn validate(&self, submitted: &EnvironmentInputs) -> Vec<EnvironmentIssue> {
        if submitted == &self.document {
            Vec::new()
        } else {
            vec![EnvironmentIssue::ObservationMismatch]
        }
    }
}

#[cfg(test)]
mod tests;
