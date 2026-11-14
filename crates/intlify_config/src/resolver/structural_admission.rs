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
//! Each evaluation of a structural check the analysis found is a candidate
//! of a unit of its scope level (D15:2173): a required member, a value's
//! type, a value's closed constraints, or an object's undeclared members, at
//! a field of the root, of a profile entry, of a Target ID entry, or of a
//! Group ID entry, by the field's logical path from its entry, and the key
//! of an entry of `profiles`, `targetProfiles`, or `deploymentGroups`. An
//! entry's admitted key scopes the subjects under it by its identity, and a
//! key without the identifier syntax by its Subject content projection, so
//! such entries of one map join one unit while each stays a candidate of its
//! own (D15:2184, 2190). A candidate waits on the type check of the value
//! that holds it, or of its own value, a key on its map's type check, and
//! the root's own type check on the analysis's measure (D15:2149, Decision
//! 015-195). A value whose type check failed is that check's to report, so
//! its other checks are not evaluated (D15:2184).
//!
//! The analysis these checks admit is what later phases read, and a
//! configuration structural admission does not admit whole starts no
//! semantic phase (plan 025 PR 5f-2f). Phase 2's Findings carry their
//! occurrences once its Evidence comes (plan 025 PR 5f-2e).

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::materialize::{ByteSpan, MaterializedDocument};
use crate::structural::facts::{
    self, At, CandidateId, Entry, Facts, FieldPathId, Level, Outcome, ScopeId,
};
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

/// A subject of structural admission's scope levels (D15:2173, 2180): a
/// field of the root or of an identity map's entry, by its logical path from
/// the entry, or the key of an entry of `profiles`, of a profile's
/// `targetProfiles`, or of its `deploymentGroups`. The keys of the entries
/// that hold a subject scope it, the outermost first (D15:2190).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StructuralSubject {
    /// A field of the root.
    RootField(Field),
    /// The key of an entry of `profiles`.
    ProfileKey(EntryKey),
    /// A field of a profile entry, under the profile's key.
    ProfileField(EntryKey, Field),
    /// The key of an entry of a profile's `targetProfiles`, under the
    /// profile's key.
    TargetProfileKey(EntryKey, EntryKey),
    /// A field of a Target ID entry, under the profile's and the target's
    /// keys.
    TargetField(EntryKey, EntryKey, Field),
    /// The key of an entry of a profile's `deploymentGroups`, under the
    /// profile's key.
    DeploymentGroupKey(EntryKey, EntryKey),
    /// A field of a Group ID entry, under the profile's and the group's keys.
    GroupField(EntryKey, EntryKey, Field),
}

impl StructuralSubject {
    /// The logical subject, which every raw candidate of the unit joins: the
    /// subject without where a candidate looked, and each key without an
    /// identity as its content projection alone (D15:2184, 2190).
    pub(crate) fn logical(&self) -> Self {
        match self {
            Self::RootField(field) => Self::RootField(field.logical()),
            Self::ProfileKey(profile) => Self::ProfileKey(profile.logical()),
            Self::ProfileField(profile, field) => {
                Self::ProfileField(profile.logical(), field.logical())
            }
            Self::TargetProfileKey(profile, target) => {
                Self::TargetProfileKey(profile.logical(), target.logical())
            }
            Self::TargetField(profile, target, field) => {
                Self::TargetField(profile.logical(), target.logical(), field.logical())
            }
            Self::DeploymentGroupKey(profile, group) => {
                Self::DeploymentGroupKey(profile.logical(), group.logical())
            }
            Self::GroupField(profile, group, field) => {
                Self::GroupField(profile.logical(), group.logical(), field.logical())
            }
        }
    }
}

/// The key of an identity map's entry (D15:2181, 2184): its admitted
/// canonical identity, or, for a key without the identifier syntax, its
/// Subject content projection, a string, with where one raw entry's key is.
/// Identities order before content, and by their bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EntryKey {
    Identity(Arc<str>),
    /// None for the logical subject.
    Content(Option<ByteSpan>),
}

