// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What the host the run executes on reports about itself.
//!
//! Three views, kept apart because they answer different questions: the
//! target this library was compiled for, the kernel it is running on, and the
//! parallelism the standard library reports as available. None of them is a
//! physical-host claim, and a kernel release is not an operating system or
//! build version.
//!
//! Every identifier is read into a closed vocabulary. An unknown value is
//! reported as unsupported rather than echoed, so a host name or a private
//! suffix never reaches a record.

use intlify_shared_json::quantity::Repetitions;
use serde::{Deserialize, Serialize};

use super::{Acquired, AcquisitionReason};
use crate::environment::{Architecture as CommonArchitecture, SystemFamily};
use crate::identity::VersionedIdentity;

fn observation<T>(value: Result<T, AcquisitionReason>) -> Acquired<T> {
    match value {
        Ok(value) => Acquired::Observed { value },
        Err(reason) => Acquired::Unavailable { reason },
    }
}

/// The kernel families this acquisition names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KernelFamily {
    Linux,
    Darwin,
}

/// The operating systems a compiled target names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetOs {
    Linux,
    #[serde(rename = "macos")]
    MacOs,
    Windows,
    Android,
    Ios,
    Freebsd,
}

/// The architectures a kernel or a compiled target names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Architecture {
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

impl Architecture {
    const fn common(self) -> CommonArchitecture {
        match self {
            Self::X86 => CommonArchitecture::X86,
            Self::X86_64 => CommonArchitecture::X86_64,
            Self::Arm => CommonArchitecture::Arm,
            Self::Aarch64 => CommonArchitecture::Aarch64,
            Self::Riscv32 => CommonArchitecture::Riscv32,
            Self::Riscv64 => CommonArchitecture::Riscv64,
            Self::Wasm32 => CommonArchitecture::Wasm32,
            Self::Wasm64 => CommonArchitecture::Wasm64,
        }
    }
}

pub(crate) fn kernel_family(value: &[u8]) -> Acquired<KernelFamily> {
    observation(match value {
        b"Linux" => Ok(KernelFamily::Linux),
        b"Darwin" => Ok(KernelFamily::Darwin),
        _ => Err(AcquisitionReason::UnsupportedSystemIdentifier),
    })
}

pub(crate) fn target_os(value: &str) -> Acquired<TargetOs> {
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

pub(crate) fn architecture(value: &[u8]) -> Acquired<Architecture> {
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
pub(crate) fn kernel_release(value: &[u8]) -> Acquired<String> {
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

/// The target this library was compiled for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LibraryTarget {
    pub os: Acquired<TargetOs>,
    pub architecture: Acquired<Architecture>,
}

impl LibraryTarget {
    /// Read the compiled target's own constants.
    #[must_use]
    pub fn acquire() -> Self {
        Self {
            os: target_os(std::env::consts::OS),
            architecture: architecture(std::env::consts::ARCH.as_bytes()),
        }
    }
}

/// The controlled fields of the running kernel's own report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KernelView {
    pub provider: String,
    pub provider_revision: String,
    pub method: String,
    pub family: Acquired<KernelFamily>,
    pub machine: Acquired<Architecture>,
    pub release: Acquired<String>,
}

impl KernelView {
    /// Ask the running kernel, through `uname`.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[must_use]
    pub fn acquire() -> Acquired<Self> {
        let uname = rustix::system::uname();
        // Do not format/debug Uname: that would include nodename and full
        // version. Read only the fields admitted by the controlled projections
        // above.
        Acquired::Observed {
            value: Self {
                provider: "rustix".into(),
                provider_revision: "1.1.4".into(),
                method: "uname-controlled-kernel-view".into(),
                family: kernel_family(uname.sysname().to_bytes()),
                machine: architecture(uname.machine().to_bytes()),
                release: kernel_release(uname.release().to_bytes()),
            },
        }
    }

    /// Ask the running kernel, through `uname`.
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    #[must_use]
    pub fn acquire() -> Acquired<Self> {
        Acquired::Unavailable {
            reason: AcquisitionReason::UnsupportedPlatform,
        }
    }
}

/// How a kernel-reported identifier was read, as 026 records the method.
#[must_use]
pub fn uname_method() -> VersionedIdentity {
    VersionedIdentity::literal("posix-uname-controlled-kernel-view", "0")
}

/// The system family a kernel view reports, when it reports one.
#[must_use]
pub fn system_family(kernel: &Acquired<KernelView>) -> Option<SystemFamily> {
    let Acquired::Observed { value } = kernel else {
        return None;
    };
    let Acquired::Observed { value } = value.family else {
        return None;
    };
    Some(match value {
        KernelFamily::Linux => SystemFamily::Linux,
        KernelFamily::Darwin => SystemFamily::Darwin,
    })
}

/// The CPU architecture a kernel view reports, when it reports one.
#[must_use]
pub fn cpu_architecture(kernel: &Acquired<KernelView>) -> Option<CommonArchitecture> {
    let Acquired::Observed { value } = kernel else {
        return None;
    };
    let Acquired::Observed { value } = value.machine else {
        return None;
    };
    Some(value.common())
}

/// The standard library's only available-parallelism query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ParallelismMethod {
    RustStdAvailableParallelism,
}

