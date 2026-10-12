// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the reader's fixture artifacts, or check that the committed ones are
//! current.
//!
//! The artifacts realize the small dataset of `fixtures/dataset/vectors.json`:
//! its conformance payload as canonical JSON text, its marker payloads as a
//! blob in each index layout, and three sealed manifests. Two differ only in
//! the blob they point at; the third only in where its CLDR sources were
//! read from. The specification is the real one; the dataset identity names a
//! fixture, since none of this is the dataset revision "0" admits.

use std::path::PathBuf;
use std::process::ExitCode;

use intlify_locale_icu4x::generate::{payload_blob, widen};
use intlify_locale_icu4x::{
    FileDigest, LocaleCanonicalizationDataArtifact, LocaleCanonicalizationDataset,
    LocaleConformanceData, LogicalPayload, Marker,
};
use intlify_shared_json::text;
use serde_json::{json, Value};

const DATASET_VECTORS: &str = include_str!("../fixtures/dataset/vectors.json");
const DIRECTORY: &str = "fixtures/artifact";

/// Where the fixture's CLDR sources were read from, relative to the CLDR
/// distribution root, and their file digests in CLDR 48.2.0.
const CLDR_SOURCES: [(&str, &str); 2] = [
    (
        "common/bcp47/calendar.xml",
        "sha256:fb24616f77fac1ab3246f869b69efc8a4b86d2437a947f3f6a53843ac0b60821",
    ),
    (
        "common/validity/region.xml",
        "sha256:e751e0eedd46b52c38f3cdb72b0fab61ac8b48e052e8b28ba74b6ac26c4c8cb1",
    ),
];

/// The published checksum of `icu_locale_data` 2.2.0, as Cargo.lock records it.
const MARKER_DATA_CHECKSUM: &str =
    "sha256:d5fdcc9ac77c6d74ff5cf6e65ef3181d6af32003b16fce3a77fb451d2f695993";

fn marker(name: &Value) -> Result<Marker, String> {
    Marker::ALL
        .into_iter()
        .find(|marker| name == marker.name())
        .ok_or_else(|| format!("{name} is not a marker"))
}

/// The payload entry of `bytes` at `address` under `format`.
fn payload(role: &str, format: (&str, &str), address: &str, bytes: &[u8]) -> Value {
    json!({
        "role": role,
        "format": {"identity": format.0, "revision": format.1},
        "address": address,
        "byteLength": bytes.len().to_string(),
        "digest": FileDigest::of(bytes).as_str(),
    })
}

/// One sealed manifest, written as canonical JSON text.
fn manifest(
    semantic_digest: &str,
    sources_under: &str,
    conformance: &[u8],
    (markers_address, markers): (&str, &[u8]),
) -> Result<Vec<u8>, String> {
    let sources: Vec<Value> = CLDR_SOURCES
        .iter()
        .map(|(address, digest)| json!({"address": format!("{sources_under}{address}"), "digest": digest}))
        .collect();
    let value = json!({
        "kind": "locale-canonicalization-data",
        "schemaRevision": "0",
        "specification": {"identity": "intlify-locale-canonicalization", "revision": "0"},
        "dataset": {
            "identity": "intlify-locale-canonicalization-fixture",
            "schema": {"identity": "intlify-locale-canonicalization-dataset", "revision": "0"},
            "semanticDigest": semantic_digest,
        },
        "generation": {
            "markerData": {
                "package": "icu_locale_data",
                "version": "2.2.0",
                "checksum": MARKER_DATA_CHECKSUM,
            },
            "cldr": {"revision": "48.2.0", "sources": sources},
            "markers": Marker::ALL.map(Marker::name),
        },
        "payloads": [
            payload(
                "conformance",
                intlify_locale_icu4x::CONFORMANCE_FORMAT,
                "conformance.json",
                conformance,
            ),
            payload(
                "markers",
                intlify_locale_icu4x::MARKERS_FORMAT,
                markers_address,
                markers,
            ),
        ],
        "integrityDigest": format!("sha256:{}", "0".repeat(64)),
    });
    let sealed = serde_json::from_value::<LocaleCanonicalizationDataArtifact>(value)
        .map_err(|error| error.to_string())?
        .seal()
        .map_err(|failure| format!("{failure:?}"))?;
    let value = serde_json::to_value(sealed).map_err(|error| error.to_string())?;
    text::write(&value)
        .map(String::into_bytes)
        .map_err(|failure| format!("{failure:?}"))
}

