// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What this owner observed about the build and the machine it ran on.
//!
//! This harness collects very little. That is a statement, not an omission:
//! every field it does not collect is reported as unavailable with the reason,
//! because an absent observation must never read as a weaker one. What it does
//! observe — the package it measures, the clock it acquired, the concurrency
//! it used, the instrumentation it installed — it observes directly.

use intlify_measurement::acquisition::{ClockFailure, MonotonicClock};
use intlify_measurement::build::{missing as missing_build, Build};
use intlify_measurement::environment::{
    missing as missing_field, ClockObservation, Concurrency, ConditionalObservation, Datum,
    Environment, Instrumentation, InventoryFailure, Observation as FieldObservation,
    RequiredObservation, RunnerContext,
};
use intlify_measurement::identity::{
    IdentityFailure, NativeChecksum, OwnerLabel, RecordIdentity, Token, VersionedIdentity,
};
use intlify_measurement::plan::BuildIdentity;
use intlify_measurement::reason::{BuildField, EnvironmentField as Field, MissingObservation};
use intlify_shared_json::quantity::{Quantity, Repetitions};
use serde::{Deserialize, Serialize};

use super::observation::{Digest, Frame};

/// The harness that produced these observations.
pub(super) fn harness() -> VersionedIdentity {
    VersionedIdentity::literal("intlify-authoring-owner-run-harness", "1")
}

/// The projection from this owner's records into 026's.
pub(super) fn projection() -> VersionedIdentity {
    VersionedIdentity::literal("intlify-authoring-minimum-to-026", "0")
}

fn owner_framing() -> OwnerLabel {
    OwnerLabel::literal("intlify-authoring-minimum-observation/0")
}

// Generic over the field's own value type: each Build field admits its own, so
// one closure cannot stand in for all of them.
fn unattested<T>(
    parent: &RecordIdentity,
    field: BuildField,
    cause: MissingObservation,
) -> FieldObservation<T> {
    FieldObservation::Unavailable {
        reasons: missing_build(parent, field, cause),
    }
}

/// What this owner could observe about the build it measured.
///
/// It is deliberately narrow. No source tree digest, dependency lock, or
/// effective compiler configuration is collected, so none of them is claimed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BuildObservation {
    pub(super) package: String,
    pub(super) version: String,
    pub(super) architecture: String,
    pub(super) operating_system: String,
    pub(super) assertions: bool,
    pub(super) checksum: Digest,
}

impl BuildObservation {
    fn acquire() -> Self {
        let package = env!("CARGO_PKG_NAME");
        let version = env!("CARGO_PKG_VERSION");
        let architecture = std::env::consts::ARCH;
        let operating_system = std::env::consts::OS;
        let assertions = cfg!(debug_assertions);
        let mut frame = Frame::new("build-observation");
        frame.text(package);
        frame.text(version);
        frame.text(architecture);
        frame.text(operating_system);
        frame.flag(assertions);
        Self {
            package: package.into(),
            version: version.into(),
            architecture: architecture.into(),
            operating_system: operating_system.into(),
            assertions,
            checksum: frame.finish(),
        }
    }

    /// Bind the Run Plan to this observation's exact content.
    pub(super) fn identity(&self) -> BuildIdentity {
        BuildIdentity {
            owner_schema: OwnerLabel::literal("intlify-authoring-build-observation/0"),
            algorithm: OwnerLabel::literal("blake3-256"),
            framing: owner_framing(),
            domain: OwnerLabel::literal("build-observation"),
            checksum: NativeChecksum::from_bytes(self.checksum.bytes()),
        }
    }

    /// Project the applicable Build fields, naming every absence.
    pub(super) fn build(
        &self,
        parent: &RecordIdentity,
        profile: VersionedIdentity,
    ) -> Result<Build, IdentityFailure> {
        Ok(Build {
            // This harness reads no source tree, so it attests to none.
            source_content: unattested(
                parent,
                BuildField::SourceContent,
                MissingObservation::NotCollected,
            ),
            implementation: VersionedIdentity::new(&self.package, &self.version)?,
            executable: unattested(
                parent,
                BuildField::Executable,
                MissingObservation::ExecutableNotAttested,
            ),
            dependency_lock: unattested(
                parent,
                BuildField::DependencyLock,
                MissingObservation::NotCollected,
            ),
            configuration: unattested(
                parent,
                BuildField::BuildConfiguration,
                MissingObservation::EffectiveBuildNotAttested,
            ),
            physical_engine: OwnerLabel::literal("rust-native-component"),
            source_control_state: unattested::<intlify_measurement::build::SourceControlState>(
                parent,
                BuildField::SourceControlState,
                MissingObservation::SourceControlStateNotAttested,
            ),
            measurement_profile: profile,
        })
    }
}

