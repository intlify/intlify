// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Bounded finite record resolution and native-backed semantic admission.
//! Valid negative evaluations resolve as records, never as measured evidence.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{evaluate, owner::OwnerInput, Artifacts};
use crate::benchmark::run::RecordedRun;
use crate::benchmark::shared::identity::{
    InstanceDomain, IntegrityDigest, RecordIdentity, Token, VersionedIdentity,
};
use crate::benchmark::shared::measurement::{
    native_attempt_reference, CaseEvidence, CaseResult, Evaluation, Evidence, EvidenceBody,
    InputState, Outcome, Report, Section,
};
use crate::benchmark::shared::plan::RunPlanRecord;
use crate::benchmark::shared::reason::valid_reasons;
use crate::benchmark::shared::record::Reference;
use crate::benchmark::shared::{decode, encoding};

const MAX_RECORDS: usize = 8;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) enum ValidationFailure {
    Decode(decode::DecodeFailure),
    Capacity,
    DuplicateRecord,
    IdentityConflict,
    DuplicateLocalIdentity,
    DuplicateCase,
    MissingRecord,
    WrongRecordKind,
    Plan,
    Evidence,
    Evaluation,
    Report,
    Reference,
}
impl From<decode::DecodeFailure> for ValidationFailure {
    fn from(error: decode::DecodeFailure) -> Self {
        Self::Decode(error)
    }
}

enum Document {
    Plan(Box<RunPlanRecord>),
    Evidence(Box<Evidence>),
    Evaluation(Box<Evaluation>),
    Report(Box<Report>),
}
impl Document {
    fn identity(&self) -> &RecordIdentity {
        match self {
            Self::Plan(v) => v.identity(),
            Self::Evidence(v) => v.identity(),
            Self::Evaluation(v) => v.identity(),
            Self::Report(v) => v.identity(),
        }
    }
    fn locals(&self) -> Vec<&Token> {
        match self {
            Self::Plan(value) => value
                .body
                .case_inventory
                .iter()
                .map(|entry| &entry.local_record_identity)
                .collect(),
            Self::Evidence(value) => value
                .body
                .environment
                .local_identities()
                .chain(value.body.cases.iter().flat_map(|case| {
                    std::iter::once(&case.local_record_identity).chain(
                        case.samples
                            .iter()
                            .map(|sample| &sample.local_record_identity),
                    )
                }))
                .collect(),
            Self::Evaluation(value) => value
                .body
                .case_inventory
                .iter()
                .map(|entry| &entry.local_record_identity)
                .chain(
                    value
                        .body
                        .cases
                        .iter()
                        .map(|case| &case.local_record_identity),
                )
                .collect(),
            Self::Report(value) => value
                .body
                .sections
                .iter()
                .flat_map(|section| {
                    let Section::MeasurementObservation {
                        local_record_identity,
                        rows,
                        missing_case_inventory,
                        ..
                    } = section;
                    std::iter::once(local_record_identity)
                        .chain(rows.iter().flat_map(|row| {
                            std::iter::once(&row.local_record_identity).chain(
                                row.samples
                                    .iter()
                                    .map(|sample| &sample.local_record_identity),
                            )
                        }))
                        .chain(
                            missing_case_inventory
                                .iter()
                                .map(|case| &case.local_record_identity),
                        )
                })
                .collect(),
        }
    }
    fn unique_cases(&self) -> bool {
        fn unique<'a>(
            values: impl Iterator<Item = &'a crate::benchmark::shared::identity::CaseIdentity>,
        ) -> bool {
            let mut seen = BTreeSet::new();
            values.into_iter().all(|id| seen.insert(id))
        }
        match self {
            Self::Plan(value) => unique(
                value
                    .body
                    .case_inventory
                    .iter()
                    .map(|entry| &entry.case_identity),
            ),
            Self::Evidence(value) => {
                unique(value.body.cases.iter().map(|entry| &entry.case_identity))
            }
            Self::Evaluation(value) => {
                unique(
                    value
                        .body
                        .case_inventory
                        .iter()
                        .map(|entry| &entry.case_identity),
                ) && unique(value.body.cases.iter().map(|entry| &entry.case_identity))
            }
            Self::Report(value) => value.body.sections.iter().all(|section| {
                let Section::MeasurementObservation {
                    rows,
                    missing_case_inventory,
                    ..
                } = section;
                unique(
                    rows.iter().map(|entry| &entry.case_identity).chain(
                        missing_case_inventory
                            .iter()
                            .map(|entry| &entry.case_identity),
                    ),
                )
            }),
        }
    }
}

