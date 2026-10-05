// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! One observational smoke run, and the standalone runner around it.
//!
//! A smoke run captures, produces the common records, and can re-admit them
//! from the bytes that were actually written to disk. Re-admission uses the run
//! that was retained here, not the submitted documents: a record set that is
//! merely self-consistent proves nothing about what was measured.

use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use super::run::{PreparedRun, RecordedRun, RunFailure};
use super::Owner;
use crate::measurement::Outcome;
use crate::pipeline::{self, Artifacts, Validation, ValidationFailure};

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
/// The smoke profile has no numeric-decision facility, so nothing downstream
/// may compare these against a threshold.
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
pub struct Observation<O: Owner> {
    run: RecordedRun<O>,
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
pub fn observe<O: Owner>() -> Result<Observation<O>, SmokeFailure> {
    let run = PreparedRun::<O>::acquire()
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

impl<O: Owner> Observation<O> {
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
        self.validate(&[owner], &common)
    }

    /// Admit owner documents and common records against this retained run.
    ///
    /// This is [`Observation::verify_saved`] without the file names, so a
    /// caller can offer what a submitter could: no owner document, two of
    /// them, or one from another run.
    pub fn validate(&self, owner: &[&[u8]], common: &[&[u8]]) -> Result<Summary, SmokeFailure> {
        pipeline::validate_records(&self.run, owner, common, &self.artifacts.report_identity)
            .map(Summary::from)
            .map_err(SmokeFailure::Admission)
    }

    /// Borrow the owner document this run wrote.
    #[must_use]
    pub fn owner_document(&self) -> &[u8] {
        &self.owner
    }

    /// Borrow the retained run.
    #[must_use]
    pub const fn run(&self) -> &RecordedRun<O> {
        &self.run
    }
}

/// What one owner's standalone smoke runner accepts and prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Runner {
    /// The usage line, naming the bench target.
    pub usage: &'static str,
    /// The format of the presentation-only summary it prints.
    pub summary_format: &'static str,
}

// Private reader capacity for one saved record, not a project-wide limit.
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

enum Command {
    Help,
    TestBuild,
    Smoke { output: Option<PathBuf> },
}

fn parse(usage: &'static str, args: Vec<OsString>) -> Result<Command, &'static str> {
    let args = args
        .into_iter()
        .filter(|arg| arg != "--bench")
        .collect::<Vec<_>>();
    // Cargo can execute harness=false benches as compilation-only test targets.
    if args.is_empty() || args == ["--test"] {
        return Ok(Command::TestBuild);
    }
    if args == ["--help"] {
        return Ok(Command::Help);
    }
    let mut profile = false;
    let mut validate = false;
    let mut output = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--profile" && !profile && args.next().as_deref() == Some(OsStr::new("smoke")) {
            profile = true;
        } else if arg == "--validate" && !validate {
            validate = true;
        } else if arg == "--output-dir" && output.is_none() {
            output = Some(PathBuf::from(args.next().ok_or(usage)?));
        } else {
            return Err(usage);
        }
    }
    if !profile || !validate {
        return Err(usage);
    }
    Ok(Command::Smoke { output })
}

