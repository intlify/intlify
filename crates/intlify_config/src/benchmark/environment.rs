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
use intlify_measurement::acquisition::memory::ObserverDescription;
pub(super) use intlify_measurement::acquisition::Acquired;

use super::descriptor::ClockObservation;
use super::instrument::InstrumentDescription;
use super::observation::{Digest, Frame};
use super::shared::plan::present;

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
    /// The clock a duration run times with. A memory run reads none.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) clock: Option<ClockObservation>,
    /// The observer a memory run counts with, as it describes itself. A
    /// duration run has none.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) memory_observer: Option<ObserverFacts>,
}

/// What an observer said about itself when the run acquired it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ObserverFacts {
    pub(super) provider: String,
    pub(super) provider_revision: String,
    pub(super) identity: String,
    pub(super) revision: String,
    pub(super) counting: String,
    pub(super) limitations: Vec<String>,
}

impl ObserverFacts {
    fn observe(description: ObserverDescription) -> Self {
        Self {
            provider: description.provider.into(),
            provider_revision: description.provider_revision.into(),
            identity: description.identity.into(),
            revision: description.revision.into(),
            counting: description.counting.into(),
            limitations: description
                .limitations
                .iter()
                .map(|limitation| (*limitation).into())
                .collect(),
        }
    }
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
    pub(super) fn acquire(instrument: InstrumentDescription) -> Result<Self, EnvironmentIssue> {
        // The host views are the shared acquisition's, so every owner reads
        // the same machine the same way. This owner's codec frames them.
        let host = HostView::acquire();
        let (clock, memory_observer) = match instrument {
            InstrumentDescription::Clock(clock) => (Some(clock.observation()), None),
            InstrumentDescription::Observer(observer) => {
                (None, Some(ObserverFacts::observe(observer)))
            }
        };
        let document = EnvironmentInputs {
            codec: "intlify-config-environment-inputs/0".into(),
            library_target: host.library_target,
            kernel_view: host.kernel_view,
            available_parallelism_hint: host.available_parallelism_hint,
            runner: RunnerContext::LocalUncontrolled,
            clock,
            memory_observer,
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