/// Every fixture file, by name, with its exact bytes.
fn files() -> Result<Vec<(&'static str, Vec<u8>)>, String> {
    let vectors: Value =
        serde_json::from_str(DATASET_VECTORS).map_err(|error| error.to_string())?;
    let payloads = vectors["markers"]
        .as_array()
        .ok_or("the vectors have no markers")?
        .iter()
        .map(|entry| {
            LogicalPayload::new(marker(&entry["marker"])?, entry["payload"].clone())
                .map_err(|failure| format!("{failure:?}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let conformance: LocaleConformanceData = serde_json::from_value(vectors["conformance"].clone())
        .map_err(|error| error.to_string())?;
    let conformance_bytes = conformance.to_text().into_bytes();

    let digest = LocaleCanonicalizationDataset::new(payloads.clone(), conformance)
        .map_err(|failure| format!("{failure:?}"))?
        .semantic_digest()
        .map_err(|failure| format!("{failure:?}"))?;
    if vectors["semanticDigest"] != digest.as_str() {
        return Err("the dataset vectors are stale".into());
    }

    let narrow = payload_blob(&payloads).map_err(|failure| format!("{failure:?}"))?;
    let wide = widen(&narrow).map_err(|failure| format!("{failure:?}"))?;
    let digest = digest.as_str();
    Ok(vec![
        ("conformance.json", conformance_bytes.clone()),
        ("markers-narrow.postcard", narrow.clone()),
        ("markers-wide.postcard", wide.clone()),
        (
            "manifest-narrow.json",
            manifest(
                digest,
                "",
                &conformance_bytes,
                ("markers-narrow.postcard", &narrow),
            )?,
        ),
        (
            "manifest-wide.json",
            manifest(
                digest,
                "",
                &conformance_bytes,
                ("markers-wide.postcard", &wide),
            )?,
        ),
        (
            "manifest-vendored.json",
            manifest(
                digest,
                "vendor/cldr/",
                &conformance_bytes,
                ("markers-narrow.postcard", &narrow),
            )?,
        ),
    ])
}

fn main() -> ExitCode {
    let write = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [flag] if flag == "--write" => true,
        [flag] if flag == "--check" => false,
        _ => {
            eprintln!("usage: generate_artifact_fixtures (--check | --write)");
            return ExitCode::FAILURE;
        }
    };
    let files = match files() {
        Ok(files) => files,
        Err(error) => {
            eprintln!("the fixtures could not be built: {error}");
            return ExitCode::FAILURE;
        }
    };
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DIRECTORY);
    if write {
        if let Err(error) = std::fs::create_dir_all(&directory) {
            eprintln!("could not create {}: {error}", directory.display());
            return ExitCode::FAILURE;
        }
    }
    let mut failed = false;
    for (name, bytes) in files {
        let path = directory.join(name);
        if write {
            if let Err(error) = std::fs::write(&path, bytes) {
                eprintln!("could not write {}: {error}", path.display());
                failed = true;
            }
        } else if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
            // The files are exact bytes: their digests are in the manifests.
            eprintln!("{} is stale; rerun with --write", path.display());
            failed = true;
        }
    }
    if failed {
        return ExitCode::FAILURE;
    }
    println!(
        "{} {}",
        directory.display(),
        if write { "written" } else { "is fresh" }
    );
    ExitCode::SUCCESS
}
