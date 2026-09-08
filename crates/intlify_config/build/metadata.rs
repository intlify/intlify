// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Build-script-only observation acquisition. No source paths, arbitrary flags,
//! environment values, command lines, or compiler stderr enter the output.
//! Source checksums describe this bounded input snapshot, not an attestation of
//! the complete compiler dependency graph or a 017 artifact/integrity encoding.

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

pub(super) const CODEC: &str = "intlify-config-build-observation/0";
const SOURCE_FRAMING: &str = "intlify-config-build-source/0";
const MAX_SOURCE_FILES: usize = 512;
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_COMPILER_OUTPUT: u64 = 16 * 1024;

/// Closed acquisition causes. Error objects/paths and rejected text are dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Failure {
    MissingInput,
    UnsupportedInput,
    Io,
    SourceLimitExceeded,
    UnsupportedSourceEntry,
    InvalidSourcePath,
    SourceChanged,
    CompilerInvocationFailed,
    CompilerOutputUnsupported,
    CompilerOutputLimit,
    CompilerWrappersPresent,
}

impl Failure {
    fn code(self) -> &'static str {
        match self {
            Self::MissingInput => "missing-input",
            Self::UnsupportedInput => "unsupported-input",
            Self::Io => "io",
            Self::SourceLimitExceeded => "source-limit-exceeded",
            Self::UnsupportedSourceEntry => "unsupported-source-entry",
            Self::InvalidSourcePath => "invalid-source-path",
            Self::SourceChanged => "source-changed",
            Self::CompilerInvocationFailed => "compiler-invocation-failed",
            Self::CompilerOutputUnsupported => "compiler-output-unsupported",
            Self::CompilerOutputLimit => "compiler-output-limit",
            Self::CompilerWrappersPresent => "compiler-wrappers-present",
        }
    }
}

fn observation(result: Result<Value, Failure>) -> Value {
    match result {
        Ok(value) => json!({"state": "observed", "value": value}),
        Err(reason) => json!({"state": "unavailable", "reason": reason.code()}),
    }
}

/// Checked values are Cargo inputs, not a claim about the effective rustc/linker
/// invocation. Additional flags and wrappers can change that invocation.
pub(super) fn controlled(value: Option<&OsStr>, allowed: &[&str]) -> Result<Value, Failure> {
    let text = value
        .ok_or(Failure::MissingInput)?
        .to_str()
        .ok_or(Failure::UnsupportedInput)?;
    if allowed.contains(&text) {
        Ok(json!(text))
    } else {
        Err(Failure::UnsupportedInput)
    }
}

fn input(name: &str, allowed: &[&str]) -> Value {
    observation(controlled(std::env::var_os(name).as_deref(), allowed))
}

fn present(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn frame(hasher: &mut blake3::Hasher, bytes: &[u8]) -> Result<(), Failure> {
    let length = u64::try_from(bytes.len()).map_err(|_| Failure::SourceLimitExceeded)?;
    hasher.update(&length.to_le_bytes());
    hasher.update(bytes);
    Ok(())
}

pub(super) fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, Failure> {
    let before = fs::symlink_metadata(path).map_err(|_| Failure::Io)?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(Failure::UnsupportedSourceEntry);
    }
    if before.len() > maximum {
        return Err(Failure::SourceLimitExceeded);
    }
    let limit = maximum.checked_add(1).ok_or(Failure::SourceLimitExceeded)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| Failure::Io)?
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::Io)?;
    let length = u64::try_from(bytes.len()).map_err(|_| Failure::SourceLimitExceeded)?;
    if length > maximum {
        return Err(Failure::SourceLimitExceeded);
    }
    let after = fs::symlink_metadata(path).map_err(|_| Failure::Io)?;
    if !after.is_file()
        || after.file_type().is_symlink()
        || before.len() != length
        || after.len() != length
        || before.modified().map_err(|_| Failure::Io)?
            != after.modified().map_err(|_| Failure::Io)?
    {
        return Err(Failure::SourceChanged);
    }
    Ok(bytes)
}

fn gather(
    directory: &Path,
    paths: &mut Vec<PathBuf>,
    depth: usize,
    visited: &mut usize,
) -> Result<(), Failure> {
    if depth > 64 {
        return Err(Failure::SourceLimitExceeded);
    }
    let metadata = fs::symlink_metadata(directory).map_err(|_| Failure::Io)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(Failure::UnsupportedSourceEntry);
    }
    for entry in fs::read_dir(directory).map_err(|_| Failure::Io)? {
        *visited = visited.checked_add(1).ok_or(Failure::SourceLimitExceeded)?;
        if *visited > 2048 {
            return Err(Failure::SourceLimitExceeded);
        }
        let entry = entry.map_err(|_| Failure::Io)?;
        let kind = entry.file_type().map_err(|_| Failure::Io)?;
        if kind.is_dir() {
            gather(&entry.path(), paths, depth + 1, visited)?;
        } else if kind.is_file() {
            if paths.len() == MAX_SOURCE_FILES {
                return Err(Failure::SourceLimitExceeded);
            }
            paths.push(entry.path());
        } else {
            return Err(Failure::UnsupportedSourceEntry);
        }
    }
    Ok(())
}

