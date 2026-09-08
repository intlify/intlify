// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Acquisition inputs for the eventual complete 026 Environment Observation.
//! This owner-local snapshot is not the 27-field common record or its admission.
//! Kernel and compiled-target views remain distinct from physical-host claims.
//! Missing OS/build/device/qualification/projection knowledge is not invented.

use serde::{Deserialize, Serialize};

use super::clock::MonotonicClock;
use super::descriptor::ClockObservation;
use super::observation::{Digest, Frame};
use super::quantity::Repetitions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum AcquisitionReason {
    UnsupportedPlatform,
    UnsupportedSystemIdentifier,
    UnsupportedArchitecture,
    UnsupportedKernelRelease,
    ParallelismQueryFailed,
    InvalidParallelism,
    QuantityOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
enum Acquired<T> {
    Observed { value: T },
    Unavailable { reason: AcquisitionReason },
}

fn observation<T>(value: Result<T, AcquisitionReason>) -> Acquired<T> {
    match value {
        Ok(value) => Acquired::Observed { value },
        Err(reason) => Acquired::Unavailable { reason },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum KernelFamily {
    Linux,
    Darwin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum TargetOs {
    Linux,
    #[serde(rename = "macos")]
    MacOs,
    Windows,
    Android,
    Ios,
    Freebsd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Architecture {
    X86,
    #[serde(rename = "x86_64")]
    X86_64,
    Arm,
    Aarch64,
    Riscv32,
    Riscv64,
    Wasm32,
    Wasm64,
}

fn kernel_family(value: &[u8]) -> Acquired<KernelFamily> {
    observation(match value {
        b"Linux" => Ok(KernelFamily::Linux),
        b"Darwin" => Ok(KernelFamily::Darwin),
        _ => Err(AcquisitionReason::UnsupportedSystemIdentifier),
    })
}

fn target_os(value: &str) -> Acquired<TargetOs> {
    observation(match value {
        "linux" => Ok(TargetOs::Linux),
        "macos" => Ok(TargetOs::MacOs),
        "windows" => Ok(TargetOs::Windows),
        "android" => Ok(TargetOs::Android),
        "ios" => Ok(TargetOs::Ios),
        "freebsd" => Ok(TargetOs::Freebsd),
        _ => Err(AcquisitionReason::UnsupportedSystemIdentifier),
    })
}

fn architecture(value: &[u8]) -> Acquired<Architecture> {
    observation(match value {
        b"x86" | b"i386" | b"i686" => Ok(Architecture::X86),
        b"x86_64" => Ok(Architecture::X86_64),
        b"arm" | b"armv7l" => Ok(Architecture::Arm),
        b"aarch64" | b"arm64" => Ok(Architecture::Aarch64),
        b"riscv32" => Ok(Architecture::Riscv32),
        b"riscv64" => Ok(Architecture::Riscv64),
        b"wasm32" => Ok(Architecture::Wasm32),
        b"wasm64" => Ok(Architecture::Wasm64),
        _ => Err(AcquisitionReason::UnsupportedArchitecture),
    })
}

/// Retain only an entire bounded numeric release. Never strip a private suffix
/// and claim the remaining prefix is the original release or OS/build identity.
fn kernel_release(value: &[u8]) -> Acquired<String> {
    let admitted = !value.is_empty()
        && value.len() <= 32
        && value.split(|byte| *byte == b'.').count() <= 4
        && value.split(|byte| *byte == b'.').all(|part| {
            !part.is_empty()
                && part.iter().all(u8::is_ascii_digit)
                && (part.len() == 1 || part[0] != b'0')
        });
    observation(if admitted {
        // The admission above proves ASCII; conversion never retains bad bytes.
        String::from_utf8(value.to_vec()).map_err(|_| AcquisitionReason::UnsupportedKernelRelease)
    } else {
        Err(AcquisitionReason::UnsupportedKernelRelease)
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LibraryTarget {
    os: Acquired<TargetOs>,
    architecture: Acquired<Architecture>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KernelView {
    provider: String,
    provider_revision: String,
    method: String,
    family: Acquired<KernelFamily>,
    machine: Acquired<Architecture>,
    release: Acquired<String>,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn acquire_kernel() -> Acquired<KernelView> {
    let uname = rustix::system::uname();
    // Do not format/debug Uname: that would include nodename and full version.
    // Read only the fields admitted by the controlled projections below.
    Acquired::Observed {
        value: KernelView {
            provider: "rustix".into(),
            provider_revision: "1.1.4".into(),
            method: "uname-controlled-kernel-view".into(),
            family: kernel_family(uname.sysname().to_bytes()),
            machine: architecture(uname.machine().to_bytes()),
            release: kernel_release(uname.release().to_bytes()),
        },
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn acquire_kernel() -> Acquired<KernelView> {
    Acquired::Unavailable {
        reason: AcquisitionReason::UnsupportedPlatform,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ParallelismMethod {
    RustStdAvailableParallelism,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ParallelismHint {
    method: ParallelismMethod,
    count: Repetitions,
}

fn parallelism_hint(value: Result<u64, AcquisitionReason>) -> Acquired<ParallelismHint> {
    observation(value.and_then(|value| {
        Repetitions::new(value)
            .map(|count| ParallelismHint {
                method: ParallelismMethod::RustStdAvailableParallelism,
                count,
            })
            .map_err(|_| AcquisitionReason::InvalidParallelism)
    }))
}

/// This path has no qualification/preflight admission, so it cannot construct
/// either controlled or qualified contexts from CI flags or caller strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RunnerContext {
    LocalUncontrolled,
}

/// A source of acquired facts, not the common environment-field inventory.
/// Additional required fields and admitted harness/projection references belong
/// to the enclosing complete record. No absent field here proves non-applicability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct EnvironmentInputs {
    codec: String,
    library_target: LibraryTarget,
    kernel_view: Acquired<KernelView>,
    available_parallelism_hint: Acquired<ParallelismHint>,
    runner: RunnerContext,
    clock: ClockObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnvironmentIssue {
    Encoding,
    ObservationMismatch,
}

/// No Deserialize or mutable access. Acquisition, not a submitted document,
/// defines the observation used to bind every operation in this capture context.
pub(super) struct ObservedEnvironment {
    document: EnvironmentInputs,
    checksum: Digest,
}

fn checksum(document: &EnvironmentInputs) -> Result<Digest, EnvironmentIssue> {
    let mut frame = Frame::new("environment-inputs");
    frame.json(&serde_json::to_value(document).map_err(|_| EnvironmentIssue::Encoding)?);
    Ok(frame.finish())
}

impl ObservedEnvironment {
    pub(super) fn acquire(clock: &MonotonicClock) -> Result<Self, EnvironmentIssue> {
        let parallelism = std::thread::available_parallelism()
            .map_err(|_| AcquisitionReason::ParallelismQueryFailed)
            .and_then(|value| {
                u64::try_from(value.get()).map_err(|_| AcquisitionReason::QuantityOverflow)
            });
        let document = EnvironmentInputs {
            codec: "intlify-config-environment-inputs/0".into(),
            library_target: LibraryTarget {
                os: target_os(std::env::consts::OS),
                architecture: architecture(std::env::consts::ARCH.as_bytes()),
            },
            kernel_view: acquire_kernel(),
            available_parallelism_hint: parallelism_hint(parallelism),
            runner: RunnerContext::LocalUncontrolled,
            clock: clock.description().into(),
        };
        let checksum = checksum(&document)?;
        Ok(Self { document, checksum })
    }

    pub(super) fn document(&self) -> &EnvironmentInputs {
        &self.document
    }

    pub(super) const fn checksum(&self) -> Digest {
        self.checksum
    }

    pub(super) fn validate(&self, submitted: &EnvironmentInputs) -> Vec<EnvironmentIssue> {
        if submitted == &self.document {
            Vec::new()
        } else {
            vec![EnvironmentIssue::ObservationMismatch]
        }
    }
}

#[cfg(test)]
mod tests;
