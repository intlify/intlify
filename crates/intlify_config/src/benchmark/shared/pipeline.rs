// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! One observational projection from an already executed owner run. It never
//! recaptures a duration or makes its submitted records the input authority.

use serde::Serialize;

use super::encoding::EncodingFailure;
use super::identity::{IdentityFailure, InstanceDomain, RecordIdentity};
use super::measurement::{CaseEvidence, Evaluation, Evidence, EvidenceBody, Report};
use super::record::{EvaluationKind, EvidenceKind, ReportKind};
use crate::benchmark::run::{RecordedRun, RunFailure};

mod catalog;
mod evaluate;
mod owner;

pub(in crate::benchmark) use catalog::{validate, validate_records, ValidationFailure};

#[derive(Debug)]
pub(in crate::benchmark) enum Failure {
    OwnerEncoding(RunFailure),
    Identity(IdentityFailure),
    Encoding(EncodingFailure),
    Json,
    Plan,
    Capacity,
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

/// Completed record bytes only. No Profile, locale core, partially filled
/// record, or caller-controlled measured callback crosses this boundary.
#[derive(Clone)]
pub(in crate::benchmark) struct Artifacts {
    pub(in crate::benchmark) report_identity: RecordIdentity,
    pub(in crate::benchmark) run_plan: Vec<u8>,
    pub(in crate::benchmark) owner_result: Vec<Vec<u8>>,
    pub(in crate::benchmark) evidence: Option<Vec<u8>>,
    pub(in crate::benchmark) evaluation: Vec<u8>,
    pub(in crate::benchmark) report: Vec<u8>,
}

fn encode(record: &impl Serialize) -> Result<Vec<u8>, Failure> {
    let bytes = serde_json::to_vec(record).map_err(|_| Failure::Json)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Failure::Capacity);
    }
    Ok(bytes)
}

pub(in crate::benchmark) fn produce(run: &RecordedRun) -> Result<Artifacts, Failure> {
    let native = run.encode().map_err(Failure::OwnerEncoding)?;
    produce_with_owner(run, &[&native])
}

pub(in crate::benchmark) fn produce_with_owner(
    run: &RecordedRun,
    native_inputs: &[&[u8]],
) -> Result<Artifacts, Failure> {
    if native_inputs.len() > 8
        || native_inputs
            .iter()
            .any(|input| input.len() > 16 * 1024 * 1024)
    {
        return Err(Failure::Capacity);
    }
    let plan = run.plan_record();
    let projections = run.common_plan().projections();
    if projections.len() != plan.body.case_inventory.len() {
        return Err(Failure::Plan);
    }
    let owner = owner::OwnerInput::resolve(run, native_inputs);
    let evidence: Option<Evidence> = if let Some(checked) = &owner.checked {
        let source = checked.projection_source();
        let mut cases = Vec::new();
        for ((attempt, planned), projection) in source
            .document()
            .result()
            .attempts
            .iter()
            .zip(&plan.body.case_inventory)
            .zip(projections)
        {
            if projection.identity().map_err(|_| Failure::Plan)? != planned.case_identity {
                return Err(Failure::Plan);
            }
            if let Some(case) =
                CaseEvidence::project(&source, attempt, &planned.case_identity, projection)?
            {
                cases.push(case);
            }
        }
        if cases.is_empty() {
            None
        } else {
            let identity = RecordIdentity::fresh(InstanceDomain::Record)?;
            match EvidenceBody::from_source(&source, plan, &identity, cases) {
                Ok(body) => Some(Evidence::seal(EvidenceKind::Value, identity, body)?),
                // A valid native observation whose identifiers cannot be mapped
                // losslessly is projection-ineligible, not a fabricated value.
                Err(IdentityFailure::InvalidToken) => None,
                Err(error) => return Err(error.into()),
            }
        }
    } else {
        None
    };
    let evaluation = Evaluation::seal(
        EvaluationKind::Value,
        RecordIdentity::fresh(InstanceDomain::Record)?,
        evaluate::evaluate(run, &owner, evidence.as_ref())?,
    )?;
    let report = Report::seal(
        ReportKind::Value,
        RecordIdentity::fresh(InstanceDomain::Record)?,
        evaluate::report(&evaluation, evidence.as_ref())?,
    )?;
    let artifacts = Artifacts {
        report_identity: report.identity().clone(),
        run_plan: run.common_plan().encode().map_err(|_| Failure::Plan)?,
        owner_result: native_inputs.iter().map(|input| input.to_vec()).collect(),
        evidence: evidence.as_ref().map(encode).transpose()?,
        evaluation: encode(&evaluation)?,
        report: encode(&report)?,
    };
    validate(run, &artifacts).map_err(Failure::Admission)?;
    Ok(artifacts)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
