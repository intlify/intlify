// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Phase 2, `structural-admission`, over the value Phase 1
//! admitted. The phase's first kind owns the materialized dependency records
//! and waits on nothing (D15:2516). The Configuration Schema Selection is one
//! singleton subject, ranked before the scope levels (D15:2173), whose four
//! checks find an object root, its `schemaVersion` member, the member's
//! string, and a schema the construction admitted for that string, each
//! waiting on the one before (D15:2130, 2149). The schema-guided analysis is
//! then measured under that schema against the capability's
//! `maxStructuralAnalysisUnits` (D15:1887, 2008), and the
//! `profileSpecification` group's owner, the phase's last kind, waits on
//! Phase 0 alone (D15:2516).
//!
//! Each evaluation of a structural check the analysis found at the root is
//! a candidate of the unit over its field's logical path: a required member,
//! a value's type, a value's closed constraints, or the root's undeclared
//! members (D15:2173, 2184). A candidate waits on the type check of the
//! value that holds it, or of its own value, and the root's own type check
//! waits on the analysis's measure (D15:2149, Decision 015-195). A value
//! whose type check failed is that check's to report, so its other checks
//! are not evaluated (D15:2184).
//!
//! The identity maps' keys and the fields of their entries come with their
//! units (plan 025 PR 5f-2d-2); until then the analysis these checks admit
//! is what later phases read, and a configuration structural admission does
//! not admit whole starts no semantic phase. Phase 2's Findings carry their
//! occurrences once its Evidence comes (plan 025 PR 5f-2e).

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::materialize::MaterializedDocument;
use crate::structural::facts::{self, At, CandidateId, Facts, FieldPathId, Level, Outcome};
use crate::structural::{
    AdmissionInvariant, AuthoringSchema, StructuralAnalysis, StructuralFailure, StructuralLimits,
};

use super::dependency::{own, SemanticGroup};
use super::evaluation::{
    Actual, Admitted, Blocking, Check, Code, Evaluation, EvaluationInvariant, LimitExceeded, Phase,
    Subject, Unit,
};
use super::limits::{BoundId, Group};

/// The unit that owns the materialized dependency records.
pub(crate) const OWNER: Unit = Unit {
    phase: Phase::StructuralAdmission,
    subject: Subject::MaterializedDependencies,
    check: Check::EstablishedSlots,
};

/// The unit of the Configuration Schema Selection's `check`.
pub(crate) const fn selection(check: Check) -> Unit {
    Unit {
        phase: Phase::StructuralAdmission,
        subject: Subject::SchemaSelection,
        check,
    }
}

/// The unit that measures the analysis under the capability.
pub(crate) const CAPABILITY: Unit = Unit {
    phase: Phase::StructuralAdmission,
    subject: Subject::StructuralCapability,
    check: Check::WithinCapability,
};

/// A subject of structural admission's scope levels: a field of the root, by
/// its logical path (D15:2173, 2180). The identity maps' keys and the fields
/// of their entries come with their units (plan 025 PR 5f-2d-2).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StructuralSubject {
    /// A field of the root.
    RootField(Field),
}

impl StructuralSubject {
    /// The logical subject, which every raw candidate of the unit joins: the
    /// field without where a candidate looked (D15:2184).
    pub(crate) fn logical(&self) -> Self {
        match self {
            Self::RootField(field) => Self::RootField(field.logical()),
        }
    }
}

/// A field: its logical path from its entry, in the schema's member names
/// and `*` for an array element or an entry of a map whose keys are not
/// identities, and where one raw candidate of its unit looked (D15:2180,
/// 2184). Paths order segment by segment by their bytes, a shorter exact
/// prefix first.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Field {
    pub(crate) path: Arc<[Box<str>]>,
    /// None for the logical subject.
    pub(crate) at: Option<At>,
}

impl Field {
    fn logical(&self) -> Self {
        Self {
            path: Arc::clone(&self.path),
            at: None,
        }
    }
}

/// The bound the analysis is measured against.
const UNITS: BoundId = BoundId {
    group: Group::ConfigurationInput,
    name: "maxStructuralAnalysisUnits",
};

