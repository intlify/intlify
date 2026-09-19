// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The 026 Run Plan, issued before capture from owner-selected inputs.
//!
//! Revalidation needs that issuance authority. A self-consistent submitted
//! document does not select a new profile, inventory, build, or runner, and
//! Evidence and Run Evaluation must later resolve this exact Plan rather than
//! manufacture another one.

use std::collections::BTreeSet;
use std::fmt::Debug;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::encoding::{self, Domain, EncodingFailure};
use crate::identity::{
    CaseIdentity, CommonDomain, IdentityFailure, NativeChecksum, OwnerLabel, OwnerRecordIdentity,
    RecordIdentity, Token, VersionedIdentity,
};
use crate::record::{Record, RunPlanKind};

/// Private reader capacity for one Plan document, not a project-wide limit.
pub const MAX_PLAN_BYTES: usize = 1024 * 1024;

/// One owner's complete Measurement Case projection.
///
/// The projection is the owner's, because its dimensions name the owner's
/// fixtures, boundaries, workload, and method. What 026 owns is that the case
/// identity is a digest of exactly this value under the registered domain, so
/// two owners cannot produce the same identity for different work.
pub trait CaseProjection:
    Serialize + for<'de> Deserialize<'de> + JsonSchema + Clone + PartialEq + Debug
{
    /// Return the owner phase this case measures.
    fn phase(&self) -> &str;
    /// Return the owner cost this case measures.
    fn cost(&self) -> &str;
}

macro_rules! literal_type {
    ($name:ident, $value:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub enum $name {
            #[serde(rename = $value)]
            Value,
        }
    };
}

literal_type!(RevisionZero, "0", "The revision-zero identity schema.");
literal_type!(Required, "required", "A case an inventory cannot omit.");
literal_type!(
    SubjectKind,
    "toolchain-component",
    "A subject that is one component of a toolchain."
);

/// What exactly was measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Subject {
    pub kind: SubjectKind,
    pub identity: Token,
}

/// A content binding to an owner's actual Build Observation.
///
/// It is not a common record identity, a compiler attestation, or an
/// executable digest, so the framing that produced the checksum travels with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildIdentity {
    pub owner_schema: OwnerLabel,
    pub algorithm: OwnerLabel,
    pub framing: OwnerLabel,
    pub domain: OwnerLabel,
    pub checksum: NativeChecksum,
}

/// One planned case, named locally inside the Plan that declares it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InventoryEntry {
    pub local_record_identity: Token,
    pub case_identity: CaseIdentity,
    pub requirement: Required,
}

/// The complete Run Plan body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Body {
    pub measurement_run: RecordIdentity,
    pub measurement_profile: VersionedIdentity,
    pub verification_subject: Subject,
    pub build_identity: BuildIdentity,
    pub case_inventory: Vec<InventoryEntry>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub planned_runner_class: Option<Token>,
    pub runner_instance_identity: OwnerRecordIdentity,
}

/// The Run Plan is one common record; its envelope is not a second one.
pub type RunPlanRecord = Record<RunPlanKind, Body>;

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CaseIdentityInput<'a, P> {
    governing_specification: VersionedIdentity,
    identity_schema_revision: RevisionZero,
    projection: &'a P,
}

/// Compute one Measurement Case identity from a complete owner projection.
pub fn case_identity<P: Serialize>(projection: &P) -> Result<CaseIdentity, PlanFailure> {
    let value = serde_json::to_value(CaseIdentityInput {
        governing_specification: crate::identity::specification(),
        identity_schema_revision: RevisionZero::Value,
        projection,
    })
    .map_err(|_| PlanFailure::Serialization)?;
    Ok(CaseIdentity::from_hash(encoding::hash(
        Domain::MeasurementCase,
        &value,
    )?))
}

/// Build the complete preimage value one case identity is computed from.
///
/// Callers that check a generated schema against real projections need the
/// exact value that is hashed, not a reconstruction of it.
pub fn case_identity_input<P: Serialize>(
    projection: &P,
) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(CaseIdentityInput {
        governing_specification: crate::identity::specification(),
        identity_schema_revision: RevisionZero::Value,
        projection,
    })
}

/// Generate the complete input representation of a Measurement Case identity.
pub fn case_identity_schema<P: CaseProjection + 'static>(
) -> Result<serde_json::Value, serde_json::Error> {
    crate::schema::draft7_schema::<CaseIdentityInput<'static, P>>()
}