struct Catalog {
    documents: BTreeMap<RecordIdentity, Document>,
}
impl Catalog {
    fn read(inputs: &[&[u8]]) -> Result<Self, ValidationFailure> {
        if inputs.len() > MAX_RECORDS {
            return Err(ValidationFailure::Capacity);
        }
        let total = inputs
            .iter()
            .try_fold(0_usize, |sum, input| sum.checked_add(input.len()))
            .ok_or(ValidationFailure::Capacity)?;
        if total > MAX_TOTAL_BYTES {
            return Err(ValidationFailure::Capacity);
        }
        let mut documents = BTreeMap::new();
        let mut canonical_values = BTreeMap::new();
        for input in inputs {
            // Future tuples are unsupported, not declared corrupt merely because
            // an unknown body might encode numbers differently. Known bodies are
            // strictly typed and framed only after exact tuple selection.
            let value = decode::value(input, false)?;
            let envelope = value
                .get("envelope")
                .and_then(Value::as_object)
                .ok_or(decode::DecodeFailure::Shape)?;
            let kind = envelope
                .get("recordKind")
                .and_then(Value::as_str)
                .ok_or(decode::DecodeFailure::Shape)?;
            let revision: Token = decode::typed(
                envelope
                    .get("recordSchemaRevision")
                    .cloned()
                    .ok_or(decode::DecodeFailure::Shape)?,
            )?;
            let governing: VersionedIdentity = decode::typed(
                envelope
                    .get("governingSpecification")
                    .cloned()
                    .ok_or(decode::DecodeFailure::Shape)?,
            )?;
            if revision != Token::literal("0") || governing != VersionedIdentity::specification() {
                return Err(decode::DecodeFailure::Unsupported.into());
            }
            let document = match kind {
                "measurement-run-plan" => Document::Plan(Box::new(decode::typed(value.clone())?)),
                "measurement-evidence-set" => {
                    Document::Evidence(Box::new(decode::typed(value.clone())?))
                }
                "measurement-run-evaluation" => {
                    Document::Evaluation(Box::new(decode::typed(value.clone())?))
                }
                "structured-report" => Document::Report(Box::new(decode::typed(value.clone())?)),
                _ => return Err(decode::DecodeFailure::Unsupported.into()),
            };
            if document.identity().domain() != InstanceDomain::Record {
                return Err(decode::DecodeFailure::Shape.into());
            }
            let submitted: IntegrityDigest = decode::typed(
                envelope
                    .get("integrityDigest")
                    .cloned()
                    .ok_or(decode::DecodeFailure::Shape)?,
            )?;
            let digest = encoding::record_hash(&value).map_err(|_| decode::DecodeFailure::Shape)?;
            if submitted != IntegrityDigest::from_hash(digest) {
                return Err(decode::DecodeFailure::Integrity.into());
            }
            let identity = document.identity().clone();
            if let Some(previous) = canonical_values.get(&identity) {
                return Err(if previous == &value {
                    ValidationFailure::DuplicateRecord
                } else {
                    ValidationFailure::IdentityConflict
                });
            }
            let locals = document.locals();
            if locals.iter().copied().collect::<BTreeSet<_>>().len() != locals.len() {
                return Err(ValidationFailure::DuplicateLocalIdentity);
            }
            if !document.unique_cases() {
                return Err(ValidationFailure::DuplicateCase);
            }
            canonical_values.insert(identity.clone(), value);
            documents.insert(identity, document);
        }
        Ok(Self { documents })
    }

