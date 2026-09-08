// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The adopted 026 Run Plan is issued before capture from owner-selected inputs.
//! Revalidation needs that issuance authority; a self-consistent submitted digest
//! does not select a new profile, inventory, build, or runner. Evidence and Run
//! Evaluation must later resolve this exact Plan, not manufacture another one.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::benchmark::cases::registry::Registry;
use crate::benchmark::cases::{LimitEdge, LimitKind, Recipe, Selector};
use crate::benchmark::context::CaptureContext;
use crate::benchmark::descriptor::{Boundary, Execution, Method};
use crate::benchmark::observation::Digest as NativeDigest;
use crate::benchmark::quantity::Quantity;
use crate::benchmark::work::LogicalWork;

use super::encoding::{self, Domain, EncodingFailure};
use super::identity::{
    CaseIdentity, IdentityFailure, InstanceDomain, IntegrityDigest, RecordIdentity, Token,
    VersionedIdentity,
};

const MAX_PLAN_BYTES: usize = 1024 * 1024;

pub(in crate::benchmark) fn record_schema() -> Result<serde_json::Value, serde_json::Error> {
    crate::schema::draft7_schema::<RunPlanRecord>()
}

pub(in crate::benchmark) fn case_schema() -> Result<serde_json::Value, serde_json::Error> {
    crate::schema::draft7_schema::<CaseIdentityInput<'static>>()
}

macro_rules! literal_type {
    ($name:ident, $value:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        enum $name {
            #[serde(rename = $value)]
            Value,
        }
    };
}

literal_type!(RunPlanKind, "measurement-run-plan");
literal_type!(RevisionZero, "0");
literal_type!(Required, "required");
literal_type!(SubjectKind, "toolchain-component");
literal_type!(Category, "toolchain");
literal_type!(OperationClass, "component");
literal_type!(Surface, "core");
literal_type!(Metric, "wall_duration");
literal_type!(Aggregation, "batch_total");
literal_type!(Scale, "fixed-owner-recipe");
literal_type!(ExecutionModel, "same-process-synchronous-ordinary-core");
literal_type!(Concurrency, "sequential-cases-calling-thread-no-workers");
literal_type!(NativeAlgorithm, "blake3");
literal_type!(NativeFraming, "intlify-config-minimum-observation/0");
literal_type!(NativeBuildSchema, "intlify-config-build-observation/0");
literal_type!(NativeBuildDomain, "build-observation");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Subject {
    kind: SubjectKind,
    identity: Token,
}

impl Subject {
    fn minimum() -> Self {
        Self {
            kind: SubjectKind::Value,
            identity: Token::literal("intlify-config-minimum-project-profile"),
        }
    }
}

/// A typed native-owner fragment. Recipe/limit/selector tags retain their owner
/// spelling and revision; they are not a new generic common selector language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Variant {
    recipe: Recipe,
    #[serde(deserialize_with = "Option::deserialize")]
    limit: Option<(LimitKind, LimitEdge)>,
    selector: Selector,
}

/// Exactly the applicable 026 Case dimensions. No source path, build revision,
/// clock observation, run/record ID, expected result checksum, or sample value.
/// Artifact/Target/Release/service-profile and memory-domain fields do not apply
/// to these six internal duration boundaries and are absent, not guessed.
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

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CaseIdentityInput<'a> {
    governing_specification: VersionedIdentity,
    identity_schema_revision: RevisionZero,
    projection: &'a CaseProjection,
}

impl CaseProjection {
    fn all(registry: &Registry, profile_revision: &str) -> Result<Vec<Self>, PlanFailure> {
        registry
            .expectations()
            .iter()
            .map(|expected| {
                let declaration = expected.declaration();
                let operation = declaration.operation;
                Ok(Self {
                    owner_identity: Token::literal("intlify-config"),
                    owner_result_schema_revision: Token::literal("1"),
                    owner_benchmark_profile_revision: Token::new(profile_revision)?,
                    owner_phase: operation.phase().into(),
                    owner_cost: operation.cost().into(),
                    category: Category::Value,
                    operation_class: OperationClass::Value,
                    performance_surface: Surface::Value,
                    interval_boundary: Boundary::for_operation(operation),
                    fixture: VersionedIdentity::new(
                        "intlify-config-minimum-fixtures",
                        &declaration.fixture_revision,
                    )?,
                    variant: Variant {
                        recipe: declaration.fixture,
                        limit: declaration.limit,
                        selector: declaration.selector,
                    },
                    scale: Scale::Value,
                    verification_subject: Subject::minimum(),
                    execution_model: ExecutionModel::Value,
                    execution_state: Execution::prepared_core(operation),
                    concurrency: Concurrency::Value,
                    workload: expected.work().clone(),
                    metric: Metric::Value,
                    measurement_method: Method::monotonic_invocation(),
                    sample_aggregation: Aggregation::Value,
                    measurement_profile_revision: Token::new(profile_revision)?,
                })
            })
            .collect()
    }

