// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const REVISION_DOMAIN: &[u8] = b"dev.intlify/js-reference/build-revision\0";

fn main() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo supplies CARGO_MANIFEST_DIR"),
    );
    let workspace_dir = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("the producer crate is nested below the workspace root");
    let contract_dir = workspace_dir.join("crates/intlify_contract");
    let producer_source_dir = manifest_dir.join("src");
    let contract_source_dir = contract_dir.join("src");

    let mut inputs = vec![
        workspace_dir.join("Cargo.lock"),
        workspace_dir.join("Cargo.toml"),
        manifest_dir.join("Cargo.toml"),
        manifest_dir.join("build.rs"),
        contract_dir.join("Cargo.toml"),
    ];
    for directory in [&producer_source_dir, &contract_source_dir] {
        println!("cargo:rerun-if-changed={}", directory.display());
        collect_files(directory, &mut inputs);
    }
    inputs.sort();

    let mut hasher = blake3::Hasher::new();
    hasher.update(REVISION_DOMAIN);
    for path in inputs {
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path
            .strip_prefix(workspace_dir)
            .expect("every revision input is workspace-relative");
        let relative = relative
            .components()
            .map(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .expect("workspace source paths are valid UTF-8")
            })
            .collect::<Vec<_>>()
            .join("/");
        let bytes = fs::read(&path).expect("revision inputs remain readable during the build");
        hash_field(&mut hasher, relative.as_bytes());
        hash_field(&mut hasher, &bytes);
    }

    let package_version = env::var("CARGO_PKG_VERSION").expect("Cargo supplies CARGO_PKG_VERSION");
    let revision = format!("{package_version}+src.{}", hasher.finalize().to_hex());
    println!("cargo:rustc-env=INTLIFY_JS_PRODUCER_REVISION={revision}");
}

fn collect_files(directory: &Path, output: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .expect("producer source directory remains readable")
        .map(|entry| {
            entry
                .expect("producer source entry remains readable")
                .path()
        })
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_files(&path, output);
        } else {
            output.push(path);
        }
    }
}

fn hash_field(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(
        &u64::try_from(value.len())
            .expect("a Rust slice length fits into u64")
            .to_be_bytes(),
    );
    hasher.update(value);
}
