// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::benchmark::clock::ClockFailure;
use crate::benchmark::collect::CollectionFailure;
use crate::benchmark::measure::MeasurementFailure;
use crate::benchmark::profile::ProfileCollectionFailure;
use crate::benchmark::run::{attempt_outcome, AttemptResult, OwnerOutcome, RecordedRun};
use crate::benchmark::sample::CaptureFailureCause;
use crate::benchmark::shared::identity::{IdentityFailure, Token};
use crate::benchmark::shared::measurement::{
    native_attempt_reference, Aggregation, CaseEvaluation, CaseResult, Category, Evaluation,
    EvaluationBody, Evidence, InputState, Metric, MissingCase, ObservationalOnly, OperationClass,
    Outcome, ReportBody, ReportSample, Requirement, Row, RunBinding, Section, Surface, Truncation,
    UnavailableKind, Unit,
};
use crate::benchmark::shared::reason::{
    ordered, CommonCode as C, Detail, InvocationFailure as Invocation, Reason, Stage,
};
use crate::benchmark::shared::record::Reference;
use crate::benchmark::work::WorkFailure;

use super::owner::OwnerInput;

fn failed_invocation(cause: &CaptureFailureCause) -> Invocation {
    match cause {
        CaptureFailureCause::Measurement(MeasurementFailure::Clock(
            ClockFailure::DurationConversionOverflow,
        )) => Invocation::DurationConversionOverflow,
        CaptureFailureCause::Measurement(MeasurementFailure::Clock(_)) => Invocation::ClockFailure,
        CaptureFailureCause::Measurement(MeasurementFailure::InvocationPanicked) => {
            Invocation::InvocationPanicked
        }
        CaptureFailureCause::MeasurementOverflow => Invocation::MeasurementOverflow,
        CaptureFailureCause::LogicalWorkObservation(WorkFailure::UnrepresentableCounter) => {
            Invocation::CounterOverflow
        }
        CaptureFailureCause::CollectorAllocation => Invocation::CollectorAllocation,
        CaptureFailureCause::ObservationPanicked => Invocation::ObservationPanicked,
        CaptureFailureCause::PrerequisiteUnavailable
        | CaptureFailureCause::Measurement(MeasurementFailure::PrerequisiteUnavailable) => {
            Invocation::PrerequisiteUnavailable
        }
        CaptureFailureCause::Output(_) => Invocation::OutputFailure,
        CaptureFailureCause::LogicalWorkObservation(_)
        | CaptureFailureCause::LogicalWorkMismatch(_)
        | CaptureFailureCause::SemanticObservationMismatch(_) => Invocation::WorkObservationFailure,
    }
}

