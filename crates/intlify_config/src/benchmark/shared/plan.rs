// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! This owner's Measurement Case projection and its Run Plan issuance.
//!
//! The Plan record, the inventory rule, and the Measurement Case identity
//! belong to `intlify_measurement`. What this module owns is the projection
//! itself: the fixtures, interval boundary, workload, and method that make one
//! case this owner's rather than another's.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use intlify_measurement::measurement::{
    Aggregation, Category, CommonDimensions, Metric, OperationClass, Surface,
};
use intlify_measurement::memory::MemoryObservationDomain;
use intlify_measurement::plan::{self, CaseProjection as CommonProjection, Issuance, SubjectKind};

pub(super) use intlify_measurement::plan::{BuildIdentity, Subject};

pub(in crate::benchmark) use intlify_measurement::plan::{
    case_identity, PlanFailure, RunPlanRecord,
};

use crate::benchmark::cases::registry::{FixtureExpectation, Registry};
use crate::benchmark::cases::{EntryKind, LimitEdge, LimitKind, Recipe, ResolverState, Selector};
use crate::benchmark::context::CaptureContext;
use crate::benchmark::descriptor::{prepared_core, Boundary, Execution, Method};
use crate::benchmark::instrument::InstrumentDescription;
use crate::benchmark::memory_domain;
use crate::benchmark::operation::PeakMemory;

use super::identity::{
    CaseIdentity, NativeChecksum, OwnerLabel, OwnerRecordIdentity, Token, VersionedIdentity,
    LOCAL_RUNNER_DOMAIN,
};
use super::record::producing_tool;
use crate::benchmark::work::LogicalWork;

pub(in crate::benchmark) fn record_schema() -> Result<serde_json::Value, serde_json::Error> {
    plan::record_schema()
}

pub(in crate::benchmark) fn case_schema() -> Result<serde_json::Value, serde_json::Error> {
    plan::case_identity_schema::<CaseProjection>()
}

macro_rules! literal_type {
    ($name:ident, $value:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub(super) enum $name {
            #[serde(rename = $value)]
            Value,
        }
    };
}

literal_type!(Scale, "fixed-owner-recipe");
literal_type!(ExecutionModel, "same-process-synchronous-ordinary-core");
literal_type!(Concurrency, "sequential-cases-calling-thread-no-workers");

/// Design 026's common grouping of the costs of one of design 015's phases:
/// their category, operation class, and Performance Surface (D26:2804-2813).
/// The complete invocation's costs are workflows on the product workflow
/// surface, and the peak memory's are on the core surface whatever interval
/// they observe; every other phase of 015's is a component on the core
/// surface.
fn grouping(phase: &str) -> (Category, OperationClass, Surface) {
    match phase {
        "profile_resolve_e2e" => (
            Category::Toolchain,
            OperationClass::Workflow,
            Surface::ProductWorkflow,
        ),
        "profile_resolve_peak_memory" => {
            (Category::Memory, OperationClass::PeakLive, Surface::Core)
        }
        _ => (
            Category::Toolchain,
            OperationClass::Component,
            Surface::Core,
        ),
    }
}

fn subject() -> Subject {
    Subject {
        kind: SubjectKind::Value,
        identity: Token::literal("intlify-config-minimum-project-profile"),
    }
}

// A dimension that applies to only some cases is absent from the others, not
// null: an explicit null is refused, and the cases it does not apply to keep
// exactly the bytes, and so the identities, they had before it existed.
pub(in crate::benchmark) fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}

/// A typed native-owner fragment. Recipe/limit/selector tags retain their owner
/// spelling and revision; they are not a new generic common selector language.
/// The entry kind and the resolver state appear only on the cases they apply to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(transform = intlify_measurement::schema::require_nullable_members)]
struct Variant {
    recipe: Recipe,
    #[serde(deserialize_with = "Option::deserialize")]
    limit: Option<(LimitKind, LimitEdge)>,
    selector: Selector,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "EntryKind")]
    entry_kind: Option<EntryKind>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "ResolverState")]
    resolver: Option<ResolverState>,
}

/// Exactly the applicable 026 Case dimensions. No source path, build revision,
/// clock observation, run/record ID, expected result checksum, or sample value.
/// Artifact/Target/Release/service-profile fields do not apply to these internal
/// boundaries and are absent, not guessed. The Memory Observation Domain applies
/// only to a memory case, so a duration case has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::benchmark) struct CaseProjection {
    owner_identity: Token,
    owner_result_schema_revision: Token,
    owner_benchmark_profile_revision: Token,
    owner_phase: String,
    owner_cost: String,
    category: Category,
    operation_class: OperationClass,
    performance_surface: Surface,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "MemoryObservationDomain")]
    memory_observation_domain: Option<MemoryObservationDomain>,
    interval_boundary: Boundary,
    fixture: VersionedIdentity,
    variant: Variant,
    scale: Scale,
    verification_subject: Subject,
    execution_model: ExecutionModel,
    execution_state: Execution,
    concurrency: Concurrency,
    // Complete, closed, versioned owner workload fragment; not its checksum.
    workload: LogicalWork,
    metric: Metric,
    measurement_method: Method,
    sample_aggregation: Aggregation,
    measurement_profile_revision: Token,
}

impl CommonProjection for CaseProjection {
    fn phase(&self) -> &str {
        &self.owner_phase
    }
    fn cost(&self) -> &str {
        &self.owner_cost
    }
    fn common(&self) -> CommonDimensions<'_> {
        CommonDimensions {
            category: self.category,
            operation_class: self.operation_class,
            surface: self.performance_surface,
            memory_domain: self.memory_observation_domain.as_ref(),
            metric: self.metric,
            aggregation: self.sample_aggregation,
        }
    }
}

