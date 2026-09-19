// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Evaluating one run against the inventory its Plan fixed, and reporting it.
//!
//! Every planned case appears in the result. A case that produced no
//! measurement is recorded with the reason it did not, because removing it
//! would turn an incomplete run into a complete one.

use intlify_shared_json::quantity::Quantity;

use crate::identity::Token;
use crate::measurement::{
    Aggregation, CaseEvaluation, CaseResult, Category, Evaluation, EvaluationBody, Evidence,
    InputState, Metric, MissingCase, ObservationalOnly, OperationClass, Outcome, ReportBody,
    ReportSample, Requirement, Row, RunBinding, Section, Surface, Truncation, UnavailableKind,
    Unit,
};
use crate::owner::{CaseOutcome, ObservedDescriptors, OwnerRun};
use crate::plan::CaseProjection;
use crate::reason::{ordered, CommonCode as C, Detail, Reason, Stage};
use crate::record::Reference;

use super::{Failure, Resolved};

type OwnerEvidence<O> = Evidence<
    <O as OwnerRun>::Projection,
    <O as OwnerRun>::Descriptors,
    <O as OwnerRun>::Observation,
>;

pub(super) fn evaluate<O: OwnerRun>(
    run: &O,
    resolved: &Resolved<O>,
    evidence: Option<&OwnerEvidence<O>>,
) -> Result<EvaluationBody, Failure> {
    let plan = run.plan();
    let outcomes = resolved
        .admitted
        .as_ref()
        .map(|admitted| run.outcomes(admitted));
    if let Some(outcomes) = &outcomes {
        if outcomes.len() != plan.body.case_inventory.len() {
            return Err(Failure::Plan);
        }
    }
    let mut cases = Vec::with_capacity(plan.body.case_inventory.len());
    for (index, planned) in plan.body.case_inventory.iter().enumerate() {
        let affected = CaseEvaluation::selector(plan, &planned.case_identity);
        let reason = |code, detail| {
            vec![Reason::new(
                code,
                Stage::CaseResult,
                affected.clone(),
                detail,
            )]
        };
        let result = if let (Some(admitted), Some(outcomes)) = (&resolved.admitted, &outcomes) {
            let ordinal = Quantity::new(u64::try_from(index).map_err(|_| Failure::Plan)?);
            let diagnostic = run.attempt_reference(admitted, ordinal)?;
            match outcomes[index] {
                CaseOutcome::Measured => {
                    if let Some((evidence, case)) = evidence.and_then(|record| {
                        record
                            .body
                            .cases
                            .iter()
                            .find(|case| case.case_identity == planned.case_identity)
                            .map(|case| (record, case))
                    }) {
                        CaseResult::Measured {
                            evidence: Reference::nested(
                                evidence.identity(),
                                case.local_record_identity.clone(),
                            ),
                        }
                    } else {
                        // A measured case whose identifiers cannot be mapped
                        // losslessly is projection-ineligible, not a fabricated
                        // value and not a silently dropped case.
                        CaseResult::Unavailable {
                            unavailable_kind: UnavailableKind::ProjectionIneligible,
                            reasons: reason(C::ProjectionIneligible, Detail::ProjectionMismatch {}),
                            diagnostic_partial_observations: vec![diagnostic],
                        }
                    }
                }
                CaseOutcome::Failed(subtype) => CaseResult::Unavailable {
                    unavailable_kind: UnavailableKind::Failed,
                    reasons: reason(
                        C::FailedInvocation,
                        Detail::InvocationFailed {
                            subtype,
                            diagnostic: diagnostic.clone(),
                        },
                    ),
                    diagnostic_partial_observations: vec![diagnostic],
                },
                CaseOutcome::SemanticMismatch => CaseResult::Invalid {
                    reasons: reason(
                        C::SemanticObservationMismatch,
                        Detail::SemanticMismatch { diagnostic },
                    ),
                },
                CaseOutcome::Invalid => CaseResult::Invalid {
                    reasons: reason(
                        C::InconsistentRecord,
                        Detail::InvalidOwnerCase { diagnostic },
                    ),
                },
            }
        } else if let Some(kind) = resolved.absence {
            let (code, detail) = match kind {
                UnavailableKind::Missing => (C::MissingRequiredCase, Detail::MissingInput {}),
                UnavailableKind::Unsupported => {
                    (C::UnsupportedMeasurement, Detail::UnsupportedTuple {})
                }
                _ => return Err(Failure::Plan),
            };
            CaseResult::Unavailable {
                unavailable_kind: kind,
                reasons: reason(code, detail),
                diagnostic_partial_observations: Vec::new(),
            }
        } else {
            let code = resolved.failure_code.ok_or(Failure::Plan)?;
            let detail = match code {
                C::AmbiguousBinding => Detail::DuplicateInput {},
                C::InconsistentRecord => Detail::BindingMismatch {},
                _ => Detail::InvalidInput {},
            };
            CaseResult::Invalid {
                reasons: reason(code, detail),
            }
        };
        cases.push(CaseEvaluation {
            local_record_identity: Token::new(&format!("case-evaluation-{index}"))
                .map_err(Failure::Identity)?,
            case_identity: planned.case_identity.clone(),
            result,
        });
    }
    let mut reasons = match &resolved.resolution.result {
        InputState::Resolved { .. } => Vec::new(),
        InputState::Unavailable { reasons, .. } | InputState::Invalid { reasons, .. } => {
            reasons.clone()
        }
    };
    let mut outcome = Outcome::Complete;
    for case in &cases {
        match &case.result {
            CaseResult::Measured { .. } | CaseResult::NotApplicable { .. } => {}
            CaseResult::Unavailable {
                reasons: current, ..
            } => {
                outcome = outcome.max(Outcome::Incomplete);
                reasons.extend(current.clone());
            }
            CaseResult::Invalid { reasons: current } => {
                outcome = Outcome::Invalid;
                reasons.extend(current.clone());
            }
        }
    }
    Ok(EvaluationBody {
        binding: RunBinding::from_plan(plan),
        owner_result_input: resolved.resolution.clone(),
        case_inventory: plan.body.case_inventory.clone(),
        cases,
        outcome,
        reasons: ordered(reasons),
    })
}

