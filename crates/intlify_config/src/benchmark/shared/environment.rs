// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! This owner's environment observations, projected into 026's field inventory.
//!
//! The inventory, the state union each field admits, the local identifiers, and
//! the ordering belong to `intlify_measurement`. What this module decides is
//! what this owner actually observed and, for every field it did not, the exact
//! reason it is absent. A native hint never becomes a stronger fact.

use intlify_measurement::environment::{
    missing, Architecture, Concurrency, ConditionalObservation, Datum, Instrumentation,
    Observation, ReportedIdentifier, RequiredObservation, RunnerContext, SystemFamily,
};

pub(super) use intlify_measurement::environment::{Environment, Toolchain};

use super::identity::{IdentityFailure, RecordIdentity, Token, VersionedIdentity};
use super::reason::{EnvironmentField as Field, MissingObservation as Missing};
use crate::benchmark::build::{Acquisition, BuildObservation};
use crate::benchmark::environment::{self as native, Acquired};
use crate::benchmark::quantity::{Quantity, Repetitions};
use crate::benchmark::run::ProjectionSource;

pub(super) fn harness_identity() -> VersionedIdentity {
    VersionedIdentity::new("intlify-config-owner-run-harness", "1").expect("registered harness")
}
pub(super) fn projection_identity() -> VersionedIdentity {
    VersionedIdentity::new("intlify-config-minimum-to-026", "0").expect("registered projection")
}

pub(super) fn toolchain(build: &BuildObservation) -> Result<Option<Toolchain>, IdentityFailure> {
    match &build.compiler {
        Acquisition::Observed { value } => Ok(Some(Toolchain {
            compiler: VersionedIdentity::new(&value.identity, &value.release)?,
            commit: value.commit.as_deref().map(Token::new).transpose()?,
            backend: VersionedIdentity::new("llvm", &value.llvm)?,
        })),
        Acquisition::Unavailable { .. } => Ok(None),
    }
}

