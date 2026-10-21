// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the artifact envelope vectors an independent implementation checks.
//!
//! The vectors hold collection members with what each declares, the tuple it
//! establishes, whether its integrity holds, how its body admission ends, and
//! its canonical bytes, with the digests, spellings, orders, content changes,
//! and references that depend on them. The Node checker decides every one of
//! these from the members itself, so a disagreement says whether this crate or
//! the checker is wrong.

use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::Value;

const ARTIFACT: &str = "fixtures/envelope/vectors.json";

fn main() -> ExitCode {
    let write = match std::env::args().nth(1).as_deref() {
        None | Some("--check") => false,
        Some("--write") => true,
        Some(_) => {
            eprintln!("usage: generate_envelope_vectors [--check | --write]");
            return ExitCode::FAILURE;
        }
    };
    let document = match intlify_config::vector_support::envelope::document() {
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