impl Runner {
    /// Run the fixed developer smoke: capture, write, reread, and re-admit.
    ///
    /// `args` excludes the program name. Without `--output-dir`, the records
    /// go into the directory `scratch` creates. There is no average, no
    /// threshold, and no product API here.
    pub fn main<O: Owner>(
        self,
        args: Vec<OsString>,
        scratch: impl FnOnce() -> std::io::Result<PathBuf>,
    ) -> ExitCode {
        match self.run::<O>(args, scratch) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::FAILURE
            }
        }
    }

    fn run<O: Owner>(
        self,
        args: Vec<OsString>,
        scratch: impl FnOnce() -> std::io::Result<PathBuf>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let output = match parse(self.usage, args)? {
            Command::Help => {
                println!("{}", self.usage);
                return Ok(());
            }
            Command::TestBuild => {
                println!("smoke not executed; use {}", self.usage);
                return Ok(());
            }
            Command::Smoke { output } => output,
        };
        let directory = match output {
            Some(directory) => {
                std::fs::create_dir(&directory)?;
                directory
            }
            None => scratch()?,
        };
        eprintln!("Observation records: {}", directory.display());

        let observation = observe::<O>()?;
        let records = observation.records();
        for (name, bytes) in &records {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(name))?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }

        // Re-read what was actually written. Admission runs against the
        // retained run, so a record set that is merely self-consistent is not
        // admitted.
        let mut saved = Vec::with_capacity(records.len());
        for (name, _) in &records {
            let mut bytes = Vec::new();
            File::open(directory.join(name))?
                .take(MAX_FILE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if u64::try_from(bytes.len())? > MAX_FILE_BYTES {
                return Err("saved record exceeds reader capacity".into());
            }
            saved.push((*name, bytes));
        }
        let inputs = saved
            .iter()
            .map(|(name, bytes)| (*name, bytes.as_slice()))
            .collect::<Vec<_>>();
        let summary = observation.verify_saved(&inputs)?;

        // Withholding any one record must not admit. This is checked here
        // rather than only in unit tests, because the files on disk are what a
        // consumer would actually be handed.
        for withheld in 0..inputs.len() {
            let partial = inputs
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != withheld)
                .map(|(_, record)| *record)
                .collect::<Vec<_>>();
            if observation.verify_saved(&partial).is_ok() {
                return Err(format!("withholding {} was admitted", inputs[withheld].0).into());
            }
        }

        // Presentation only: the authoritative structured report is retained
        // above.
        println!(
            "{}",
            serde_json::json!({
                "format": self.summary_format,
                "outcome": match summary.outcome {
                    ObservationOutcome::Complete => "complete",
                    ObservationOutcome::Incomplete => "incomplete",
                    ObservationOutcome::Invalid => "invalid",
                },
                "plannedCases": summary.planned_cases.to_string(),
                "measuredCases": summary.measured_cases.to_string(),
                "nonMeasuredCases": summary.non_measured_cases.to_string(),
                "numericDecisions": "forbidden",
                "savedRecordsRevalidated": true,
                "withheldRecordsRejected": true
            })
        );
        if summary.outcome == ObservationOutcome::Complete {
            Ok(())
        } else {
            Err("the run did not complete its planned inventory".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USAGE: &str = "owner --profile smoke --validate [--output-dir NEW_DIRECTORY]";

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_runner_accepts_exactly_the_smoke_profile_with_validation() {
        assert!(matches!(
            parse(USAGE, args(&["--profile", "smoke", "--validate"])),
            Ok(Command::Smoke { output: None })
        ));
        assert!(matches!(
            parse(
                USAGE,
                args(&[
                    "--bench",
                    "--validate",
                    "--profile",
                    "smoke",
                    "--output-dir",
                    "out"
                ])
            ),
            Ok(Command::Smoke { output: Some(_) })
        ));
        // Cargo's own invocations do not run a smoke.
        assert!(matches!(parse(USAGE, args(&[])), Ok(Command::TestBuild)));
        assert!(matches!(
            parse(USAGE, args(&["--test"])),
            Ok(Command::TestBuild)
        ));
        assert!(matches!(parse(USAGE, args(&["--help"])), Ok(Command::Help)));
        // Without validation, with another profile, or with a repeated or
        // unknown flag, nothing runs.
        for refused in [
            &["--profile", "smoke"][..],
            &["--validate"],
            &["--profile", "full", "--validate"],
            &["--profile", "smoke", "--validate", "--validate"],
            &["--profile", "smoke", "--validate", "--output-dir"],
            &["--profile", "smoke", "--validate", "--average"],
        ] {
            assert_eq!(
                parse(USAGE, args(refused)).err(),
                Some(USAGE),
                "{refused:?}"
            );
        }
    }
}
