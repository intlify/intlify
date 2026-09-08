// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Build-time observations retained by the benchmark, never the resolver.
//! A source snapshot is not a compiler-input attestation, package signature,
//! common Build Identity, or 017 digest. Cargo inputs are not automatically the
//! effective compiler/linker configuration. Missing attestations stay explicit.

use serde::{Deserialize, Serialize};

use super::observation::{Digest, Frame};
use super::quantity::{Quantity, Repetitions};

const CODEC: &str = "intlify-config-build-observation/0";
const EMBEDDED: &str = include_str!(concat!(env!("OUT_DIR"), "/intlify-build-observation.json"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum AcquisitionReason {
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
    EffectiveInvocationNotAttested,
    ExecutedImageNotAttested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
enum Acquisition<T> {
    Observed { value: T },
    Unavailable { reason: AcquisitionReason },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Package {
    identity: String,
    revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceSnapshot {
    algorithm: String,
    framing: String,
    digest: Digest,
    files: Repetitions,
    bytes: Quantity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LockSnapshot {
    algorithm: String,
    representation: String,
    digest: Digest,
    bytes: Quantity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Compiler {
    identity: String,
    release: String,
    #[serde(deserialize_with = "Option::deserialize")]
    commit: Option<String>,
    llvm: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CargoInputs {
    profile: Acquisition<String>,
    optimization: Acquisition<String>,
    debug_setting: Acquisition<String>,
    target: Acquisition<String>,
    features: Vec<String>,
    additional_flags_present: bool,
    compiler_wrapper_present: bool,
    workspace_compiler_wrapper_present: bool,
}

/// These capabilities are not attested by a build-script observation. A free
/// string or open object cannot be submitted as an observed effective build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
enum Unattested {
    Unavailable { reason: AcquisitionReason },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Execution {
    debug_assertions: bool,
    pointer_width_bits: Quantity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BuildObservation {
    codec: String,
    package: Package,
    source: Acquisition<SourceSnapshot>,
    dependency_lock: Acquisition<LockSnapshot>,
    compiler: Acquisition<Compiler>,
    cargo_inputs: CargoInputs,
    effective_configuration: Unattested,
    executable: Unattested,
    execution: Execution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BuildIssue {
    MalformedEmbeddedObservation,
    CodecMismatch,
    PackageMismatch,
    FeatureMismatch,
    SourceMethodMismatch,
    LockMethodMismatch,
    UnexpectedAttestation,
    ObservationMismatch,
    Encoding,
}

/// No deserialization or caller-defined acquisition input. An external row must
/// be compared with this separately acquired observation of the linked library.
#[derive(Debug)]
pub(super) struct ObservedBuild {
    document: BuildObservation,
    checksum: Digest,
}

impl ObservedBuild {
    pub(super) fn acquire() -> Result<Self, BuildIssue> {
        let mut value: serde_json::Value =
            serde_json::from_str(EMBEDDED).map_err(|_| BuildIssue::MalformedEmbeddedObservation)?;
        let fields = value
            .as_object_mut()
            .ok_or(BuildIssue::MalformedEmbeddedObservation)?;
        // Execution facts come from the actual linked library, not build.rs's
        // host/optimization context. Do not overwrite an unexpected input field.
        if fields
            .insert(
                "execution".into(),
                serde_json::json!({
                    "debugAssertions": cfg!(debug_assertions),
                    "pointerWidthBits": usize::BITS.to_string(),
                }),
            )
            .is_some()
        {
            return Err(BuildIssue::MalformedEmbeddedObservation);
        }
        let document: BuildObservation =
            serde_json::from_value(value).map_err(|_| BuildIssue::MalformedEmbeddedObservation)?;
        document.check_build_binding()?;
        let mut frame = Frame::new("build-observation");
        frame.json(&serde_json::to_value(&document).map_err(|_| BuildIssue::Encoding)?);
        Ok(Self {
            document,
            checksum: frame.finish(),
        })
    }

    pub(super) fn document(&self) -> &BuildObservation {
        &self.document
    }
    pub(super) const fn checksum(&self) -> Digest {
        self.checksum
    }

    pub(super) fn validate(&self, submitted: &BuildObservation) -> Vec<BuildIssue> {
        if submitted == &self.document {
            Vec::new()
        } else {
            vec![BuildIssue::ObservationMismatch]
        }
    }
}

impl BuildObservation {
    fn check_build_binding(&self) -> Result<(), BuildIssue> {
        if self.codec != CODEC {
            return Err(BuildIssue::CodecMismatch);
        }
        if self.package.identity != env!("CARGO_PKG_NAME")
            || self.package.revision != env!("CARGO_PKG_VERSION")
        {
            return Err(BuildIssue::PackageMismatch);
        }
        let mut features = vec!["benchmark"];
        if cfg!(feature = "default") {
            features.push("default");
        }
        if self.cargo_inputs.features != features {
            return Err(BuildIssue::FeatureMismatch);
        }
        if let Acquisition::Observed { value } = &self.source {
            if value.algorithm != "blake3-256" || value.framing != "intlify-config-build-source/0" {
                return Err(BuildIssue::SourceMethodMismatch);
            }
        }
        if let Acquisition::Observed { value } = &self.dependency_lock {
            if value.algorithm != "blake3-256"
                || value.representation != "complete-lock-file-octets"
            {
                return Err(BuildIssue::LockMethodMismatch);
            }
        }
        if self.effective_configuration
            != (Unattested::Unavailable {
                reason: AcquisitionReason::EffectiveInvocationNotAttested,
            })
            || self.executable
                != (Unattested::Unavailable {
                    reason: AcquisitionReason::ExecutedImageNotAttested,
                })
        {
            return Err(BuildIssue::UnexpectedAttestation);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

// Exercise the same producer code with explicit fixture inputs. Tests never
// call emit(), inspect arbitrary environment values, or write into OUT_DIR.
#[cfg(test)]
#[path = "../../build/metadata.rs"]
mod producer;
