// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! One observational projection from an already executed owner run.
//!
//! Nothing here recaptures a duration, and a submitted document never becomes
//! its own input authority: the retained run is what a submission is checked
//! against. Producing these records is not admission either — integrity,
//! binding, inventory, and projection eligibility remain separate checks, and
//! the catalog below performs them over the finished bytes.

mod catalog;
mod evaluate;
mod resolve;

pub use catalog::{validate, validate_records, Validation, ValidationFailure};

use serde::Serialize;

use crate::encoding::EncodingFailure;
use crate::identity::{CommonDomain, IdentityFailure, RecordIdentity};
use crate::measurement::{Evaluation, Evidence, Report};
use crate::owner::{OwnerFailure, OwnerRun};
use crate::record::{EvaluationKind, EvidenceKind, ReportKind};

pub(crate) use resolve::Resolved;

// Private reader capacity for one produced record, not a project-wide limit.
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;
const MAX_OWNER_INPUTS: usize = 8;

/// Complete failure to produce the common records of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// The owner could not answer about its own run.
    Owner(OwnerFailure),
    /// An instance identity could not be issued.
    Identity(IdentityFailure),
    /// A record could not be canonically encoded.
    Encoding(EncodingFailure),
    /// A record could not be serialized.
    Serialization,
    /// The Plan and the owner's answers disagree about the inventory.
    Plan,
    /// A produced record exceeds the reader capacity.
    Capacity,
    /// The produced records did not survive their own admission.
    Admission(ValidationFailure),
}
impl From<IdentityFailure> for Failure {
    fn from(value: IdentityFailure) -> Self {
        Self::Identity(value)
    }
}
impl From<EncodingFailure> for Failure {
    fn from(value: EncodingFailure) -> Self {
        Self::Encoding(value)
    }
}
impl From<OwnerFailure> for Failure {
    fn from(value: OwnerFailure) -> Self {
        Self::Owner(value)
    }
}

/// Completed record bytes only.
///
/// No Plan object, partially filled record, or caller-controlled measured
/// callback crosses this boundary.
#[derive(Clone)]
pub struct Artifacts {
    /// The report's own instance identity.
    pub report_identity: RecordIdentity,
    /// The issued Run Plan.
    pub run_plan: Vec<u8>,
    /// The owner's own result documents.
    pub owner_result: Vec<Vec<u8>>,
    /// The Evidence Set, when any case was projection-eligible.
    pub evidence: Option<Vec<u8>>,
    /// The Run Evaluation.
    pub evaluation: Vec<u8>,
    /// The structured report.
    pub report: Vec<u8>,
}

fn encode(record: &impl Serialize) -> Result<Vec<u8>, Failure> {
    let bytes = serde_json::to_vec(record).map_err(|_| Failure::Serialization)?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(Failure::Capacity);
    }
    Ok(bytes)
}

/// Produce the common records from one run and its own owner document.
pub fn produce<O: OwnerRun>(run: &O) -> Result<Artifacts, Failure> {
    let native = run.encode()?;
    produce_with_owner(run, &[&native])
}

/// Produce the common records from one run and the documents submitted for it.
pub fn produce_with_owner<O: OwnerRun>(
    run: &O,
    owner_inputs: &[&[u8]],
) -> Result<Artifacts, Failure> {
    if owner_inputs.len() > MAX_OWNER_INPUTS
        || owner_inputs
            .iter()
            .any(|input| input.len() > MAX_RECORD_BYTES)
    {
        return Err(Failure::Capacity);
    }
    let plan = run.plan();
    let projections = run.projections();
    if projections.len() != plan.body.case_inventory.len() {
        return Err(Failure::Plan);
    }
    for (projection, planned) in projections.iter().zip(&plan.body.case_inventory) {
        if crate::plan::case_identity(projection).map_err(|_| Failure::Plan)?
            != planned.case_identity
        {
            return Err(Failure::Plan);
        }
    }
    let resolved = resolve::resolve(run, owner_inputs);
    let evidence = match &resolved.admitted {
        Some(admitted) => {
            let identity = RecordIdentity::fresh(CommonDomain::Record)?;
            match run.evidence(admitted, &identity)? {
                Some(body) => Some(Evidence::seal(
                    EvidenceKind::Value,
                    identity,
                    &run.producing_tool(),
                    body,
                )?),
                None => None,
            }
        }
        None => None,
    };
    let evaluation = Evaluation::seal(
        EvaluationKind::Value,
        RecordIdentity::fresh(CommonDomain::Record)?,
        &run.producing_tool(),
        evaluate::evaluate(run, &resolved, evidence.as_ref())?,
    )?;
    let report = Report::seal(
        ReportKind::Value,
        RecordIdentity::fresh(CommonDomain::Record)?,
        &run.producing_tool(),
        evaluate::report::<O>(&evaluation, evidence.as_ref())?,
    )?;
    let artifacts = Artifacts {
        report_identity: report.identity().clone(),
        run_plan: encode(plan)?,
        owner_result: owner_inputs.iter().map(|input| input.to_vec()).collect(),
        evidence: evidence.as_ref().map(encode).transpose()?,
        evaluation: encode(&evaluation)?,
        report: encode(&report)?,
    };
    validate(run, &artifacts).map_err(Failure::Admission)?;
    Ok(artifacts)
}
