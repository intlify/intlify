// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The toolchain this crate was built with, as its build script observed it.
//!
//! The compiler is asked once, at build time, and only its controlled fields
//! are retained: release, commit, and LLVM version. A compiler behind a
//! wrapper is reported as such rather than described as the inner compiler,
//! and the target triple is named only when it is one of a closed set.
//!
//! An owner built in the same Cargo invocation was built by the same
//! compiler, which is why one observation here can serve every owner.

use serde::{Deserialize, Serialize};

use super::Acquired;
use crate::environment::Toolchain;
use crate::identity::{IdentityFailure, Token, VersionedIdentity};

const EMBEDDED: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/intlify-measurement-toolchain.json"
));

/// The controlled fields of the compiler's own report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Compiler {
    pub identity: String,
    pub release: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub commit: Option<String>,
    pub llvm: String,
}

impl Compiler {
    /// Present it as 026's toolchain field.
    pub fn toolchain(&self) -> Result<Toolchain, IdentityFailure> {
        Ok(Toolchain {
            compiler: VersionedIdentity::new(&self.identity, &self.release)?,
            commit: self.commit.as_deref().map(Token::new).transpose()?,
            backend: VersionedIdentity::new("llvm", &self.llvm)?,
        })
    }
}

/// The compiler and target this crate's build script observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolchainView {
    pub compiler: Acquired<Compiler>,
    pub target: Acquired<String>,
}

impl ToolchainView {
    /// Read what the build script observed.
    ///
    /// # Panics
    ///
    /// Panics when the embedded observation does not read back. The build
    /// script writes it from closed forms, so that is a defect, not an input.
    #[must_use]
    pub fn acquire() -> Self {
        serde_json::from_str(EMBEDDED).expect("the build script writes a readable observation")
    }

    /// Present the compiler as 026's toolchain field, when it was observed.
    pub fn toolchain(&self) -> Result<Option<Toolchain>, IdentityFailure> {
        match &self.compiler {
            Acquired::Observed { value } => value.toolchain().map(Some),
            Acquired::Unavailable { .. } => Ok(None),
        }
    }

    /// Present the target triple as 026's token, when it was observed.
    pub fn target_triple(&self) -> Result<Option<Token>, IdentityFailure> {
        match &self.target {
            Acquired::Observed { value } => Token::new(value).map(Some),
            Acquired::Unavailable { .. } => Ok(None),
        }
    }
}

