// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Regenerate or check the committed schemas.
//!
//! This is a contributor entry point, not a product command. It writes only the
//! artifacts this crate owns and performs no other filesystem work.

use std::path::PathBuf;
use std::process::ExitCode;

use intlify_authoring::schema::{format_schema, COMMITTED_SCHEMAS};

const USAGE: &str = "generate_authoring_schema [--check|--write]";

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

    let mut failed = false;
    for schema in &COMMITTED_SCHEMAS {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(schema.path);
        if !process(&path, (schema.generate)(), write) {
            failed = true;
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Write or check one schema, reporting what happened.
fn process(
    path: &std::path::Path,
    generated: Result<serde_json::Value, serde_json::Error>,
    write: bool,
) -> bool {
    let Ok(generated) = generated else {
        eprintln!("the schema for {} could not be generated", path.display());
        return false;
    };
    let Ok(rendered) = format_schema(generated.clone()) else {
        eprintln!("the schema for {} could not be formatted", path.display());
        return false;
    };

    if write {
        // Leave an unchanged schema alone. The repository formatter owns the
        // committed layout, so rewriting an equal value would only undo it.
        let unchanged = std::fs::read_to_string(path)
            .ok()
            .and_then(|committed| serde_json::from_str::<serde_json::Value>(&committed).ok())
            .is_some_and(|committed| committed == generated);
        if unchanged {
            println!("{} is already fresh", path.display());
            return true;
        }
        return match std::fs::write(path, rendered) {
            Ok(()) => {
                println!("wrote {}", path.display());
                true
            }
            Err(error) => {
                eprintln!("could not write {}: {error}", path.display());
                false
            }
        };
    }

    let Ok(committed) = std::fs::read_to_string(path) else {
        eprintln!("could not read {}", path.display());
        return false;
    };
    // Compare decoded values: the repository formatter owns the committed
    // file's layout, so a byte comparison would report a formatting difference
    // as a stale schema.
    match serde_json::from_str::<serde_json::Value>(&committed) {
        Ok(value) if value == generated => {
            println!("{} is fresh", path.display());
            true
        }
        Ok(_) => {
            eprintln!("{} is stale; rerun with --write", path.display());
            false
        }
        Err(error) => {
            eprintln!("{} is not valid JSON: {error}", path.display());
            false
        }
    }
}