    fn record(&self, id: &RecordIdentity) -> Result<&Document, ValidationFailure> {
        self.documents
            .get(id)
            .ok_or(ValidationFailure::MissingRecord)
    }
    fn resolve(&self, reference: &Reference) -> Result<&Document, ValidationFailure> {
        match reference {
            Reference::TopLevel { record_identity } => self.record(record_identity),
            Reference::NestedRecord { reference } => {
                let parent = self.record(&reference.parent_record_identity)?;
                if !parent.locals().contains(&&reference.local_record_identity) {
                    return Err(ValidationFailure::Reference);
                }
                Ok(parent)
            }
        }
    }

    fn resolve_supported(
        &self,
        owner: &OwnerInput,
        reference: &Reference,
    ) -> Result<(), ValidationFailure> {
        let parent = match reference {
            Reference::TopLevel { record_identity } => record_identity,
            Reference::NestedRecord { reference } => &reference.parent_record_identity,
        };
        if parent.domain() != InstanceDomain::NativeOwnerResult {
            return self.resolve(reference).map(|_| ());
        }
        let source = owner
            .checked
            .as_ref()
            .ok_or(ValidationFailure::Reference)?
            .document();
        if parent != &source.result().record_identity {
            return Err(ValidationFailure::Reference);
        }
        match reference {
            Reference::TopLevel { .. } => Ok(()),
            Reference::NestedRecord { .. }
                if source.result().attempts.iter().any(|attempt| {
                    native_attempt_reference(source, attempt.ordinal).as_ref() == Ok(reference)
                }) =>
            {
                Ok(())
            }
            Reference::NestedRecord { .. } => Err(ValidationFailure::Reference),
        }
    }
}

/// A successful admission may describe an incomplete/invalid measurement run.
/// Counts are diagnostic; this initial profile has no numeric-decision facility.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::benchmark) struct Validation {
    pub(in crate::benchmark) outcome: Outcome,
    pub(in crate::benchmark) planned_cases: usize,
    pub(in crate::benchmark) measured_cases: usize,
    pub(in crate::benchmark) non_measured_cases: usize,
}

pub(in crate::benchmark) fn validate(
    run: &RecordedRun,
    artifacts: &Artifacts,
) -> Result<Validation, ValidationFailure> {
    let mut records = vec![artifacts.run_plan.as_slice()];
    if let Some(evidence) = &artifacts.evidence {
        records.push(evidence);
    }
    records.push(&artifacts.evaluation);
    records.push(&artifacts.report);
    let owner_inputs = artifacts
        .owner_result
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    validate_records(run, &owner_inputs, &records, &artifacts.report_identity)
}

