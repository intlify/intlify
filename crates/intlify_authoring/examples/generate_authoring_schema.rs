// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Regenerate or check the committed projection schema.
//!
//! This is a contributor entry point, not a product command. It writes only the
//! one artifact this crate owns and performs no other filesystem work.

use std::path::PathBuf;
use std::process::ExitCode;

use intlify_authoring::schema::{format_schema, intent_projection_schema};

const USAGE: &str = "generate_authoring_schema [--check|--write]";
const ARTIFACT: &str = "schema/intent-projection-v0.schema.json";

fn artifact_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ARTIFACT)
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let write = match arguments.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["--check"] | [] => false,
        ["--write"] => true,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };

    let Ok(generated) = intent_projection_schema() else {
        eprintln!("the projection schema could not be generated");
        return ExitCode::FAILURE;
    };
    let Ok(rendered) = format_schema(generated.clone()) else {
        eprintln!("the projection schema could not be formatted");
        return ExitCode::FAILURE;
    };

    let path = artifact_path();
    if write {
        return match std::fs::write(&path, rendered) {
            Ok(()) => {
                println!("wrote {}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("could not write {}: {error}", path.display());
                ExitCode::FAILURE
            }
        };
    }

    let Ok(committed) = std::fs::read_to_string(&path) else {
        eprintln!("could not read {}", path.display());
        return ExitCode::FAILURE;
    };
    // Compare decoded values: the repository formatter owns the committed
    // file's layout, so a byte comparison would report a formatting difference
    // as a stale schema.
    match serde_json::from_str::<serde_json::Value>(&committed) {
        Ok(value) if value == generated => {
            println!("{} is fresh", path.display());
            ExitCode::SUCCESS
        }
        Ok(_) => {
            eprintln!("{} is stale; rerun with --write", path.display());
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("{} is not valid JSON: {error}", path.display());
            ExitCode::FAILURE
        }
    }
}
