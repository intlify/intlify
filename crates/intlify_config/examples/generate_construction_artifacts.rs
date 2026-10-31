// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the built-in Resolver Construction Root Package's artifacts.
//!
//! The five members and the Root Package artifact are written from this
//! crate's values. A file digest covers every byte, so `--check` compares the
//! exact bytes, not decoded values, and the artifacts are kept out of the
//! repository formatter. `--write` also prints the two digests the crate pins,
//! which belong in `src/construction/root.rs`.

use std::path::PathBuf;
use std::process::ExitCode;

const ROOT: &str = "conformance";

fn main() -> ExitCode {
    let write = match std::env::args().nth(1).as_deref() {
        None | Some("--check") => false,
        Some("--write") => true,
        Some(_) => {
            eprintln!("usage: generate_construction_artifacts [--check | --write]");
            return ExitCode::FAILURE;
        }
    };
    let support = intlify_config::construction_support::artifacts()
        .and_then(|artifacts| Ok((artifacts, intlify_config::construction_support::pinned()?)));
    let (artifacts, pinned) = match support {
        Ok(support) => support,
        Err(error) => {
            eprintln!("the artifacts could not be written: {error}");
            return ExitCode::FAILURE;
        }
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ROOT);
    let mut stale = false;
    for artifact in &artifacts {
        let path = root.join(artifact.path);
        if write {
            if let Some(parent) = path.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    eprintln!("could not create {}: {error}", parent.display());
                    return ExitCode::FAILURE;
                }
            }
            if let Err(error) = std::fs::write(&path, &artifact.bytes) {
                eprintln!("could not write {}: {error}", path.display());
                return ExitCode::FAILURE;
            }
            println!("wrote {}", path.display());
        } else if std::fs::read(&path).ok().as_deref() == Some(artifact.bytes.as_slice()) {
            println!("{} is fresh", path.display());
        } else {
            eprintln!("{} is stale; rerun with --write", path.display());
            stale = true;
        }
    }
    if write {
        println!("pinned digest: {}", pinned.digest);
        println!("pinned canonical digest: {}", pinned.canonical_digest);
    }
    if stale {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
