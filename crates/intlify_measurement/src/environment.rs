// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The complete 026 environment-field inventory.
//!
//! The inventory, the state union each field admits, the local identifiers, and
//! the ordering are 026's and live here. The observed values are the owner's:
//! this module never acquires a fact, and an unknown fact stays unavailable
//! with its reason rather than becoming a weaker value that reads like one.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use intlify_shared_json::quantity::{Quantity, Repetitions};

use crate::identity::{IdentityFailure, RecordIdentity, Token, VersionedIdentity};
use crate::reason::{
    valid_reasons, EnvironmentField as Field, MissingObservation as Missing, Reason, Selector,
};

/// One observation that is either present or unavailable with its reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Observation<T> {
    Observed {
        value: T,
    },
    Unavailable {
        #[schemars(length(min = 1))]
        reasons: Vec<Reason>,
    },
}

/// One observation a record cannot omit, so it has no unavailable state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RequiredObservation<T> {
    Observed { value: T },
}

/// One observation that can also be inapplicable under a registered rule.
///
/// Inapplicable is not a third spelling of absent: it names the rule that makes
/// the field meaningless for this subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ConditionalObservation<T> {
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

/// The system families this inventory admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SystemFamily {
    Linux,
    Darwin,
}

/// The processor architectures this inventory admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
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
pub struct ReportedIdentifier<T> {
    pub identity: T,
    pub method: VersionedIdentity,
}

/// The compiler and backend a measured build was produced with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Toolchain {
    pub compiler: VersionedIdentity,
    #[serde(deserialize_with = "Option::deserialize")]
    pub commit: Option<Token>,
    pub backend: VersionedIdentity,
}

/// The effective build settings a measured artifact was produced under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildConfiguration {
    pub profile: Token,
    pub optimization: Token,
    pub assertions: bool,
    pub link_mode: VersionedIdentity,
    pub features: Vec<Token>,
}

/// Where a run executed. An uncontrolled local machine is not a qualified runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RunnerContext {
    LocalUncontrolled {},
}

/// An actual clock acquisition fact, not a guessed precision or a semantic output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClockObservation {
    pub provider: String,
    pub provider_revision: String,
    pub clock: String,
    pub resolution_nanoseconds: Quantity,
    pub resolution_source: String,
    pub conversion: String,
}

/// What a harness installed while measuring, and what it did not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Instrumentation {
    pub descriptor: VersionedIdentity,
    pub timing: Token,
    pub allocation: Token,
    pub trace: Token,
    pub profiling: Token,
    #[serde(rename = "externalInstrumentation")]
    pub external: Token,
}

/// How much concurrency a run actually used, and under which policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Concurrency {
    pub descriptor: VersionedIdentity,
    pub scope: Token,
    pub processes: Repetitions,
    pub calling_threads: Repetitions,
    pub internal_workers: Quantity,
    pub policy: Token,
    pub external_activity: Token,
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
pub enum Datum {
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

impl Datum {
    /// Return which registered field this observation is about.
    #[must_use]
    pub const fn field(&self) -> Field {
        match self {
            Self::OsFamily(_) => Field::OsFamily,
            Self::OsVersion(_) => Field::OsVersion,
            Self::KernelBuild(_) => Field::KernelBuild,
            Self::CpuArchitecture(_) => Field::CpuArchitecture,
            Self::TargetTriple(_) => Field::TargetTriple,
            Self::RunnerContext(_) => Field::RunnerContext,
            Self::ExecutionKind(_) => Field::ExecutionKind,
            Self::ProcessorClass(_) => Field::ProcessorClass,
            Self::LogicalCpuCount(_) => Field::LogicalCpuCount,
            Self::MemoryCapacityClass(_) => Field::MemoryCapacityClass,
            Self::PowerThermalPolicy(_) => Field::PowerThermalPolicy,
            Self::LanguageRuntime(_) => Field::LanguageRuntime,
            Self::Browser(_) => Field::Browser,
            Self::VirtualMachine(_) => Field::VirtualMachine,
            Self::Device(_) => Field::Device,
            Self::JitGcConfiguration(_) => Field::JitGcConfiguration,
            Self::Toolchain(_) => Field::Toolchain,
            Self::BuildConfiguration(_) => Field::BuildConfiguration,
            Self::Instrumentation(_) => Field::Instrumentation,
            Self::Allocator(_) => Field::Allocator,
            Self::MemoryObserver(_) => Field::MemoryObserver,
            Self::ClockOrSampler(_) => Field::ClockOrSampler,
            Self::Concurrency(_) => Field::Concurrency,
            Self::ContainerEmulatorSimulator(_) => Field::ContainerEmulatorSimulator,
            Self::LocaleService(_) => Field::LocaleService,
            Self::Harness(_) => Field::Harness,
            Self::Projection(_) => Field::Projection,
        }
    }

