// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the conformance suite's Suite Index, its schema, the Case Schema,
//! and the construction cases' manifests, fixtures, and changed bodies.
//!
//! The index pins each construction root member, authority, and Registry
//! Package member by the file digest of the bytes this crate writes, so it is
//! rewritten whenever one of them is: rerun this after
//! `generate_construction_artifacts`. A file digest covers every byte, so
//! `--check` compares the exact bytes, and the files are kept out of the
//! repository formatter. `--write` also prints the two schemas' file digests,
//! which the runner pins in `src/conformance/index.rs` and
//! `src/conformance/case.rs`.

use std::path::PathBuf;
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
