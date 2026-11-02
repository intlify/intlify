// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the built-in Resolver Construction Root Package's artifacts and the
//! construction authorities.
//!
//! The five members, the Root Package artifact, and the four authorities are
//! written from this crate's values. A file digest covers every byte, so
//! `--check` compares the exact bytes, not decoded values, and the artifacts
//! are kept out of the repository formatter. `--write` also prints the digests
//! the crate pins: the Root Package's two belong in
//! `src/construction/root.rs`, and the authorities' in
//! `src/construction/profile.rs` and `src/construction/authority_set.rs`.

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
    let support = (|| {
        use intlify_config::construction_support as support;
        let mut artifacts = support::artifacts()?;
        artifacts.extend(support::authorities()?);
        Ok::<_, serde_json::Error>((artifacts, support::pinned()?, support::authority_pins()?))
    })();
    let (artifacts, pinned, authority_pins) = match support {
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
        println!(
            "pinned Profile Specification Schema digest: {}",
            authority_pins.profile_specification_schema
        );
        println!(
            "pinned Profile Specification semantic digest: {}",
            authority_pins.profile_specification_semantic
        );
        println!(
            "pinned Configuration Schema Authority Set digest: {}",
            authority_pins.configuration_schema_authority_set
        );
        println!(
            "pinned configuration schema digest: {}",
            authority_pins.configuration_schema
        );
    }
    if stale {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
