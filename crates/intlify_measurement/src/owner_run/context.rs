// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What an owner run observed about the build and the machine it ran on.
//!
//! The harness collects very little. That is a statement, not an omission:
//! every field it does not collect is reported as unavailable with the reason,
//! because an absent observation must never read as a weaker one. What it does
//! observe — the package it measures, the clock it acquired, the concurrency
//! it used, the instrumentation it installed — it observes directly.

use intlify_shared_json::quantity::{Quantity, Repetitions};
use serde::{Deserialize, Serialize};

use super::observation::{Digest, Framing};
use super::{Labels, Package};
use crate::acquisition::{ClockFailure, MonotonicClock};
use crate::build::{missing as missing_build, Build, SourceControlState};
use crate::environment::{
    missing as missing_field, ClockObservation, Concurrency, ConditionalObservation, Datum,
    Environment, Instrumentation, InventoryFailure, Observation as FieldObservation,
    RequiredObservation, RunnerContext,
};
use crate::identity::{
    IdentityFailure, NativeChecksum, OwnerLabel, RecordIdentity, Token, VersionedIdentity,
};
use crate::plan::BuildIdentity;
use crate::reason::{BuildField, EnvironmentField as Field, MissingObservation};

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

/// What an owner run could observe about the build it measured.
///
/// It is deliberately narrow. No source tree digest, dependency lock, or
/// effective compiler configuration is collected, so none of them is claimed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildObservation {
    pub package: String,
    pub version: String,
    pub architecture: String,
    pub operating_system: String,
    pub assertions: bool,
    pub checksum: Digest,
}

impl BuildObservation {
    /// Observe the owner's package, as its own crate described it.
    #[must_use]
    pub fn acquire(framing: Framing, package: Package) -> Self {
        let architecture = std::env::consts::ARCH;
        let operating_system = std::env::consts::OS;
        let mut frame = framing.frame("build-observation");
        frame.text(package.name);
        frame.text(package.version);
        frame.text(architecture);
        frame.text(operating_system);
        frame.flag(package.assertions);
        Self {
            package: package.name.into(),
            version: package.version.into(),
            architecture: architecture.into(),
            operating_system: operating_system.into(),
            assertions: package.assertions,
            checksum: frame.finish(),
        }
    }

    /// Bind the Run Plan to this observation's exact content.
    #[must_use]
    pub fn identity(&self, labels: &Labels) -> BuildIdentity {
        BuildIdentity {
            owner_schema: OwnerLabel::literal(labels.build_schema),
            algorithm: OwnerLabel::literal("blake3-256"),
            framing: OwnerLabel::literal(labels.framing.label()),
            domain: OwnerLabel::literal("build-observation"),
            checksum: NativeChecksum::from_bytes(self.checksum.bytes()),
        }
    }

    /// Project the applicable Build fields, naming every absence.
    pub fn build(
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
            source_control_state: unattested::<SourceControlState>(
                parent,
                BuildField::SourceControlState,
                MissingObservation::SourceControlStateNotAttested,
            ),
            measurement_profile: profile,
        })
    }
}

/// The sampling a run performs, fixed before any capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SamplingPolicy {
    pub warmup_repetitions: Quantity,
    pub measured_samples: Repetitions,
    pub repetitions_per_sample: Repetitions,
}

impl SamplingPolicy {
    /// The sampling the first smoke profile performs.
    ///
    /// Fixed warmups and one measured sample of one repetition. It is not a
    /// numeric profile, so nothing here is tuned for a stable statistic.
    #[must_use]
    pub fn smoke() -> Self {
        Self {
            warmup_repetitions: Quantity::new(2),
            measured_samples: Repetitions::new(1).expect("one measured sample"),
            repetitions_per_sample: Repetitions::new(1).expect("one repetition"),
        }
    }
}

/// Everything fixed at acquisition, retained with the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextObservation {
    pub profile: VersionedIdentity,
    pub sampling: SamplingPolicy,
    pub clock: ClockObservation,
    pub build: BuildObservation,
}

/// The clock and the observations one run was acquired with.
pub struct CaptureContext {
    clock: MonotonicClock,
    observation: ContextObservation,
}

impl CaptureContext {
    /// Acquire the clock and record what this run can observe.
    pub fn acquire(
        profile: VersionedIdentity,
        sampling: SamplingPolicy,
        build: BuildObservation,
    ) -> Result<Self, ClockFailure> {
        let clock = MonotonicClock::acquire()?;
        let observation = ContextObservation {
            profile,
            sampling,
            clock: clock.description().observation(),
            build,
        };
        Ok(Self { clock, observation })
    }