    pub(in crate::benchmark) fn identity(&self) -> Result<CaseIdentity, PlanFailure> {
        let value = serde_json::to_value(CaseIdentityInput {
            governing_specification: VersionedIdentity::specification(),
            identity_schema_revision: RevisionZero::Value,
            projection: self,
        })
        .map_err(|_| PlanFailure::Serialization)?;
        Ok(CaseIdentity::from_hash(encoding::hash(
            Domain::MeasurementCase,
            &value,
        )?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InventoryEntry {
    local_record_identity: Token,
    case_identity: CaseIdentity,
    requirement: Required,
}

/// This is a content binding to the actual native Build Observation, not a
/// common record ID, complete compiler attestation, or executable digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BuildIdentity {
    owner_schema: NativeBuildSchema,
    algorithm: NativeAlgorithm,
    framing: NativeFraming,
    domain: NativeBuildDomain,
    checksum: NativeDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Body {
    measurement_run: RecordIdentity,
    measurement_profile: VersionedIdentity,
    verification_subject: Subject,
    build_identity: BuildIdentity,
    case_inventory: Vec<InventoryEntry>,
    #[serde(deserialize_with = "Option::deserialize")]
    planned_runner_class: Option<Token>,
    runner_instance_identity: RecordIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreationContext {
    created_at_unix_nanoseconds: Quantity,
}

fn present<'de, D: Deserializer<'de>>(decoder: D) -> Result<Option<CreationContext>, D::Error> {
    CreationContext::deserialize(decoder).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    record_kind: RunPlanKind,
    record_schema_revision: RevisionZero,
    governing_specification: VersionedIdentity,
    record_identity: RecordIdentity,
    integrity_digest: IntegrityDigest,
    producing_tool: VersionedIdentity,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "CreationContext")]
    creation_context: Option<CreationContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::benchmark) struct RunPlanRecord {
    envelope: Envelope,
    body: Body,
}

impl RunPlanRecord {
    fn digest(&self) -> Result<IntegrityDigest, PlanFailure> {
        Ok(IntegrityDigest::from_hash(encoding::record_hash(self)?))
    }
    pub(in crate::benchmark) fn identity(&self) -> &RecordIdentity {
        &self.envelope.record_identity
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) enum PlanFailure {
    Identity(IdentityFailure),
    Encoding(EncodingFailure),
    Serialization,
    CaseIdentityCollision,
    InvalidRecord,
    SizeLimit,
}
impl From<IdentityFailure> for PlanFailure {
    fn from(value: IdentityFailure) -> Self {
        Self::Identity(value)
    }
}
impl From<EncodingFailure> for PlanFailure {
    fn from(value: EncodingFailure) -> Self {
        Self::Encoding(value)
    }
}

/// No Deserialize, Clone or mutable access. This retained object is issuance
/// authority, not an admission decision derived from a submitted Plan's body.
pub(in crate::benchmark) struct IssuedRunPlan {
    record: RunPlanRecord,
    projections: Vec<CaseProjection>,
}

impl IssuedRunPlan {
    pub(in crate::benchmark) fn issue(
        registry: &Registry,
        context: &CaptureContext,
    ) -> Result<Self, PlanFailure> {
        let (profile_identity, profile_revision) = context.profile().identity_revision();
        let projections = CaseProjection::all(registry, profile_revision)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut case_inventory = Vec::new();
        for (index, projection) in projections.iter().enumerate() {
            let case_identity = projection.identity()?;
            if !seen.insert(case_identity.clone()) {
                return Err(PlanFailure::CaseIdentityCollision);
            }
            case_inventory.push(InventoryEntry {
                local_record_identity: Token::new(&format!("inventory-case-{index}"))?,
                case_identity,
                requirement: Required::Value,
            });
        }
        let mut record = RunPlanRecord {
            envelope: Envelope {
                record_kind: RunPlanKind::Value,
                record_schema_revision: RevisionZero::Value,
                governing_specification: VersionedIdentity::specification(),
                record_identity: RecordIdentity::fresh(InstanceDomain::Record)?,
                integrity_digest: IntegrityDigest::from_hash([0; 32]),
                producing_tool: VersionedIdentity::new(
                    "intlify-config-minimum-measurement",
                    env!("CARGO_PKG_VERSION"),
                )?,
                creation_context: None,
            },
            body: Body {
                measurement_run: RecordIdentity::fresh(InstanceDomain::Run)?,
                measurement_profile: VersionedIdentity::new(profile_identity, profile_revision)?,
                verification_subject: Subject::minimum(),
                build_identity: BuildIdentity {
                    owner_schema: NativeBuildSchema::Value,
                    algorithm: NativeAlgorithm::Value,
                    framing: NativeFraming::Value,
                    domain: NativeBuildDomain::Value,
                    checksum: context.build_checksum(),
                },
                case_inventory,
                planned_runner_class: None,
                runner_instance_identity: RecordIdentity::fresh(
                    InstanceDomain::LocalRunnerInstance,
                )?,
            },
        };
        record.envelope.integrity_digest = record.digest()?;
        Ok(Self {
            record,
            projections,
        })
    }

    pub(in crate::benchmark) fn document(&self) -> &RunPlanRecord {
        &self.record
    }
    pub(in crate::benchmark) fn projections(&self) -> &[CaseProjection] {
        &self.projections
    }
    pub(in crate::benchmark) fn encode(&self) -> Result<Vec<u8>, PlanFailure> {
        let bytes = serde_json::to_vec(&self.record).map_err(|_| PlanFailure::Serialization)?;
        if bytes.len() > MAX_PLAN_BYTES {
            return Err(PlanFailure::SizeLimit);
        }
        Ok(bytes)
    }
    pub(in crate::benchmark) fn decode_checked(
        &self,
        bytes: &[u8],
    ) -> Result<RunPlanRecord, PlanFailure> {
        if bytes.len() > MAX_PLAN_BYTES {
            return Err(PlanFailure::SizeLimit);
        }
        let record: RunPlanRecord =
            serde_json::from_slice(bytes).map_err(|_| PlanFailure::InvalidRecord)?;
        // Equality binds all body fields, order, local IDs, domains, tool and
        // schema/specification revisions, creation context, and instance IDs.
        if record != self.record || record.digest()? != record.envelope.integrity_digest {
            return Err(PlanFailure::InvalidRecord);
        }
        Ok(record)
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
