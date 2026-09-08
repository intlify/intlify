// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Complete 026 environment-field inventory projected from admitted native
//! inputs. Unknown facts stay unavailable; native hints are not stronger facts.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::identity::{IdentityFailure, RecordIdentity, Token, VersionedIdentity};
use super::reason::{
    valid_reasons, EnvironmentField as Field, MissingObservation as Missing, Reason, Selector,
};
use crate::benchmark::build::{Acquisition, BuildObservation};
use crate::benchmark::descriptor::ClockObservation;
use crate::benchmark::environment::{self as native, Acquired};
use crate::benchmark::quantity::{Quantity, Repetitions};
use crate::benchmark::run::ProjectionSource;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum Observation<T> {
    Observed {
        value: T,
    },
    Unavailable {
        #[schemars(length(min = 1))]
        reasons: Vec<Reason>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum RequiredObservation<T> {
    Observed { value: T },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum ConditionalObservation<T> {
    Observed {
        value: T,
    },
    NotApplicable {
        applicability_rule: VersionedIdentity,
    },
    Unavailable {
        #[schemars(length(min = 1))]
        reasons: Vec<Reason>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum SystemFamily {
    Linux,
    Darwin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Architecture {
    X86,
    X86_64,
    Arm,
    Aarch64,
    Riscv32,
    Riscv64,
    Wasm32,
    Wasm64,
}

/// The acquisition method qualifies this identifier: neither uname's system
/// family nor its machine view is a distribution/device/physical-host attestation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReportedIdentifier<T> {
    identity: T,
    method: VersionedIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Toolchain {
    compiler: VersionedIdentity,
    #[serde(deserialize_with = "Option::deserialize")]
    commit: Option<Token>,
    backend: VersionedIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BuildConfiguration {
    profile: Token,
    optimization: Token,
    assertions: bool,
    link_mode: VersionedIdentity,
    features: Vec<Token>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum RunnerContext {
    LocalUncontrolled {},
}

macro_rules! literal {
    ($name:ident, $wire:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        enum $name {
            #[serde(rename = $wire)]
            Value,
        }
    };
}
literal!(Timing, "compiled-and-enabled-posix-monotonic-invocation");
literal!(NotInstalled, "not-installed-by-this-harness");
literal!(Unattested, "not-attested");
literal!(ConcurrencyScope, "one-owner-run-in-the-calling-process");
literal!(
    ConcurrencyPolicy,
    "sequential-cases-synchronous-calling-thread-no-workers"
);
literal!(ExternalActivity, "uncontrolled");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Instrumentation {
    descriptor: VersionedIdentity,
    timing: Timing,
    allocation: NotInstalled,
    trace: NotInstalled,
    profiling: NotInstalled,
    #[serde(rename = "externalInstrumentation")]
    external: Unattested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Concurrency {
    descriptor: VersionedIdentity,
    scope: ConcurrencyScope,
    processes: Repetitions,
    calling_threads: Repetitions,
    internal_workers: Quantity,
    policy: ConcurrencyPolicy,
    external_activity: ExternalActivity,
}

// Each field has its own value type and allowed state union. A forbidden state
// or a value belonging to another field is rejected by the generated schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "field",
    content = "state",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Datum {
    OsFamily(Observation<ReportedIdentifier<SystemFamily>>),
    OsVersion(Observation<VersionedIdentity>),
    KernelBuild(ConditionalObservation<VersionedIdentity>),
    CpuArchitecture(Observation<ReportedIdentifier<Architecture>>),
    TargetTriple(ConditionalObservation<Token>),
    RunnerContext(RequiredObservation<RunnerContext>),
    ExecutionKind(Observation<VersionedIdentity>),
    ProcessorClass(Observation<VersionedIdentity>),
    LogicalCpuCount(Observation<Repetitions>),
    MemoryCapacityClass(Observation<VersionedIdentity>),
    PowerThermalPolicy(ConditionalObservation<VersionedIdentity>),
    LanguageRuntime(ConditionalObservation<VersionedIdentity>),
    Browser(ConditionalObservation<VersionedIdentity>),
    VirtualMachine(ConditionalObservation<VersionedIdentity>),
    Device(ConditionalObservation<VersionedIdentity>),
    JitGcConfiguration(ConditionalObservation<VersionedIdentity>),
    Toolchain(Observation<Toolchain>),
    BuildConfiguration(Observation<BuildConfiguration>),
    Instrumentation(RequiredObservation<Instrumentation>),
    Allocator(ConditionalObservation<VersionedIdentity>),
    MemoryObserver(ConditionalObservation<VersionedIdentity>),
    ClockOrSampler(ConditionalObservation<ClockObservation>),
    Concurrency(Observation<Concurrency>),
    ContainerEmulatorSimulator(ConditionalObservation<VersionedIdentity>),
    LocaleService(ConditionalObservation<VersionedIdentity>),
    Harness(RequiredObservation<VersionedIdentity>),
    Projection(RequiredObservation<VersionedIdentity>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    local_record_identity: Token,
    observation: Datum,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Environment {
    field_registry: VersionedIdentity,
    fields: Vec<Entry>,
}

impl JsonSchema for Environment {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Environment".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let entry = generator.subschema_for::<Entry>();
        let registry = generator.subschema_for::<VersionedIdentity>();
        let fields = Field::ALL.map(|field| serde_json::json!({
            "allOf": [entry, {"properties": {"observation": {"properties": {"field": {"const": field.name()}}}}}]
        }));
        schemars::json_schema!({
            "type": "object", "additionalProperties": false,
            "required": ["fieldRegistry", "fields"],
            "properties": {
                "fieldRegistry": registry,
                "fields": {"type": "array", "minItems": 27, "maxItems": 27,
                    "items": fields, "additionalItems": false}
            }
        })
    }
}

pub(super) fn harness_identity() -> VersionedIdentity {
    VersionedIdentity::new("intlify-config-owner-run-harness", "1").expect("registered harness")
}
pub(super) fn projection_identity() -> VersionedIdentity {
    VersionedIdentity::new("intlify-config-minimum-to-026", "0").expect("registered projection")
}

fn missing(parent: &RecordIdentity, field: Field, cause: Missing) -> Vec<Reason> {
    vec![Reason::missing_observation(
        Selector::EnvironmentField {
            record_identity: parent.clone(),
            field,
        },
        cause,
    )]
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

impl Environment {
    pub(super) fn local_identities(&self) -> impl Iterator<Item = &Token> {
        self.fields.iter().map(|field| &field.local_record_identity)
    }
    pub(super) fn project(
        source: &ProjectionSource<'_>,
        parent: &RecordIdentity,
    ) -> Result<Self, IdentityFailure> {
        let context = &source.document().result().context;
        let build = &context.build;
        let native = &context.environment;
        let uname_method = || {
            VersionedIdentity::new("posix-uname-controlled-kernel-view", "0")
                .expect("registered method")
        };
        let family = match &native.kernel_view {
            Acquired::Observed { value } => match value.family {
                Acquired::Observed { value } => Some(match value {
                    native::KernelFamily::Linux => SystemFamily::Linux,
                    native::KernelFamily::Darwin => SystemFamily::Darwin,
                }),
                Acquired::Unavailable { .. } => None,
            },
            Acquired::Unavailable { .. } => None,
        };
        let architecture = match &native.kernel_view {
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
        let values = vec![
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
                    timing: Timing::Value,
                    allocation: NotInstalled::Value,
                    trace: NotInstalled::Value,
                    profiling: NotInstalled::Value,
                    external: Unattested::Value,
                },
            }),
            Datum::Allocator(conditional_unavailable!(Allocator, NotCollected)),
            Datum::MemoryObserver(ConditionalObservation::NotApplicable {
                applicability_rule: duration_rule,
            }),
            Datum::ClockOrSampler(ConditionalObservation::Observed {
                value: native.clock.clone(),
            }),
            Datum::Concurrency(Observation::Observed {
                value: Concurrency {
                    descriptor: VersionedIdentity::new(
                        "intlify-config-owner-run-concurrency",
                        "0",
                    )?,
                    scope: ConcurrencyScope::Value,
                    processes: Repetitions::new(1).expect("one process"),
                    calling_threads: Repetitions::new(1).expect("one calling thread"),
                    internal_workers: Quantity::new(0),
                    policy: ConcurrencyPolicy::Value,
                    external_activity: ExternalActivity::Value,
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
        ];
        let fields = values
            .into_iter()
            .zip(Field::ALL)
            .map(|(observation, field)| {
                Ok(Entry {
                    local_record_identity: Token::new(&format!(
                        "environment-field-{}",
                        field.name()
                    ))?,
                    observation,
                })
            })
            .collect::<Result<_, IdentityFailure>>()?;
        Ok(Self {
            field_registry: VersionedIdentity::new("intlify-design-026-environment-fields", "0")?,
            fields,
        })
    }

    pub(super) fn validate(&self, source: &ProjectionSource<'_>, parent: &RecordIdentity) -> bool {
        // The expected complete value is rebuilt from independently checked
        // owner inputs, not from the submitted field states or applicability IDs.
        Self::project(source, parent).is_ok_and(|expected| expected == *self)
            && self.reasons().all(valid_reasons)
    }

    fn reasons(&self) -> impl Iterator<Item = &[Reason]> {
        self.fields.iter().filter_map(|entry| {
            macro_rules! observed {
                ($value:expr) => {
                    match $value {
                        Observation::Unavailable { reasons } => Some(reasons.as_slice()),
                        _ => None,
                    }
                };
            }
            macro_rules! conditional {
                ($value:expr) => {
                    match $value {
                        ConditionalObservation::Unavailable { reasons } => Some(reasons.as_slice()),
                        _ => None,
                    }
                };
            }
            match &entry.observation {
                Datum::OsFamily(v) => observed!(v),
                Datum::OsVersion(v)
                | Datum::ExecutionKind(v)
                | Datum::ProcessorClass(v)
                | Datum::MemoryCapacityClass(v) => observed!(v),
                Datum::CpuArchitecture(v) => observed!(v),
                Datum::TargetTriple(v) => conditional!(v),
                Datum::LogicalCpuCount(v) => observed!(v),
                Datum::KernelBuild(v)
                | Datum::PowerThermalPolicy(v)
                | Datum::LanguageRuntime(v)
                | Datum::Browser(v)
                | Datum::VirtualMachine(v)
                | Datum::Device(v)
                | Datum::JitGcConfiguration(v)
                | Datum::Allocator(v)
                | Datum::MemoryObserver(v)
                | Datum::ContainerEmulatorSimulator(v)
                | Datum::LocaleService(v) => conditional!(v),
                Datum::Toolchain(v) => observed!(v),
                Datum::BuildConfiguration(v) => observed!(v),
                Datum::ClockOrSampler(v) => conditional!(v),
                Datum::Concurrency(v) => observed!(v),
                Datum::RunnerContext(_)
                | Datum::Instrumentation(_)
                | Datum::Harness(_)
                | Datum::Projection(_) => None,
            }
        })
    }
}