// The build script's own code, exercised with explicit inputs. These tests
// never call its emit, read the environment, or write into OUT_DIR.
#[cfg(test)]
#[path = "../../build/toolchain.rs"]
#[allow(dead_code, reason = "only the build script emits")]
mod producer;

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use serde_json::json;

    use super::producer::{
        compiler_release, compiler_snapshot, document, parse_compiler, target, Failure,
        MAX_COMPILER_OUTPUT,
    };
    use super::*;
    use crate::acquisition::AcquisitionReason;

    const REPORT: &[u8] = b"rustc 1.95.0 (ignored)\nbinary: rustc\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\ncommit-date: 2026-01-01\nhost: private-host-name\nrelease: 1.95.0\nLLVM version: 22.1.0\nprivate: /Users/private/repository\n";

    fn read(
        compiler: Result<producer::Compiler, Failure>,
        target: Result<&str, Failure>,
    ) -> ToolchainView {
        serde_json::from_str(&document(compiler, target)).unwrap()
    }

    #[test]
    fn compiler_projection_retains_controlled_fields_without_host_or_path_text() {
        let view = read(parse_compiler(REPORT), Ok("aarch64-apple-darwin"));
        assert_eq!(
            serde_json::to_value(&view).unwrap(),
            json!({
                "compiler": {"state": "observed", "value": {
                    "identity": "rustc",
                    "release": "1.95.0",
                    "commit": "0123456789abcdef0123456789abcdef01234567",
                    "llvm": "22.1.0"
                }},
                "target": {"state": "observed", "value": "aarch64-apple-darwin"}
            })
        );
        let text = serde_json::to_string(&view).unwrap();
        assert!(!text.contains("private"));
        assert!(!text.contains("Users"));
        for release in ["1.95.0-nightly", "1.95.0-beta", "1.95.0-dev"] {
            assert!(compiler_release(release));
        }
        let unknown =
            b"rustc 1.95.0\ncommit-hash: unknown\nrelease: 1.95.0\nLLVM version: 22.1.0\n";
        let view = read(parse_compiler(unknown), Ok("x86_64-unknown-linux-gnu"));
        assert_eq!(
            serde_json::to_value(&view.compiler).unwrap()["value"]["commit"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn an_unsupported_report_is_named_unsupported_and_never_echoed() {
        let duplicate = [REPORT, b"release: 1.95.0\n"].concat();
        assert_eq!(
            parse_compiler(&duplicate),
            Err(Failure::CompilerOutputUnsupported)
        );
        assert_eq!(
            parse_compiler(&vec![
                b'a';
                usize::try_from(MAX_COMPILER_OUTPUT).unwrap() + 1
            ]),
            Err(Failure::CompilerOutputLimit)
        );
        for bad in [
            b"bad compiler".as_slice(),
            b"rustc 1.95.0\nrelease: private-secret\ncommit-hash: unknown\nLLVM version: 22.0.0\n",
            b"rustc 1.95.0\nrelease: 1.95.0\ncommit-hash: ABCDEF\nLLVM version: 22.0.0\n",
            &[0xff],
        ] {
            let view = read(parse_compiler(bad), Ok("wasm32-unknown-unknown"));
            assert_eq!(
                view.compiler,
                Acquired::Unavailable {
                    reason: AcquisitionReason::CompilerOutputUnsupported
                }
            );
        }
        // A wrapper is not silently described as the inner unwrapped compiler,
        // and an absent compiler is missing rather than failed.
        assert_eq!(
            compiler_snapshot(Some(OsStr::new("rustc")), true),
            Err(Failure::CompilerWrappersPresent)
        );
        assert_eq!(compiler_snapshot(None, false), Err(Failure::MissingInput));
    }

    #[test]
    fn a_target_is_named_only_from_its_closed_set() {
        assert_eq!(
            target(Some(OsStr::new("x86_64-unknown-linux-gnu"))),
            Ok("x86_64-unknown-linux-gnu")
        );
        assert_eq!(target(None), Err(Failure::MissingInput));
        let view = read(
            Err(Failure::MissingInput),
            target(Some(OsStr::new("/private/custom-target.json"))),
        );
        assert_eq!(
            serde_json::to_value(&view.target).unwrap(),
            json!({"state": "unavailable", "reason": "unsupported-input"})
        );
        assert!(!serde_json::to_string(&view).unwrap().contains("private"));
    }

    #[test]
    fn every_failure_reads_back_as_the_reason_it_names() {
        for (failure, reason) in [
            (Failure::MissingInput, AcquisitionReason::MissingInput),
            (
                Failure::UnsupportedInput,
                AcquisitionReason::UnsupportedInput,
            ),
            (
                Failure::CompilerInvocationFailed,
                AcquisitionReason::CompilerInvocationFailed,
            ),
            (
                Failure::CompilerOutputUnsupported,
                AcquisitionReason::CompilerOutputUnsupported,
            ),
            (
                Failure::CompilerOutputLimit,
                AcquisitionReason::CompilerOutputLimit,
            ),
            (
                Failure::CompilerWrappersPresent,
                AcquisitionReason::CompilerWrappersPresent,
            ),
        ] {
            let view = read(Err(failure), Err(failure));
            assert_eq!(view.compiler, Acquired::Unavailable { reason });
            assert_eq!(view.target, Acquired::Unavailable { reason });
        }
    }

    #[test]
    fn the_embedded_observation_reads_back_and_projects() {
        let view = ToolchainView::acquire();
        // Wrappers or an unsupported compiler stay explicit; they do not force
        // this test to claim an unwrapped rustc build.
        match &view.compiler {
            Acquired::Observed { value } => {
                assert_eq!(value.identity, "rustc");
                let toolchain = view.toolchain().unwrap().unwrap();
                assert_eq!(toolchain.compiler.identity().as_str(), "rustc");
                assert_eq!(toolchain.backend.identity().as_str(), "llvm");
            }
            Acquired::Unavailable { .. } => assert_eq!(view.toolchain(), Ok(None)),
        }
        match &view.target {
            Acquired::Observed { value } => {
                assert_eq!(view.target_triple().unwrap().unwrap().as_str(), value);
            }
            Acquired::Unavailable { .. } => assert_eq!(view.target_triple(), Ok(None)),
        }
    }
}
