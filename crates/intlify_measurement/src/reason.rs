// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Revision-zero reason vocabulary for the observational evaluator.
//! Ordering is semantic (including numeric quantities), never serialized JSON.

use std::cmp::Ordering;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use intlify_shared_json::quantity::Quantity;

use crate::identity::{CaseIdentity, RecordIdentity};
use crate::record::Reference;

/// The stage of the pipeline a reason was raised in.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    InputAdmission,
    Plan,
    Binding,
    Inventory,
    CaseResult,
    Evidence,
    Environment,
    Reporting,
}

macro_rules! cause_codes {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// The registered common cause codes.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub enum CommonCode { $(#[serde(rename = $name)] $variant),+ }
        impl CommonCode {
            fn name(self) -> &'static str { match self { $(Self::$variant => $name),+ } }
        }
    };
}
cause_codes! {
    MissingEvidence => "missing-evidence",
    MissingRequiredCase => "missing-required-case",
    SkippedRequiredCase => "skipped-required-case",
    UnsupportedMeasurement => "unsupported-measurement",
    FailedInvocation => "failed-invocation",
    ProjectionIneligible => "projection-ineligible",
    StaleEvidence => "stale-evidence",
    InsufficientSamples => "insufficient-samples",
    SchemaInvalid => "schema-invalid",
    IntegrityDigestMismatch => "integrity-digest-mismatch",
    InconsistentRecord => "inconsistent-record",
    DuplicateCase => "duplicate-case",
    UnknownCase => "unknown-case",
    InvalidStateCombination => "invalid-state-combination",
    InvalidProfile => "invalid-profile",
    AmbiguousBinding => "ambiguous-binding",
    CaseDimensionMismatch => "case-dimension-mismatch",
    EnvironmentMismatch => "environment-mismatch",
    MethodMismatch => "method-mismatch",
    IntervalMismatch => "interval-mismatch",
    SamplingPolicyMismatch => "sampling-policy-mismatch",
    SemanticObservationMismatch => "semantic-observation-mismatch",
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Code {
    Common { code: CommonCode },
}

/// Declaration order is the exact 026 environment registry order, not alphabetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentField {
    OsFamily,
    OsVersion,
    KernelBuild,
    CpuArchitecture,
    TargetTriple,
    RunnerContext,
    ExecutionKind,
    ProcessorClass,
    LogicalCpuCount,
    MemoryCapacityClass,
    PowerThermalPolicy,
    LanguageRuntime,
    Browser,
    VirtualMachine,
    Device,
    JitGcConfiguration,
    Toolchain,
    BuildConfiguration,
    Instrumentation,
    Allocator,
    MemoryObserver,
    ClockOrSampler,
    Concurrency,
    ContainerEmulatorSimulator,
    LocaleService,
    Harness,
    Projection,
}
impl EnvironmentField {
    /// Return the exact registered spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OsFamily => "os_family",
            Self::OsVersion => "os_version",
            Self::KernelBuild => "kernel_build",
            Self::CpuArchitecture => "cpu_architecture",
            Self::TargetTriple => "target_triple",
            Self::RunnerContext => "runner_context",
            Self::ExecutionKind => "execution_kind",
            Self::ProcessorClass => "processor_class",
            Self::LogicalCpuCount => "logical_cpu_count",
            Self::MemoryCapacityClass => "memory_capacity_class",
            Self::PowerThermalPolicy => "power_thermal_policy",
            Self::LanguageRuntime => "language_runtime",
            Self::Browser => "browser",
            Self::VirtualMachine => "virtual_machine",
            Self::Device => "device",
            Self::JitGcConfiguration => "jit_gc_configuration",
            Self::Toolchain => "toolchain",
            Self::BuildConfiguration => "build_configuration",
            Self::Instrumentation => "instrumentation",
            Self::Allocator => "allocator",
            Self::MemoryObserver => "memory_observer",
            Self::ClockOrSampler => "clock_or_sampler",
            Self::Concurrency => "concurrency",
            Self::ContainerEmulatorSimulator => "container_emulator_simulator",
            Self::LocaleService => "locale_service",
            Self::Harness => "harness",
            Self::Projection => "projection",
        }
    }

    /// The complete registered inventory, in 026's declaration order.
    pub const ALL: [Self; 27] = [
        Self::OsFamily,
        Self::OsVersion,
        Self::KernelBuild,
        Self::CpuArchitecture,
        Self::TargetTriple,
        Self::RunnerContext,
        Self::ExecutionKind,
        Self::ProcessorClass,
        Self::LogicalCpuCount,
        Self::MemoryCapacityClass,
        Self::PowerThermalPolicy,
        Self::LanguageRuntime,
        Self::Browser,
        Self::VirtualMachine,
        Self::Device,
        Self::JitGcConfiguration,
        Self::Toolchain,
        Self::BuildConfiguration,
        Self::Instrumentation,
        Self::Allocator,
        Self::MemoryObserver,
        Self::ClockOrSampler,
        Self::Concurrency,
        Self::ContainerEmulatorSimulator,
        Self::LocaleService,
        Self::Harness,
        Self::Projection,
    ];
}
impl Ord for EnvironmentField {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name().as_bytes().cmp(other.name().as_bytes())
    }
}
impl PartialOrd for EnvironmentField {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The complete applicable Build fields one record reports on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BuildField {
    SourceContent,
    Implementation,
    Executable,
    DependencyLock,
    BuildConfiguration,
    PhysicalEngine,
    SourceControlState,
}
impl BuildField {
    fn name(self) -> &'static str {
        match self {
            Self::SourceContent => "source-content",
            Self::Implementation => "implementation",
            Self::Executable => "executable",
            Self::DependencyLock => "dependency-lock",
            Self::BuildConfiguration => "build-configuration",
            Self::PhysicalEngine => "physical-engine",
            Self::SourceControlState => "source-control-state",
        }
    }
}
impl Ord for BuildField {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name().as_bytes().cmp(other.name().as_bytes())
    }
}
impl PartialOrd for BuildField {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// What exactly a reason is about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Selector {
    Record {
        reference: Reference,
    },
    PlannedCase {
        run_plan: RecordIdentity,
        case_identity: CaseIdentity,
    },
    EnvironmentField {
        record_identity: RecordIdentity,
        field: EnvironmentField,
    },
    BuildField {
        record_identity: RecordIdentity,
        field: BuildField,
    },
}

