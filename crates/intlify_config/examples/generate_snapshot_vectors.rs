// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the Programmatic Entry Snapshot vectors an independent
//! implementation checks.
//!
//! The vectors hold canonical Snapshots with their framing `S`, their content
//! digest, and their counts under the `bootstrapOnly.snapshot` bounds. The
//! Node checker frames, digests, and counts every one itself, so a
//! disagreement says whether this crate or the checker is wrong.

use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::Value;

const ARTIFACT: &str = "fixtures/snapshot/vectors.json";

fn main() -> ExitCode {
    // At most one flag: an extra argument, as in `--write --check`, is refused
    // before anything is written (plan 025 decision #297).
    let args: Vec<String> = std::env::args().skip(1).collect();
    let write = match args.as_slice() {
        [] => false,
        [flag] if flag == "--check" => false,
        [flag] if flag == "--write" => true,
        _ => {
            eprintln!("usage: generate_snapshot_vectors [--check | --write]");
            return ExitCode::FAILURE;
        }
    };
    let document = intlify_config::vector_support::snapshot::document();
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