    /// Borrow the acquired clock.
    #[must_use]
    pub const fn clock(&self) -> &MonotonicClock {
        &self.clock
    }

    /// Borrow what was observed at acquisition.
    #[must_use]
    pub const fn observation(&self) -> &ContextObservation {
        &self.observation
    }
}

/// Project a native owner run's observations into 026's field inventory.
///
/// It describes one Rust component measured in the calling process, on the
/// calling thread, with no instrumentation beyond the clock. The measured
/// context canonicalizes from a finite declared rule table, which is not a
/// locale service with a profile.
pub fn native_environment(
    labels: &Labels,
    context: &ContextObservation,
    parent: &RecordIdentity,
) -> Result<Environment, InventoryFailure> {
    let native_rule = || labels.native_rule.versioned();
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
                    descriptor: labels.instrumentation.versioned(),
                    timing: Token::literal("compiled-and-enabled-posix-monotonic-invocation"),
                    allocation: Token::literal("not-installed-by-this-harness"),
                    trace: Token::literal("not-installed-by-this-harness"),
                    profiling: Token::literal("not-installed-by-this-harness"),
                    external: Token::literal("not-attested"),
                },
            }),
            Datum::Allocator(conditionally_absent!(Allocator, NotCollected)),
            Datum::MemoryObserver(ConditionalObservation::NotApplicable {
                applicability_rule: labels.memory_rule.versioned(),
            }),
            Datum::ClockOrSampler(ConditionalObservation::Observed {
                value: context.clock.clone(),
            }),
            Datum::Concurrency(FieldObservation::Observed {
                value: Concurrency {
                    descriptor: labels.concurrency.versioned(),
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
            Datum::LocaleService(conditionally_absent!(
                LocaleService,
                FiniteProviderHasNoLocaleServiceProfile
            )),
            Datum::Harness(RequiredObservation::Observed {
                value: labels.harness.versioned(),
            }),
            Datum::Projection(RequiredObservation::Observed {
                value: labels.projection.versioned(),
            }),
        ],
    )
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use crate::identity::CommonDomain;
    use crate::owner_run::test_owner::LABELS;

    fn package() -> Package {
        Package {
            name: "intlify_measurement",
            version: "0.0.0",
            assertions: true,
        }
    }

    fn context() -> ContextObservation {
        CaptureContext::acquire(
            LABELS.profile.versioned(),
            SamplingPolicy::smoke(),
            BuildObservation::acquire(LABELS.framing, package()),
        )
        .unwrap()
        .observation
    }

    #[test]
    fn the_build_observation_is_the_same_for_two_acquisitions_of_one_build() {
        // Nothing in it comes from the clock or the run, so the Run Plan's
        // build binding does not change between runs of the same binary.
        let first = BuildObservation::acquire(LABELS.framing, package());
        let second = BuildObservation::acquire(LABELS.framing, package());
        assert_eq!(first, second);
        assert_eq!(first.identity(&LABELS), second.identity(&LABELS));
        // What the owner's crate says about itself is part of the binding.
        let other = BuildObservation::acquire(
            LABELS.framing,
            Package {
                assertions: false,
                ..package()
            },
        );
        assert_ne!(first.identity(&LABELS), other.identity(&LABELS));
    }

    #[test]
    fn every_unobserved_field_carries_its_reason_and_names_this_record() {
        let parent = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        let inventory = native_environment(&LABELS, &context(), &parent).unwrap();
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
        let value =
            serde_json::to_value(native_environment(&LABELS, &context, &parent).unwrap()).unwrap();
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
        // The harness and projection are the owner's own, as registered.
        assert_eq!(
            field("harness")["observation"]["state"]["value"]["identity"],
            LABELS.harness.identity
        );
    }

    #[test]
    fn a_build_projection_attests_only_to_what_was_read() {
        let parent = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        let build = BuildObservation::acquire(LABELS.framing, package())
            .build(&parent, LABELS.profile.versioned())
            .unwrap();
        let value = serde_json::to_value(&build).unwrap();
        // The package is observed under the name its own crate gave, not a
        // prettier registered alias. Everything that would need a build script
        // or a source digest is not claimed.
        assert_eq!(value["implementation"]["identity"], "intlify_measurement");
        assert_eq!(value["implementation"]["revision"], "0.0.0");
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
