// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Bounded resolution of finished records, and owner-backed admission.
//!
//! Everything here works on the produced bytes. A record that decodes and
//! verifies its own integrity has proved only that it is self-consistent: it
//! still has to resolve the exact Plan, the exact inventory, and the exact
//! owner document this run was issued for. A valid negative evaluation
//! resolves as a record, never as measured evidence.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::decode::{self, DecodeFailure};
use crate::encoding;
use crate::identity::{
    self, AnyIdentity, CaseIdentity, CommonDomain, IntegrityDigest, RecordIdentity, Token,
    VersionedIdentity,
};
use crate::measurement::{CaseResult, Evaluation, Evidence, InputState, Outcome, Report, Section};
use crate::owner::OwnerRun;
use crate::plan::RunPlanRecord;
use crate::reason::valid_reasons;
use crate::record::Reference;

use super::{evaluate, resolve, Artifacts, Resolved};

const MAX_RECORDS: usize = 8;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_OWNER_BYTES: usize = 16 * 1024 * 1024;

/// Why a set of produced records was not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationFailure {
    Decode(DecodeFailure),
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
impl From<DecodeFailure> for ValidationFailure {
    fn from(error: DecodeFailure) -> Self {
        Self::Decode(error)
    }
}

type OwnerEvidence<O> = Evidence<
    <O as OwnerRun>::Projection,
    <O as OwnerRun>::Descriptors,
    <O as OwnerRun>::Observation,
>;

enum Document<O: OwnerRun> {
    Plan(Box<RunPlanRecord>),
    Evidence(Box<OwnerEvidence<O>>),
    Evaluation(Box<Evaluation>),
    Report(Box<Report>),
}

