// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the conformance suite's Suite Index, its schema, the Case Schema,
//! the construction cases' manifests, fixtures, and changed bodies, the
//! input admission cases' manifests, fixtures, and raw inputs, and the
//! Resolver Cases' manifests, fixtures, raw inputs, expected outcomes, and
//! copy of the locale data. Each expected outcome is what the resolver does
//! with its entry, a candidate for review rather than output accepted
//! because the implementation produced it.
//!
//! The index pins each construction root member, authority, and Registry
//! Package member by the file digest of the bytes this crate writes, so it is
//! rewritten whenever one of them is: rerun this after
//! `generate_construction_artifacts`. A file digest covers every byte, so
//! `--check` compares the exact bytes, and the files are kept out of the
//! repository formatter. `--write` also prints the two schemas' file digests,
//! which the runner pins in `src/conformance/index.rs` and
//! `src/conformance/case.rs`. A file below the case and fixture directories
//! that the suite no longer holds is stale: `--write` removes it, with any
//! directory it leaves empty, and `--check` reports it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const ROOT: &str = "conformance";

fn main() -> ExitCode {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let write = match args.iter().map(|arg| arg.to_str()).collect::<Vec<_>>()[..] {
        [] | [Some("--check")] => false,
        [Some("--write")] => true,
        _ => {
            eprintln!("usage: generate_conformance_suite [--check | --write]");
            return ExitCode::FAILURE;
        }
    };
    let support = (|| {
        use intlify_config::conformance_support as support;
        Ok::<_, serde_json::Error>((
            support::artifacts()?,
            support::schema_digest()?,
            support::case_schema_digest()?,
        ))
    })();
    let (artifacts, schema_digest, case_schema_digest) = match support {
        Ok(support) => support,
        Err(error) => {
            eprintln!("the suite index could not be written: {error}");
            return ExitCode::FAILURE;
        }
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ROOT);
    let mut stale = false;
    for artifact in &artifacts {
        let path = root.join(&artifact.path);
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
    let written = artifacts
        .iter()
        .map(|artifact| artifact.path.as_str())
        .collect::<BTreeSet<_>>();
    for directory in intlify_config::conformance_support::WRITTEN_DIRECTORIES {
        let mut held = Vec::new();
        if let Err(error) = files_below(&root.join(directory), &mut held) {
            eprintln!("could not read {}: {error}", root.join(directory).display());
            return ExitCode::FAILURE;
        }
        held.sort();
        for path in held
            .iter()
            .filter(|path| !written.contains(relative(path, &root).as_str()))
        {
            if !write {
                eprintln!("{} is not written; rerun with --write", path.display());
                stale = true;
            } else if let Err(error) = remove(path, &root.join(directory)) {
                eprintln!("could not remove {}: {error}", path.display());
                return ExitCode::FAILURE;
            } else {
                println!("removed {}", path.display());
            }
        }
    }
    if write {
        println!("pinned Suite Index Schema digest: {schema_digest}");
        println!("pinned Case Schema digest: {case_schema_digest}");
    }
    if stale {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Collect every file below `directory`, at any depth.
fn files_below(directory: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            files_below(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

/// Remove the file at `path`, then each directory it leaves empty, up to but
/// not including `top`.
fn remove(path: &Path, top: &Path) -> std::io::Result<()> {
    std::fs::remove_file(path)?;
    let mut parent = path.parent();
    while let Some(directory) = parent.filter(|directory| *directory != top) {
        if std::fs::read_dir(directory)?.next().is_some() {
            break;
        }
        std::fs::remove_dir(directory)?;
        parent = directory.parent();
    }
    Ok(())
}

/// The path of `path` below `root`, its components joined by `/` as the
/// suite's paths are.
fn relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
