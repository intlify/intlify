// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fixed developer smoke: actual capture, retained records, disk reread, and
//! native-backed common admission. No average, threshold, or product CLI API.

use std::error::Error;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use intlify_config::measurement::{observe_smoke, ObservationOutcome};

const USAGE: &str = "profile_resolver --profile smoke --validate [--output-dir NEW_DIRECTORY]";
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
            .prefix("intlify-config-smoke-")
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
    // Presentation only: the authoritative structured report is retained above.
    println!(
        "{}",
        serde_json::json!({
            "format": "intlify-config-observational-smoke-summary/0",
            "outcome": match summary.outcome { ObservationOutcome::Complete => "complete",
                ObservationOutcome::Incomplete => "incomplete", ObservationOutcome::Invalid => "invalid" },
            "plannedCases": summary.planned_cases.to_string(),
            "measuredCases": summary.measured_cases.to_string(),
            "nonMeasuredCases": summary.non_measured_cases.to_string(),
            "numericDecisions": "forbidden", "savedRecordsRevalidated": true
        })
    );
    if summary.outcome != ObservationOutcome::Complete {
        return Err(
            "required smoke inventory did not complete; retained records contain diagnostic causes"
                .into(),
        );
    }
    Ok(())
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
