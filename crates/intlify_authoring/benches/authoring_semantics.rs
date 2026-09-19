// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fixed developer smoke: actual capture, retained records, disk reread, and
//! owner-backed common admission. No average, threshold, or product CLI API.

use std::error::Error;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use intlify_authoring::benchmark::facade::{observe_smoke, ObservationOutcome};

const USAGE: &str = "authoring_semantics --profile smoke --validate [--output-dir NEW_DIRECTORY]";
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

enum Command {
    Help,
    TestBuild,
    Smoke { output: Option<PathBuf> },
}

fn parse(args: Vec<OsString>) -> Result<Command, &'static str> {
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
        if arg == "--profile"
            && !profile
            && args.next().as_deref() == Some(std::ffi::OsStr::new("smoke"))
        {
            profile = true;
        } else if arg == "--validate" && !validate {
            validate = true;
        } else if arg == "--output-dir" && output.is_none() {
            output = Some(PathBuf::from(args.next().ok_or(USAGE)?));
        } else {
            return Err(USAGE);
        }
    }
    if !profile || !validate {
        return Err(USAGE);
    }
    Ok(Command::Smoke { output })
}

fn run() -> Result<(), Box<dyn Error>> {
    let output = match parse(std::env::args_os().skip(1).collect())? {
        Command::Help => {
            println!("{USAGE}");
            return Ok(());
        }
        Command::TestBuild => {
            println!("smoke not executed; use {USAGE}");
            return Ok(());
        }
        Command::Smoke { output } => output,
    };
    let directory = match output {
        Some(directory) => {
            std::fs::create_dir(&directory)?;
            directory
        }
        None => tempfile::Builder::new()
            .prefix("intlify-authoring-smoke-")
            .tempdir()?
            .keep(),
    };
    eprintln!("Observation records: {}", directory.display());

    let observation = observe_smoke()?;
    let records = observation.records();
    for (name, bytes) in &records {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    // Re-read what was actually written. Admission runs against the retained
    // run, so a record set that is merely self-consistent is not admitted.
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

    // Withholding any one record must not admit. This is checked here rather
    // than only in unit tests, because the files on disk are what a consumer
    // would actually be handed.
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

    // Presentation only: the authoritative structured report is retained above.
    println!(
        "{}",
        serde_json::json!({
            "format": "intlify-authoring-observational-smoke-summary/0",
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

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