/// A resolver bug Phase 2 found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StructuralInvariant {
    Evaluation(EvaluationInvariant),
    /// The schema-guided analysis found a defect of its own schema binding.
    Admission(AdmissionInvariant),
    /// The analysis selected no schema, or stopped short of its work, for a
    /// reason no check reported.
    UnreportedFailure,
}

impl From<EvaluationInvariant> for StructuralInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

/// Phase 2 over `document`, the value Phase 1 admitted, with `schema`, the
/// configuration schema construction admitted, under `limits`: the analysis
/// when the selection selected `schema` and the analysis stayed within the
/// capability's bound, or the causes of its absence. `project_profile` is
/// what Phase 0 admitted, which the `profileSpecification` group's owner
/// waits on.
///
/// # Errors
///
/// Returns the resolver bug Phase 2 found.
pub(crate) fn admit_structure<Policy, Target>(
    evaluation: &mut Evaluation,
    document: MaterializedDocument,
    schema: &AuthoringSchema<Policy, Target>,
    limits: StructuralLimits,
    project_profile: Admitted<()>,
) -> Result<Admitted<StructuralAnalysis<Policy, Target>>, StructuralInvariant> {
    // The owner records the materialized dependencies, and nothing waits on
    // it.
    let _: Admitted<()> = evaluation.check(OWNER, Ok(()), |()| Ok::<(), Blocking>(()))?;
    let analysis = schema
        .analyze(Arc::new(document), limits)
        .map_err(StructuralInvariant::Admission)?;
    let failed = |reason| analysis.issues().iter().any(|issue| issue.reason == reason);
    let object = evaluation.check(selection(Check::ObjectRoot), Ok(()), |()| {
        rejected(
            failed(StructuralFailure::RootTypeInvalid),
            Code::FieldTypeInvalid,
        )
    })?;
    let present = evaluation.check(selection(Check::PresentSchemaVersion), object, |()| {
        rejected(
            failed(StructuralFailure::SchemaVersionMissing),
            Code::RequiredFieldMissing,
        )
    })?;
    let string = evaluation.check(selection(Check::StringSchemaVersion), present, |()| {
        rejected(
            failed(StructuralFailure::SchemaVersionTypeInvalid),
            Code::FieldTypeInvalid,
        )
    })?;
    let supported = evaluation.check(selection(Check::SupportedSchemaVersion), string, |()| {
        rejected(
            failed(StructuralFailure::SchemaVersionUnsupported),
            Code::SchemaVersionUnsupported,
        )
    })?;
    // The checks decide, so a check a test makes block holds the analysis
    // back too; a schema the checks admit and the analysis did not select is
    // a resolver bug.
    if supported.is_ok() && !analysis.version_selected() {
        return Err(StructuralInvariant::UnreportedFailure);
    }
    let within = evaluation.check(CAPABILITY, supported, |()| {
        exceeded(&analysis).map_or(Ok(()), Err)
    })?;
    if within.is_ok() && analysis.structural_units().is_none() {
        return Err(StructuralInvariant::UnreportedFailure);
    }
    // An analysis past the capability stopped before its candidates, which
    // alone name the fields' subjects.
    if let Some(facts) = analysis.facts() {
        root_fields(evaluation, facts, &within)?;
    }
    // The assertion Phase 0 admitted never selects structure: only the owner
    // of the group it establishes waits on it.
    own(
        evaluation,
        SemanticGroup::ProfileSpecification,
        project_profile,
    )?;
    Ok(within.map(|()| analysis))
}

