// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Build-script-only toolchain acquisition.
//!
//! The compiler that builds this crate is the compiler that builds every
//! owner in the same Cargo invocation, so it is read once here rather than by
//! each owner. Only controlled fields leave this module: no path, host name,
//! command line, flag, or compiler stderr enters the output, and a value
//! outside its closed form is reported as unsupported rather than echoed.
//!
//! The same code is compiled into the library's tests with explicit inputs.
//! Those tests never call [`emit`], read the environment, or write to
//! `OUT_DIR`.

use std::ffi::OsStr;
use std::io::Read;
use std::process::{Command, Stdio};

pub(crate) const MAX_COMPILER_OUTPUT: u64 = 16 * 1024;

/// The target triples this acquisition names. Any other is unsupported.
const TARGETS: &[&str] = &[
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
    "x86_64-pc-windows-gnu",
    "wasm32-unknown-unknown",
];

/// Closed acquisition causes. Error objects and rejected text are dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    MissingInput,
    UnsupportedInput,
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
            Self::CompilerInvocationFailed => "compiler-invocation-failed",
            Self::CompilerOutputUnsupported => "compiler-output-unsupported",
            Self::CompilerOutputLimit => "compiler-output-limit",
            Self::CompilerWrappersPresent => "compiler-wrappers-present",
        }
    }
}

/// The controlled fields of one `rustc --version --verbose` report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Compiler {
    pub(crate) release: String,
    pub(crate) commit: Option<String>,
    pub(crate) llvm: String,
}

fn numeric_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

pub(crate) fn compiler_release(value: &str) -> bool {
    let (base, suffix) = value.split_once('-').unwrap_or((value, ""));
    numeric_version(base) && ["", "nightly", "beta", "dev"].contains(&suffix)
}

/// Read the controlled fields of a compiler report, or say why not.
pub(crate) fn parse_compiler(bytes: &[u8]) -> Result<Compiler, Failure> {
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
    Ok(Compiler {
        release: release.into(),
        commit: (commit != "unknown").then(|| commit.into()),
        llvm: llvm.into(),
    })
}

/// Ask the compiler Cargo supplied, unless a wrapper stands in front of it.
///
/// A wrapper is not silently described as the inner unwrapped compiler.
pub(crate) fn compiler_snapshot(
    compiler: Option<&OsStr>,
    wrappers: bool,
) -> Result<Compiler, Failure> {
    if wrappers {
        return Err(Failure::CompilerWrappersPresent);
    }
    let compiler = compiler.ok_or(Failure::MissingInput)?;
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

/// Name the target only when it is one this acquisition knows.
pub(crate) fn target(value: Option<&OsStr>) -> Result<&'static str, Failure> {
    let text = value
        .ok_or(Failure::MissingInput)?
        .to_str()
        .ok_or(Failure::UnsupportedInput)?;
    TARGETS
        .iter()
        .find(|known| **known == text)
        .copied()
        .ok_or(Failure::UnsupportedInput)
}

// Every value written below was admitted from a closed form of ASCII digits,
// lowercase hexadecimal, dots, hyphens, and underscores, so none needs escaping.
fn string(value: &str) -> String {
    debug_assert!(value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')));
    format!("\"{value}\"")
}

fn observation(value: Result<String, Failure>) -> String {
    match value {
        Ok(value) => format!("{{\"state\":\"observed\",\"value\":{value}}}"),
        Err(reason) => format!(
            "{{\"state\":\"unavailable\",\"reason\":{}}}",
            string(reason.code())
        ),
    }
}

/// Write the toolchain observation as the library reads it.
pub(crate) fn document(
    compiler: Result<Compiler, Failure>,
    target: Result<&str, Failure>,
) -> String {
    let compiler = compiler.map(|compiler| {
        format!(
            "{{\"identity\":\"rustc\",\"release\":{},\"commit\":{},\"llvm\":{}}}",
            string(&compiler.release),
            compiler
                .commit
                .as_deref()
                .map_or_else(|| "null".into(), string),
            string(&compiler.llvm)
        )
    });
    format!(
        "{{\"compiler\":{},\"target\":{}}}",
        observation(compiler),
        observation(target.map(string))
    )
}

fn present(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

/// Acquire the toolchain Cargo is building with and write it to `OUT_DIR`.
pub(crate) fn emit() {
    for name in [
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "TARGET",
    ] {
        println!("cargo::rerun-if-env-changed={name}");
    }
    let wrappers = present("RUSTC_WRAPPER") || present("RUSTC_WORKSPACE_WRAPPER");
    let document = document(
        compiler_snapshot(std::env::var_os("RUSTC").as_deref(), wrappers),
        target(std::env::var_os("TARGET").as_deref()),
    );
    let out = std::path::PathBuf::from(
        std::env::var_os("OUT_DIR").expect("Cargo supplies a build output directory"),
    );
    std::fs::write(out.join("intlify-measurement-toolchain.json"), document)
        .expect("write the toolchain observation");
}