/// The sampling this run performs, fixed before any capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SamplingPolicy {
    pub(super) warmup_repetitions: Quantity,
    pub(super) measured_samples: Repetitions,
    pub(super) repetitions_per_sample: Repetitions,
}

/// Everything fixed at acquisition, retained with the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ContextObservation {
    pub(super) profile: VersionedIdentity,
    pub(super) sampling: SamplingPolicy,
    pub(super) clock: ClockObservation,
    pub(super) build: BuildObservation,
}

/// The clock and the observations one run was acquired with.
pub(super) struct CaptureContext {
    clock: MonotonicClock,
    observation: ContextObservation,
}

impl CaptureContext {
    /// Acquire the clock and record what this owner can observe.
    pub(super) fn acquire(
        profile: VersionedIdentity,
        sampling: SamplingPolicy,
    ) -> Result<Self, ClockFailure> {
        let clock = MonotonicClock::acquire()?;
        let observation = ContextObservation {
            profile,
            sampling,
            clock: clock.description().observation(),
            build: BuildObservation::acquire(),
        };
        Ok(Self { clock, observation })
    }

    pub(super) const fn clock(&self) -> &MonotonicClock {
        &self.clock
    }

    pub(super) const fn observation(&self) -> &ContextObservation {
        &self.observation
    }
}