impl<O: OwnerRun> Document<O> {
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
        fn unique<'a>(values: impl Iterator<Item = &'a CaseIdentity>) -> bool {
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

struct Catalog<O: OwnerRun> {
    documents: BTreeMap<RecordIdentity, Document<O>>,
}

impl<O: OwnerRun> Catalog<O> {
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
                .ok_or(DecodeFailure::Shape)?;
            let kind = envelope
                .get("recordKind")
                .and_then(Value::as_str)
                .ok_or(DecodeFailure::Shape)?;
            let revision: Token = decode::typed(
                envelope
                    .get("recordSchemaRevision")
                    .cloned()
                    .ok_or(DecodeFailure::Shape)?,
            )?;
            let governing: VersionedIdentity = decode::typed(
                envelope
                    .get("governingSpecification")
                    .cloned()
                    .ok_or(DecodeFailure::Shape)?,
            )?;
            if revision != Token::literal("0") || governing != identity::specification() {
                return Err(DecodeFailure::Unsupported.into());
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
                _ => return Err(DecodeFailure::Unsupported.into()),
            };
            if document.identity().domain() != CommonDomain::Record {
                return Err(DecodeFailure::Shape.into());
            }
            let submitted: IntegrityDigest = decode::typed(
                envelope
                    .get("integrityDigest")
                    .cloned()
                    .ok_or(DecodeFailure::Shape)?,
            )?;
            let digest = encoding::record_hash(&value).map_err(|_| DecodeFailure::Shape)?;
            if submitted != IntegrityDigest::from_hash(digest) {
                return Err(DecodeFailure::Integrity.into());
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

    fn common(&self, id: &RecordIdentity) -> Result<&Document<O>, ValidationFailure> {
        self.documents
            .get(id)
            .ok_or(ValidationFailure::MissingRecord)
    }

    fn record(&self, id: &AnyIdentity) -> Result<&Document<O>, ValidationFailure> {
        // Only a common record can be resolved in this catalog. An owner
        // instance is a valid reference target but is not one of these
        // documents, so it is resolved against the admitted owner input.
        id.as_common()
            .and_then(|identity| self.documents.get(identity))
            .ok_or(ValidationFailure::MissingRecord)
    }

    fn resolve(&self, reference: &Reference) -> Result<&Document<O>, ValidationFailure> {
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
        run: &O,
        resolved: &Resolved<O>,
        reference: &Reference,
    ) -> Result<(), ValidationFailure> {
        let parent = match reference {
            Reference::TopLevel { record_identity } => record_identity,
            Reference::NestedRecord { reference } => &reference.parent_record_identity,
        };
        let Some(owner_instance) = parent.as_owner() else {
            return self.resolve(reference).map(|_| ());
        };
        let admitted = resolved
            .admitted
            .as_ref()
            .ok_or(ValidationFailure::Reference)?;
        if owner_instance != run.result_identity(admitted) {
            return Err(ValidationFailure::Reference);
        }
        match reference {
            Reference::TopLevel { .. } => Ok(()),
            Reference::NestedRecord { .. } => {
                let count = run.outcomes(admitted).len();
                let matches = (0..count).any(|index| {
                    let ordinal = intlify_shared_json::quantity::Quantity::new(
                        u64::try_from(index).expect("pre-admitted case count"),
                    );
                    run.attempt_reference(admitted, ordinal).as_ref() == Ok(reference)
                });
                if matches {
                    Ok(())
                } else {
                    Err(ValidationFailure::Reference)
                }
            }
        }
    }
}

/// A successful admission may describe an incomplete or invalid run.
///
/// The counts are diagnostic. This profile has no numeric-decision facility,
/// so nothing downstream may compare them against a threshold.
#[derive(Debug, PartialEq, Eq)]
pub struct Validation {
    pub outcome: Outcome,
    pub planned_cases: usize,
    pub measured_cases: usize,
    pub non_measured_cases: usize,
}

/// Admit the records one run just produced.
pub fn validate<O: OwnerRun>(
    run: &O,
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

/// Admit a set of submitted records against the run that issued their Plan.
pub fn validate_records<O: OwnerRun>(
    run: &O,
    owner_inputs: &[&[u8]],
    common_inputs: &[&[u8]],
    report_identity: &RecordIdentity,
) -> Result<Validation, ValidationFailure> {
    if owner_inputs.len() > MAX_RECORDS
        || owner_inputs
            .iter()
            .any(|input| input.len() > MAX_OWNER_BYTES)
    {
        return Err(ValidationFailure::Capacity);
    }
    let catalog = Catalog::<O>::read(common_inputs)?;
    let Document::Plan(plan) = catalog.common(run.plan().identity())? else {
        return Err(ValidationFailure::WrongRecordKind);
    };
    if **plan != *run.plan() {
        return Err(ValidationFailure::Plan);
    }
    let Document::Report(report) = catalog.common(report_identity)? else {
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
    let resolved = resolve::resolve(run, owner_inputs);
    let evidence: Option<&OwnerEvidence<O>> = match evidence_sets.as_slice() {
        [] => {
            if let Some(admitted) = &resolved.admitted {
                // The parent here only tests lossless projection; no record is
                // minted. A producer cannot declare eligible measured cases
                // projection-ineligible simply by withholding the evidence.
                if run
                    .evidence(admitted, plan.identity())
                    .map_err(|_| ValidationFailure::Evidence)?
                    .is_some()
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
            let Some(admitted) = &resolved.admitted else {
                return Err(ValidationFailure::Evidence);
            };
            let expected = run
                .evidence(admitted, evidence.identity())
                .map_err(|_| ValidationFailure::Evidence)?
                .ok_or(ValidationFailure::Evidence)?;
            if evidence.body != expected {
                return Err(ValidationFailure::Evidence);
            }
            Some(evidence)
        }
        _ => return Err(ValidationFailure::Reference),
    };
    let expected =
        evaluate::evaluate(run, &resolved, evidence).map_err(|_| ValidationFailure::Evaluation)?;
    if evaluation.body != expected {
        return Err(ValidationFailure::Evaluation);
    }
    if evaluation.body.outcome != Outcome::Complete && !valid_reasons(&evaluation.body.reasons) {
        return Err(ValidationFailure::Evaluation);
    }
    if let InputState::Resolved { reference } = &evaluation.body.owner_result_input.result {
        catalog.resolve_supported(run, &resolved, reference)?;
    }
    for reason in &evaluation.body.reasons {
        for reference in reason.references() {
            catalog.resolve_supported(run, &resolved, reference)?;
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
                catalog.resolve_supported(run, &resolved, reference)?;
            }
        }
    }
    if let Some(evidence) = evidence {
        for case in &evidence.body.cases {
            catalog.resolve_supported(run, &resolved, &case.owner_attempt)?;
        }
    }
    let expected_report =
        evaluate::report::<O>(evaluation, evidence).map_err(|_| ValidationFailure::Report)?;
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
