// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The one entry point the smoke runner uses.
//!
//! It captures a run, produces the common records, and can re-admit them from
//! the bytes that were actually written to disk. Re-admission uses the run
//! that was retained, not the submitted documents: a record set that is merely
//! self-consistent proves nothing about what was measured.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use intlify_measurement::owner_run::smoke::{self, Runner};

pub use intlify_measurement::owner_run::smoke::{ObservationOutcome, SmokeFailure, Summary};

use super::owner::AuthoringSemantics;

/// One captured run and the records it produced.
pub type Observation = smoke::Observation<AuthoringSemantics>;

const RUNNER: Runner = Runner {
    usage: "authoring_semantics --profile smoke --validate [--output-dir NEW_DIRECTORY]",
    summary_format: "intlify-authoring-observational-smoke-summary/0",
};

/// Capture one run and produce its common records.
pub fn observe_smoke() -> Result<Observation, SmokeFailure> {
    smoke::observe()
}

/// Run the standalone smoke with the given arguments, excluding the program.
///
/// Without `--output-dir`, the records are written into the directory
/// `scratch` creates.
pub fn smoke_main(
    args: Vec<OsString>,
    scratch: impl FnOnce() -> std::io::Result<PathBuf>,
) -> ExitCode {
    RUNNER.main::<AuthoringSemantics>(args, scratch)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn a_run_produces_records_that_re_admit_from_their_own_bytes() {
        let observation = observe_smoke().unwrap();
        let summary = observation.verify_saved(&observation.records()).unwrap();
        assert_eq!(summary.outcome, ObservationOutcome::Complete);
        assert_eq!(summary.planned_cases, super::super::cases::FIXTURES.len());
        assert_eq!(summary.measured_cases, summary.planned_cases);
        assert_eq!(summary.non_measured_cases, 0);
    }

    #[test]
    fn a_withheld_record_is_not_a_successful_run() {
        let observation = observe_smoke().unwrap();
        let records = observation.records();
        for withheld in 0..records.len() {
            let saved = records
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != withheld)
                .map(|(_, record)| *record)
                .collect::<Vec<_>>();
            assert!(
                observation.verify_saved(&saved).is_err(),
                "withholding {} was admitted",
                records[withheld].0
            );
        }
    }

    #[test]
    fn a_document_from_another_run_does_not_bind_to_this_one() {
        // Both runs measured the same fixtures and agree on every case
        // identity. What they do not share is which run was captured.
        let first = observe_smoke().unwrap();
        let second = observe_smoke().unwrap();
        let common = first
            .records()
            .into_iter()
            .filter(|(name, _)| *name != "owner-result.json")
            .map(|(_, bytes)| bytes)
            .collect::<Vec<_>>();
        assert!(first.validate(&[second.owner_document()], &common).is_err());
    }
}
