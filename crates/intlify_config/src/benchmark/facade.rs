// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Feature-isolated developer facade for completed observations. It accepts no
//! project settings, arbitrary measured callbacks, or partial Profile state.
//! Revalidation retains the original independent capture authority privately.

use super::run::{PreparedRun, RecordedRun};
use super::shared::{measurement::Outcome, pipeline};

/// Diagnostic outcome, never a numeric performance decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationOutcome {
    Complete,
    Incomplete,
    Invalid,
}

/// Counts describe inventory completeness, not timings or an aggregate score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationSummary {
    pub outcome: ObservationOutcome,
    pub planned_cases: usize,
    pub measured_cases: usize,
    pub non_measured_cases: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationError {
    CaptureFailed,
    RecordProductionFailed,
    InvalidFileInventory,
    InvalidRecords,
}

impl std::fmt::Display for ObservationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::CaptureFailed => {
                "minimum observational capture could not be acquired or completed"
            }
            Self::RecordProductionFailed => {
                "common observation records could not be produced and admitted"
            }
            Self::InvalidFileInventory => {
                "saved observation files do not match the complete issued file inventory"
            }
            Self::InvalidRecords => "saved observation records failed native-backed admission",
        })
    }
}
impl std::error::Error for ObservationError {}

/// An owned completed capture and its admitted serialized records. No mutable
/// accessor, deserializer, public constructor, or underlying locale core exists.
pub struct CompletedObservation {
    run: RecordedRun,
    artifacts: pipeline::Artifacts,
}

/// Execute the fixed 127-case, six-boundary native observational smoke profile.
/// POSIX clock acquisition is supported on Linux/macOS; unsupported acquisition
/// fails explicitly rather than selecting an undeclared alternative method.
pub fn observe_smoke() -> Result<CompletedObservation, ObservationError> {
    let run = PreparedRun::acquire()
        .and_then(PreparedRun::collect)
        .map_err(|_| ObservationError::CaptureFailed)?;
    let artifacts =
        pipeline::produce(&run).map_err(|_| ObservationError::RecordProductionFailed)?;
    Ok(CompletedObservation { run, artifacts })
}

impl CompletedObservation {
    /// Complete record bytes with fixed developer filenames, not storage paths.
    /// A negative run can legitimately have no Measurement Evidence Set.
    pub fn records(&self) -> Vec<(&'static str, &[u8])> {
        let mut records = vec![
            ("run-plan.json", self.artifacts.run_plan.as_slice()),
            (
                "owner-result.json",
                self.artifacts.owner_result[0].as_slice(),
            ),
        ];
        if let Some(evidence) = &self.artifacts.evidence {
            records.push(("measurement-evidence.json", evidence));
        }
        records.push(("run-evaluation.json", &self.artifacts.evaluation));
        records.push(("report.json", &self.artifacts.report));
        records
    }

    /// Revalidate bounded saved bytes against this capture, not against another
    /// run or verification conditions learned from the submitted files. No I/O.
    pub fn verify_saved(
        &self,
        records: &[(&str, &[u8])],
    ) -> Result<ObservationSummary, ObservationError> {
        let expected = self.records();
        if records.len() != expected.len()
            || expected
                .iter()
                .any(|(name, _)| records.iter().filter(|(given, _)| name == given).count() != 1)
        {
            return Err(ObservationError::InvalidFileInventory);
        }
        let native = records
            .iter()
            .filter(|(name, _)| *name == "owner-result.json")
            .map(|(_, bytes)| *bytes)
            .collect::<Vec<_>>();
        let common = records
            .iter()
            .filter(|(name, _)| *name != "owner-result.json")
            .map(|(_, bytes)| *bytes)
            .collect::<Vec<_>>();
        let admitted = pipeline::validate_records(
            &self.run,
            &native,
            &common,
            &self.artifacts.report_identity,
        )
        .map_err(|_| ObservationError::InvalidRecords)?;
        Ok(ObservationSummary {
            outcome: match admitted.outcome {
                Outcome::Complete => ObservationOutcome::Complete,
                Outcome::Incomplete => ObservationOutcome::Incomplete,
                Outcome::Invalid => ObservationOutcome::Invalid,
            },
            planned_cases: admitted.planned_cases,
            measured_cases: admitted.measured_cases,
            non_measured_cases: admitted.non_measured_cases,
        })
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn facade_returns_only_completed_bytes_and_revalidates_saved_inventory() {
        let observation = observe_smoke().unwrap();
        let records = observation.records();
        let summary = observation.verify_saved(&records).unwrap();
        assert_eq!(
            summary,
            ObservationSummary {
                outcome: ObservationOutcome::Complete,
                planned_cases: 127,
                measured_cases: 127,
                non_measured_cases: 0
            }
        );
        assert_eq!(
            observation.verify_saved(&records[..4]),
            Err(ObservationError::InvalidFileInventory)
        );
        let mut reversed = records.clone();
        reversed.reverse();
        assert_eq!(observation.verify_saved(&reversed).unwrap(), summary);
        let mut corrupt = records.clone();
        corrupt[1].1 = b"{}";
        assert_eq!(
            observation.verify_saved(&corrupt),
            Err(ObservationError::InvalidRecords)
        );
        let mut duplicate = records.clone();
        duplicate[1] = duplicate[0];
        assert_eq!(
            observation.verify_saved(&duplicate),
            Err(ObservationError::InvalidFileInventory)
        );
    }
}
