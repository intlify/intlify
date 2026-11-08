// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! This owner's environment observations, projected into 026's field inventory.
//!
//! The inventory, the state union each field admits, the local identifiers, and
//! the ordering belong to `intlify_measurement`. What this module decides is
//! what this owner actually observed and, for every field it did not, the exact
//! reason it is absent. A native hint never becomes a stronger fact.

use intlify_measurement::acquisition::host::{cpu_architecture, system_family, uname_method};
use intlify_measurement::environment::{
    missing, Concurrency, ConditionalObservation, Datum, Instrumentation, Observation,
    ReportedIdentifier, RequiredObservation, RunnerContext,
};

pub(super) use intlify_measurement::environment::{Environment, Toolchain};

use super::identity::{IdentityFailure, RecordIdentity, Token, VersionedIdentity};
use super::reason::{EnvironmentField as Field, MissingObservation as Missing};
use crate::benchmark::build::{Acquisition, BuildObservation};
use crate::benchmark::environment::Acquired;
use crate::benchmark::quantity::{Quantity, Repetitions};
use crate::benchmark::run::ProjectionSource;

pub(super) fn harness_identity() -> VersionedIdentity {
    VersionedIdentity::new("intlify-config-owner-run-harness", "0").expect("registered harness")
}
pub(super) fn projection_identity() -> VersionedIdentity {
    VersionedIdentity::new("intlify-config-minimum-to-026", "0").expect("registered projection")
}

pub(super) fn toolchain(build: &BuildObservation) -> Result<Option<Toolchain>, IdentityFailure> {
    match &build.compiler {
        Acquired::Observed { value } => value.toolchain().map(Some),
        Acquired::Unavailable { .. } => Ok(None),
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
    let family = system_family(&native_context.kernel_view);
    let architecture = cpu_architecture(&native_context.kernel_view);
    let native_rule = || {
        VersionedIdentity::new("intlify-config-native-unmanaged-component-context", "0")
            .expect("registered rule")
    };
    let duration_rule =
        VersionedIdentity::new("intlify-config-duration-only-no-memory-observer", "0")
            .expect("registered rule");
    // A memory run is handed its observer and reads no clock (#139).
    let memory = native_context.memory_observer.is_some();
    let memory_rule = || {
        VersionedIdentity::new("intlify-config-memory-run-reads-no-clock", "0")
            .expect("registered rule")
    };
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
    Environment::new(
        parent,
        [
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
                value: if memory {
                    Instrumentation {
                        descriptor: VersionedIdentity::new(
                            "intlify-config-owner-memory-run-instrumentation",
                            "0",
                        )?,
                        timing: Token::literal("not-installed-by-this-harness"),
                        allocation: Token::literal(
                            "observer-counting-global-allocator-installed-by-the-binary",
                        ),
                        trace: Token::literal("not-installed-by-this-harness"),
                        profiling: Token::literal("not-installed-by-this-harness"),
                        external: Token::literal("not-attested"),
                    }
                } else {
                    Instrumentation {
                        descriptor: VersionedIdentity::new(
                            "intlify-config-owner-run-instrumentation",
                            "0",
                        )?,
                        timing: Token::literal("compiled-and-enabled-posix-monotonic-invocation"),
                        allocation: Token::literal("not-installed-by-this-harness"),
                        trace: Token::literal("not-installed-by-this-harness"),
                        profiling: Token::literal("not-installed-by-this-harness"),
                        external: Token::literal("not-attested"),
                    }
                },
            }),
            // The observer counts what is requested through the allocator it
            // wraps; that allocator itself is not identified.
            Datum::Allocator(conditional_unavailable!(Allocator, NotCollected)),
            Datum::MemoryObserver(match &native_context.memory_observer {
                Some(observer) => ConditionalObservation::Observed {
                    value: VersionedIdentity::new(&observer.identity, &observer.revision)?,
                },
                None => ConditionalObservation::NotApplicable {
                    applicability_rule: duration_rule,
                },
            }),
            Datum::ClockOrSampler(match &native_context.clock {
                Some(clock) => ConditionalObservation::Observed {
                    value: clock.clone(),
                },
                None => ConditionalObservation::NotApplicable {
                    applicability_rule: memory_rule(),
                },
            }),
            Datum::Concurrency(Observation::Observed {
                value: Concurrency {
                    descriptor: VersionedIdentity::new(
                        "intlify-config-owner-run-concurrency",
                        "0",
                    )?,
                    scope: Token::literal("one-owner-run-in-the-calling-process"),
                    processes: Repetitions::new(1).expect("one process"),
                    calling_threads: Repetitions::new(1).expect("one calling thread"),
                    internal_workers: Quantity::new(0),
                    policy: Token::literal(
                        "sequential-cases-synchronous-calling-thread-no-workers",
                    ),
                    external_activity: Token::literal("uncontrolled"),
                },
            }),
            Datum::ContainerEmulatorSimulator(conditional_unavailable!(
                ContainerEmulatorSimulator,
                NotCollected
            )),
            Datum::LocaleService(conditional_unavailable!(
                LocaleService,
                CanonicalizationProviderHasNoLocaleServiceProfile
            )),
            Datum::Harness(RequiredObservation::Observed {
                value: harness_identity(),
            }),
            Datum::Projection(RequiredObservation::Observed {
                value: projection_identity(),
            }),
        ],
    )
    .map_err(|failure| match failure {
        intlify_measurement::environment::InventoryFailure::Identity(failure) => failure,
        // The inventory above is a literal in 026's registered order, and each
        // absent field's reasons are built from that same field and parent, so
        // either failure would be a defect in this module rather than an input.
        intlify_measurement::environment::InventoryFailure::FieldOrder
        | intlify_measurement::environment::InventoryFailure::ReasonBinding => {
            unreachable!("registered field order and reason binding")
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
        && environment.reasons_are_valid(parent)
}