impl CaseProjection {
    pub(super) fn phase_cost(&self) -> (&str, &str) {
        (&self.owner_phase, &self.owner_cost)
    }

    pub(in crate::benchmark) fn identity(&self) -> Result<CaseIdentity, PlanFailure> {
        case_identity(self)
    }

    /// The projections of the cases a run measuring with the instrument
    /// plans: a duration run every row, and a memory run each row whose
    /// operation has a memory boundary.
    fn all(
        registry: &Registry,
        profile_revision: &str,
        instrument: InstrumentDescription,
    ) -> Result<Vec<Self>, PlanFailure> {
        registry
            .expectations()
            .iter()
            .filter_map(|expected| match instrument {
                InstrumentDescription::Clock(_) => Some(Self::duration(expected, profile_revision)),
                InstrumentDescription::Observer(observer) => {
                    let operation = expected.declaration().operation;
                    let memory = operation.peak_memory()?;
                    let boundary = Boundary::for_memory(operation)?;
                    let method = Method::observed_peak(observer);
                    Some(
                        Self::duration(expected, profile_revision)
                            .map(|duration| Self::memory(duration, memory, boundary, method)),
                    )
                }
            })
            .collect()
    }

    fn duration(
        expected: &FixtureExpectation,
        profile_revision: &str,
    ) -> Result<Self, PlanFailure> {
        let declaration = expected.declaration();
        let operation = declaration.operation;
        let (category, operation_class, performance_surface) = grouping(operation.phase());
        Ok(Self {
            owner_identity: Token::literal("intlify-config"),
            owner_result_schema_revision: Token::literal("0"),
            owner_benchmark_profile_revision: Token::new(profile_revision)?,
            owner_phase: operation.phase().into(),
            owner_cost: operation.cost().into(),
            category,
            operation_class,
            performance_surface,
            memory_observation_domain: None,
            interval_boundary: Boundary::for_operation(operation),
            fixture: VersionedIdentity::new(
                "intlify-config-minimum-fixtures",
                &declaration.fixture_revision,
            )?,
            variant: Variant {
                recipe: declaration.fixture,
                limit: declaration.limit,
                selector: declaration.selector,
                entry_kind: declaration.fixture.entry_kind(),
                resolver: declaration.fixture.resolver_state(),
            },
            scale: Scale::Value,
            verification_subject: subject(),
            execution_model: ExecutionModel::Value,
            execution_state: prepared_core(
                operation,
                declaration.fixture.icu4x_engine(),
                declaration.fixture.resolver_state(),
            ),
            concurrency: Concurrency::Value,
            workload: expected.work().clone(),
            metric: Metric::WallDuration,
            measurement_method: Method::monotonic_invocation(),
            sample_aggregation: Aggregation::BatchTotal,
            measurement_profile_revision: Token::new(profile_revision)?,
        })
    }

    /// The peak live memory of a case's interval: the same fixture, workload,
    /// and execution state as its duration, and its variant with the entry
    /// kind the memory cost does not name (D15:2792), under the memory pair
    /// and its grouping, the observer's method, and the domain of what it
    /// observes: an invocation of a resolver in the case's state, or a
    /// construction, whose case names no resolver state (D15:2805).
    fn memory(duration: Self, memory: PeakMemory, boundary: Boundary, method: Method) -> Self {
        let (category, operation_class, performance_surface) = grouping(memory.phase);
        let domain = match duration.variant.resolver {
            Some(resolver) => memory_domain::invocation(resolver),
            None => memory_domain::construction(),
        };
        let variant = Variant {
            entry_kind: memory.entry_kind,
            ..duration.variant.clone()
        };
        Self {
            owner_phase: memory.phase.into(),
            owner_cost: memory.cost.into(),
            category,
            operation_class,
            performance_surface,
            memory_observation_domain: Some(domain),
            interval_boundary: boundary,
            variant,
            metric: Metric::PeakLiveBytes,
            measurement_method: method,
            sample_aggregation: Aggregation::PeakOverBatch,
            ..duration
        }
    }
}

/// The issued Plan of this owner's projections.
pub(in crate::benchmark) type IssuedRunPlan = plan::IssuedRunPlan<CaseProjection>;

/// Issue one Plan for the cases this owner's registry declares.
pub(in crate::benchmark) fn issue(
    registry: &Registry,
    context: &CaptureContext,
) -> Result<IssuedRunPlan, PlanFailure> {
    let (profile_identity, profile_revision) = context.profile().identity_revision();
    plan::IssuedRunPlan::issue(Issuance {
        projections: CaseProjection::all(registry, profile_revision, context.instrument())?,
        measurement_profile: VersionedIdentity::new(profile_identity, profile_revision)?,
        verification_subject: subject(),
        build_identity: BuildIdentity {
            owner_schema: OwnerLabel::literal("intlify-config-build-observation/0"),
            algorithm: OwnerLabel::literal("blake3"),
            framing: OwnerLabel::literal("intlify-config-minimum-observation/0"),
            domain: OwnerLabel::literal("build-observation"),
            checksum: NativeChecksum::from_bytes(context.build_checksum().bytes()),
        },
        planned_runner_class: None,
        runner_instance_identity: OwnerRecordIdentity::fresh(LOCAL_RUNNER_DOMAIN)?,
        producing_tool: producing_tool(),
    })
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