/// Project this owner's observations into 026's complete field inventory.
pub(super) fn environment(
    context: &ContextObservation,
    parent: &RecordIdentity,
) -> Result<Environment, InventoryFailure> {
    let native_rule =
        || VersionedIdentity::literal("intlify-authoring-native-unmanaged-component-context", "0");
    macro_rules! absent {
        ($field:ident, $cause:ident) => {
            FieldObservation::Unavailable {
                reasons: missing_field(parent, Field::$field, MissingObservation::$cause),
            }
        };
    }
    macro_rules! conditionally_absent {
        ($field:ident, $cause:ident) => {
            ConditionalObservation::Unavailable {
                reasons: missing_field(parent, Field::$field, MissingObservation::$cause),
            }
        };
    }
    Environment::new(
        parent,
        [
            // A kernel view is acquirable, but this harness does not acquire
            // one. Reporting it as not collected is the honest answer; it is
            // not evidence that the machine has no operating system.
            Datum::OsFamily(absent!(OsFamily, NotCollected)),
            Datum::OsVersion(absent!(OsVersion, NotCollected)),
            Datum::KernelBuild(conditionally_absent!(KernelBuild, NotCollected)),
            Datum::CpuArchitecture(absent!(CpuArchitecture, NotCollected)),
            Datum::TargetTriple(conditionally_absent!(TargetTriple, NotCollected)),
            Datum::RunnerContext(RequiredObservation::Observed {
                value: RunnerContext::LocalUncontrolled {},
            }),
            Datum::ExecutionKind(absent!(ExecutionKind, NotCollected)),
            Datum::ProcessorClass(absent!(ProcessorClass, NotCollected)),
            Datum::LogicalCpuCount(absent!(LogicalCpuCount, NotCollected)),
            Datum::MemoryCapacityClass(absent!(MemoryCapacityClass, NotCollected)),
            Datum::PowerThermalPolicy(conditionally_absent!(PowerThermalPolicy, NotCollected)),
            Datum::LanguageRuntime(ConditionalObservation::NotApplicable {
                applicability_rule: native_rule(),
            }),
            Datum::Browser(ConditionalObservation::NotApplicable {
                applicability_rule: native_rule(),
            }),
            Datum::VirtualMachine(ConditionalObservation::NotApplicable {
                applicability_rule: native_rule(),
            }),
            Datum::Device(conditionally_absent!(Device, NotCollected)),
            Datum::JitGcConfiguration(ConditionalObservation::NotApplicable {
                applicability_rule: native_rule(),
            }),
            Datum::Toolchain(absent!(Toolchain, NotCollected)),
            Datum::BuildConfiguration(absent!(BuildConfiguration, EffectiveBuildNotAttested)),
            Datum::Instrumentation(RequiredObservation::Observed {
                value: Instrumentation {
                    descriptor: VersionedIdentity::literal(
                        "intlify-authoring-owner-run-instrumentation",
                        "0",
                    ),
                    timing: Token::literal("compiled-and-enabled-posix-monotonic-invocation"),
                    allocation: Token::literal("not-installed-by-this-harness"),
                    trace: Token::literal("not-installed-by-this-harness"),
                    profiling: Token::literal("not-installed-by-this-harness"),
                    external: Token::literal("not-attested"),
                },
            }),
            Datum::Allocator(conditionally_absent!(Allocator, NotCollected)),
            Datum::MemoryObserver(ConditionalObservation::NotApplicable {
                applicability_rule: VersionedIdentity::literal(
                    "intlify-authoring-duration-only-no-memory-observer",
                    "0",
                ),
            }),
            Datum::ClockOrSampler(ConditionalObservation::Observed {
                value: context.clock.clone(),
            }),
            Datum::Concurrency(FieldObservation::Observed {
                value: Concurrency {
                    descriptor: VersionedIdentity::literal(
                        "intlify-authoring-owner-run-concurrency",
                        "0",
                    ),
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
            Datum::ContainerEmulatorSimulator(conditionally_absent!(
                ContainerEmulatorSimulator,
                NotCollected
            )),
            // The measured context canonicalizes from a finite declared rule
            // table, which is not a locale service with a profile.
            Datum::LocaleService(conditionally_absent!(
                LocaleService,
                FiniteProviderHasNoLocaleServiceProfile
            )),
            Datum::Harness(RequiredObservation::Observed { value: harness() }),
            Datum::Projection(RequiredObservation::Observed {
                value: projection(),
            }),
        ],
    )
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use intlify_measurement::identity::CommonDomain;

    fn sampling() -> SamplingPolicy {
        SamplingPolicy {
            warmup_repetitions: Quantity::new(2),
            measured_samples: Repetitions::new(1).unwrap(),
            repetitions_per_sample: Repetitions::new(1).unwrap(),
        }
    }

    fn context() -> ContextObservation {
        CaptureContext::acquire(
            VersionedIdentity::literal("intlify-authoring-minimum-smoke", "0"),
            sampling(),
        )
        .unwrap()
        .observation
    }

    #[test]
    fn the_build_observation_is_the_same_for_two_acquisitions_of_one_build() {
        // Nothing in it comes from the clock or the run, so the Run Plan's
        // build binding does not change between runs of the same binary.
        let first = BuildObservation::acquire();
        let second = BuildObservation::acquire();
        assert_eq!(first, second);
        assert_eq!(first.identity(), second.identity());
    }

    #[test]
    fn every_unobserved_field_carries_its_reason_and_names_this_record() {
        let parent = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        let inventory = environment(&context(), &parent).unwrap();
        assert!(inventory.reasons_are_valid(&parent));
        // The reasons belong to the record they name, so an inventory built
        // for one Evidence Set does not validate against another.
        let other = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        assert!(!inventory.reasons_are_valid(&other));
    }

    #[test]
    fn what_this_harness_does_observe_is_observed_rather_than_described() {
        let context = context();
        let parent = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        let value = serde_json::to_value(environment(&context, &parent).unwrap()).unwrap();
        let field = |name: &str| {
            value["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["observation"]["field"] == name)
                .expect("registered field")
                .clone()
        };
        // The clock reading comes from the provider that was acquired.
        let clock = field("clock_or_sampler");
        assert_eq!(clock["observation"]["state"]["kind"], "observed");
        assert_eq!(
            clock["observation"]["state"]["value"]["clock"],
            "posix-clock-monotonic"
        );
        assert!(
            clock["observation"]["state"]["value"]["resolutionNanoseconds"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                > 0
        );
        // A field this harness does not collect says so, with its reason.
        let family = field("os_family");
        assert_eq!(family["observation"]["state"]["kind"], "unavailable");
    }

    #[test]
    fn a_build_projection_attests_only_to_what_was_read() {
        let parent = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        let build = BuildObservation::acquire()
            .build(
                &parent,
                VersionedIdentity::literal("intlify-authoring-minimum-smoke", "0"),
            )
            .unwrap();
        let value = serde_json::to_value(&build).unwrap();
        // The package this harness measures is observed under its own cargo
        // name, not a prettier registered alias. Everything that would need a
        // build script or a source digest is not claimed.
        assert_eq!(value["implementation"]["identity"], env!("CARGO_PKG_NAME"));
        assert_eq!(
            value["implementation"]["revision"],
            env!("CARGO_PKG_VERSION")
        );
        for absent in [
            "sourceContent",
            "executable",
            "dependencyLock",
            "buildConfiguration",
            "sourceControlState",
        ] {
            assert_eq!(value[absent]["kind"], "unavailable", "{absent}");
        }
    }
}
