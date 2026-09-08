// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::inventory::*;

fn binding() -> Binding<&'static str> {
    Binding {
        run: "run",
        plan: "plan",
        profile: "profile",
        subject: "subject",
        build: "build",
    }
}

fn entry(case: &'static str, requirement: Requirement) -> PlannedCase<&'static str> {
    PlannedCase {
        case,
        requirement,
        applicability: None,
    }
}

#[test]
fn a_missing_evaluation_is_invalid_but_explicit_required_unavailability_is_incomplete() {
    let planned = vec![entry("a", Requirement::Required)];
    let plan = InventoryPlan::issue(binding(), planned.clone()).unwrap();
    let mut submitted = Submission {
        binding: binding(),
        inventory: planned,
        cases: vec![],
    };
    let missing = plan.evaluate(&submitted, &[], &[]);
    assert_eq!(missing.outcome(), Outcome::Invalid);
    assert!(missing
        .issues()
        .iter()
        .any(|issue| issue.code == IssueCode::MissingCaseEvaluation));
    submitted.cases.push(CaseAttempt {
        case: "a",
        result: CaseResult::Unavailable {
            kind: UnavailableKind::Missing,
            reasons: ReasonCoverage::CommonCause,
        },
    });
    assert_eq!(
        plan.evaluate(&submitted, &[], &[]).outcome(),
        Outcome::Incomplete
    );
}

#[test]
fn optional_unavailability_is_diagnostic_and_not_a_successful_measurement() {
    let planned = vec![entry("a", Requirement::Optional)];
    let plan = InventoryPlan::issue(binding(), planned.clone()).unwrap();
    let submitted = Submission {
        binding: binding(),
        inventory: planned,
        cases: vec![CaseAttempt {
            case: "a",
            result: CaseResult::Unavailable {
                kind: UnavailableKind::Unsupported,
                reasons: ReasonCoverage::CommonCause,
            },
        }],
    };
    let evaluation = plan.evaluate(&submitted, &[], &[]);
    assert_eq!(evaluation.outcome(), Outcome::Complete);
    assert!(evaluation.successful_references().is_empty());
}

fn reference(case: &'static str) -> Reference<&'static str> {
    Reference {
        parent: "evidence-parent",
        local: case,
    }
}

fn measured() -> (
    InventoryPlan<&'static str>,
    Submission<&'static str>,
    Vec<EvidenceInput<&'static str>>,
) {
    let planned: Vec<_> = ["a", "b", "c"]
        .into_iter()
        .map(|case| entry(case, Requirement::Required))
        .collect();
    let plan = InventoryPlan::issue(binding(), planned.clone()).unwrap();
    let cases = planned
        .iter()
        .map(|entry| CaseAttempt {
            case: entry.case,
            result: CaseResult::Measured {
                reference: reference(entry.case),
            },
        })
        .collect();
    let evidence = planned
        .iter()
        .map(|entry| EvidenceInput {
            reference: reference(entry.case),
            state: EvidenceState::Resolved {
                binding: binding(),
                case: entry.case,
            },
        })
        .collect();
    (
        plan,
        Submission {
            binding: binding(),
            inventory: planned,
            cases,
        },
        evidence,
    )
}

fn unavailable(kind: UnavailableKind) -> CaseResult<&'static str> {
    CaseResult::Unavailable {
        kind,
        reasons: ReasonCoverage::CommonCause,
    }
}

#[test]
fn every_explicit_unavailability_kind_obeys_required_optional_accounting() {
    for kind in [
        UnavailableKind::Missing,
        UnavailableKind::Skipped,
        UnavailableKind::Unsupported,
        UnavailableKind::Failed,
        UnavailableKind::ProjectionIneligible,
        UnavailableKind::Stale,
    ] {
        for requirement in [Requirement::Required, Requirement::Optional] {
            let planned = vec![entry("a", requirement)];
            let plan = InventoryPlan::issue(binding(), planned.clone()).unwrap();
            let submitted = Submission {
                binding: binding(),
                inventory: planned,
                cases: vec![CaseAttempt {
                    case: "a",
                    result: unavailable(kind),
                }],
            };
            let evaluation = plan.evaluate(&submitted, &[], &[]);
            assert_eq!(
                evaluation.outcome(),
                if requirement == Requirement::Required {
                    Outcome::Incomplete
                } else {
                    Outcome::Complete
                }
            );
            assert_eq!(
                evaluation.cases(),
                &[("a", CaseAssessment::Unavailable { kind })]
            );
            assert!(evaluation.successful_references().is_empty());
        }
    }
}

#[test]
fn duplicate_plan_cases_cannot_create_an_immutable_inventory_view() {
    let duplicate = vec![
        entry("a", Requirement::Required),
        entry("a", Requirement::Optional),
    ];
    let errors = InventoryPlan::issue(binding(), duplicate).err().unwrap();
    assert_eq!(
        errors,
        vec![Issue {
            stage: Stage::Plan,
            code: IssueCode::DuplicateInventoryCase,
            detail: Detail::Case("a")
        }]
    );
}

#[test]
fn changed_inventory_members_order_requirements_and_rules_are_invalid() {
    let (plan, original, evidence) = measured();
    let mut missing = original.clone();
    missing.inventory.pop();
    let mut duplicate = original.clone();
    duplicate.inventory[1] = duplicate.inventory[0].clone();
    let mut unknown = original.clone();
    unknown
        .inventory
        .push(entry("unknown", Requirement::Optional));
    let mut order = original.clone();
    order.inventory.swap(0, 1);
    let mut requirement = original.clone();
    requirement.inventory[0].requirement = Requirement::Optional;
    let mut applicability = original;
    applicability.inventory[0].applicability = Some("invented-rule");
    for (submitted, code) in [
        (missing, IssueCode::MissingInventoryCase),
        (duplicate, IssueCode::DuplicateInventoryCase),
        (unknown, IssueCode::UnknownInventoryCase),
        (order, IssueCode::InventoryOrderMismatch),
        (requirement, IssueCode::RequirementMismatch),
        (applicability, IssueCode::ApplicabilityMismatch),
    ] {
        let evaluation = plan.evaluate(&submitted, &evidence, &[]);
        assert_eq!(evaluation.outcome(), Outcome::Invalid);
        assert!(
            evaluation.issues().iter().any(|issue| issue.code == code),
            "{code:?}"
        );
        assert!(evaluation.successful_references().is_empty());
    }
}

#[test]
fn evaluation_rows_are_exactly_one_per_case_and_keep_the_owner_plan_order() {
    let (plan, original, evidence) = measured();
    let mut missing = original.clone();
    missing.cases.pop();
    let mut duplicate = original.clone();
    duplicate.cases.push(duplicate.cases[0].clone());
    let mut unknown = original.clone();
    unknown.cases.push(CaseAttempt {
        case: "unknown",
        result: unavailable(UnavailableKind::Missing),
    });
    let mut order = original;
    order.cases.swap(0, 1);
    for (submitted, code) in [
        (missing, IssueCode::MissingCaseEvaluation),
        (duplicate, IssueCode::DuplicateCaseEvaluation),
        (unknown, IssueCode::UnknownCaseEvaluation),
        (order, IssueCode::CaseEvaluationOrderMismatch),
    ] {
        let evaluation = plan.evaluate(&submitted, &evidence, &[]);
        assert_eq!(evaluation.outcome(), Outcome::Invalid);
        assert!(evaluation.issues().iter().any(|issue| issue.code == code));
        assert!(evaluation.successful_references().is_empty());
    }
}

fn change_binding(binding: &mut Binding<&'static str>, field: BindingField) {
    match field {
        BindingField::Run => binding.run = "another-run",
        BindingField::Plan => binding.plan = "another-plan",
        BindingField::Profile => binding.profile = "another-profile",
        BindingField::Subject => binding.subject = "another-subject",
        BindingField::Build => binding.build = "another-build",
    }
}

#[test]
fn both_the_submission_and_resolved_evidence_must_match_every_plan_binding() {
    let (plan, original, original_evidence) = measured();
    for field in [
        BindingField::Run,
        BindingField::Plan,
        BindingField::Profile,
        BindingField::Subject,
        BindingField::Build,
    ] {
        let mut submitted = original.clone();
        change_binding(&mut submitted.binding, field);
        let evaluation = plan.evaluate(&submitted, &original_evidence, &[]);
        assert_eq!(evaluation.outcome(), Outcome::Invalid);
        assert!(evaluation
            .issues()
            .iter()
            .any(|issue| issue.detail == Detail::Binding(field)));
        let mut evidence = original_evidence.clone();
        let EvidenceState::Resolved { binding, .. } = &mut evidence[0].state else {
            unreachable!()
        };
        change_binding(binding, field);
        let evaluation = plan.evaluate(&original, &evidence, &[]);
        assert_eq!(evaluation.outcome(), Outcome::Invalid);
        assert!(evaluation.issues().iter().any(|issue| issue.code
            == IssueCode::EvidenceBindingMismatch
            && issue.detail
                == Detail::EvidenceBinding {
                    case: "a",
                    reference: reference("a"),
                    field
                }));
        assert!(evaluation.successful_references().is_empty());
    }
}

#[test]
fn reference_resolution_distinguishes_absence_staleness_corruption_and_case_mismatch() {
    let (plan, submitted, original) = measured();
    let mut missing = original.clone();
    missing.remove(0);
    let evaluation = plan.evaluate(&submitted, &missing, &[]);
    assert_eq!(evaluation.outcome(), Outcome::Incomplete);
    assert_eq!(
        evaluation.successful_references(),
        &[reference("b"), reference("c")]
    );
    for (state, outcome, code) in [
        (
            EvidenceState::Unavailable {
                kind: UnavailableKind::Stale,
            },
            Outcome::Incomplete,
            IssueCode::EvidenceUnavailable,
        ),
        (
            EvidenceState::Invalid,
            Outcome::Invalid,
            IssueCode::EvidenceInvalid,
        ),
        (
            EvidenceState::Resolved {
                binding: binding(),
                case: "other-case",
            },
            Outcome::Invalid,
            IssueCode::EvidenceCaseMismatch,
        ),
    ] {
        let mut evidence = original.clone();
        evidence[0].state = state;
        let evaluation = plan.evaluate(&submitted, &evidence, &[]);
        assert_eq!(evaluation.outcome(), outcome);
        assert!(evaluation.issues().iter().any(|issue| issue.code == code));
    }
    let mut wrong_nested = original;
    wrong_nested[0].reference.local = "missing-child";
    assert_eq!(
        plan.evaluate(&submitted, &wrong_nested, &[]).outcome(),
        Outcome::Incomplete
    );
}

#[test]
fn not_applicable_requires_both_the_planned_rule_and_independent_owner_proof() {
    let planned = vec![PlannedCase {
        case: "a",
        requirement: Requirement::Required,
        applicability: Some("outside-domain"),
    }];
    let plan = InventoryPlan::issue(binding(), planned.clone()).unwrap();
    let mut submitted = Submission {
        binding: binding(),
        inventory: planned,
        cases: vec![CaseAttempt {
            case: "a",
            result: CaseResult::NotApplicable {
                rule: "outside-domain",
            },
        }],
    };
    let proof = ApplicabilityProof {
        binding: binding(),
        case: "a",
        rule: "outside-domain",
    };
    assert_eq!(
        plan.evaluate(&submitted, &[], &[]).outcome(),
        Outcome::Invalid
    );
    let valid = plan.evaluate(&submitted, &[], std::slice::from_ref(&proof));
    assert_eq!(valid.outcome(), Outcome::Complete);
    assert!(valid.successful_references().is_empty());
    assert_eq!(
        valid.cases(),
        &[(
            "a",
            CaseAssessment::NotApplicable {
                rule: "outside-domain"
            }
        )]
    );
    assert_eq!(
        plan.evaluate(
            &submitted,
            &[],
            &[ApplicabilityProof {
                binding: binding(),
                case: "b",
                rule: "outside-domain"
            }]
        )
        .outcome(),
        Outcome::Invalid
    );
    submitted.cases[0].result = CaseResult::NotApplicable { rule: "other-rule" };
    assert_eq!(
        plan.evaluate(&submitted, &[], &[proof]).outcome(),
        Outcome::Invalid
    );

    let (unconditional, mut submitted, _) = measured();
    submitted.cases[0].result = CaseResult::NotApplicable {
        rule: "environment-failed",
    };
    assert_eq!(
        unconditional
            .evaluate(
                &submitted,
                &[],
                &[ApplicabilityProof {
                    binding: binding(),
                    case: "a",
                    rule: "environment-failed"
                }]
            )
            .outcome(),
        Outcome::Invalid
    );
}

#[test]
fn failure_views_without_a_common_cause_are_invalid_even_for_optional_cases() {
    for reasons in [ReasonCoverage::None, ReasonCoverage::OwnerSpecificOnly] {
        for result in [
            CaseResult::Unavailable {
                kind: UnavailableKind::Failed,
                reasons,
            },
            CaseResult::Invalid { reasons },
        ] {
            let planned = vec![entry("a", Requirement::Optional)];
            let plan = InventoryPlan::issue(binding(), planned.clone()).unwrap();
            let submitted = Submission {
                binding: binding(),
                inventory: planned,
                cases: vec![CaseAttempt { case: "a", result }],
            };
            let evaluation = plan.evaluate(&submitted, &[], &[]);
            assert_eq!(evaluation.outcome(), Outcome::Invalid);
            assert!(evaluation
                .issues()
                .iter()
                .any(|issue| issue.code == IssueCode::MissingCommonCause));
        }
    }
}

#[test]
fn invalid_overrides_incomplete_and_successful_cases_are_not_a_numeric_prefix() {
    let (plan, mut submitted, evidence) = measured();
    submitted.cases[1].result = unavailable(UnavailableKind::Failed);
    let incomplete = plan.evaluate(&submitted, &evidence, &[]);
    assert_eq!(incomplete.outcome(), Outcome::Incomplete);
    assert_eq!(
        incomplete.successful_references(),
        &[reference("a"), reference("c")]
    );
    submitted.cases[2].result = CaseResult::Invalid {
        reasons: ReasonCoverage::CommonCause,
    };
    let invalid = plan.evaluate(&submitted, &evidence, &[]);
    assert_eq!(invalid.outcome(), Outcome::Invalid);
    assert!(matches!(
        invalid.cases()[0].1,
        CaseAssessment::Measured { .. }
    ));
    assert!(invalid.successful_references().is_empty());
}

#[test]
fn duplicate_conflicting_references_and_cases_have_no_first_or_last_wins_path() {
    let (plan, mut submitted, mut evidence) = measured();
    evidence.push(EvidenceInput {
        reference: reference("a"),
        state: EvidenceState::Invalid,
    });
    let first = plan.evaluate(&submitted, &evidence, &[]);
    assert_eq!(first.outcome(), Outcome::Invalid);
    assert!(first
        .issues()
        .iter()
        .any(|issue| issue.code == IssueCode::AmbiguousEvidenceReference));
    evidence.reverse();
    let reversed = plan.evaluate(&submitted, &evidence, &[]);
    assert_eq!(first.issues(), reversed.issues());
    assert_eq!(first.cases(), reversed.cases());
    submitted.cases.push(CaseAttempt {
        case: "b",
        result: unavailable(UnavailableKind::Failed),
    });
    let first = plan.evaluate(&submitted, &evidence, &[]);
    submitted.cases.reverse();
    let reversed = plan.evaluate(&submitted, &evidence, &[]);
    assert_eq!(first.outcome(), Outcome::Invalid);
    assert_eq!(first.issues(), reversed.issues());
    assert_eq!(first.cases(), reversed.cases());
    assert!(first.issues().windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn non_applicability_proof_from_a_different_run_plan_profile_subject_or_build_is_unusable() {
    let planned = vec![PlannedCase {
        case: "a",
        requirement: Requirement::Required,
        applicability: Some("outside-domain"),
    }];
    let plan = InventoryPlan::issue(binding(), planned.clone()).unwrap();
    let submitted = Submission {
        binding: binding(),
        inventory: planned,
        cases: vec![CaseAttempt {
            case: "a",
            result: CaseResult::NotApplicable {
                rule: "outside-domain",
            },
        }],
    };
    for field in [
        BindingField::Run,
        BindingField::Plan,
        BindingField::Profile,
        BindingField::Subject,
        BindingField::Build,
    ] {
        let mut proof = ApplicabilityProof {
            binding: binding(),
            case: "a",
            rule: "outside-domain",
        };
        change_binding(&mut proof.binding, field);
        let evaluation = plan.evaluate(&submitted, &[], &[proof]);
        assert_eq!(evaluation.outcome(), Outcome::Invalid);
        assert!(evaluation
            .issues()
            .iter()
            .any(|issue| issue.code == IssueCode::ApplicabilityNotProven));
        assert!(evaluation.successful_references().is_empty());
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn all_127_native_fixture_collections_are_accounted_for_and_cannot_hide_a_missing_attempt() {
    use super::cases::{declarations, registry::Registry};
    use super::clock::MonotonicClock;
    use super::collect::collect_operation;
    use super::observation::Frame;
    use super::quantity::{Quantity, Repetitions};
    use super::sample::{CaptureBinding, CaptureCapacity, Sampling};
    let id = |label: &str| {
        let mut frame = Frame::new("inventory-integration-test");
        frame.text(label);
        frame.finish()
    };
    let binding = Binding {
        run: id("run"),
        plan: id("plan"),
        profile: id("profile"),
        subject: id("subject"),
        build: id("build"),
    };
    let registry = Registry::load().unwrap();
    let clock = MonotonicClock::acquire().unwrap();
    let sampling = Sampling::admit(
        Quantity::new(1),
        Repetitions::new(1).unwrap(),
        Repetitions::new(2).unwrap(),
        CaptureCapacity {
            warmup_repetitions: Quantity::new(1),
            samples: Repetitions::new(1).unwrap(),
            repetitions_per_sample: Repetitions::new(2).unwrap(),
            total_invocations: Repetitions::new(3).unwrap(),
        },
    )
    .unwrap();
    let fixtures: Vec<_> = declarations()
        .iter()
        .map(|declaration| registry.prepare(declaration).unwrap())
        .collect();
    // Context digests are test-only local labels here. This does not implement
    // the complete Measurement Case identity or common Evidence envelope codec.
    let inventory: Vec<_> = fixtures
        .iter()
        .map(|fixture| PlannedCase {
            case: fixture.input_context(),
            requirement: Requirement::Required,
            applicability: None,
        })
        .collect();
    let plan = InventoryPlan::issue(binding.clone(), inventory.clone()).unwrap();
    let mut cases = Vec::new();
    let mut evidence = Vec::new();
    for fixture in fixtures {
        let capture_binding = CaptureBinding {
            run: binding.run,
            case: fixture.input_context(),
        };
        let collected = collect_operation(&clock, &fixture, sampling, capture_binding).unwrap();
        assert!(collected
            .validate(&fixture, clock.description(), sampling, capture_binding)
            .is_empty());
        let reference = Reference {
            parent: id("test-only-observation-parent"),
            local: fixture.input_context(),
        };
        cases.push(CaseAttempt {
            case: fixture.input_context(),
            result: CaseResult::Measured {
                reference: reference.clone(),
            },
        });
        // The native owner fragment was checked above. Full common envelope /
        // evidence admission remains outside this non-serialized test view.
        evidence.push(EvidenceInput {
            reference,
            state: EvidenceState::Resolved {
                binding: binding.clone(),
                case: fixture.input_context(),
            },
        });
    }
    let mut submitted = Submission {
        binding,
        inventory,
        cases,
    };
    let evaluation = plan.evaluate(&submitted, &evidence, &[]);
    assert_eq!(evaluation.outcome(), Outcome::Complete);
    assert_eq!(evaluation.successful_references().len(), 127);
    assert!(evaluation.issues().is_empty());
    evidence.reverse();
    let reversed = plan.evaluate(&submitted, &evidence, &[]);
    assert_eq!(
        evaluation.successful_references(),
        reversed.successful_references()
    );
    assert_eq!(evaluation.cases(), reversed.cases());

    submitted.cases[0].result = CaseResult::Unavailable {
        kind: UnavailableKind::Unsupported,
        reasons: ReasonCoverage::CommonCause,
    };
    let incomplete = plan.evaluate(&submitted, &evidence, &[]);
    assert_eq!(incomplete.outcome(), Outcome::Incomplete);
    assert_eq!(incomplete.successful_references().len(), 126);
    submitted.cases.remove(0);
    let missing = plan.evaluate(&submitted, &evidence, &[]);
    assert_eq!(missing.outcome(), Outcome::Invalid);
    assert!(missing.successful_references().is_empty());
}
