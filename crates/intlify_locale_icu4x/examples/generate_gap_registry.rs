// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Developer-only writer and check of the corpus gate's registry in
//! `evidence/`: CLDR 48.2.0's locale canonicalization corpus, run through the
//! committed artifact in `data/`. The check fails when a case is blocking or
//! the committed registry is stale.

use std::env;
use std::error::Error;
use std::path::Path;
use std::process::ExitCode;

use intlify_locale_icu4x::corpus::{GapRegistry, CORPUS_PATH};
use intlify_locale_icu4x::{ConformanceLimits, ReadLimits, Role, VerifiedManifest};

/// Explicit capacity the committed artifact is within.
const LIMITS: ReadLimits = ReadLimits {
    manifest_bytes: 16_384,
    manifest_depth: 8,
    payloads: 2,
    payload_bytes: 1 << 20,
    marker_bytes: 1 << 20,
    conformance: ConformanceLimits {
        validity_groups: 64,
        codes: 1 << 15,
        extension_keys: 64,
        types: 2_048,
        corrections: 128,
        work: 1 << 16,
    },
};

fn run() -> Result<(), Box<dyn Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let data = root.join("data");
    let manifest = VerifiedManifest::read(&std::fs::read(data.join("manifest.json"))?, &LIMITS)
        .map_err(|failure| format!("the committed manifest is not admitted: {failure:?}"))?;
    let conformance = std::fs::read(data.join(manifest.address(Role::Conformance).as_str()))?;
    let markers = std::fs::read(data.join(manifest.address(Role::Markers).as_str()))?;
    let admitted = manifest
        .admit(
            Some(&conformance),
            Some(markers.into_boxed_slice()),
            &LIMITS,
        )
        .map_err(|failure| format!("the committed artifact is not admitted: {failure:?}"))?;
    let corpus = std::fs::read_to_string(root.join("cldr").join(CORPUS_PATH))?;
    let registry = GapRegistry::run(&corpus, &admitted)
        .map_err(|failure| format!("the corpus gate does not run: {failure:?}"))?;
    let counts = registry.counts();
    println!(
        "{} cases: {} delegated, {} corrected, {} outside the domain, {} blocking",
        counts.cases(),
        counts.delegated,
        counts.corrected,
        counts.outside_domain,
        counts.blocking
    );
    let path = root.join("evidence").join("gap-registry.json");
    match args.as_slice() {
        [flag] if flag == "--check" => {
            if std::fs::read_to_string(&path)? != registry.text() {
                return Err(
                    "evidence/gap-registry.json is stale; regenerate it with --write".into(),
                );
            }
        }
        [flag] if flag == "--write" => std::fs::write(&path, registry.text())?,
        _ => return Err("usage: generate_gap_registry (--check | --write)".into()),
    }
    if !registry.admitted() {
        return Err("a blocking case refuses the adapter".into());
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
