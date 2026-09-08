// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Developer-only companion-schema generation. No arbitrary destination, input
//! record, discovery, caller-selected schema, or measurement execution.

use std::error::Error;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

fn run() -> Result<(), Box<dyn Error>> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let (name, schema) = match args.first().and_then(|arg| arg.to_str()) {
        Some("plan") => (
            "measurement-run-plan-v0.schema.json",
            intlify_config::schema::measurement_run_plan_schema()?,
        ),
        Some("case") => (
            "measurement-case-identity-v0.schema.json",
            intlify_config::schema::measurement_case_identity_schema()?,
        ),
        _ => return Err("usage: generate_measurement_schema <plan|case> [--check|--write]".into()),
    };
    let output = intlify_config::schema::format_schema(schema)?;
    let artifact = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("schema")
        .join(name);
    match &args[1..] {
        [] => io::stdout().lock().write_all(output.as_bytes())?,
        [flag] if flag == "--check" => {
            if std::fs::read(artifact)? != output.as_bytes() {
                return Err("measurement schema is stale; regenerate it with --write".into());
            }
        }
        [flag] if flag == "--write" => std::fs::write(artifact, output.as_bytes())?,
        _ => return Err("usage: generate_measurement_schema <plan|case> [--check|--write]".into()),
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
