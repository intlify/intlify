// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Developer-only output/freshness entry for the vocabulary artifact schema.

use std::env;
use std::error::Error;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

fn run() -> Result<(), Box<dyn Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let schema = intlify_config::schema::intent_surface_class_vocabulary_schema()?;
    let output = intlify_config::schema::format_schema(schema.clone())?;
    let artifact = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("schema/intent-surface-class-vocabulary-v0.schema.json");
    match args.as_slice() {
        [] => io::stdout().lock().write_all(output.as_bytes())?,
        [flag] if flag == "--check" => {
            // Compare decoded values: the repository formatter owns the layout.
            let stored: serde_json::Value = serde_json::from_slice(&std::fs::read(&artifact)?)?;
            if stored != schema {
                return Err("vocabulary schema is stale; regenerate it with --write".into());
            }
        }
        [flag] if flag == "--write" => std::fs::write(artifact, output.as_bytes())?,
        _ => return Err("usage: generate_vocabulary_schema [--check | --write]".into()),
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