/// Only the owning crate's source/build trees and declared manifest inputs are
/// scanned at build time. Fixture selection at measurement time is still closed.
#[derive(Debug, PartialEq)]
pub(super) struct SourceInputs {
    pub(super) source: Value,
    pub(super) lock: Value,
}

pub(super) fn source_snapshot(
    crate_root: &Path,
    workspace: &Path,
) -> Result<SourceInputs, Failure> {
    let mut paths = vec![
        crate_root.join("Cargo.toml"),
        crate_root.join("README.md"),
        crate_root.join("build.rs"),
    ];
    let mut visited = 0;
    gather(&crate_root.join("src"), &mut paths, 0, &mut visited)?;
    gather(&crate_root.join("build"), &mut paths, 0, &mut visited)?;
    let mut entries = Vec::with_capacity(paths.len() + 2);
    for path in paths {
        let relative = path
            .strip_prefix(crate_root)
            .map_err(|_| Failure::InvalidSourcePath)?;
        let name = relative
            .to_str()
            .ok_or(Failure::InvalidSourcePath)?
            .replace('\\', "/");
        // Windows separators are normalized. Literal backslashes on Unix would
        // alias a separator, so reject that unsupported source name instead.
        #[cfg(unix)]
        if relative.as_os_str().as_encoded_bytes().contains(&b'\\') {
            return Err(Failure::InvalidSourcePath);
        }
        entries.push((format!("crate/{name}"), path));
    }
    entries.push(("workspace/Cargo.toml".into(), workspace.join("Cargo.toml")));
    entries.push(("workspace/Cargo.lock".into(), workspace.join("Cargo.lock")));
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, SOURCE_FRAMING.as_bytes())?;
    let mut total = 0_u64;
    let mut lock = None;
    for (name, path) in &entries {
        let bytes = read_bounded(
            path,
            MAX_SOURCE_BYTES
                .checked_sub(total)
                .ok_or(Failure::SourceLimitExceeded)?,
        )?;
        total = total
            .checked_add(u64::try_from(bytes.len()).map_err(|_| Failure::SourceLimitExceeded)?)
            .ok_or(Failure::SourceLimitExceeded)?;
        frame(&mut hasher, name.as_bytes())?;
        frame(&mut hasher, &bytes)?;
        if name == "workspace/Cargo.lock" {
            lock = Some(lock_observation(&bytes));
        }
    }
    let source = json!({"algorithm": "blake3-256", "framing": SOURCE_FRAMING,
        "digest": hasher.finalize().to_hex().to_string(),
        "files": entries.len().to_string(), "bytes": total.to_string()});
    Ok(SourceInputs {
        source,
        lock: lock.ok_or(Failure::MissingInput)?,
    })
}

pub(super) fn lock_snapshot(path: &Path) -> Result<Value, Failure> {
    let bytes = read_bounded(path, MAX_SOURCE_BYTES)?;
    Ok(lock_observation(&bytes))
}

fn lock_observation(bytes: &[u8]) -> Value {
    json!({"algorithm": "blake3-256", "representation": "complete-lock-file-octets",
        "digest": blake3::hash(bytes).to_hex().to_string(), "bytes": bytes.len().to_string()})
}

fn numeric_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn compiler_release(value: &str) -> bool {
    let (base, suffix) = value.split_once('-').unwrap_or((value, ""));
    numeric_version(base) && ["", "nightly", "beta", "dev"].contains(&suffix)
}

pub(super) fn parse_compiler(bytes: &[u8]) -> Result<Value, Failure> {
    if bytes.len() > usize::try_from(MAX_COMPILER_OUTPUT).expect("small fixed bound") {
        return Err(Failure::CompilerOutputLimit);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Failure::CompilerOutputUnsupported)?;
    if !text
        .lines()
        .next()
        .is_some_and(|line| line.starts_with("rustc "))
    {
        return Err(Failure::CompilerOutputUnsupported);
    }
    let field = |name: &str| -> Result<&str, Failure> {
        let mut matches = text.lines().filter_map(|line| line.strip_prefix(name));
        let value = matches.next().ok_or(Failure::CompilerOutputUnsupported)?;
        if matches.next().is_some() {
            return Err(Failure::CompilerOutputUnsupported);
        }
        Ok(value)
    };
    let release = field("release: ")?;
    let commit = field("commit-hash: ")?;
    let llvm = field("LLVM version: ")?;
    if !compiler_release(release)
        || !numeric_version(llvm)
        || !(commit == "unknown"
            || (commit.len() == 40
                && commit
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))))
    {
        return Err(Failure::CompilerOutputUnsupported);
    }
    Ok(json!({"identity": "rustc", "release": release,
        "commit": (commit != "unknown").then_some(commit), "llvm": llvm}))
}