/// Why an observation is absent. Absence is never reported as a value.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum MissingObservation {
    NotCollected,
    NativeAcquisitionUnavailable,
    EffectiveBuildNotAttested,
    ExecutableNotAttested,
    KernelReleaseIsNotBuildIdentity,
    ParallelismHintIsNotLogicalCpuCount,
    FiniteProviderHasNoLocaleServiceProfile,
    SourceControlStateNotAttested,
}

/// How exactly a measured invocation failed.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum InvocationFailure {
    MeasurementOverflow,
    CounterOverflow,
    RepetitionOverflow,
    DurationConversionOverflow,
    ClockFailure,
    InvocationPanicked,
    PrerequisiteUnavailable,
    CollectorAllocation,
    OutputFailure,
    ObservationPanicked,
    WorkObservationFailure,
}

/// The typed payload of a reason.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Detail {
    MissingObservation {
        cause: MissingObservation,
    },
    MissingInput {},
    UnsupportedTuple {},
    InvalidInput {},
    DuplicateInput {},
    BindingMismatch {},
    InventoryMismatch {},
    InvocationFailed {
        subtype: InvocationFailure,
        diagnostic: Reference,
    },
    InvalidOwnerCase {
        diagnostic: Reference,
    },
    SemanticMismatch {
        diagnostic: Reference,
    },
    SampleCount {
        required: Quantity,
        available: Quantity,
    },
    ProjectionMismatch {},
    ReportMismatch {},
}

/// One complete reason: code, stage, what it affects, and its typed payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reason {
    code: Code,
    stage: Stage,
    affected: Selector,
    related: Vec<Reference>,
    detail: Detail,
}

impl Reason {
    /// Retain one reason under a registered common code.
    #[must_use]
    pub fn new(code: CommonCode, stage: Stage, affected: Selector, detail: Detail) -> Self {
        Self {
            code: Code::Common { code },
            stage,
            affected,
            related: Vec::new(),
            detail,
        }
    }

    /// Retain the reason an observation is absent rather than valued.
    #[must_use]
    pub fn missing_observation(affected: Selector, cause: MissingObservation) -> Self {
        Self::new(
            CommonCode::MissingEvidence,
            Stage::Environment,
            affected,
            Detail::MissingObservation { cause },
        )
    }

    /// Borrow what this reason is about.
    #[must_use]
    pub const fn affected(&self) -> &Selector {
        &self.affected
    }

    /// Affected selectors can name missing or rejected inputs. Diagnostic and
    /// related references, in contrast, promise retained resolvable records.
    pub fn references(&self) -> impl Iterator<Item = &Reference> {
        let diagnostic = match &self.detail {
            Detail::InvocationFailed { diagnostic, .. }
            | Detail::InvalidOwnerCase { diagnostic }
            | Detail::SemanticMismatch { diagnostic } => Some(diagnostic),
            _ => None,
        };
        self.related.iter().chain(diagnostic)
    }