/// A hint at available parallelism, which is not a logical CPU count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParallelismHint {
    pub method: ParallelismMethod,
    pub count: Repetitions,
}

pub(crate) fn parallelism_hint(value: Result<u64, AcquisitionReason>) -> Acquired<ParallelismHint> {
    observation(value.and_then(|value| {
        Repetitions::new(value)
            .map(|count| ParallelismHint {
                method: ParallelismMethod::RustStdAvailableParallelism,
                count,
            })
            .map_err(|_| AcquisitionReason::InvalidParallelism)
    }))
}

impl ParallelismHint {
    /// Ask the standard library once.
    #[must_use]
    pub fn acquire() -> Acquired<Self> {
        parallelism_hint(
            std::thread::available_parallelism()
                .map_err(|_| AcquisitionReason::ParallelismQueryFailed)
                .and_then(|value| {
                    u64::try_from(value.get()).map_err(|_| AcquisitionReason::QuantityOverflow)
                }),
        )
    }
}

/// Every host view one run is acquired with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostView {
    pub library_target: LibraryTarget,
    pub kernel_view: Acquired<KernelView>,
    pub available_parallelism_hint: Acquired<ParallelismHint>,
}

impl HostView {
    /// Acquire every view once.
    #[must_use]
    pub fn acquire() -> Self {
        Self {
            library_target: LibraryTarget::acquire(),
            kernel_view: KernelView::acquire(),
            available_parallelism_hint: ParallelismHint::acquire(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn system_identifiers_are_closed_and_never_echo_unknown_input() {
        for (input, expected) in [
            ("linux", TargetOs::Linux),
            ("macos", TargetOs::MacOs),
            ("windows", TargetOs::Windows),
        ] {
            assert_eq!(target_os(input), Acquired::Observed { value: expected });
        }
        for input in ["unknown", "private-host", " Linux"] {
            assert!(matches!(target_os(input), Acquired::Unavailable { .. }));
        }
        for (input, expected) in [
            (b"Linux".as_slice(), KernelFamily::Linux),
            (b"Darwin".as_slice(), KernelFamily::Darwin),
        ] {
            assert_eq!(kernel_family(input), Acquired::Observed { value: expected });
        }
        for (input, expected) in [
            (b"aarch64".as_slice(), Architecture::Aarch64),
            (b"arm64".as_slice(), Architecture::Aarch64),
            (b"x86_64".as_slice(), Architecture::X86_64),
            (b"riscv64".as_slice(), Architecture::Riscv64),
        ] {
            assert_eq!(architecture(input), Acquired::Observed { value: expected });
        }
        for input in [b"private-host-name".as_slice(), &[0xff], b"", b" Linux"] {
            let family = kernel_family(input);
            let architecture = architecture(input);
            assert!(matches!(family, Acquired::Unavailable { .. }));
            assert!(matches!(architecture, Acquired::Unavailable { .. }));
            assert!(!serde_json::to_string(&family).unwrap().contains("private"));
            assert!(!serde_json::to_string(&architecture)
                .unwrap()
                .contains("private"));
        }
    }

    #[test]
    fn kernel_release_is_whole_controlled_input_not_an_os_or_build_version_guess() {
        for input in ["6.8.0", "24.1.0", "1.2.3.4"] {
            let acquired = kernel_release(input.as_bytes());
            assert_eq!(
                serde_json::to_value(acquired).unwrap(),
                json!({"state":"observed","value":input})
            );
        }
        for input in [
            "",
            "1..0",
            "01.2",
            "1.2.3.4.5",
            "24.1.0-private-host",
            "6.8.0/secret",
            "6.8.0\n",
            " 6.8.0",
        ] {
            let acquired = kernel_release(input.as_bytes());
            assert_eq!(
                serde_json::to_value(acquired).unwrap(),
                json!({"state":"unavailable","reason":"unsupported-kernel-release"})
            );
        }
        assert!(matches!(
            kernel_release(&[0xff]),
            Acquired::Unavailable { .. }
        ));
        assert!(matches!(
            kernel_release("1".repeat(33).as_bytes()),
            Acquired::Unavailable { .. }
        ));
        assert!(matches!(
            kernel_release("1".repeat(32).as_bytes()),
            Acquired::Observed { .. }
        ));
    }

    #[test]
    fn parallelism_hint_is_positive_exact_and_distinct_from_hardware_cpu_count() {
        for count in [1, 9_007_199_254_740_992, u64::MAX] {
            let acquired = parallelism_hint(Ok(count));
            let encoded = serde_json::to_value(&acquired).unwrap();
            assert_eq!(encoded["value"]["count"], count.to_string());
            assert_eq!(encoded["value"]["method"], "rust-std-available-parallelism");
            assert_eq!(
                serde_json::from_value::<Acquired<ParallelismHint>>(encoded).unwrap(),
                acquired
            );
        }
        for count in [
            Ok(0),
            Err(AcquisitionReason::ParallelismQueryFailed),
            Err(AcquisitionReason::QuantityOverflow),
        ] {
            assert!(matches!(
                parallelism_hint(count),
                Acquired::Unavailable { .. }
            ));
        }
        let mut invalid = serde_json::to_value(parallelism_hint(Ok(1))).unwrap();
        invalid["value"]["count"] = json!("0");
        assert!(serde_json::from_value::<Acquired<ParallelismHint>>(invalid).is_err());
    }

    #[test]
    fn a_kernel_view_projects_only_what_it_observed() {
        let observed = |family, machine| Acquired::Observed {
            value: KernelView {
                provider: "rustix".into(),
                provider_revision: "1.1.4".into(),
                method: "uname-controlled-kernel-view".into(),
                family,
                machine,
                release: kernel_release(b"6.8.0"),
            },
        };
        let view = observed(kernel_family(b"Darwin"), architecture(b"arm64"));
        assert_eq!(system_family(&view), Some(SystemFamily::Darwin));
        assert_eq!(cpu_architecture(&view), Some(CommonArchitecture::Aarch64));
        // An unsupported family or machine is absent, never a guess.
        let view = observed(kernel_family(b"Plan9"), architecture(b"sparc"));
        assert_eq!(system_family(&view), None);
        assert_eq!(cpu_architecture(&view), None);
        let unavailable = Acquired::Unavailable {
            reason: AcquisitionReason::UnsupportedPlatform,
        };
        assert_eq!(system_family(&unavailable), None);
        assert_eq!(cpu_architecture(&unavailable), None);
        // Every native architecture has a common counterpart.
        for (native, common) in [
            (Architecture::X86, CommonArchitecture::X86),
            (Architecture::X86_64, CommonArchitecture::X86_64),
            (Architecture::Arm, CommonArchitecture::Arm),
            (Architecture::Aarch64, CommonArchitecture::Aarch64),
            (Architecture::Riscv32, CommonArchitecture::Riscv32),
            (Architecture::Riscv64, CommonArchitecture::Riscv64),
            (Architecture::Wasm32, CommonArchitecture::Wasm32),
            (Architecture::Wasm64, CommonArchitecture::Wasm64),
        ] {
            assert_eq!(native.common(), common);
        }
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn a_host_view_is_acquired_without_host_names_or_paths() {
        let view = HostView::acquire();
        let value = serde_json::to_value(&view).unwrap();
        assert_eq!(value["kernelView"]["state"], "observed");
        assert_eq!(value["libraryTarget"]["os"]["state"], "observed");
        let text = serde_json::to_string(&view).unwrap();
        for forbidden in ["hostname", "username", "nodename", "domainname"] {
            assert!(!text.contains(forbidden));
        }
        // The running kernel and the compiled target are separate views; a
        // translated or 32-bit process can see them disagree, so neither is
        // derived from the other.
        assert!(cpu_architecture(&view.kernel_view).is_some());
        assert!(matches!(
            view.library_target.architecture,
            Acquired::Observed { .. }
        ));
        assert_eq!(serde_json::from_value::<HostView>(value).unwrap(), view);
    }
}