/// Records each candidate the analysis found at the root as one candidate of
/// the unit over its field's logical path. A candidate waits on the result
/// of the type check it needs, and the root's own type check on `measured`,
/// the analysis's measure (D15:2149, 2184).
fn root_fields(
    evaluation: &mut Evaluation,
    facts: &Facts,
    measured: &Admitted<()>,
) -> Result<(), EvaluationInvariant> {
    let mut paths: BTreeMap<Option<FieldPathId>, Arc<[Box<str>]>> = BTreeMap::new();
    let mut typed: BTreeMap<CandidateId, Admitted<()>> = BTreeMap::new();
    for (id, candidate) in facts.identified() {
        if facts.scope(candidate.scope).level != Level::Root {
            continue;
        }
        let (check, code) = match candidate.check {
            facts::Check::Required => (Check::RequiredField, Code::RequiredFieldMissing),
            facts::Check::Type => (Check::FieldType, Code::FieldTypeInvalid),
            facts::Check::Value => (Check::FieldValue, Code::FieldValueInvalid),
            facts::Check::Known => (Check::KnownField, Code::UnknownField),
            // The keys of `profiles` come with their units (plan 025 PR
            // 5f-2d-2).
            facts::Check::Key => continue,
        };
        // The walk records a candidate after the type check it needs.
        let prerequisite = match candidate.after {
            Some(after) => typed
                .get(&after)
                .expect("a type check is recorded before what needs it")
                .clone(),
            None => measured.clone(),
        };
        let path = paths
            .entry(candidate.path)
            .or_insert_with(|| {
                facts
                    .steps(candidate.path)
                    .into_iter()
                    .map(|step| Box::from(step.name()))
                    .collect()
            })
            .clone();
        let unit = Unit {
            phase: Phase::StructuralAdmission,
            subject: Subject::Structural(StructuralSubject::RootField(Field {
                path,
                at: Some(candidate.at),
            })),
            check,
        };
        let result = evaluation.check(unit, prerequisite, |()| match candidate.outcome {
            Outcome::Passed => Ok(()),
            Outcome::Failed => rejected(true, code),
            // The value's failed type check is what it needs.
            Outcome::Excluded => unreachable!("an excluded value's type check failed"),
        })?;
        if candidate.check == facts::Check::Type {
            typed.insert(id, result);
        }
    }
    Ok(())
}

/// A structural check's result: `code` when the analysis found its failure.
/// The code's field is the path the unit names.
const fn rejected(failed: bool, code: Code) -> Result<(), Blocking> {
    if failed {
        Err(Blocking {
            code,
            reason: None,
            details: None,
        })
    } else {
        Ok(())
    }
}

/// The capability's bound and the analysis's complete exact count, when the
/// count is past the bound (D15:1887, 2006).
fn exceeded<Policy, Target>(analysis: &StructuralAnalysis<Policy, Target>) -> Option<Blocking> {
    analysis
        .issues()
        .iter()
        .find_map(|issue| match issue.reason {
            StructuralFailure::StructuralWorkLimit { limit, actual } => {
                Some(Blocking::limit(LimitExceeded {
                    bound: UNITS,
                    limit,
                    actual: Actual::Exact(actual),
                }))
            }
            _ => None,
        })
}