pub(super) fn evaluate(
    run: &RecordedRun,
    owner: &OwnerInput,
    evidence: Option<&Evidence>,
) -> Result<EvaluationBody, IdentityFailure> {
    let plan = run.plan_record();
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
        let result = if let Some(source) = &owner.checked {
            // Native admission has already checked exact inventory/order/binding.
            let attempt = &source.document().result().attempts[index];
            let diagnostic = native_attempt_reference(source.document(), attempt.ordinal)?;
            match &attempt.result {
                AttemptResult::Measured(_) => {
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
                        CaseResult::Unavailable {
                            unavailable_kind: UnavailableKind::ProjectionIneligible,
                            reasons: reason(C::ProjectionIneligible, Detail::ProjectionMismatch {}),
                            diagnostic_partial_observations: vec![diagnostic],
                        }
                    }
                }
                AttemptResult::CollectionFailed(ProfileCollectionFailure::Collection(
                    CollectionFailure::Capture(failure),
                )) if attempt_outcome(attempt) == OwnerOutcome::Incomplete => {
                    CaseResult::Unavailable {
                        unavailable_kind: UnavailableKind::Failed,
                        reasons: reason(
                            C::FailedInvocation,
                            Detail::InvocationFailed {
                                subtype: failed_invocation(&failure.cause),
                                diagnostic: diagnostic.clone(),
                            },
                        ),
                        diagnostic_partial_observations: vec![diagnostic],
                    }
                }
                AttemptResult::CollectionFailed(ProfileCollectionFailure::Collection(
                    CollectionFailure::Capture(failure),
                )) if matches!(
                    failure.cause,
                    CaptureFailureCause::SemanticObservationMismatch(_)
                        | CaptureFailureCause::LogicalWorkMismatch(_)
                ) =>
                {
                    CaseResult::Invalid {
                        reasons: reason(
                            C::SemanticObservationMismatch,
                            Detail::SemanticMismatch { diagnostic },
                        ),
                    }
                }
                _ => CaseResult::Invalid {
                    reasons: reason(
                        C::InconsistentRecord,
                        Detail::InvalidOwnerCase { diagnostic },
                    ),
                },
            }
        } else if let Some(kind) = owner.absence {
            let (code, detail) = match kind {
                UnavailableKind::Missing => (C::MissingRequiredCase, Detail::MissingInput {}),
                UnavailableKind::Unsupported => {
                    (C::UnsupportedMeasurement, Detail::UnsupportedTuple {})
                }
                _ => unreachable!("closed native adapter emits missing or unsupported absence"),
            };
            CaseResult::Unavailable {
                unavailable_kind: kind,
                reasons: reason(code, detail),
                diagnostic_partial_observations: Vec::new(),
            }
        } else {
            let code = owner
                .failure_code
                .expect("invalid native input has a common cause");
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
            local_record_identity: Token::new(&format!("case-evaluation-{index}"))?,
            case_identity: planned.case_identity.clone(),
            result,
        });
    }
    let mut reasons = match &owner.resolution.result {
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
        owner_result_input: owner.resolution.clone(),
        case_inventory: plan.body.case_inventory.clone(),
        cases,
        outcome,
        reasons: ordered(reasons),
    })
}

pub(super) fn report(
    evaluation: &Evaluation,
    evidence: Option<&Evidence>,
) -> Result<ReportBody, IdentityFailure> {
    let mut rows = Vec::new();
    let mut missing_case_inventory = Vec::new();
    for (index, case) in evaluation.body.cases.iter().enumerate() {
        let case_evaluation =
            Reference::nested(evaluation.identity(), case.local_record_identity.clone());
        if let CaseResult::Measured {
            evidence: case_reference,
        } = &case.result
        {
            let evidence = evidence.expect("admitted measured case has its evidence set");
            let observation = evidence
                .body
                .cases
                .iter()
                .find(|candidate| candidate.case_identity == case.case_identity)
                .expect("admitted measured case has its exact evidence case");
            let (phase, cost) = observation.identity_projection.phase_cost();
            let samples = observation
                .samples
                .iter()
                .map(|sample| {
                    Ok(ReportSample {
                        local_record_identity: Token::new(&format!(
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
                .collect::<Result<_, IdentityFailure>>()?;
            rows.push(Row {
                local_record_identity: Token::new(&format!("measurement-row-{index}"))?,
                case_identity: case.case_identity.clone(),
                case_evaluation,
                evidence: case_reference.clone(),
                owner_phase: phase.into(),
                owner_cost: cost.into(),
                category: Category::Value,
                operation_class: OperationClass::Value,
                performance_surface: Surface::Value,
                execution_state: observation.observed_descriptors.execution.clone(),
                metric: Metric::Value,
                unit: Unit::Value,
                sample_aggregation: Aggregation::Value,
                samples,
            });
        } else {
            missing_case_inventory.push(MissingCase {
                local_record_identity: Token::new(&format!("missing-case-{index}"))?,
                case_identity: case.case_identity.clone(),
                requirement: Requirement::Value,
                evaluation: case_evaluation,
                result: case.result.clone(),
            });
        }
    }
    Ok(ReportBody {
        report_specification: super::super::identity::VersionedIdentity::specification(),
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