    fn reasons(&self) -> Option<&[Reason]> {
        macro_rules! observed {
            ($value:expr) => {
                match $value {
                    Observation::Unavailable { reasons } => Some(reasons.as_slice()),
                    Observation::Observed { .. } => None,
                }
            };
        }
        macro_rules! conditional {
            ($value:expr) => {
                match $value {
                    ConditionalObservation::Unavailable { reasons } => Some(reasons.as_slice()),
                    ConditionalObservation::Observed { .. }
                    | ConditionalObservation::NotApplicable { .. } => None,
                }
            };
        }
        match self {
            Self::OsVersion(v)
            | Self::ExecutionKind(v)
            | Self::ProcessorClass(v)
            | Self::MemoryCapacityClass(v) => observed!(v),
            Self::OsFamily(v) => observed!(v),
            Self::CpuArchitecture(v) => observed!(v),
            Self::LogicalCpuCount(v) => observed!(v),
            Self::Toolchain(v) => observed!(v),
            Self::BuildConfiguration(v) => observed!(v),
            Self::Concurrency(v) => observed!(v),
            Self::TargetTriple(v) => conditional!(v),
            Self::KernelBuild(v)
            | Self::PowerThermalPolicy(v)
            | Self::LanguageRuntime(v)
            | Self::Browser(v)
            | Self::VirtualMachine(v)
            | Self::Device(v)
            | Self::JitGcConfiguration(v)
            | Self::Allocator(v)
            | Self::MemoryObserver(v)
            | Self::ContainerEmulatorSimulator(v)
            | Self::LocaleService(v) => conditional!(v),
            Self::ClockOrSampler(v) => conditional!(v),
            Self::RunnerContext(_)
            | Self::Instrumentation(_)
            | Self::Harness(_)
            | Self::Projection(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    local_record_identity: Token,
    observation: Datum,
}

/// The complete environment-field inventory of one measured run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Environment {
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
        // Both the field and its local identifier are fixed per position.
        // Leaving the identifier open would let a document pass validation and
        // then fail admission, which reconstructs the exact inventory.
        let fields = Field::ALL.map(|field| {
            serde_json::json!({"allOf": [entry, {"properties": {
                "localRecordIdentity": {"const": local_identity(field)},
                "observation": {"properties": {"field": {"const": field.name()}}}
            }}]})
        });
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

/// The local identifier 026 gives one field inside its inventory.
fn local_identity(field: Field) -> String {
    format!("environment-field-{}", field.name())
}

/// Return whether reasons are canonical and belong to this exact field.
///
/// A reason that is well formed but selects another field, or another record,
/// explains nothing about the field it was filed under. Shape alone is
/// therefore not enough to call an absence explained.
fn bound_reasons(reasons: &[Reason], parent: &RecordIdentity, field: Field) -> bool {
    valid_reasons(reasons)
        && reasons.iter().all(|reason| {
            matches!(
                reason.affected(),
                Selector::EnvironmentField {
                    record_identity,
                    field: selected,
                } if record_identity == parent && *selected == field
            )
        })
}

/// Build the reasons one absent field carries.
///
/// The owner chooses the cause, because only the owner knows why it could not
/// observe the field; the selector and the record it belongs to are 026's.
#[must_use]
pub fn missing(parent: &RecordIdentity, field: Field, cause: Missing) -> Vec<Reason> {
    vec![Reason::missing_observation(
        Selector::EnvironmentField {
            record_identity: parent.clone(),
            field,
        },
        cause,
    )]
}

/// Complete failure to assemble the field inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryFailure {
    /// An observation was supplied for the wrong field, or out of order.
    FieldOrder,
    /// An absent field's reasons do not belong to that field and record.
    ReasonBinding,
    /// A local identifier could not be rendered.
    Identity(IdentityFailure),
}
impl From<IdentityFailure> for InventoryFailure {
    fn from(value: IdentityFailure) -> Self {
        Self::Identity(value)
    }
}

impl Environment {
    /// Retain one complete inventory in 026's registered field order.
    ///
    /// The observations arrive in that exact order and are checked against it,
    /// so an owner cannot report one field's state under another field's name.
    pub fn new(
        parent: &RecordIdentity,
        observations: [Datum; 27],
    ) -> Result<Self, InventoryFailure> {
        let mut fields = Vec::with_capacity(observations.len());
        for (observation, field) in observations.into_iter().zip(Field::ALL) {
            if observation.field() != field {
                return Err(InventoryFailure::FieldOrder);
            }
            if let Some(reasons) = observation.reasons() {
                if !bound_reasons(reasons, parent, field) {
                    return Err(InventoryFailure::ReasonBinding);
                }
            }
            fields.push(Entry {
                local_record_identity: Token::new(&local_identity(field))?,
                observation,
            });
        }
        Ok(Self {
            field_registry: VersionedIdentity::new("intlify-design-026-environment-fields", "0")?,
            fields,
        })
    }

    /// Borrow the local identifier of every field in the inventory.
    pub fn local_identities(&self) -> impl Iterator<Item = &Token> {
        self.fields.iter().map(|field| &field.local_record_identity)
    }

    /// Return whether every unavailable field explains its own absence.
    ///
    /// A submitted inventory is checked here, so the reasons must be canonical
    /// and must select the field and record they were filed under.
    #[must_use]
    pub fn reasons_are_valid(&self, parent: &RecordIdentity) -> bool {
        self.fields.iter().all(|entry| {
            entry
                .observation
                .reasons()
                .is_none_or(|reasons| bound_reasons(reasons, parent, entry.observation.field()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::CommonDomain;

    fn parent() -> RecordIdentity {
        RecordIdentity::fresh(CommonDomain::Record).unwrap()
    }

    // Generic over the field's own value type: each field admits its own, so a
    // single closure cannot stand in for all of them.
    fn absent<T>(parent: &RecordIdentity, field: Field) -> Observation<T> {
        Observation::Unavailable {
            reasons: missing(parent, field, Missing::NotCollected),
        }
    }

    fn conditional_absent<T>(parent: &RecordIdentity, field: Field) -> ConditionalObservation<T> {
        ConditionalObservation::Unavailable {
            reasons: missing(parent, field, Missing::NotCollected),
        }
    }

    fn complete(parent: &RecordIdentity) -> [Datum; 27] {
        let rule = || VersionedIdentity::literal("intlify-measurement-test-rule", "0");
        [
            Datum::OsFamily(absent(parent, Field::OsFamily)),
            Datum::OsVersion(absent(parent, Field::OsVersion)),
            Datum::KernelBuild(conditional_absent(parent, Field::KernelBuild)),
            Datum::CpuArchitecture(absent(parent, Field::CpuArchitecture)),
            Datum::TargetTriple(conditional_absent(parent, Field::TargetTriple)),
            Datum::RunnerContext(RequiredObservation::Observed {
                value: RunnerContext::LocalUncontrolled {},
            }),
            Datum::ExecutionKind(absent(parent, Field::ExecutionKind)),
            Datum::ProcessorClass(absent(parent, Field::ProcessorClass)),
            Datum::LogicalCpuCount(absent(parent, Field::LogicalCpuCount)),
            Datum::MemoryCapacityClass(absent(parent, Field::MemoryCapacityClass)),
            Datum::PowerThermalPolicy(conditional_absent(parent, Field::PowerThermalPolicy)),
            Datum::LanguageRuntime(ConditionalObservation::NotApplicable {
                applicability_rule: rule(),
            }),
            Datum::Browser(conditional_absent(parent, Field::Browser)),
            Datum::VirtualMachine(conditional_absent(parent, Field::VirtualMachine)),
            Datum::Device(conditional_absent(parent, Field::Device)),
            Datum::JitGcConfiguration(conditional_absent(parent, Field::JitGcConfiguration)),
            Datum::Toolchain(absent(parent, Field::Toolchain)),
            Datum::BuildConfiguration(absent(parent, Field::BuildConfiguration)),
            Datum::Instrumentation(RequiredObservation::Observed {
                value: Instrumentation {
                    descriptor: VersionedIdentity::literal("intlify-measurement-test-harness", "0"),
                    timing: Token::literal("enabled"),
                    allocation: Token::literal("not-installed"),
                    trace: Token::literal("not-installed"),
                    profiling: Token::literal("not-installed"),
                    external: Token::literal("not-attested"),
                },
            }),
            Datum::Allocator(conditional_absent(parent, Field::Allocator)),
            Datum::MemoryObserver(conditional_absent(parent, Field::MemoryObserver)),
            Datum::ClockOrSampler(conditional_absent(parent, Field::ClockOrSampler)),
            Datum::Concurrency(absent(parent, Field::Concurrency)),
            Datum::ContainerEmulatorSimulator(conditional_absent(
                parent,
                Field::ContainerEmulatorSimulator,
            )),
            Datum::LocaleService(conditional_absent(parent, Field::LocaleService)),
            Datum::Harness(RequiredObservation::Observed {
                value: VersionedIdentity::literal("intlify-measurement-test-harness", "0"),
            }),
            Datum::Projection(RequiredObservation::Observed {
                value: VersionedIdentity::literal("intlify-measurement-test-projection", "0"),
            }),
        ]
    }

    #[test]
    fn the_inventory_keeps_every_registered_field_in_its_registered_order() {
        let parent = parent();
        let environment = Environment::new(&parent, complete(&parent)).unwrap();
        assert!(environment.reasons_are_valid(&parent));
        let value = serde_json::to_value(&environment).unwrap();
        let fields = value["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 27);
        for (entry, field) in fields.iter().zip(Field::ALL) {
            assert_eq!(entry["observation"]["field"], field.name());
            assert_eq!(
                entry["localRecordIdentity"],
                format!("environment-field-{}", field.name())
            );
        }
    }

    #[test]
    fn the_generated_schema_pins_each_position_the_way_admission_does() {
        // Admission reconstructs the exact inventory and compares it, so a
        // schema that leaves a position's identifier open would admit records
        // that cannot survive admission.
        let parent = parent();
        let schema = crate::schema::draft7_schema::<Environment>().unwrap();
        let validator = jsonschema::draft7::new(&schema).unwrap();
        let environment = Environment::new(&parent, complete(&parent)).unwrap();
        let value = serde_json::to_value(&environment).unwrap();
        assert!(
            validator.is_valid(&value),
            "{:?}",
            validator.iter_errors(&value).collect::<Vec<_>>()
        );

        let mut renamed = value.clone();
        renamed["fields"][1]["localRecordIdentity"] =
            serde_json::json!("environment-field-renamed");
        assert!(!validator.is_valid(&renamed));

        // Another registered field's identifier is not a free spelling either:
        // each position carries exactly its own.
        let mut borrowed = value;
        borrowed["fields"][1]["localRecordIdentity"] =
            serde_json::json!("environment-field-kernel_build");
        assert!(!validator.is_valid(&borrowed));
    }

    #[test]
    fn an_observation_reported_under_another_field_is_rejected() {
        let parent = parent();
        let mut swapped = complete(&parent);
        swapped.swap(1, 2);
        assert_eq!(
            Environment::new(&parent, swapped).map(|_| ()),
            Err(InventoryFailure::FieldOrder)
        );
    }

    #[test]
    fn an_absent_field_keeps_its_reason_rather_than_a_weaker_value() {
        let parent = parent();
        let mut fields = complete(&parent);
        // An empty reason list is not a valid way to report an absence.
        fields[1] = Datum::OsVersion(Observation::Unavailable {
            reasons: Vec::new(),
        });
        assert_eq!(
            Environment::new(&parent, fields).map(|_| ()),
            Err(InventoryFailure::ReasonBinding)
        );
    }

    #[test]
    fn a_reason_must_explain_the_field_and_record_it_was_filed_under() {
        let parent = parent();

        // Well formed, but about another field: it explains nothing about the
        // field whose absence it was filed under.
        let mut wrong_field = complete(&parent);
        wrong_field[1] = Datum::OsVersion(Observation::Unavailable {
            reasons: missing(&parent, Field::KernelBuild, Missing::NotCollected),
        });
        assert_eq!(
            Environment::new(&parent, wrong_field).map(|_| ()),
            Err(InventoryFailure::ReasonBinding)
        );

        // Well formed and about the right field, but about another record.
        let other = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        let mut wrong_record = complete(&parent);
        wrong_record[1] = Datum::OsVersion(Observation::Unavailable {
            reasons: missing(&other, Field::OsVersion, Missing::NotCollected),
        });
        assert_eq!(
            Environment::new(&parent, wrong_record).map(|_| ()),
            Err(InventoryFailure::ReasonBinding)
        );

        // A submitted inventory is checked the same way, so a decoded document
        // cannot carry a binding that construction would have refused.
        let environment = Environment::new(&parent, complete(&parent)).unwrap();
        assert!(environment.reasons_are_valid(&parent));
        assert!(
            !environment.reasons_are_valid(&other),
            "the reasons belong to the record they name"
        );
    }
}
