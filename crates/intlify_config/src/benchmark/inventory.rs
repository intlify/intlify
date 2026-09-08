// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Pure 026 inventory/evaluation relationship checks for the adopting harness.
//! IDs are supplied by the enclosing admitted record layer; this module neither
//! invents their encoding nor admits envelopes, evidence content, or reason bodies.
//! Views retain no samples: diagnostic prefixes cannot become numeric input here.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Binding<Id> {
    pub(super) run: Id,
    pub(super) plan: Id,
    pub(super) profile: Id,
    pub(super) subject: Id,
    pub(super) build: Id,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Requirement {
    Required,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlannedCase<Id> {
    pub(super) case: Id,
    pub(super) requirement: Requirement,
    pub(super) applicability: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Reference<Id> {
    pub(super) parent: Id,
    pub(super) local: Id,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum UnavailableKind {
    Missing,
    Skipped,
    Unsupported,
    Failed,
    ProjectionIneligible,
    Stale,
}

/// A view of already validated reasons, not a replacement for their full typed
/// content. The enclosing record retains all reasons and diagnostic observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReasonCoverage {
    None,
    OwnerSpecificOnly,
    CommonCause,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CaseResult<Id> {
    Measured {
        reference: Reference<Id>,
    },
    NotApplicable {
        rule: Id,
    },
    Unavailable {
        kind: UnavailableKind,
        reasons: ReasonCoverage,
    },
    Invalid {
        reasons: ReasonCoverage,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CaseAttempt<Id> {
    pub(super) case: Id,
    pub(super) result: CaseResult<Id>,
}

/// Non-serialized views keep record admission and payload ownership outside this
/// relationship checker. Constructing a view alone never certifies a raw record.
#[derive(Debug, Clone)]
pub(super) struct Submission<Id> {
    pub(super) binding: Binding<Id>,
    pub(super) inventory: Vec<PlannedCase<Id>>,
    pub(super) cases: Vec<CaseAttempt<Id>>,
}

#[derive(Debug, Clone)]
pub(super) enum EvidenceState<Id> {
    /// Retrieval, kind, integrity, nested selection, and sample/result admission
    /// must have succeeded before the enclosing record layer supplies this view.
    Resolved {
        binding: Binding<Id>,
        case: Id,
    },
    Unavailable {
        kind: UnavailableKind,
    },
    Invalid,
}

#[derive(Debug, Clone)]
pub(super) struct EvidenceInput<Id> {
    pub(super) reference: Reference<Id>,
    pub(super) state: EvidenceState<Id>,
}

/// Fact supplied by the owner's applicability check, never inferred from an
/// invocation/environment failure or from the submitted not-applicable label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ApplicabilityProof<Id> {
    pub(super) binding: Binding<Id>,
    pub(super) case: Id,
    pub(super) rule: Id,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Outcome {
    Complete,
    Incomplete,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CaseAssessment<Id> {
    Measured { reference: Reference<Id> },
    NotApplicable { rule: Id },
    Unavailable { kind: UnavailableKind },
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Stage {
    Plan,
    Binding,
    Inventory,
    CaseInventory,
    CaseResult,
    Evidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum IssueCode {
    DuplicateInventoryCase,
    BindingMismatch,
    MissingInventoryCase,
    UnknownInventoryCase,
    InventoryOrderMismatch,
    RequirementMismatch,
    ApplicabilityMismatch,
    MissingCaseEvaluation,
    DuplicateCaseEvaluation,
    UnknownCaseEvaluation,
    CaseEvaluationOrderMismatch,
    MissingCommonCause,
    InvalidCase,
    ApplicabilityNotProven,
    RequiredCaseUnavailable,
    AmbiguousEvidenceReference,
    EvidenceUnavailable,
    EvidenceInvalid,
    EvidenceBindingMismatch,
    EvidenceCaseMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum BindingField {
    Run,
    Plan,
    Profile,
    Subject,
    Build,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Detail<Id> {
    None,
    Binding(BindingField),
    Case(Id),
    Applicability {
        case: Id,
        rule: Id,
    },
    Evidence {
        case: Id,
        reference: Reference<Id>,
    },
    EvidenceBinding {
        case: Id,
        reference: Reference<Id>,
        field: BindingField,
    },
    EvidenceCase {
        case: Id,
        reference: Reference<Id>,
        actual: Id,
    },
    Unavailable {
        case: Id,
        kind: UnavailableKind,
    },
}

/// Internal relationship failures, not serialized 026 Verification Reasons.
/// The common record layer maps them without discarding its source reasons.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Issue<Id> {
    pub(super) stage: Stage,
    pub(super) code: IssueCode,
    pub(super) detail: Detail<Id>,
}

pub(super) struct InventoryPlan<Id> {
    binding: Binding<Id>,
    cases: Vec<PlannedCase<Id>>,
}

pub(super) struct Evaluation<Id> {
    outcome: Outcome,
    cases: Vec<(Id, CaseAssessment<Id>)>,
    successful_references: Vec<Reference<Id>>,
    issues: Vec<Issue<Id>>,
}

impl<Id> Evaluation<Id> {
    pub(super) const fn outcome(&self) -> Outcome {
        self.outcome
    }
    pub(super) fn cases(&self) -> &[(Id, CaseAssessment<Id>)] {
        &self.cases
    }
    pub(super) fn successful_references(&self) -> &[Reference<Id>] {
        &self.successful_references
    }
    pub(super) fn issues(&self) -> &[Issue<Id>] {
        &self.issues
    }
}

impl<Id: Clone + Ord> InventoryPlan<Id> {
    /// The owner supplies the full selected catalog in its fixed order. This
    /// freezes one admitted inventory view, not a globally issued Run Plan or
    /// proof that a profile's required catalog was selected by the owner.
    pub(super) fn issue(
        binding: Binding<Id>,
        cases: Vec<PlannedCase<Id>>,
    ) -> Result<Self, Vec<Issue<Id>>> {
        let mut seen = BTreeSet::new();
        let mut issues = BTreeSet::new();
        for case in &cases {
            if !seen.insert(&case.case) {
                issues.insert(issue(
                    Stage::Plan,
                    IssueCode::DuplicateInventoryCase,
                    Detail::Case(case.case.clone()),
                ));
            }
        }
        if issues.is_empty() {
            Ok(Self { binding, cases })
        } else {
            Err(issues.into_iter().collect())
        }
    }

    pub(super) fn evaluate(
        &self,
        submitted: &Submission<Id>,
        evidence: &[EvidenceInput<Id>],
        proven_applicability: &[ApplicabilityProof<Id>],
    ) -> Evaluation<Id> {
        let mut issues = BTreeSet::new();
        for field in binding_differences(&self.binding, &submitted.binding) {
            issues.insert(issue(
                Stage::Binding,
                IssueCode::BindingMismatch,
                Detail::Binding(field),
            ));
        }
        self.check_inventory(&submitted.inventory, &mut issues);
        let expected: BTreeSet<_> = self.cases.iter().map(|case| &case.case).collect();
        let mut attempts = BTreeMap::new();
        for attempt in &submitted.cases {
            if !expected.contains(&attempt.case) {
                issues.insert(issue(
                    Stage::CaseInventory,
                    IssueCode::UnknownCaseEvaluation,
                    Detail::Case(attempt.case.clone()),
                ));
            }
            match attempts.entry(&attempt.case) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(Some(attempt));
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    entry.insert(None);
                    issues.insert(issue(
                        Stage::CaseInventory,
                        IssueCode::DuplicateCaseEvaluation,
                        Detail::Case(attempt.case.clone()),
                    ));
                }
            }
        }
        // The initial owner profile keeps evaluation rows in the frozen plan's
        // order. A concurrent producer must restore that order before reporting.
        if !self
            .cases
            .iter()
            .map(|case| &case.case)
            .eq(submitted.cases.iter().map(|case| &case.case))
        {
            issues.insert(issue(
                Stage::CaseInventory,
                IssueCode::CaseEvaluationOrderMismatch,
                Detail::None,
            ));
        }
        let mut outcome = if issues.is_empty() {
            Outcome::Complete
        } else {
            Outcome::Invalid
        };
        // Index once; no repeated evidence scans per planned case. Duplicate
        // references become ambiguous, never a first/last-record-wins decision.
        let mut lookup = BTreeMap::new();
        for input in evidence {
            lookup
                .entry(&input.reference)
                .and_modify(|state| *state = None)
                .or_insert(Some(&input.state));
        }
        let proofs: BTreeSet<_> = proven_applicability
            .iter()
            .filter(|proof| proof.binding == self.binding)
            .map(|proof| (&proof.case, &proof.rule))
            .collect();
        let mut cases = Vec::with_capacity(self.cases.len());
        let mut successful_references = Vec::new();
        for planned in &self.cases {
            let assessment = match attempts.get(&planned.case) {
                None => {
                    issues.insert(issue(
                        Stage::CaseInventory,
                        IssueCode::MissingCaseEvaluation,
                        Detail::Case(planned.case.clone()),
                    ));
                    CaseAssessment::Invalid
                }
                Some(None) => CaseAssessment::Invalid,
                Some(Some(attempt)) => {
                    self.assess(planned, &attempt.result, &lookup, &proofs, &mut issues)
                }
            };
            match &assessment {
                CaseAssessment::Invalid => outcome = Outcome::Invalid,
                CaseAssessment::Unavailable { kind }
                    if planned.requirement == Requirement::Required =>
                {
                    outcome = outcome.max(Outcome::Incomplete);
                    issues.insert(issue(
                        Stage::CaseResult,
                        IssueCode::RequiredCaseUnavailable,
                        Detail::Unavailable {
                            case: planned.case.clone(),
                            kind: *kind,
                        },
                    ));
                }
                CaseAssessment::Measured { reference } => {
                    successful_references.push(reference.clone());
                }
                _ => {}
            }
            cases.push((planned.case.clone(), assessment));
        }
        // Retain diagnostic assessments but never expose a successful prefix
        // from a structurally invalid submitted run to projection consumers.
        if outcome == Outcome::Invalid {
            successful_references.clear();
        }
        Evaluation {
            outcome,
            cases,
            successful_references,
            issues: issues.into_iter().collect(),
        }
    }

    fn check_inventory(&self, submitted: &[PlannedCase<Id>], issues: &mut BTreeSet<Issue<Id>>) {
        let expected: BTreeMap<_, _> = self.cases.iter().map(|case| (&case.case, case)).collect();
        let mut seen = BTreeSet::new();
        for entry in submitted {
            let detail = Detail::Case(entry.case.clone());
            if !seen.insert(&entry.case) {
                issues.insert(issue(
                    Stage::Inventory,
                    IssueCode::DuplicateInventoryCase,
                    detail.clone(),
                ));
            }
            let Some(planned) = expected.get(&entry.case) else {
                issues.insert(issue(
                    Stage::Inventory,
                    IssueCode::UnknownInventoryCase,
                    detail,
                ));
                continue;
            };
            if planned.requirement != entry.requirement {
                issues.insert(issue(
                    Stage::Inventory,
                    IssueCode::RequirementMismatch,
                    detail.clone(),
                ));
            }
            if planned.applicability != entry.applicability {
                issues.insert(issue(
                    Stage::Inventory,
                    IssueCode::ApplicabilityMismatch,
                    detail,
                ));
            }
        }
        for entry in &self.cases {
            if !seen.contains(&entry.case) {
                issues.insert(issue(
                    Stage::Inventory,
                    IssueCode::MissingInventoryCase,
                    Detail::Case(entry.case.clone()),
                ));
            }
        }
        if !self
            .cases
            .iter()
            .map(|case| &case.case)
            .eq(submitted.iter().map(|case| &case.case))
        {
            issues.insert(issue(
                Stage::Inventory,
                IssueCode::InventoryOrderMismatch,
                Detail::None,
            ));
        }
    }

    fn assess(
        &self,
        planned: &PlannedCase<Id>,
        result: &CaseResult<Id>,
        evidence: &BTreeMap<&Reference<Id>, Option<&EvidenceState<Id>>>,
        proofs: &BTreeSet<(&Id, &Id)>,
        issues: &mut BTreeSet<Issue<Id>>,
    ) -> CaseAssessment<Id> {
        match result {
            CaseResult::Unavailable { kind, reasons } => {
                if *reasons != ReasonCoverage::CommonCause {
                    issues.insert(issue(
                        Stage::CaseResult,
                        IssueCode::MissingCommonCause,
                        Detail::Case(planned.case.clone()),
                    ));
                    return CaseAssessment::Invalid;
                }
                CaseAssessment::Unavailable { kind: *kind }
            }
            CaseResult::Invalid { reasons } => {
                if *reasons != ReasonCoverage::CommonCause {
                    issues.insert(issue(
                        Stage::CaseResult,
                        IssueCode::MissingCommonCause,
                        Detail::Case(planned.case.clone()),
                    ));
                }
                issues.insert(issue(
                    Stage::CaseResult,
                    IssueCode::InvalidCase,
                    Detail::Case(planned.case.clone()),
                ));
                CaseAssessment::Invalid
            }
            CaseResult::NotApplicable { rule } => {
                let detail = Detail::Applicability {
                    case: planned.case.clone(),
                    rule: rule.clone(),
                };
                if planned.applicability.as_ref() != Some(rule) {
                    issues.insert(issue(
                        Stage::CaseResult,
                        IssueCode::ApplicabilityMismatch,
                        detail,
                    ));
                    return CaseAssessment::Invalid;
                }
                if !proofs.contains(&(&planned.case, rule)) {
                    issues.insert(issue(
                        Stage::CaseResult,
                        IssueCode::ApplicabilityNotProven,
                        detail,
                    ));
                    return CaseAssessment::Invalid;
                }
                CaseAssessment::NotApplicable { rule: rule.clone() }
            }
            CaseResult::Measured { reference } => {
                self.assess_reference(&planned.case, reference, evidence, issues)
            }
        }
    }

    fn assess_reference(
        &self,
        case: &Id,
        reference: &Reference<Id>,
        evidence: &BTreeMap<&Reference<Id>, Option<&EvidenceState<Id>>>,
        issues: &mut BTreeSet<Issue<Id>>,
    ) -> CaseAssessment<Id> {
        let detail = Detail::Evidence {
            case: case.clone(),
            reference: reference.clone(),
        };
        match evidence.get(reference) {
            None => {
                issues.insert(issue(
                    Stage::Evidence,
                    IssueCode::EvidenceUnavailable,
                    detail,
                ));
                CaseAssessment::Unavailable {
                    kind: UnavailableKind::Missing,
                }
            }
            Some(None) => {
                issues.insert(issue(
                    Stage::Evidence,
                    IssueCode::AmbiguousEvidenceReference,
                    detail,
                ));
                CaseAssessment::Invalid
            }
            Some(Some(EvidenceState::Invalid)) => {
                issues.insert(issue(Stage::Evidence, IssueCode::EvidenceInvalid, detail));
                CaseAssessment::Invalid
            }
            Some(Some(EvidenceState::Unavailable { kind })) => {
                issues.insert(issue(
                    Stage::Evidence,
                    IssueCode::EvidenceUnavailable,
                    detail,
                ));
                CaseAssessment::Unavailable { kind: *kind }
            }
            Some(Some(EvidenceState::Resolved {
                binding,
                case: actual,
            })) => {
                let differences = binding_differences(&self.binding, binding);
                for field in &differences {
                    issues.insert(issue(
                        Stage::Evidence,
                        IssueCode::EvidenceBindingMismatch,
                        Detail::EvidenceBinding {
                            case: case.clone(),
                            reference: reference.clone(),
                            field: *field,
                        },
                    ));
                }
                if actual != case {
                    issues.insert(issue(
                        Stage::Evidence,
                        IssueCode::EvidenceCaseMismatch,
                        Detail::EvidenceCase {
                            case: case.clone(),
                            reference: reference.clone(),
                            actual: actual.clone(),
                        },
                    ));
                }
                if !differences.is_empty() || actual != case {
                    return CaseAssessment::Invalid;
                }
                CaseAssessment::Measured {
                    reference: reference.clone(),
                }
            }
        }
    }
}

fn binding_differences<Id: Eq>(expected: &Binding<Id>, actual: &Binding<Id>) -> Vec<BindingField> {
    [
        (BindingField::Run, &expected.run, &actual.run),
        (BindingField::Plan, &expected.plan, &actual.plan),
        (BindingField::Profile, &expected.profile, &actual.profile),
        (BindingField::Subject, &expected.subject, &actual.subject),
        (BindingField::Build, &expected.build, &actual.build),
    ]
    .into_iter()
    .filter_map(|(field, left, right)| (left != right).then_some(field))
    .collect()
}

fn issue<Id>(stage: Stage, code: IssueCode, detail: Detail<Id>) -> Issue<Id> {
    Issue {
        stage,
        code,
        detail,
    }
}