pub(super) fn report<O: OwnerRun>(
    evaluation: &Evaluation,
    evidence: Option<&OwnerEvidence<O>>,
) -> Result<ReportBody, Failure> {
    let mut rows = Vec::new();
    let mut missing_case_inventory = Vec::new();
    for (index, case) in evaluation.body.cases.iter().enumerate() {
        let case_evaluation =
            Reference::nested(evaluation.identity(), case.local_record_identity.clone());
        let CaseResult::Measured {
            evidence: case_reference,
        } = &case.result
        else {
            missing_case_inventory.push(MissingCase {
                local_record_identity: local(&format!("missing-case-{index}"))?,
                case_identity: case.case_identity.clone(),
                requirement: Requirement::Value,
                evaluation: case_evaluation,
                result: case.result.clone(),
            });
            continue;
        };
        // A measured case is only ever recorded after its evidence resolved,
        // so a row without one would mean the evaluation above disagreed with
        // itself rather than that a row is missing.
        let evidence = evidence.ok_or(Failure::Plan)?;
        let observation = evidence
            .body
            .cases
            .iter()
            .find(|candidate| candidate.case_identity == case.case_identity)
            .ok_or(Failure::Plan)?;
        let samples = observation
            .samples
            .iter()
            .map(|sample| {
                Ok(ReportSample {
                    local_record_identity: local(&format!(
                        "report-sample-{index}-{}",
                        sample.ordinal.get()
                    ))?,
                    raw_sample: Reference::nested(
                        evidence.identity(),
                        sample.local_record_identity.clone(),
                    ),
                    ordinal: sample.ordinal,
                    repetition_count: sample.repetition_count,
                    aggregate_quantity: sample.aggregate_quantity,
                })
            })
            .collect::<Result<_, Failure>>()?;
        rows.push(Row {
            local_record_identity: local(&format!("measurement-row-{index}"))?,
            case_identity: case.case_identity.clone(),
            case_evaluation,
            evidence: case_reference.clone(),
            owner_phase: observation.identity_projection.phase().into(),
            owner_cost: observation.identity_projection.cost().into(),
            category: Category::Value,
            operation_class: OperationClass::Value,
            performance_surface: Surface::Value,
            execution_state: observation.observed_descriptors.execution().clone(),
            metric: Metric::Value,
            unit: Unit::Value,
            sample_aggregation: Aggregation::Value,
            samples,
        });
    }
    Ok(ReportBody {
        report_specification: crate::identity::specification(),
        sections: vec![Section::MeasurementObservation {
            local_record_identity: Token::literal("measurement-observation-section-0"),
            run_evaluation: Reference::top(evaluation.identity()),
            evidence_sets: evidence
                .map(|value| vec![Reference::top(value.identity())])
                .unwrap_or_default(),
            outcome: evaluation.body.outcome,
            reasons: evaluation.body.reasons.clone(),
            numeric_policy: ObservationalOnly::Value,
            rows,
            missing_case_inventory,
            truncation: Truncation::Complete {},
        }],
    })
}

fn local(value: &str) -> Result<Token, Failure> {
    Token::new(value).map_err(Failure::Identity)
}