    /// A common cause cannot carry the payload or meaning of an unrelated cause.
    #[must_use]
    pub fn valid(&self) -> bool {
        use CommonCode as C;
        use Detail as D;
        let Code::Common { code } = self.code;
        matches!(
            (code, &self.detail),
            (
                C::MissingEvidence,
                D::MissingObservation { .. } | D::MissingInput {}
            ) | (
                C::MissingRequiredCase | C::SkippedRequiredCase | C::StaleEvidence,
                D::MissingInput {}
            ) | (C::UnsupportedMeasurement, D::UnsupportedTuple {})
                | (C::FailedInvocation, D::InvocationFailed { .. })
                | (C::InsufficientSamples, D::SampleCount { .. })
                | (
                    C::SchemaInvalid
                        | C::IntegrityDigestMismatch
                        | C::InvalidStateCombination
                        | C::InvalidProfile,
                    D::InvalidInput {}
                )
                | (
                    C::InconsistentRecord,
                    D::InvalidOwnerCase { .. } | D::BindingMismatch {} | D::ReportMismatch {}
                )
                | (
                    C::AmbiguousBinding,
                    D::DuplicateInput {} | D::BindingMismatch {}
                )
                | (C::DuplicateCase | C::UnknownCase, D::InventoryMismatch {})
                | (
                    C::ProjectionIneligible
                        | C::CaseDimensionMismatch
                        | C::EnvironmentMismatch
                        | C::MethodMismatch
                        | C::IntervalMismatch
                        | C::SamplingPolicyMismatch,
                    D::ProjectionMismatch {}
                )
                | (C::SemanticObservationMismatch, D::SemanticMismatch { .. })
        )
    }
}

impl Ord for Reason {
    fn cmp(&self, other: &Self) -> Ordering {
        let Code::Common { code: left } = self.code;
        let Code::Common { code: right } = other.code;
        // Only common codes are activated: namespace and owner identity/revision
        // are equal/absent. Add no owner-specific success or free-text fallback.
        self.stage
            .cmp(&other.stage)
            .then_with(|| left.name().as_bytes().cmp(right.name().as_bytes()))
            .then_with(|| self.affected.cmp(&other.affected))
            .then_with(|| self.related.cmp(&other.related))
            .then_with(|| self.detail.cmp(&other.detail))
    }
}
impl PartialOrd for Reason {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Put reasons in their canonical order and remove exact duplicates.
#[must_use]
pub fn ordered(mut reasons: Vec<Reason>) -> Vec<Reason> {
    reasons.sort();
    reasons.dedup();
    reasons
}

/// Return whether a reason list is non-empty, well formed, and canonical.
#[must_use]
pub fn valid_reasons(reasons: &[Reason]) -> bool {
    !reasons.is_empty()
        && reasons.iter().all(Reason::valid)
        && reasons.windows(2).all(|pair| pair[0] < pair[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{OwnerRecordIdentity, Token};

    #[test]
    fn canonical_reason_order_uses_stage_then_code_then_typed_detail_not_json_text() {
        let id: RecordIdentity = serde_json::from_value(serde_json::json!({
            "domain":"intlify-verification-record-v0", "value":"0".repeat(64)
        }))
        .unwrap();
        let affected = Selector::Record {
            reference: Reference::top(&id),
        };
        let small = Reason::new(
            CommonCode::InsufficientSamples,
            Stage::Evidence,
            affected.clone(),
            Detail::SampleCount {
                required: Quantity::new(2),
                available: Quantity::new(1),
            },
        );
        let large = Reason::new(
            CommonCode::InsufficientSamples,
            Stage::Evidence,
            affected.clone(),
            Detail::SampleCount {
                required: Quantity::new(10),
                available: Quantity::new(1),
            },
        );
        assert!(small < large);
        let earlier_stage = Reason::new(
            CommonCode::UnknownCase,
            Stage::Inventory,
            affected.clone(),
            Detail::InventoryMismatch {},
        );
        let earlier_code = Reason::new(
            CommonCode::EnvironmentMismatch,
            Stage::Evidence,
            affected,
            Detail::ProjectionMismatch {},
        );
        let expected = vec![
            earlier_stage.clone(),
            earlier_code.clone(),
            small.clone(),
            large.clone(),
        ];
        for offset in 0..4 {
            let mut permutation = expected.clone();
            permutation.rotate_left(offset);
            permutation.reverse();
            permutation.push(small.clone());
            assert_eq!(ordered(permutation), expected);
        }
        assert!(valid_reasons(&expected));
        assert!(!valid_reasons(&[]));
        assert!(!valid_reasons(&[small.clone(), small]));
        assert!(EnvironmentField::Allocator < EnvironmentField::OsFamily);
        assert!(BuildField::BuildConfiguration < BuildField::SourceContent);
        // Instance identities and local identifiers compare by wire bytes, so an
        // owner's domain does not sort after every common one by construction.
        let owner = OwnerRecordIdentity::fresh("intlify-config-owner-result-v1").unwrap();
        assert!(Reference::top(&owner) < Reference::top(&id));
        assert!(
            Reference::nested(&id, Token::literal("sample-10"))
                < Reference::nested(&id, Token::literal("sample-2"))
        );
        let mut incompatible = large;
        incompatible.detail = Detail::MissingInput {};
        assert!(!incompatible.valid());
    }
}
