// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Developer-only schema output/freshness entry, not CLI discovery or migration.

use std::env;
use std::error::Error;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

fn run() -> Result<(), Box<dyn Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let output = intlify_config::schema::format_schema(
        intlify_config::schema::project_profile_config_schema()?,
    )?;
    let artifact =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/project-profile-config-v0.schema.json");
    match args.as_slice() {
        [] => io::stdout().lock().write_all(output.as_bytes())?,
        [flag] if flag == "--check" => {
            let stored = std::fs::read(&artifact)?;
            if stored != output.as_bytes() {
                return Err("project-profile schema is stale; regenerate it with --write".into());
            }
        }
        [flag] if flag == "--write" => std::fs::write(artifact, output.as_bytes())?,
        _ => return Err("usage: generate_config_schema [--check | --write]".into()),
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
