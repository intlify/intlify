// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the validator value vectors an independent implementation checks.
//!
//! The vectors hold the exact readings revision `"0"`'s Draft 7 validator
//! makes of numbers, JSON values, URI references, and JSON Pointers. The Node
//! checker recomputes every one with an implementation of its own, so a
//! disagreement says which of the two is wrong.

use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::Value;

const ARTIFACT: &str = "fixtures/validator-values/vectors.json";

fn main() -> ExitCode {
    let write = match std::env::args().nth(1).as_deref() {
        None | Some("--check") => false,
        Some("--write") => true,
        Some(_) => {
            eprintln!("usage: generate_validator_value_vectors [--check | --write]");
            return ExitCode::FAILURE;
        }
    };
    let document = match intlify_config::vector_support::validator_values::document() {
        Ok(document) => document,
        Err(error) => {
            eprintln!("the vectors could not be built: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut rendered = serde_json::to_string_pretty(&document).expect("serializable document");
    rendered.push('\n');

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ARTIFACT);
    if write {
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                eprintln!("could not create {}: {error}", parent.display());
                return ExitCode::FAILURE;
            }
        }
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
    // Compare decoded values: the repository formatter owns the file's layout.
    match std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    {
        Some(committed) if committed == document => {
            println!("{} is fresh", path.display());
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{} is stale; rerun with --write", path.display());
            ExitCode::FAILURE
        }
    }
}