/// Project this owner's acquired context into the complete 026 inventory.
pub(super) fn project(
    source: &ProjectionSource<'_>,
    parent: &RecordIdentity,
) -> Result<Environment, IdentityFailure> {
    let context = &source.document().result().context;
    let build = &context.build;
    let native_context = &context.environment;
    let uname_method = || {
        VersionedIdentity::new("posix-uname-controlled-kernel-view", "0")
            .expect("registered method")
    };
    let family = match &native_context.kernel_view {
        Acquired::Observed { value } => match value.family {
            Acquired::Observed { value } => Some(match value {
                native::KernelFamily::Linux => SystemFamily::Linux,
                native::KernelFamily::Darwin => SystemFamily::Darwin,
            }),
            Acquired::Unavailable { .. } => None,
        },
        Acquired::Unavailable { .. } => None,
    };
    let architecture = match &native_context.kernel_view {
        Acquired::Observed { value } => match value.machine {
            Acquired::Observed { value } => Some(match value {
                native::Architecture::X86 => Architecture::X86,
                native::Architecture::X86_64 => Architecture::X86_64,
                native::Architecture::Arm => Architecture::Arm,
                native::Architecture::Aarch64 => Architecture::Aarch64,
                native::Architecture::Riscv32 => Architecture::Riscv32,
                native::Architecture::Riscv64 => Architecture::Riscv64,
                native::Architecture::Wasm32 => Architecture::Wasm32,
                native::Architecture::Wasm64 => Architecture::Wasm64,
            }),
            Acquired::Unavailable { .. } => None,
        },
        Acquired::Unavailable { .. } => None,
    };
    let native_rule = || {
        VersionedIdentity::new("intlify-config-native-unmanaged-component-context", "0")
            .expect("registered rule")
    };
    let duration_rule =
        VersionedIdentity::new("intlify-config-duration-only-no-memory-observer", "0")
            .expect("registered rule");
    macro_rules! unavailable {
        ($field:ident, $cause:ident) => {
            Observation::Unavailable {
                reasons: missing(parent, Field::$field, Missing::$cause),
            }
        };
    }
    macro_rules! conditional_unavailable {
        ($field:ident, $cause:ident) => {
            ConditionalObservation::Unavailable {
                reasons: missing(parent, Field::$field, Missing::$cause),
            }
        };
    }
    Environment::new([
        Datum::OsFamily(family.map_or_else(
            || unavailable!(OsFamily, NativeAcquisitionUnavailable),
            |identity| Observation::Observed {
                value: ReportedIdentifier {
                    identity,
                    method: uname_method(),
                },
            },
        )),
        Datum::OsVersion(unavailable!(OsVersion, NotCollected)),
        Datum::KernelBuild(conditional_unavailable!(
            KernelBuild,
            KernelReleaseIsNotBuildIdentity
        )),
        Datum::CpuArchitecture(architecture.map_or_else(
            || unavailable!(CpuArchitecture, NativeAcquisitionUnavailable),
            |identity| Observation::Observed {
                value: ReportedIdentifier {
                    identity,
                    method: uname_method(),
                },
            },
        )),
        Datum::TargetTriple(match &build.cargo_inputs.target {
            Acquisition::Observed { value } => ConditionalObservation::Observed {
                value: Token::new(value)?,
            },
            Acquisition::Unavailable { .. } => {
                conditional_unavailable!(TargetTriple, NativeAcquisitionUnavailable)
            }
        }),
        Datum::RunnerContext(RequiredObservation::Observed {
            value: RunnerContext::LocalUncontrolled {},
        }),
        Datum::ExecutionKind(unavailable!(ExecutionKind, NotCollected)),
        Datum::ProcessorClass(unavailable!(ProcessorClass, NotCollected)),
        Datum::LogicalCpuCount(unavailable!(
            LogicalCpuCount,
            ParallelismHintIsNotLogicalCpuCount
        )),
        Datum::MemoryCapacityClass(unavailable!(MemoryCapacityClass, NotCollected)),
        Datum::PowerThermalPolicy(conditional_unavailable!(PowerThermalPolicy, NotCollected)),
        Datum::LanguageRuntime(ConditionalObservation::NotApplicable {
            applicability_rule: native_rule(),
        }),
        Datum::Browser(ConditionalObservation::NotApplicable {
            applicability_rule: native_rule(),
        }),
        Datum::VirtualMachine(ConditionalObservation::NotApplicable {
            applicability_rule: native_rule(),
        }),
        Datum::Device(conditional_unavailable!(Device, NotCollected)),
        Datum::JitGcConfiguration(ConditionalObservation::NotApplicable {
            applicability_rule: native_rule(),
        }),
        Datum::Toolchain(toolchain(build)?.map_or_else(
            || unavailable!(Toolchain, NativeAcquisitionUnavailable),
            |value| Observation::Observed { value },
        )),
        Datum::BuildConfiguration(unavailable!(BuildConfiguration, EffectiveBuildNotAttested)),
        Datum::Instrumentation(RequiredObservation::Observed {
            value: Instrumentation {
                descriptor: VersionedIdentity::new(
                    "intlify-config-owner-run-instrumentation",
                    "0",
                )?,
                timing: Token::literal("compiled-and-enabled-posix-monotonic-invocation"),
                allocation: Token::literal("not-installed-by-this-harness"),
                trace: Token::literal("not-installed-by-this-harness"),
                profiling: Token::literal("not-installed-by-this-harness"),
                external: Token::literal("not-attested"),
            },
        }),
        Datum::Allocator(conditional_unavailable!(Allocator, NotCollected)),
        Datum::MemoryObserver(ConditionalObservation::NotApplicable {
            applicability_rule: duration_rule,
        }),
        Datum::ClockOrSampler(ConditionalObservation::Observed {
            value: native_context.clock.clone(),
        }),
        Datum::Concurrency(Observation::Observed {
            value: Concurrency {
                descriptor: VersionedIdentity::new("intlify-config-owner-run-concurrency", "0")?,
                scope: Token::literal("one-owner-run-in-the-calling-process"),
                processes: Repetitions::new(1).expect("one process"),
                calling_threads: Repetitions::new(1).expect("one calling thread"),
                internal_workers: Quantity::new(0),
                policy: Token::literal("sequential-cases-synchronous-calling-thread-no-workers"),
                external_activity: Token::literal("uncontrolled"),
            },
        }),
        Datum::ContainerEmulatorSimulator(conditional_unavailable!(
            ContainerEmulatorSimulator,
            NotCollected
        )),
        Datum::LocaleService(conditional_unavailable!(
            LocaleService,
            FiniteProviderHasNoLocaleServiceProfile
        )),
        Datum::Harness(RequiredObservation::Observed {
            value: harness_identity(),
        }),
        Datum::Projection(RequiredObservation::Observed {
            value: projection_identity(),
        }),
    ])
    .map_err(|failure| match failure {
        intlify_measurement::environment::InventoryFailure::Identity(failure) => failure,
        // The inventory above is a literal in 026's registered order, so a
        // field-order failure would be a defect in this module, not an input.
        intlify_measurement::environment::InventoryFailure::FieldOrder => {
            unreachable!("registered field order")
        }
    })
}

/// Rebuild the expected inventory from independently checked owner inputs.
///
/// The submitted field states and applicability identifiers are never the
/// source of the expectation they are compared against.
pub(super) fn validate(
    environment: &Environment,
    source: &ProjectionSource<'_>,
    parent: &RecordIdentity,
) -> bool {
    project(source, parent).is_ok_and(|expected| expected == *environment)
        && environment.reasons_are_valid()
}