pub(in crate::benchmark) fn validate_records(
    run: &RecordedRun,
    native_inputs: &[&[u8]],
    common_inputs: &[&[u8]],
    report_identity: &RecordIdentity,
) -> Result<Validation, ValidationFailure> {
    if native_inputs.len() > MAX_RECORDS
        || native_inputs
            .iter()
            .any(|input| input.len() > 16 * 1024 * 1024)
    {
        return Err(ValidationFailure::Capacity);
    }
    let catalog = Catalog::read(common_inputs)?;
    let Document::Plan(plan) = catalog.record(run.plan_record().identity())? else {
        return Err(ValidationFailure::WrongRecordKind);
    };
    if **plan != *run.plan_record() {
        return Err(ValidationFailure::Plan);
    }
    let Document::Report(report) = catalog.record(report_identity)? else {
        return Err(ValidationFailure::WrongRecordKind);
    };
    let [Section::MeasurementObservation {
        run_evaluation,
        evidence_sets,
        ..
    }] = report.body.sections.as_slice()
    else {
        return Err(ValidationFailure::Report);
    };
    let Reference::TopLevel { .. } = run_evaluation else {
        return Err(ValidationFailure::Reference);
    };
    let Document::Evaluation(evaluation) = catalog.resolve(run_evaluation)? else {
        return Err(ValidationFailure::WrongRecordKind);
    };
    let owner = OwnerInput::resolve(run, native_inputs);
    let evidence: Option<&Evidence> = match evidence_sets.as_slice() {
        [] => {
            if let Some(checked) = &owner.checked {
                let source = checked.projection_source();
                let cases = source
                    .document()
                    .result()
                    .attempts
                    .iter()
                    .zip(&plan.body.case_inventory)
                    .zip(run.common_plan().projections())
                    .map(|((attempt, planned), projection)| {
                        CaseEvidence::project(&source, attempt, &planned.case_identity, projection)
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| ValidationFailure::Evidence)?
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                // The parent here is only used to test lossless projection; no
                // record is minted. A producer cannot declare eligible measured
                // cases projection-ineligible simply by withholding evidence.
                if !cases.is_empty()
                    && EvidenceBody::from_source(&source, plan, plan.identity(), cases).is_ok()
                {
                    return Err(ValidationFailure::MissingRecord);
                }
            }
            None
        }
        [reference @ Reference::TopLevel { .. }] => {
            let Document::Evidence(evidence) = catalog.resolve(reference)? else {
                return Err(ValidationFailure::WrongRecordKind);
            };
            let Some(checked) = &owner.checked else {
                return Err(ValidationFailure::Evidence);
            };
            let source = checked.projection_source();
            let mut expected_cases = Vec::new();
            for ((attempt, planned), projection) in source
                .document()
                .result()
                .attempts
                .iter()
                .zip(&plan.body.case_inventory)
                .zip(run.common_plan().projections())
            {
                if let Some(case) =
                    CaseEvidence::project(&source, attempt, &planned.case_identity, projection)
                        .map_err(|_| ValidationFailure::Evidence)?
                {
                    expected_cases.push(case);
                }
            }
            let expected =
                EvidenceBody::from_source(&source, plan, evidence.identity(), expected_cases)
                    .map_err(|_| ValidationFailure::Evidence)?;
            if expected.cases.is_empty()
                || evidence.body != expected
                || !evidence.body.build.validate(&source, evidence.identity())
                || !evidence
                    .body
                    .environment
                    .validate(&source, evidence.identity())
            {
                return Err(ValidationFailure::Evidence);
            }
            Some(evidence)
        }
        _ => return Err(ValidationFailure::Reference),
    };
    let expected =
        evaluate::evaluate(run, &owner, evidence).map_err(|_| ValidationFailure::Evaluation)?;
    if evaluation.body != expected {
        return Err(ValidationFailure::Evaluation);
    }
    if evaluation.body.outcome != Outcome::Complete && !valid_reasons(&evaluation.body.reasons) {
        return Err(ValidationFailure::Evaluation);
    }
    if let InputState::Resolved { reference } = &evaluation.body.owner_result_input.result {
        catalog.resolve_supported(&owner, reference)?;
    }
    for reason in &evaluation.body.reasons {
        for reference in reason.references() {
            catalog.resolve_supported(&owner, reference)?;
        }
    }
    for case in &evaluation.body.cases {
        if let CaseResult::Measured { evidence } = &case.result {
            if !matches!(catalog.resolve(evidence)?, Document::Evidence(_)) {
                return Err(ValidationFailure::Reference);
            }
        }
        if let CaseResult::Unavailable {
            diagnostic_partial_observations,
            ..
        } = &case.result
        {
            for reference in diagnostic_partial_observations {
                catalog.resolve_supported(&owner, reference)?;
            }
        }
    }
    if let Some(evidence) = evidence {
        for case in &evidence.body.cases {
            catalog.resolve_supported(&owner, &case.owner_attempt)?;
        }
    }
    let expected_report =
        evaluate::report(evaluation, evidence).map_err(|_| ValidationFailure::Report)?;
    if report.body != expected_report {
        return Err(ValidationFailure::Report);
    }
    let Section::MeasurementObservation {
        rows,
        missing_case_inventory,
        ..
    } = &report.body.sections[0];
    // Exact regenerated references are additionally resolved through the finite
    // catalog, including every sample and every negative case evaluation.
    for row in rows {
        catalog.resolve(&row.case_evaluation)?;
        catalog.resolve(&row.evidence)?;
        for sample in &row.samples {
            catalog.resolve(&sample.raw_sample)?;
        }
    }
    for case in missing_case_inventory {
        catalog.resolve(&case.evaluation)?;
    }
    Ok(Validation {
        outcome: evaluation.body.outcome,
        planned_cases: plan.body.case_inventory.len(),
        measured_cases: rows.len(),
        non_measured_cases: missing_case_inventory.len(),
    })
}
