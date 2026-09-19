// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The one entry point the smoke runner uses.
//!
//! It captures a run, produces the common records, and can re-admit them from
//! the bytes that were actually written to disk. Re-admission uses the run
//! that was retained here, not the submitted documents: a record set that is
//! merely self-consistent proves nothing about what was measured.

use intlify_measurement::measurement::Outcome;
use intlify_measurement::pipeline::{self, Artifacts, Validation, ValidationFailure};

use super::run::{PreparedRun, RecordedRun, RunFailure};

/// Complete failure of one observational smoke run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmokeFailure {
    /// The run could not be acquired or captured.
    Run(RunFailure),
    /// The common records could not be produced.
    Produce(pipeline::Failure),
    /// A saved record was not the one this run produced.
    Admission(ValidationFailure),
    /// A record this run wrote was not offered back for re-admission.
    MissingSavedRecord,
}

impl std::fmt::Display for SmokeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SmokeFailure {}

/// Whether the run produced a complete result for its planned inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationOutcome {
    Complete,
    Incomplete,
    Invalid,
}

/// Counts for presentation only.
///
/// This profile has no numeric-decision facility, so nothing downstream may
/// compare these against a threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub outcome: ObservationOutcome,
    pub planned_cases: usize,
    pub measured_cases: usize,
    pub non_measured_cases: usize,
}

impl From<Validation> for Summary {
    fn from(value: Validation) -> Self {
        Self {
            outcome: match value.outcome {
                Outcome::Complete => ObservationOutcome::Complete,
                Outcome::Incomplete => ObservationOutcome::Incomplete,
                Outcome::Invalid => ObservationOutcome::Invalid,
            },
            planned_cases: value.planned_cases,
            measured_cases: value.measured_cases,
            non_measured_cases: value.non_measured_cases,
        }
    }
}

/// One captured run and the records it produced.
pub struct Observation {
    run: RecordedRun,
    artifacts: Artifacts,
    owner: Vec<u8>,
}

/// The file each record is written as.
const RUN_PLAN: &str = "run-plan.json";
const OWNER_RESULT: &str = "owner-result.json";
const EVIDENCE: &str = "measurement-evidence.json";
const EVALUATION: &str = "run-evaluation.json";
const REPORT: &str = "report.json";

/// Capture one run and produce its common records.
pub fn observe_smoke() -> Result<Observation, SmokeFailure> {
    let run = PreparedRun::acquire()
        .map_err(SmokeFailure::Run)?
        .collect()
        .map_err(SmokeFailure::Run)?;
    let owner = run.encode().map_err(SmokeFailure::Run)?;
    let artifacts = pipeline::produce_with_owner(&run, &[&owner]).map_err(SmokeFailure::Produce)?;
    Ok(Observation {
        run,
        artifacts,
        owner,
    })
}

impl Observation {
    /// Every record this run produced, with the name to write it under.
    #[must_use]
    pub fn records(&self) -> Vec<(&'static str, &[u8])> {
        let mut records: Vec<(&'static str, &[u8])> = vec![
            (RUN_PLAN, &self.artifacts.run_plan),
            (OWNER_RESULT, &self.owner),
            (EVALUATION, &self.artifacts.evaluation),
            (REPORT, &self.artifacts.report),
        ];
        if let Some(evidence) = &self.artifacts.evidence {
            records.push((EVIDENCE, evidence));
        }
        records
    }

    /// Re-admit the records from the bytes that were actually saved.
    pub fn verify_saved(&self, saved: &[(&str, &[u8])]) -> Result<Summary, SmokeFailure> {
        let find = |name: &str| {
            saved
                .iter()
                .find(|(saved, _)| *saved == name)
                .map(|(_, bytes)| *bytes)
                .ok_or(SmokeFailure::MissingSavedRecord)
        };
        let owner = find(OWNER_RESULT)?;
        let mut common = vec![find(RUN_PLAN)?, find(EVALUATION)?, find(REPORT)?];
        if self.artifacts.evidence.is_some() {
            common.push(find(EVIDENCE)?);
        }
        pipeline::validate_records(
            &self.run,
            &[owner],
            &common,
            &self.artifacts.report_identity,
        )
        .map(Summary::from)
        .map_err(SmokeFailure::Admission)
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn a_run_produces_records_that_re_admit_from_their_own_bytes() {
        let observation = observe_smoke().unwrap();
        let records = observation.records();
        // Every record the run wrote is offered back exactly as written.
        let saved = records
            .iter()
            .map(|(name, bytes)| (*name, *bytes))
            .collect::<Vec<_>>();
        let summary = observation.verify_saved(&saved).unwrap();
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
                .map(|(_, (name, bytes))| (*name, *bytes))
                .collect::<Vec<_>>();
            assert!(
                observation.verify_saved(&saved).is_err(),
                "withholding {} was admitted",
                records[withheld].0
            );
        }
    }

    #[test]
    fn a_rehashed_change_to_a_saved_record_is_not_admitted() {
        let observation = observe_smoke().unwrap();
        let records = observation.records();

        // Drop one measured row from the report, then recompute the record's
        // own integrity digest. That is what a submitter can actually do, and
        // the result is a perfectly self-consistent document.
        let mut report: serde_json::Value =
            serde_json::from_slice(&observation.artifacts.report).unwrap();
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
                if *name == REPORT {
                    (*name, changed.as_slice())
                } else {
                    (*name, *bytes)
                }
            })
            .collect::<Vec<_>>();
        // Self-consistency is not admission: the report must be the one this
        // run's evaluation produced.
        assert!(observation.verify_saved(&saved).is_err());
    }
}
