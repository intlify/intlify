// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Regenerate or check the committed registry schemas.
//!
//! This is a contributor entry point, not a product command. It writes only the
//! artifacts this crate owns and performs no other filesystem work. It works
//! the way `intlify_authoring`'s `generate_authoring_schema` does.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use intlify_authoring::schema::format_schema;
use intlify_authoring_identity::schema::COMMITTED_SCHEMAS;
use serde_json::Value;

const USAGE: &str = "generate_identity_schema [--check|--write]";

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
fn process(path: &Path, generated: Result<Value, serde_json::Error>, write: bool) -> bool {
    let Ok(generated) = generated else {
        eprintln!("the schema for {} could not be generated", path.display());
        return false;
    };
    // Compare decoded values: the repository formatter owns the committed
    // file's layout, so a byte comparison would report a formatting difference
    // as a stale schema, and rewriting an equal value would only undo it.
    let committed = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let fresh = committed.as_ref() == Some(&generated);

    if !write {
        if fresh {
            println!("{} is fresh", path.display());
        } else {
            eprintln!("{} is stale; rerun with --write", path.display());
        }
        return fresh;
    }
    if fresh {
        println!("{} is already fresh", path.display());
        return true;
    }
    let Ok(rendered) = format_schema(generated) else {
        eprintln!("the schema for {} could not be formatted", path.display());
        return false;
    };
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            eprintln!("could not create {}: {error}", parent.display());
            return false;
        }
    }
    match std::fs::write(path, rendered) {
        Ok(()) => {
            println!("wrote {}", path.display());
            true
        }
        Err(error) => {
            eprintln!("could not write {}: {error}", path.display());
            false
        }
    }
}