impl EntryKey {
    fn logical(&self) -> Self {
        match self {
            Self::Identity(identity) => Self::Identity(Arc::clone(identity)),
            Self::Content(_) => Self::Content(None),
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
    // alone name the scope levels' subjects.
    if let Some(facts) = analysis.facts() {
        scope_levels(evaluation, &analysis, facts, &within)?;
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

/// Records each candidate the analysis found as one candidate of the unit
/// over its subject at its scope level. A candidate waits on the result of
/// the type check it needs, and the root's own type check on `measured`, the
/// analysis's measure (D15:2149, 2184).
fn scope_levels<Policy, Target>(
    evaluation: &mut Evaluation,
    analysis: &StructuralAnalysis<Policy, Target>,
    facts: &Facts,
    measured: &Admitted<()>,
) -> Result<(), EvaluationInvariant> {
    let mut paths: BTreeMap<Option<FieldPathId>, Arc<[Box<str>]>> = BTreeMap::new();
    let mut scopes: BTreeMap<ScopeId, Vec<EntryKey>> = BTreeMap::new();
    let mut typed: BTreeMap<CandidateId, Admitted<()>> = BTreeMap::new();
    for (id, candidate) in facts.identified() {
        let (check, code) = match candidate.check {
            facts::Check::Required => (Check::RequiredField, Code::RequiredFieldMissing),
            facts::Check::Type => (Check::FieldType, Code::FieldTypeInvalid),
            facts::Check::Value | facts::Check::Key => (Check::FieldValue, Code::FieldValueInvalid),
            facts::Check::Known => (Check::KnownField, Code::UnknownField),
        };
        // The walk records a candidate after the type check it needs.
        let prerequisite = match candidate.after {
            Some(after) => typed
                .get(&after)
                .expect("a type check is recorded before what needs it")
                .clone(),
            None => measured.clone(),
        };
        // A key's subject is its entry's; a field's, its scope's.
        let entry = match (candidate.check, candidate.at) {
            (facts::Check::Key, At::Key(entry)) => Some(entry),
            _ => None,
        };
        let scope = entry.unwrap_or(candidate.scope);
        let level = facts.scope(scope).level;
        let keys = scopes
            .entry(scope)
            .or_insert_with(|| entry_keys(analysis, facts, scope));
        let subject = if entry.is_some() {
            key_subject(level, keys)
        } else {
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
            let field = Field {
                path,
                at: Some(candidate.at),
            };
            field_subject(level, keys, field)
        };
        let unit = Unit {
            phase: Phase::StructuralAdmission,
            subject: Subject::Structural(subject),
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

/// The keys of the entries that hold `scope`, the outermost first: none for
/// the root.
fn entry_keys<Policy, Target>(
    analysis: &StructuralAnalysis<Policy, Target>,
    facts: &Facts,
    scope: ScopeId,
) -> Vec<EntryKey> {
    let mut keys = Vec::new();
    let mut next = Some(scope);
    while let Some(id) = next {
        let scope = facts.scope(id);
        if let Some(entry) = scope.entry {
            keys.push(entry_key(analysis, &entry));
        }
        next = scope.parent;
    }
    keys.reverse();
    keys
}

/// The key of `entry`: its identity when it has the identifier syntax, and
/// otherwise its content projection with where its key is.
fn entry_key<Policy, Target>(
    analysis: &StructuralAnalysis<Policy, Target>,
    entry: &Entry,
) -> EntryKey {
    if entry.admitted {
        let key = analysis
            .admitted_key(entry)
            .expect("an admitted entry is a member of its map");
        EntryKey::Identity(Arc::from(key))
    } else {
        EntryKey::Content(Some(entry.key))
    }
}

/// The subject of the key of an entry at `level`, under `keys`, the keys of
/// the entries that hold it, then its own.
fn key_subject(level: Level, keys: &[EntryKey]) -> StructuralSubject {
    match (level, keys) {
        (Level::Profile, [profile]) => StructuralSubject::ProfileKey(profile.clone()),
        (Level::Target, [profile, target]) => {
            StructuralSubject::TargetProfileKey(profile.clone(), target.clone())
        }
        (Level::Group, [profile, group]) => {
            StructuralSubject::DeploymentGroupKey(profile.clone(), group.clone())
        }
        _ => unreachable!("an entry's keys are those of the entries above it and its own"),
    }
}

/// The subject of `field` at a scope of `level`, under `keys`, the keys of
/// the entries that hold the scope.
fn field_subject(level: Level, keys: &[EntryKey], field: Field) -> StructuralSubject {
    match (level, keys) {
        (Level::Root, []) => StructuralSubject::RootField(field),
        (Level::Profile, [profile]) => StructuralSubject::ProfileField(profile.clone(), field),
        (Level::Target, [profile, target]) => {
            StructuralSubject::TargetField(profile.clone(), target.clone(), field)
        }
        (Level::Group, [profile, group]) => {
            StructuralSubject::GroupField(profile.clone(), group.clone(), field)
        }
        _ => unreachable!("a scope's keys are those of the entries that hold it"),
    }
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
        Self::RootField(Field::of(path))
    }
}

#[cfg(test)]
impl Field {
    /// The logical field at `path`.
    pub(crate) fn of(path: &[&str]) -> Self {
        Self {
            path: path.iter().map(|&name| Box::from(name)).collect(),
            at: None,
        }
    }
}

#[cfg(test)]
impl EntryKey {
    /// The admitted key `key`.
    pub(crate) fn identity(key: &str) -> Self {
        Self::Identity(Arc::from(key))
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
        admit_structure, selection, EntryKey, Field, StructuralInvariant, StructuralSubject,
        CAPABILITY, OWNER,
    };

    use StructuralSubject as S;

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
        level(StructuralSubject::root(path), check)
    }

    /// The unit of `check` over `subject`.
    fn level(subject: StructuralSubject, check: Check) -> Unit {
        Unit {
            phase: Phase::StructuralAdmission,
            subject: Subject::Structural(subject),
            check,
        }
    }

    /// The unit of `check` over the field of the profile `app` at `path`.
    fn app(path: &[&str], check: Check) -> Unit {
        level(
            S::ProfileField(EntryKey::identity("app"), Field::of(path)),
            check,
        )
    }

    /// Whether `unit` is of a level below the root: an identity map's key or
    /// a field of its entry.
    fn below_root(unit: &Unit) -> bool {
        matches!(&unit.subject, Subject::Structural(subject) if !matches!(subject, S::RootField(_)))
    }

    /// How many candidates the unit `unit` over a logical subject has.
    fn candidates(evaluation: &Evaluation, unit: &Unit) -> usize {
        evaluation
            .candidates()
            .filter(|(raw, _)| raw.logical() == *unit)
            .count()
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
        assert_eq!(
            statuses
                .keys()
                .filter(|unit| !below_root(unit))
                .cloned()
                .collect::<Vec<_>>(),
            expected
        );
        assert!(statuses.keys().any(below_root));
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

    /// Whether a unit waits on the one a test makes block.
    type Waits<'a> = &'a dyn Fn(&Unit) -> bool;

    /// The root's own type check waits on the analysis's measure, every other
    /// check of a field on the type check of the value that holds the field,
    /// or of its own value, an entry's value's type check and its key on the
    /// map's type check, and so every check under an entry on what its entry
    /// waits on (D15:2149, Decision 015-195).
    #[test]
    fn each_check_of_the_scope_levels_waits_on_the_type_checks_it_needs() {
        let structural = |unit: &Unit| matches!(unit.subject, Subject::Structural(_));
        let typed = root(&[], Check::FieldType);
        let profiles = root(&["profiles"], Check::FieldType);
        let entry = app(&[], Check::FieldType);
        let cases: [(&Unit, Waits<'_>); 4] = [
            (&CAPABILITY, &structural),
            (&typed, &|unit| structural(unit) && *unit != typed),
            (&profiles, &|unit| {
                below_root(unit) || *unit == root(&["profiles"], Check::FieldValue)
            }),
            // The key of the entry waits on its map, not on the entry's value.
            (&entry, &|unit| {
                below_root(unit)
                    && *unit != entry
                    && !matches!(unit.subject, Subject::Structural(S::ProfileKey(_)))
            }),
        ];
        for (blocked, waits) in cases {
            let (evaluation, _) = admitted_in(
                Evaluation::blocking(blocked.clone()),
                &minimal_config(),
                1_000_000,
                Ok(()),
            );
            for (unit, causes) in statuses(&evaluation) {
                let expected = waits(&unit).then(|| vec![blocked.check]);
                assert_eq!(causes, expected, "{blocked:?}: {unit:?}");
            }
        }
    }

    /// Each identity map's entry scopes the subjects under it by its key, and
    /// its key is a unit of its own: a profile's, and a target's or a group's
    /// under it (D15:2173, 2190).
    #[test]
    fn each_entry_scopes_its_fields_and_has_a_key_of_its_own() {
        let (evaluation, _) = admitted(&minimal_config(), 1_000_000);
        let statuses = statuses(&evaluation);
        let key = EntryKey::identity;
        for unit in [
            level(S::ProfileKey(key("app")), Check::FieldValue),
            app(&[], Check::FieldType),
            app(&["projectId"], Check::RequiredField),
            app(&["requestedLocales", "*"], Check::FieldType),
            level(
                S::TargetProfileKey(key("app"), key("browser")),
                Check::FieldValue,
            ),
            level(
                S::TargetField(key("app"), key("browser"), Field::of(&["profile"])),
                Check::RequiredField,
            ),
            level(
                S::DeploymentGroupKey(key("app"), key("web")),
                Check::FieldValue,
            ),
            level(
                S::GroupField(key("app"), key("web"), Field::of(&["members", "*"])),
                Check::FieldValue,
            ),
        ] {
            assert_eq!(statuses.get(&unit), Some(&None), "{unit:?}");
        }
    }

    /// Keys without the identifier syntax share one unit, of their content
    /// projection, while each entry stays a candidate of its own, and so do
    /// the fields under them; an admitted key keeps its fields apart
    /// (D15:2184, 2190).
    #[test]
    fn entries_whose_keys_fail_join_one_unit_with_a_candidate_each() {
        let mut value = minimal_config();
        let declaration = value["profiles"]["app"].clone();
        value["profiles"]["Bad Key"] = declaration.clone();
        value["profiles"]["Other Key"] = declaration;
        let (evaluation, _) = admitted(&value, 1_000_000);
        let failing = level(S::ProfileKey(EntryKey::Content(None)), Check::FieldValue);
        assert_eq!(
            issues(&evaluation),
            [(failing.clone(), Code::FieldValueInvalid)]
        );
        assert_eq!(candidates(&evaluation, &failing), 2);
        let project = |key| {
            level(
                S::ProfileField(key, Field::of(&["projectId"])),
                Check::FieldType,
            )
        };
        assert_eq!(
            candidates(&evaluation, &project(EntryKey::Content(None))),
            2
        );
        assert_eq!(
            candidates(&evaluation, &project(EntryKey::identity("app"))),
            1
        );
        assert_eq!(
            statuses(&evaluation)[&project(EntryKey::Content(None))],
            None
        );
    }

    /// So do the keys of a profile's targets and groups that fail, each under
    /// the profile's key, and the fields under them (D15:2190).
    #[test]
    fn target_and_group_entries_whose_keys_fail_join_one_unit_each() {
        let mut value = minimal_config();
        let declaration = &mut value["profiles"]["app"];
        let target = declaration["targetProfiles"]["browser"].clone();
        declaration["targetProfiles"]["Bad Target"] = target.clone();
        declaration["targetProfiles"]["Other Target"] = target;
        let group = declaration["deploymentGroups"]["web"].clone();
        declaration["deploymentGroups"]["Bad Group"] = group.clone();
        declaration["deploymentGroups"]["Other Group"] = group;
        let (evaluation, _) = admitted(&value, 1_000_000);
        let app = || EntryKey::identity("app");
        let failing = || EntryKey::Content(None);
        let target_key = level(S::TargetProfileKey(app(), failing()), Check::FieldValue);
        let group_key = level(S::DeploymentGroupKey(app(), failing()), Check::FieldValue);
        assert_eq!(
            issues(&evaluation),
            [
                (target_key.clone(), Code::FieldValueInvalid),
                (group_key.clone(), Code::FieldValueInvalid),
            ]
        );
        for unit in [
            target_key,
            group_key,
            level(
                S::TargetField(app(), failing(), Field::of(&["profile"])),
                Check::RequiredField,
            ),
            level(
                S::GroupField(app(), failing(), Field::of(&["members"])),
                Check::RequiredField,
            ),
        ] {
            assert_eq!(candidates(&evaluation, &unit), 2, "{unit:?}");
        }
    }

    /// A field of another type under an entry leaves its closed value to its
    /// type check, and its elements form no unit (D15:2184).
    #[test]
    fn a_field_of_another_type_under_an_entry_has_no_units_below_it() {
        let mut value = minimal_config();
        value["profiles"]["app"]["requestedLocales"] = json!("en");
        let (evaluation, _) = admitted(&value, 1_000_000);
        assert_eq!(
            issues(&evaluation),
            [(
                app(&["requestedLocales"], Check::FieldType),
                Code::FieldTypeInvalid
            )]
        );
        let statuses = statuses(&evaluation);
        assert_eq!(
            statuses[&app(&["requestedLocales"], Check::FieldValue)],
            Some(vec![Check::FieldType])
        );
        assert!(!statuses.contains_key(&app(&["requestedLocales", "*"], Check::FieldType)));
    }

    /// Every element of an array is a candidate of the one unit over the
    /// path with `*`, which reports one Finding when any fails (D15:2184).
    #[test]
    fn the_elements_of_an_array_join_one_unit() {
        let mut value = minimal_config();
        value["profiles"]["app"]["requestedLocales"] = json!(["en", 1, "ja"]);
        let (evaluation, _) = admitted(&value, 1_000_000);
        let element = app(&["requestedLocales", "*"], Check::FieldType);
        assert_eq!(
            issues(&evaluation),
            [(element.clone(), Code::FieldTypeInvalid)]
        );
        assert_eq!(candidates(&evaluation, &element), 3);
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
