// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the built-in Resolver Construction Root Package's artifacts and the
//! construction authorities.
//!
//! The five members, the Root Package artifact, the five authorities, and the
//! Registry Package's six members and container are written from this
//! crate's values, and the construction vectors from constructing those
//! authorities under the digests the crate pins. A file digest covers every byte, so
//! `--check` compares the exact bytes, not decoded values, and the artifacts
//! are kept out of the repository formatter. `--write` also prints the digests
//! the crate pins: the Root Package's two belong in
//! `src/construction/root.rs`, and the authorities' in
//! `src/construction/profile.rs`, `src/construction/authority_set.rs`, and
//! `src/construction/capability.rs`, and the registries' in
//! `src/construction/registry/safe_origin.rs`,
//! `src/construction/registry/check.rs`, and
//! `src/construction/registry/finding.rs`, and the container's in
//! `src/construction/registry.rs`.

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
        artifacts.extend(support::registries()?);
        Ok::<_, serde_json::Error>((
            artifacts,
            support::pinned()?,
            support::authority_pins()?,
            support::registry_pins()?,
        ))
    })();
    let (mut artifacts, pinned, authority_pins, registry_pins) = match support {
        Ok(support) => support,
        Err(error) => {
            eprintln!("the artifacts could not be written: {error}");
            return ExitCode::FAILURE;
        }
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ROOT);
    let mut stale = false;
    // The vectors construct from the written artifacts under the digests
    // compiled in, so after a value changes they wait for the new pins.
    match intlify_config::construction_support::vectors() {
        Ok(vectors) => artifacts.push(vectors),
        Err(reason) => {
            eprintln!(
                "the construction vectors could not be written: {reason}; pin the printed digests and rerun with --write"
            );
            stale = true;
        }
    }
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
        println!(
            "pinned Resolver Capability Specification digest: {}",
            authority_pins.resolver_capability_specification
        );
        println!(
            "pinned Safe Origin Kind Registry schema digest: {}",
            registry_pins.safe_origin_kind_registry_schema
        );
        println!(
            "pinned Safe Origin Kind Registry digest: {}",
            registry_pins.safe_origin_kind_registry
        );
        println!(
            "pinned Check Registry schema digest: {}",
            registry_pins.check_registry_schema
        );
        println!(
            "pinned Check Registry digest: {}",
            registry_pins.check_registry
        );
        println!(
            "pinned Finding Registry schema digest: {}",
            registry_pins.finding_registry_schema
        );
        println!(
            "pinned Finding Registry digest: {}",
            registry_pins.finding_registry
        );
        println!(
            "pinned Registry Package digest: {}",
            registry_pins.registry_package
        );
    }
    if stale {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