fn compiler_snapshot(wrappers: bool) -> Result<Value, Failure> {
    if wrappers {
        return Err(Failure::CompilerWrappersPresent);
    }
    let compiler = std::env::var_os("RUSTC").ok_or(Failure::MissingInput)?;
    let mut child = Command::new(compiler)
        .args(["--version", "--verbose"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Failure::CompilerInvocationFailed)?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(Failure::CompilerInvocationFailed);
    };
    let mut bytes = Vec::new();
    let read = stdout.take(MAX_COMPILER_OUTPUT + 1).read_to_end(&mut bytes);
    if read.is_err()
        || bytes.len() > usize::try_from(MAX_COMPILER_OUTPUT).expect("small fixed bound")
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(if read.is_err() {
            Failure::CompilerInvocationFailed
        } else {
            Failure::CompilerOutputLimit
        });
    }
    if !child
        .wait()
        .map_err(|_| Failure::CompilerInvocationFailed)?
        .success()
    {
        return Err(Failure::CompilerInvocationFailed);
    }
    parse_compiler(&bytes)
}

pub(super) fn emit() {
    for path in [
        "src",
        "build",
        "Cargo.toml",
        "README.md",
        "../../Cargo.toml",
        "../../Cargo.lock",
    ] {
        println!("cargo::rerun-if-changed={path}");
    }
    for name in [
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "CARGO_ENCODED_RUSTFLAGS",
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "TARGET",
    ] {
        println!("cargo::rerun-if-env-changed={name}");
    }
    let root = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo supplies the crate directory"),
    );
    let workspace = root
        .parent()
        .and_then(Path::parent)
        .expect("the internal crate is under workspace/crates");
    let wrapper = present("RUSTC_WRAPPER");
    let workspace_wrapper = present("RUSTC_WORKSPACE_WRAPPER");
    let mut features = Vec::new();
    if present("CARGO_FEATURE_BENCHMARK") {
        features.push("benchmark");
    }
    if present("CARGO_FEATURE_DEFAULT") {
        features.push("default");
    }
    // A successful source/lock pair refers to the same lock bytes. Do not read
    // the file twice and silently bind two different versions in one stamp.
    let (source, dependency_lock) = match source_snapshot(&root, workspace) {
        Ok(inputs) => (observation(Ok(inputs.source)), observation(Ok(inputs.lock))),
        Err(reason) => (
            observation(Err(reason)),
            observation(lock_snapshot(&workspace.join("Cargo.lock"))),
        ),
    };
    let value = json!({
        "codec": CODEC,
        "package": {"identity": env!("CARGO_PKG_NAME"), "revision": env!("CARGO_PKG_VERSION")},
        "source": source,
        "dependencyLock": dependency_lock,
        "compiler": observation(compiler_snapshot(wrapper || workspace_wrapper)),
        "cargoInputs": {
            "profile": input("PROFILE", &["debug", "release"]),
            "optimization": input("OPT_LEVEL", &["0", "1", "2", "3", "s", "z"]),
            "debugSetting": input("DEBUG", &["true", "false", "0", "1", "2", "none", "limited", "full", "line-tables-only", "line-directives-only"]),
            "target": input("TARGET", &["aarch64-apple-darwin", "x86_64-apple-darwin", "aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl", "x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl", "x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc", "x86_64-pc-windows-gnu", "wasm32-unknown-unknown"]),
            "features": features,
            "additionalFlagsPresent": present("CARGO_ENCODED_RUSTFLAGS"),
            "compilerWrapperPresent": wrapper,
            "workspaceCompilerWrapperPresent": workspace_wrapper
        },
        "effectiveConfiguration": {"state": "unavailable", "reason": "effective-invocation-not-attested"},
        "executable": {"state": "unavailable", "reason": "executed-image-not-attested"}
    });
    let out = PathBuf::from(
        std::env::var_os("OUT_DIR").expect("Cargo supplies a build output directory"),
    );
    let bytes = serde_bytes(&value);
    fs::write(out.join("intlify-build-observation.json"), bytes)
        .expect("write benchmark build observation");
}

fn serde_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("finite build metadata serializes")
}

#[cfg(test)]
#[path = "metadata_tests.rs"]
mod tests;