/// Generate the complete Run Plan representation.
pub fn record_schema() -> Result<serde_json::Value, serde_json::Error> {
    crate::schema::draft7_record_schema::<RunPlanRecord>("RunPlanRecord")
}

/// Complete failure of Plan issuance or admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanFailure {
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

/// The owner-selected inputs one Run Plan is issued from.
pub struct Issuance<P> {
    /// The complete case projections, in the order the owner will attempt them.
    pub projections: Vec<P>,
    /// The measurement profile this run is executed under.
    pub measurement_profile: VersionedIdentity,
    /// What exactly is measured.
    pub verification_subject: Subject,
    /// The content binding to the owner's actual Build Observation.
    pub build_identity: BuildIdentity,
    /// The runner class this run was planned for, when one was declared.
    pub planned_runner_class: Option<Token>,
    /// The per-run local harness instance, not a machine identity.
    pub runner_instance_identity: OwnerRecordIdentity,
    /// The tool that produces this record, which is the owner.
    pub producing_tool: VersionedIdentity,
}

/// No Deserialize, Clone or mutable access. This retained object is issuance
/// authority, not an admission decision derived from a submitted Plan's body.
pub struct IssuedRunPlan<P> {
    record: RunPlanRecord,
    projections: Vec<P>,
}

impl<P: CaseProjection> IssuedRunPlan<P> {
    /// Issue one Plan before any capture begins.
    pub fn issue(inputs: Issuance<P>) -> Result<Self, PlanFailure> {
        let mut seen = BTreeSet::new();
        let mut case_inventory = Vec::new();
        for (index, projection) in inputs.projections.iter().enumerate() {
            let identity = case_identity(projection)?;
            if !seen.insert(identity.clone()) {
                return Err(PlanFailure::CaseIdentityCollision);
            }
            case_inventory.push(InventoryEntry {
                local_record_identity: Token::new(&format!("inventory-case-{index}"))?,
                case_identity: identity,
                requirement: Required::Value,
            });
        }
        let record = RunPlanRecord::seal(
            RunPlanKind::Value,
            RecordIdentity::fresh(CommonDomain::Record)?,
            &inputs.producing_tool,
            Body {
                measurement_run: RecordIdentity::fresh(CommonDomain::Run)?,
                measurement_profile: inputs.measurement_profile,
                verification_subject: inputs.verification_subject,
                build_identity: inputs.build_identity,
                case_inventory,
                planned_runner_class: inputs.planned_runner_class,
                runner_instance_identity: inputs.runner_instance_identity,
            },
        )?;
        Ok(Self {
            record,
            projections: inputs.projections,
        })
    }

    /// Borrow the issued record.
    pub const fn document(&self) -> &RunPlanRecord {
        &self.record
    }

    /// Borrow the complete projections, in the planned order.
    pub fn projections(&self) -> &[P] {
        &self.projections
    }

    /// Serialize the issued record within the reader capacity.
    pub fn encode(&self) -> Result<Vec<u8>, PlanFailure> {
        let bytes = serde_json::to_vec(&self.record).map_err(|_| PlanFailure::Serialization)?;
        if bytes.len() > MAX_PLAN_BYTES {
            return Err(PlanFailure::SizeLimit);
        }
        Ok(bytes)
    }

