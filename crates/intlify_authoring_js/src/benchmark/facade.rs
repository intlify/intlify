// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The one entry point the smoke runner uses.
//!
//! It captures a run, produces the common records, and can re-admit them from
//! the bytes that were actually written to disk. Re-admission uses the run
//! that was retained, not the submitted documents: a record set that is merely
//! self-consistent proves nothing about what was measured.
//!
//! The standalone runner is the shared one. The bench target calls it with
//! [`RUNNER`] and this owner, so nothing here takes a file name or a path.

use intlify_measurement::owner_run::smoke;

pub use intlify_measurement::owner_run::smoke::{
    ObservationOutcome, Runner, SmokeFailure, Summary,
};

pub use super::owner::AuthoringJsDiscovery;

/// One captured run and the records it produced.
pub type Observation = smoke::Observation<AuthoringJsDiscovery>;

/// What the standalone smoke accepts and prints.
pub const RUNNER: Runner = Runner {
    usage: "authoring_js_discovery --profile smoke --validate [--output-dir NEW_DIRECTORY]",
    summary_format: "intlify-authoring-js-observational-smoke-summary/0",
};

/// Capture one run and produce its common records.
pub fn observe_smoke() -> Result<Observation, SmokeFailure> {
    smoke::observe()
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use intlify_measurement::pipeline::ValidationFailure;

    use super::*;
    use crate::benchmark::cases::FIXTURES;

    /// The common records an observation wrote, without its owner document.
    fn common(observation: &Observation) -> Vec<&[u8]> {
        observation
            .records()
            .into_iter()
            .filter(|(name, _)| *name != "owner-result.json")
            .map(|(_, bytes)| bytes)
            .collect()
    }

    #[test]
    fn a_run_produces_records_that_re_admit_from_their_own_bytes() {
        let observation = observe_smoke().unwrap();
        let summary = observation.verify_saved(&observation.records()).unwrap();
        assert_eq!(summary.outcome, ObservationOutcome::Complete);
        assert_eq!(summary.planned_cases, FIXTURES.len());
        // Blocked and refused fixtures are measured too: reporting why, and
        // refusing, are the operations they measure.
        assert_eq!(summary.measured_cases, summary.planned_cases);
        assert_eq!(summary.non_measured_cases, 0);
    }

    // The rejections below are the shared validator's, with the same reasons
    // `intlify_measurement` pins for its own test owner.

    #[test]
    fn a_withheld_record_is_missing() {
        let observation = observe_smoke().unwrap();
        let records = observation.records();
        for withheld in 0..records.len() {
            let saved = records
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != withheld)
                .map(|(_, record)| *record)
                .collect::<Vec<_>>();
            assert_eq!(
                observation.verify_saved(&saved),
                Err(SmokeFailure::MissingSavedRecord),
                "withholding {}",
                records[withheld].0
            );
        }
    }

    #[test]
    fn a_rehashed_change_to_a_saved_record_is_not_admitted() {
        let observation = observe_smoke().unwrap();
        let records = observation.records();
        let (_, report) = records
            .iter()
            .find(|(name, _)| *name == "report.json")
            .unwrap();
        // Drop one measured row and recompute the record's own digest: a
        // self-consistent document that is not the one this run produced.
        let mut report: serde_json::Value = serde_json::from_slice(report).unwrap();
        report["body"]["sections"][0]["rows"]
            .as_array_mut()
            .unwrap()
            .pop();
        let digest = intlify_measurement::identity::IntegrityDigest::from_hash(
            intlify_measurement::encoding::record_hash(&report).unwrap(),
        );
        report["envelope"]["integrityDigest"] = serde_json::to_value(digest).unwrap();
        let changed = serde_json::to_vec(&report).unwrap();
        let saved = records
            .iter()
            .map(|(name, bytes)| {
                if *name == "report.json" {
                    (*name, changed.as_slice())
                } else {
                    (*name, *bytes)
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            observation.verify_saved(&saved),
            Err(SmokeFailure::Admission(ValidationFailure::Report))
        );
    }

    #[test]
    fn a_duplicate_a_foreign_or_no_owner_document_resolves_no_evidence() {
        let first = observe_smoke().unwrap();
        let second = observe_smoke().unwrap();
        let owner = first.owner_document();
        let refused = Err(SmokeFailure::Admission(ValidationFailure::Evidence));
        // Two documents for one run is an ambiguous binding, not twice the
        // evidence.
        assert_eq!(first.validate(&[owner, owner], &common(&first)), refused);
        // Both runs measured the same fixtures and agree on every case
        // identity; what they do not share is which run was captured.
        assert_eq!(
            first.run().plan_record().body.case_inventory,
            second.run().plan_record().body.case_inventory
        );
        assert_eq!(
            first.validate(&[second.owner_document()], &common(&first)),
            refused
        );
        assert_eq!(first.validate(&[], &common(&first)), refused);
    }
}