#[cfg(test)]
impl StructuralSubject {
    /// The logical subject of the root's field at `path`.
    pub(crate) fn root(path: &[&str]) -> Self {
        Self::RootField(Field {
            path: path.iter().map(|&name| Box::from(name)).collect(),
            at: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use crate::fixtures::{minimal_config, FixturePolicyReference, FixtureTargetReference};
    use crate::input_limits::Bound;
    use crate::materialize::materialize_file;
    use crate::materialize_tests::limits;
    use crate::resolver::dependency::{unit as owner_of, SemanticGroup};
    use crate::resolver::evaluation::{
        Actual, Admitted, Blocking, Causes, Check, Code, Details, Evaluation, EvaluationInvariant,
        IssueKey, LimitExceeded, Phase, Reason, Status, Subject, Unit,
    };
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::specification::Slot;
    use crate::resolver::subject::SubjectKind;
    use crate::structural::{AuthoringSchema, StructuralLimits};

    use super::{
        admit_structure, selection, StructuralInvariant, StructuralSubject, CAPABILITY, OWNER,
    };

    type Schema = AuthoringSchema<FixturePolicyReference, FixtureTargetReference>;

    fn structural(units: u64) -> StructuralLimits {
        StructuralLimits {
            max_profiles: Bound::new(100).unwrap(),
            max_profile_id_bytes: Bound::new(256).unwrap(),
            max_structural_analysis_units: Bound::new(units).unwrap(),
        }
    }

    /// Phase 2 over `value` under `units` analysis units, after a Phase 0
    /// that admitted its assertion: the evaluation, and the admitted
    /// analysis's unit count or the causes of its absence.
    fn admitted(value: &Value, units: u64) -> (Evaluation, Result<u64, Causes>) {
        admitted_after(value, units, Ok(()))
    }

    fn admitted_after(
        value: &Value,
        units: u64,
        project_profile: Admitted<()>,
    ) -> (Evaluation, Result<u64, Causes>) {
        admitted_in(Evaluation::default(), value, units, project_profile)
    }

    /// Phase 2 as [`admitted_after`] runs it, recording into `evaluation`.
    fn admitted_in(
        mut evaluation: Evaluation,
        value: &Value,
        units: u64,
        project_profile: Admitted<()>,
    ) -> (Evaluation, Result<u64, Causes>) {
        let bytes = serde_json::to_vec(value).unwrap();
        let document = materialize_file(Arc::from(bytes), limits()).unwrap();
        let analysis = admit_structure(
            &mut evaluation,
            document,
            &Schema::for_model().unwrap(),
            structural(units),
            project_profile,
        )
        .unwrap();
        let count = analysis.map(|analysis| analysis.structural_units().unwrap());
        (evaluation, count)
    }

    /// Every Finding, as its check, code, and details.
    fn found(evaluation: &Evaluation) -> Vec<(Check, Code, Option<Details>)> {
        evaluation
            .findings()
            .iter()
            .map(|finding| {
                (
                    finding.key.unit().check,
                    finding.key.code(),
                    finding.details.copied(),
                )
            })
            .collect()
    }

    /// Each unit's status, with the checks of its causes when it was not
    /// evaluated.
    fn statuses(evaluation: &Evaluation) -> BTreeMap<Unit, Option<Vec<Check>>> {
        evaluation
            .statuses()
            .map(|(unit, status)| {
                let causes = match status {
                    Status::Evaluated => None,
                    Status::NotEvaluated(causes) => {
                        Some(causes.keys().map(|key| key.unit().check).collect())
                    }
                };
                (unit.clone(), causes)
            })
            .collect()
    }

    /// Every blocking result, as its unit and code.
    fn issues(evaluation: &Evaluation) -> Vec<(Unit, Code)> {
        evaluation
            .issues()
            .map(|key| (key.unit().clone(), key.code()))
            .collect()
    }

    /// The unit of `check` over the root's field at `path`.
    fn root(path: &[&str], check: Check) -> Unit {
        Unit {
            phase: Phase::StructuralAdmission,
            subject: Subject::Structural(StructuralSubject::root(path)),
            check,
        }
    }

    /// The units of the root's fields of a configuration without `$schema`:
    /// the root's type and known members, and the required member, type, and
    /// closed value of `profiles` and of `schemaVersion`.
    fn root_units() -> Vec<Unit> {
        let mut units = vec![root(&[], Check::FieldType), root(&[], Check::KnownField)];
        for name in ["profiles", "schemaVersion"] {
            units.extend(
                [Check::RequiredField, Check::FieldType, Check::FieldValue]
                    .map(|check| root(&[name], check)),
            );
        }
        units
    }

    /// The units of Phase 2's kinds before the scope levels, and the
    /// `profileSpecification` group's owner.
    fn units() -> Vec<Unit> {
        let mut units = vec![OWNER];
        units.extend(
            SubjectKind::ConfigurationSchemaSelection
                .checks()
                .iter()
                .map(|&check| selection(check)),
        );
        units.extend([CAPABILITY, owner_of(SemanticGroup::ProfileSpecification)]);
        units
    }

    #[test]
    fn a_selected_schema_within_the_capability_admits_the_analysis() {
        let (evaluation, count) = admitted(&minimal_config(), 1_000_000);
        assert!(count.unwrap() > 0);
        assert_eq!(found(&evaluation), []);
        let statuses = statuses(&evaluation);
        let mut expected = units();
        expected.extend(root_units());
        expected.sort();
        assert_eq!(statuses.keys().cloned().collect::<Vec<_>>(), expected);
        assert!(statuses.values().all(Option::is_none));
    }

    /// Each check of the root's fields reports what it finds under the unit
    /// over the field's path: the missing member's, the value's, or that of
    /// the object an undeclared member is in (D15:2184, 2291-2294).
    #[test]
    fn each_root_check_reports_under_its_fields_path() {
        let edit = |change: fn(&mut Value)| {
            let mut value = minimal_config();
            change(&mut value);
            value
        };
        for (value, unit, code) in [
            (
                edit(|value| {
                    value.as_object_mut().unwrap().remove("profiles");
                }),
                root(&["profiles"], Check::RequiredField),
                Code::RequiredFieldMissing,
            ),
            (
                edit(|value| value["$schema"] = json!(1)),
                root(&["$schema"], Check::FieldType),
                Code::FieldTypeInvalid,
            ),
            (
                edit(|value| value["profiles"] = json!({})),
                root(&["profiles"], Check::FieldValue),
                Code::FieldValueInvalid,
            ),
            (
                edit(|value| value["extra"] = json!(true)),
                root(&[], Check::KnownField),
                Code::UnknownField,
            ),
        ] {
            let (evaluation, count) = admitted(&value, 1_000_000);
            assert!(count.is_ok(), "{unit:?}");
            assert_eq!(issues(&evaluation), [(unit.clone(), code)]);
            assert!(statuses(&evaluation).values().all(Option::is_none));
        }
    }

    /// A value whose type check failed is that check's to report: its closed
    /// value is not evaluated, and waits on the type check (D15:2184).
    #[test]
    fn a_value_of_another_type_leaves_its_closed_value_to_its_type_check() {
        let mut value = minimal_config();
        value["profiles"] = json!(["app"]);
        let (evaluation, _) = admitted(&value, 1_000_000);
        let typed = root(&["profiles"], Check::FieldType);
        assert_eq!(
            issues(&evaluation),
            [(typed.clone(), Code::FieldTypeInvalid)]
        );
        let statuses = statuses(&evaluation);
        assert_eq!(
            statuses[&root(&["profiles"], Check::FieldValue)],
            Some(vec![Check::FieldType])
        );
        assert_eq!(statuses[&typed], None);
        assert_eq!(statuses[&root(&["profiles"], Check::RequiredField)], None);
    }

    /// The root's own type check waits on the analysis's measure, and every
    /// other check of its fields on the type check of the value that holds
    /// the field, or of its own value (D15:2149, Decision 015-195).
    #[test]
    fn the_roots_fields_wait_on_the_type_checks_they_need() {
        for (blocked, waiting) in [
            (CAPABILITY, root_units()),
            (root(&[], Check::FieldType), root_units()[1..].to_vec()),
            (
                root(&["profiles"], Check::FieldType),
                vec![root(&["profiles"], Check::FieldValue)],
            ),
        ] {
            let (evaluation, _) = admitted_in(
                Evaluation::blocking(blocked.clone()),
                &minimal_config(),
                1_000_000,
                Ok(()),
            );
            for (unit, causes) in statuses(&evaluation) {
                let expected = waiting.contains(&unit).then(|| vec![blocked.check]);
                assert_eq!(causes, expected, "{blocked:?}: {unit:?}");
            }
        }
    }

    /// Every undeclared member of the root is a candidate of the one unit of
    /// the root's known members, which reports one Finding (D15:2184).
    #[test]
    fn the_undeclared_members_of_the_root_join_one_unit() {
        let mut value = minimal_config();
        value["extra"] = json!(true);
        value["other"] = json!(null);
        let (evaluation, _) = admitted(&value, 1_000_000);
        let known = root(&[], Check::KnownField);
        let candidates: Vec<&Unit> = evaluation
            .candidates()
            .map(|(unit, _)| unit)
            .filter(|unit| unit.logical() == known)
            .collect();
        assert_eq!(candidates.len(), 2);
        assert_ne!(candidates[0], candidates[1]);
        assert_eq!(issues(&evaluation), [(known, Code::UnknownField)]);
    }

    /// Each failure of the selection holds back the checks after it, through
    /// the chain of prerequisites, and the analysis's measure (D15:2130).
    #[test]
    fn each_selection_check_holds_back_the_checks_after_it() {
        let mut missing = minimal_config();
        missing.as_object_mut().unwrap().remove("schemaVersion");
        let mut number = minimal_config();
        number["schemaVersion"] = json!(0);
        let mut unsupported = minimal_config();
        unsupported["schemaVersion"] = json!("1");
        for (value, check, code) in [
            (json!([]), Check::ObjectRoot, Code::FieldTypeInvalid),
            (
                missing,
                Check::PresentSchemaVersion,
                Code::RequiredFieldMissing,
            ),
            (number, Check::StringSchemaVersion, Code::FieldTypeInvalid),
            (
                unsupported,
                Check::SupportedSchemaVersion,
                Code::SchemaVersionUnsupported,
            ),
        ] {
            let (evaluation, count) = admitted(&value, 1_000_000);
            assert_eq!(
                count
                    .unwrap_err()
                    .keys()
                    .map(IssueKey::unit)
                    .collect::<Vec<_>>(),
                [&selection(check)],
                "{check:?}"
            );
            assert_eq!(found(&evaluation), [(check, code, None)], "{check:?}");
            let statuses = statuses(&evaluation);
            let later: Vec<Unit> = units()
                .into_iter()
                .filter(|unit| {
                    *unit == CAPABILITY
                        || (unit.subject == selection(check).subject && unit.check > check)
                })
                .collect();
            for unit in &units() {
                let expected = later.contains(unit).then(|| vec![check]);
                assert_eq!(statuses[unit], expected, "{check:?}: {unit:?}");
            }
        }
    }

    /// The analysis's exact count is within the capability's bound at the
    /// bound and past it one unit later (D15:1887, 2006).
    #[test]
    fn the_analysis_is_measured_exactly_against_the_capability() {
        let (_, count) = admitted(&minimal_config(), 1_000_000);
        let units = count.unwrap();
        let (exact, within) = admitted(&minimal_config(), units);
        assert_eq!(within.unwrap(), units);
        assert_eq!(found(&exact), []);
        let (over, causes) = admitted(&minimal_config(), units - 1);
        assert_eq!(
            causes
                .unwrap_err()
                .keys()
                .map(IssueKey::unit)
                .collect::<Vec<_>>(),
            [&CAPABILITY]
        );
        assert_eq!(
            found(&over),
            [(
                Check::WithinCapability,
                Code::ResourceLimitExceeded,
                Some(Details::Limit(LimitExceeded {
                    bound: BoundId {
                        group: Group::ConfigurationInput,
                        name: "maxStructuralAnalysisUnits",
                    },
                    limit: Bound::new(units - 1).unwrap(),
                    actual: Actual::Exact(units),
                }))
            )]
        );
        assert_eq!(
            statuses(&over)[&owner_of(SemanticGroup::ProfileSpecification)],
            None
        );
    }

    /// The `profileSpecification` group's owner waits on Phase 0 alone: it
    /// stands when the selection fails, and waits when Phase 0 does.
    #[test]
    fn the_profile_specification_owner_waits_on_phase_0_alone() {
        let owner = owner_of(SemanticGroup::ProfileSpecification);
        let (blocked, _) = admitted(&json!([]), 1_000_000);
        assert_eq!(statuses(&blocked)[&owner], None);
        // Phase 0's failure, made up in an evaluation of its own.
        let phase_0 = Unit {
            phase: Phase::ResolverBootstrapAdmission,
            subject: Subject::Slot(Slot::ProjectProfile),
            check: Check::SupportedAssertion,
        };
        let causes = Evaluation::fabricated()
            .check(phase_0, Ok(()), |()| {
                Err::<(), _>(Blocking {
                    code: Code::SpecificationInputIncomplete,
                    reason: Some(Reason::MissingRequiredSlot),
                    details: None,
                })
            })
            .unwrap()
            .unwrap_err();
        let (waiting, count) = admitted_after(&minimal_config(), 1_000_000, Err(causes));
        assert!(count.is_ok());
        assert_eq!(
            statuses(&waiting)[&owner],
            Some(vec![Check::SupportedAssertion])
        );
    }

    #[test]
    fn phase_2_records_its_units_once() {
        let bytes = serde_json::to_vec(&minimal_config()).unwrap();
        let schema = Schema::for_model().unwrap();
        let mut evaluation = Evaluation::default();
        for twice in [false, true] {
            let document = materialize_file(Arc::from(bytes.clone()), limits()).unwrap();
            let result = admit_structure(
                &mut evaluation,
                document,
                &schema,
                structural(1_000_000),
                Ok(()),
            );
            if twice {
                assert_eq!(
                    result.err(),
                    Some(StructuralInvariant::Evaluation(
                        EvaluationInvariant::UnitRecordedTwice(OWNER)
                    ))
                );
            } else {
                assert!(result.unwrap().is_ok());
            }
        }
    }
}