    /// Admit a submitted Plan only when it is the exact one issued here.
    ///
    /// Equality binds every body field, its order, local identifiers, domains,
    /// tool and schema revisions, creation context, and instance identities, so
    /// a rehashed change is self-consistent and still rejected.
    pub fn decode_checked(&self, bytes: &[u8]) -> Result<RunPlanRecord, PlanFailure> {
        if bytes.len() > MAX_PLAN_BYTES {
            return Err(PlanFailure::SizeLimit);
        }
        let record: RunPlanRecord =
            serde_json::from_slice(bytes).map_err(|_| PlanFailure::InvalidRecord)?;
        if record != self.record || !record.verify_integrity()? {
            return Err(PlanFailure::InvalidRecord);
        }
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct TestProjection {
        owner_phase: String,
        owner_cost: String,
        fixture: Token,
    }
    impl CaseProjection for TestProjection {
        fn phase(&self) -> &str {
            &self.owner_phase
        }
        fn cost(&self) -> &str {
            &self.owner_cost
        }
    }

    fn projection(fixture: &str) -> TestProjection {
        TestProjection {
            owner_phase: "message_analysis".into(),
            owner_cost: "literal_encode".into(),
            fixture: Token::new(fixture).unwrap(),
        }
    }

    fn issuance(projections: Vec<TestProjection>) -> Issuance<TestProjection> {
        Issuance {
            projections,
            measurement_profile: VersionedIdentity::literal("intlify-measurement-test", "0"),
            verification_subject: Subject {
                kind: SubjectKind::Value,
                identity: Token::literal("intlify-measurement-test-subject"),
            },
            build_identity: BuildIdentity {
                owner_schema: OwnerLabel::literal("intlify-measurement-test-build/0"),
                algorithm: OwnerLabel::literal("blake3-256"),
                framing: OwnerLabel::literal("intlify-measurement-test-observation/0"),
                domain: OwnerLabel::literal("build-observation"),
                checksum: NativeChecksum::from_bytes([0x22; 32]),
            },
            planned_runner_class: None,
            runner_instance_identity: OwnerRecordIdentity::fresh(
                "intlify-measurement-test-runner-v0",
            )
            .unwrap(),
            producing_tool: VersionedIdentity::literal("intlify-measurement-test-tool", "0"),
        }
    }

    #[test]
    fn a_case_identity_is_a_digest_of_exactly_the_complete_projection() {
        let first = case_identity(&projection("one")).unwrap();
        assert_eq!(first, case_identity(&projection("one")).unwrap());
        assert_ne!(first, case_identity(&projection("two")).unwrap());
        // The governing specification is part of the preimage, so a bare
        // projection digest is not a Measurement Case identity.
        let bare = CaseIdentity::from_hash(
            encoding::hash(
                Domain::MeasurementCase,
                &serde_json::to_value(projection("one")).unwrap(),
            )
            .unwrap(),
        );
        assert_ne!(first, bare);
    }

    #[test]
    fn an_issued_plan_inventories_every_projection_in_the_planned_order() {
        let plan = IssuedRunPlan::issue(issuance(vec![
            projection("one"),
            projection("two"),
            projection("three"),
        ]))
        .unwrap();
        let inventory = &plan.document().body.case_inventory;
        assert_eq!(inventory.len(), 3);
        for (index, (entry, projected)) in inventory.iter().zip(plan.projections()).enumerate() {
            assert_eq!(
                entry.local_record_identity.as_str(),
                format!("inventory-case-{index}")
            );
            assert_eq!(entry.case_identity, case_identity(projected).unwrap());
        }
        assert!(plan.document().verify_integrity().unwrap());
    }

    #[test]
    fn two_cases_that_measure_the_same_work_cannot_both_be_planned() {
        assert_eq!(
            IssuedRunPlan::issue(issuance(vec![projection("one"), projection("one")])).map(|_| ()),
            Err(PlanFailure::CaseIdentityCollision)
        );
    }

    #[test]
    fn a_rehashed_change_is_self_consistent_and_still_not_the_issued_plan() {
        let plan =
            IssuedRunPlan::issue(issuance(vec![projection("one"), projection("two")])).unwrap();
        assert_eq!(
            plan.decode_checked(&plan.encode().unwrap()).unwrap(),
            *plan.document()
        );
        let original = serde_json::to_value(plan.document()).unwrap();
        for pointer in [
            "/envelope/producingTool/revision",
            "/body/measurementProfile/revision",
            "/body/caseInventory/0/localRecordIdentity",
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(pointer).unwrap() = json!("changed");
            let digest = crate::identity::IntegrityDigest::from_hash(
                encoding::record_hash(&changed).unwrap(),
            );
            changed["envelope"]["integrityDigest"] = serde_json::to_value(digest).unwrap();
            let bytes = serde_json::to_vec(&changed).unwrap();
            // The document still decodes and verifies its own integrity.
            let decoded: RunPlanRecord = serde_json::from_slice(&bytes).unwrap();
            assert!(decoded.verify_integrity().unwrap());
            assert_eq!(
                plan.decode_checked(&bytes),
                Err(PlanFailure::InvalidRecord),
                "{pointer}"
            );
        }
    }
}
