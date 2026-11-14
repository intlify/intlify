// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Phase 3, `profile-selection`, under the capability's
//! `policyComparable.configurationInput` bounds (D15:2131).
//!
//! Once structural admission admitted the root's `profiles` object, its
//! declarations are checked against `maxProfiles`, every declaration
//! counted, with their exact count (D15:1885). Each declared Profile ID is a
//! subject keyed by its identity (D15:2181) and is checked against
//! `maxProfileIdBytes` with its exact bytes (D15:1886). A declaration whose
//! key structural admission did not admit counts, but has no Profile ID
//! subject (D15:2184). One bound's violation suppresses no other (D15:2008).
//!
//! The External Profile Selector Input is a singleton subject (D15:1990),
//! whose checks record a unit only when the selector has what they examine
//! (Decision 015-196): `maxProfileIdBytes` for a string selector, with its
//! exact bytes or the smallest first-over witness (D15:1886); that more
//! than one declaration has a selector, for every invocation (D15:2295);
//! and, for a supplied selector, its Profile ID admission, which an
//! over-limit selector leaves to the bound (D15:2296), and that it names a
//! declared Profile ID (D15:2297). The checks that compare the selector
//! with the declarations wait on their count within the bound and on the
//! `profiles` object holding one, which structural admission reports
//! (D15:2363).
//!
//! Each Finding Occurrence of Phase 3 is observed where its check looked: the
//! declarations' count at the `profiles` object and a Profile ID's bytes at
//! its key, through Materialized Value Evidence in the file Phase 1 read
//! (D15:790), each beside the capability's bound (D15:836); the selector's
//! checks at the selector, beside the declarations as one location once
//! structural admission admitted them (D15:812; plan 025 interpretation
//! #215), and the bound's beside the capability's bound too. A selector
//! Finding leaves no declaration selected, and so no Resource Limit Policy,
//! so the capability's bound Phase 3 runs under is the selector's final
//! projection authority (D15:817; interpretation #217). Each check's result
//! is decided before the check runs, so that only a failure is observed.
//!
//! The selection is the declaration the selector names, or the only one,
//! once its Profile ID stays within the bound and structural admission
//! admitted the declaration whole (plan 025 interpretation #205). The core's
//! provisional selection then selects it; otherwise its causes hold every
//! later phase back.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;

use crate::input_limits::Bound;
use crate::materialize::{ByteSpan, MaterializedDocument, NodeId, NodeKind};
use crate::model::{valid_identity, ProfileId};
use crate::structural::selection::{
    ProvisionalSelection, Selection, SelectionInvariant, SelectorInput, SelectorShape,
    StringLengthFact,
};
use crate::structural::{AdmissionInvariant, Edge, StructuralAnalysis, StructuralLimits};

use super::capability::CapabilityBound;
use super::entry::{EntryFile, SELECTOR_ORIGIN};
use super::evaluation::{
    both, Actual, Admitted, Blocking, Check, Code, Evaluation, EvaluationInvariant, Found,
    Observation, Phase, Subject, Unit,
};
use super::evidence::{EvidenceInvariant, EvidenceProjection};
use super::evidence_component::{ComponentInvariant, ComponentPath};
use super::limits::{BoundId, Group};
use super::structural_admission::{
    declaration, nonempty_profiles, profile_key, profile_keys, profiles,
};
use super::tally::{actual, within, Tally};

/// A subject of profile selection's checks.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SelectionSubject {
    /// The root `profiles` object's declarations, under `maxProfiles`.
    Profiles,
    /// One declared Profile ID, its own key, under `maxProfileIdBytes`.
    ProfileId(ProfileId),
    /// The normalized External Profile Selector Input.
    Selector,
}

const MAX_PROFILES: BoundId = BoundId {
    group: Group::ConfigurationInput,
    name: "maxProfiles",
};
const MAX_PROFILE_ID_BYTES: BoundId = BoundId {
    group: Group::ConfigurationInput,
    name: "maxProfileIdBytes",
};

/// The unit of `subject`'s check under the capability.
pub(crate) const fn capability(subject: SelectionSubject) -> Unit {
    Unit {
        phase: Phase::ProfileSelection,
        subject: Subject::Selection(subject),
        check: Check::WithinCapability,
    }
}

/// The unit of the selector's `check`.
pub(crate) const fn selector(check: Check) -> Unit {
    Unit {
        phase: Phase::ProfileSelection,
        subject: Subject::Selection(SelectionSubject::Selector),
        check,
    }
}

/// A check's Finding with no details.
const fn issue(code: Code) -> Blocking {
    Blocking {
        code,
        reason: None,
        details: None,
    }
}

/// A resolver bug Phase 3 found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProfileSelectionInvariant {
    Evaluation(EvaluationInvariant),
    /// The analysis found a key with the identifier syntax that is no
    /// Profile ID.
    Admission(AdmissionInvariant),
    /// The core's provisional selection found a defect of its own, such as a
    /// selector observed under another bound.
    Selection(SelectionInvariant),
    /// What a check found could not be projected as Evidence.
    Evidence(EvidenceInvariant),
    /// The capability's bound has no component path.
    Component(ComponentInvariant),
    /// The checks admitted what the analysis, or the core's provisional
    /// selection, did not.
    UnreportedFailure,
}

impl From<EvaluationInvariant> for ProfileSelectionInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

/// Phase 3 over `analysis`, what Phase 2 admitted in `file`, if anything,
/// with `input`, the selector, under `limits`, the capability's values: the
/// declaration the checks select, provisionally, or the causes of its
/// absence (D15:2131). Every check is recorded whatever the others found,
/// those without what they need not evaluated with its causes.
///
/// # Errors
///
/// Returns the resolver bug Phase 3 found.
pub(crate) fn select_profile<Policy: DeserializeOwned, Target>(
    evaluation: &mut Evaluation,
    file: &EntryFile<'_>,
    analysis: Option<&StructuralAnalysis<Policy, Target>>,
    input: &SelectorInput,
    limits: StructuralLimits,
) -> Result<Admitted<ProvisionalSelection<Policy>>, ProfileSelectionInvariant> {
    let limit = limits.max_profile_id_bytes;
    if input.bound() != limit {
        // Selection runs under the bound the selector was observed under.
        return Err(ProfileSelectionInvariant::Selection(
            SelectionInvariant::BootstrapBoundMismatch,
        ));
    }
    let declarations = analysis
        .map(StructuralAnalysis::declarations)
        .transpose()
        .map_err(ProfileSelectionInvariant::Admission)?
        .flatten();
    let observer = Observer {
        file,
        profiles: analysis.and_then(StructuralAnalysis::profiles_value),
    };
    let bounds = bound_declarations(evaluation, &observer, declarations.as_ref(), limits)?;
    let declared = declarations
        .as_ref()
        .map_or(&[][..], |(_, ids)| ids.as_slice());
    let named = check_selector(evaluation, &observer, input, &bounds, declared)?;
    let id = match named {
        Ok(Some(id)) => Ok(id),
        Ok(None) => sole(evaluation, declared)?,
        Err(causes) => Err(causes),
    };
    let id = match id {
        Ok(id) => id,
        Err(causes) => return Ok(Err(causes)),
    };
    // Only a key structural admission admitted names the ID's subject
    // (D15:2184).
    let within = match bounds.ids.get(&id) {
        Some(result) => result.clone(),
        None => match evaluation.admitted_within(|unit| *unit == profile_key(id.as_str())) {
            Err(causes) => Err(causes),
            Ok(()) => return Err(ProfileSelectionInvariant::UnreportedFailure),
        },
    };
    if let Err(causes) = both(within, declaration(evaluation, id.as_str())) {
        return Ok(Err(causes));
    }
    // The checks decide, so a declaration they select and the core does not
    // is a resolver bug.
    let selection = analysis
        .ok_or(ProfileSelectionInvariant::UnreportedFailure)?
        .select(input)
        .map_err(ProfileSelectionInvariant::Selection)?;
    match selection {
        Selection::Selected(selection) => Ok(Ok(selection)),
        _ => Err(ProfileSelectionInvariant::UnreportedFailure),
    }
}

/// What the checks of the declarations established.
struct Bounds {
    /// How many declarations there are, once within the bound.
    count: Admitted<u64>,
    /// The result of each Profile ID whose key structural admission admitted.
    ids: BTreeMap<ProfileId, Admitted<()>>,
}

/// The checks of the declarations, the count and each ID within the
/// capability's bound, over `declarations`, how many the analysis found and
/// the IDs of those whose keys have the identifier syntax. While the
/// structural checks have not admitted the `profiles` object, the count is
/// not evaluated, with their causes. Each Profile ID is checked on its own
/// once they admitted its key, and has no subject otherwise (D15:1886, 2184).
fn bound_declarations(
    evaluation: &mut Evaluation,
    observer: &Observer<'_, '_>,
    declarations: Option<&(u64, Vec<ProfileId>)>,
    limits: StructuralLimits,
) -> Result<Bounds, ProfileSelectionInvariant> {
    let count = match profiles(evaluation) {
        // The checks decide, so an object they admit and the analysis did
        // not find is a resolver bug.
        Ok(()) => Ok(declarations
            .ok_or(ProfileSelectionInvariant::UnreportedFailure)?
            .0),
        Err(causes) => Err(causes),
    };
    // A count not evaluated finds nothing.
    let limit = limits.max_profiles;
    let found = match &count {
        Ok(count) => observed(
            within(MAX_PROFILES, limit, actual(Tally::exactly(*count), limit)),
            || beside(observer.declarations()?, MAX_PROFILES),
        )?,
        Err(_) => Ok(()),
    };
    let count = evaluation.check_found(capability(SelectionSubject::Profiles), count, |count| {
        found.map(|()| count)
    })?;
    let mut ids = BTreeMap::new();
    for id in declarations.map_or(&[][..], |(_, ids)| ids.as_slice()) {
        // A declaration whose key the checks did not admit has no subject in
        // a later phase (D15:2184); the analysis checks every key it finds.
        let key = profile_key(id.as_str());
        if evaluation.admitted_within(|unit| *unit == key).is_err() {
            continue;
        }
        let limit = limits.max_profile_id_bytes;
        let found = observed(
            within(
                MAX_PROFILE_ID_BYTES,
                limit,
                actual(Tally::of(id.as_str().len()), limit),
            ),
            || beside(observer.key(id)?, MAX_PROFILE_ID_BYTES),
        )?;
        let result = evaluation.check_found(
            capability(SelectionSubject::ProfileId(id.clone())),
            Ok(()),
            |()| found,
        )?;
        ids.insert(id.clone(), result);
    }
    Ok(Bounds { count, ids })
}

/// The selector's checks of `input` under its bound, each recorded only when
/// the selector has what it examines (Decision 015-196): the declared Profile
/// ID a supplied selector names, none for an absent selector once `bounds`,
/// the declarations' checks, require none, or the causes of neither.
/// `declared` is the declarations' Profile IDs.
fn check_selector(
    evaluation: &mut Evaluation,
    observer: &Observer<'_, '_>,
    input: &SelectorInput,
    bounds: &Bounds,
    declared: &[ProfileId],
) -> Result<Admitted<Option<ProfileId>>, ProfileSelectionInvariant> {
    let limit = input.bound();
    // Comparing the selector with the declarations assumes that `profiles`
    // holds one (D15:2363).
    let compared =
        both(bounds.count.clone(), nonempty_profiles(evaluation)).map(|(count, ())| count);
    let available = profiles(evaluation).is_ok();
    let observe = || observer.selector(input, limit, available);
    // The bound covers a string selector alone (D15:1886).
    let bounded = match input
        .length_fact(limit)
        .map_err(ProfileSelectionInvariant::Selection)?
    {
        Some(length) => {
            let actual = match length {
                StringLengthFact::Exact(bytes) => Actual::Exact(bytes),
                // An over-limit selector keeps no length, only the smallest
                // first-over witness.
                StringLengthFact::OverLimit => Actual::AtLeast(limit.get() + 1),
            };
            let found = observed(within(MAX_PROFILE_ID_BYTES, limit, actual), || {
                beside(observe()?, MAX_PROFILE_ID_BYTES)
            })?;
            evaluation.check_found(selector(Check::WithinCapability), Ok(()), |()| found)?
        }
        None => Ok(()),
    };
    let absent = input.shape() == SelectorShape::Absent;
    let required = if matches!(&compared, Ok(count) if absent && *count > 1) {
        Err(issue(Code::SelectorRequired))
    } else {
        Ok(())
    };
    let found = observed(required, observe)?;
    let required =
        evaluation.check_found(selector(Check::RequiredSelector), compared.clone(), |_| {
            found
        })?;
    if absent {
        return Ok(required.map(|()| None));
    }
    // An over-limit selector is the bound's to report (D15:2296).
    let text = input.text().filter(|text| valid_identity(text));
    let found = observed(text.ok_or(issue(Code::SelectorInvalid)), observe)?;
    let admitted = evaluation.check_found(selector(Check::ValidSelector), bounded, |()| found)?;
    let named = text
        .and_then(|text| declared.iter().find(|id| id.as_str() == text))
        .cloned();
    let found = observed(named.ok_or(issue(Code::SelectorUnknown)), observe)?;
    let named = evaluation.check_found(
        selector(Check::DeclaredSelector),
        both(admitted, compared),
        |_| found,
    )?;
    // A supplied selector meets the requirement, and naming a declaration
    // waits on all the requirement waits on.
    Ok(named.map(Some))
}

/// The Profile ID of the only declaration, which an absent selector selects
/// once one declaration requires none, or the causes of its key without the
/// identifier syntax, which leaves the selection no subject (D15:2184,
/// 3341). `declared` is the declarations' Profile IDs.
fn sole(
    evaluation: &Evaluation,
    declared: &[ProfileId],
) -> Result<Admitted<ProfileId>, ProfileSelectionInvariant> {
    match declared {
        [id] => Ok(Ok(id.clone())),
        [] => match profile_keys(evaluation) {
            Err(causes) => Ok(Err(causes)),
            Ok(()) => Err(ProfileSelectionInvariant::UnreportedFailure),
        },
        // The checks admitted no selector for several declarations.
        _ => Err(ProfileSelectionInvariant::UnreportedFailure),
    }
}

/// A check's result, `result`, decided before the check runs, with its
/// Finding observed through `observe`, so that a check that admits observes
/// nothing.
fn observed<T>(
    result: Result<T, Blocking>,
    observe: impl FnOnce() -> Result<Observation, ProfileSelectionInvariant>,
) -> Result<Result<T, Found>, ProfileSelectionInvariant> {
    Ok(match result {
        Ok(value) => Ok(value),
        Err(blocking) => Err(Found::observed(blocking, observe()?)),
    })
}

/// `observation` beside the capability's `bound`, which a bound's Finding
/// relates (D15:836).
fn beside(
    mut observation: Observation,
    bound: BoundId,
) -> Result<Observation, ProfileSelectionInvariant> {
    let path = ComponentPath::capability(CapabilityBound::PolicyComparable(bound))
        .map_err(ProfileSelectionInvariant::Component)?;
    observation
        .related
        .push(EvidenceProjection::resolver_input(path));
    Ok(observation)
}

/// Where Phase 3 observes what it finds: the file Phase 1 read, with the
/// value of the root's `profiles` member in the materialized document, when
/// structural admission found one.
struct Observer<'f, 'd> {
    file: &'f EntryFile<'f>,
    profiles: Option<(&'d MaterializedDocument, NodeId)>,
}

impl Observer<'_, '_> {
    /// The root's `profiles` object, at its token (D15:790).
    fn declarations(&self) -> Result<Observation, ProfileSelectionInvariant> {
        let (document, profiles) = self
            .profiles
            .ok_or(ProfileSelectionInvariant::UnreportedFailure)?;
        self.value(&[Edge::Member("profiles")], document.node(profiles).span())
    }

    /// The key of `id` in the root's `profiles` object, at the key's token,
    /// redacted once past the capability's bound (D15:776, 790; plan 025
    /// interpretation #216).
    fn key(&self, id: &ProfileId) -> Result<Observation, ProfileSelectionInvariant> {
        let (document, profiles) = self
            .profiles
            .ok_or(ProfileSelectionInvariant::UnreportedFailure)?;
        let NodeKind::Object(members) = document.node(profiles).kind() else {
            return Err(ProfileSelectionInvariant::UnreportedFailure);
        };
        let member = members
            .get(id.as_str())
            .ok_or(ProfileSelectionInvariant::UnreportedFailure)?;
        self.value(
            &[Edge::Member("profiles"), Edge::Member(id.as_str())],
            member.key_span(),
        )
    }

    /// The selector, `input`, under the capability's bound `limit`, its final
    /// projection authority (plan 025 interpretation #217); beside the
    /// declarations when they are `available` (D15:812). A selector Finding
    /// names no declaration, since a supplied selector naming a declared
    /// Profile ID within the bound passes every check, so its projection is
    /// never a safe Profile ID (D15:807).
    fn selector(
        &self,
        input: &SelectorInput,
        limit: Bound,
        available: bool,
    ) -> Result<Observation, ProfileSelectionInvariant> {
        let primary =
            EvidenceProjection::selector(SELECTOR_ORIGIN, input, limit, std::iter::empty())
                .map_err(ProfileSelectionInvariant::Evidence)?;
        let related = if available {
            vec![self.declarations()?.primary]
        } else {
            Vec::new()
        };
        Ok(Observation { primary, related })
    }

    /// The value at the internal path `edges`, at the token `span`.
    fn value(
        &self,
        edges: &[Edge<'_>],
        span: ByteSpan,
    ) -> Result<Observation, ProfileSelectionInvariant> {
        let primary = self
            .file
            .value_evidence(edges, span)
            .map_err(ProfileSelectionInvariant::Evidence)?;
        Ok(Observation {
            primary,
            related: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use crate::fixtures::{minimal_config, FixturePolicyReference, FixtureTargetReference};
    use crate::input_limits::Bound;
    use crate::materialize::materialize_file;
    use crate::materialize_tests::limits as input_limits;
    use crate::model::ProfileId;
    use crate::resolver::capability::CapabilityBound;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::evaluation::{
        Actual, Admitted, Check, Code, Details, Evaluation, IssueKey, LimitExceeded, Phase, Status,
        Subject, Unit,
    };
    use crate::resolver::evidence::{Evidence, EvidenceProjection};
    use crate::resolver::evidence_component::ComponentPath;
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::limits::{fixture_limits, BoundId};
    use crate::resolver::occurrence::{occurrences, redacted};
    use crate::resolver::structural_admission::{
        admit_structure, profile_key, selection, StructuralSubject,
    };
    use crate::resolver::token::{Sha256, Specification};
    use crate::structural::selection::{InvalidSelectorType, SelectionInvariant, SelectorInput};
    use crate::structural::{AuthoringSchema, StructuralLimits};

    use super::{
        capability, select_profile, selector, ProfileSelectionInvariant, SelectionSubject,
        MAX_PROFILES, MAX_PROFILE_ID_BYTES,
    };

    type Schema = AuthoringSchema<FixturePolicyReference, FixtureTargetReference>;

    fn bounds(profiles: u64, id_bytes: u64) -> StructuralLimits {
        StructuralLimits {
            max_profiles: Bound::new(profiles).unwrap(),
            max_profile_id_bytes: Bound::new(id_bytes).unwrap(),
            max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
        }
    }

    /// Phases 2 and 3 over `value` under `limits` with `input`: the
    /// evaluation, and the Profile ID Phase 3 selected or its causes.
    fn selecting(
        value: &Value,
        limits: StructuralLimits,
        input: &SelectorInput,
    ) -> (Evaluation, Admitted<String>) {
        selecting_in(Evaluation::default(), value, limits, input)
    }

    /// Phases 2 and 3 as [`selecting`] runs them, recording into
    /// `evaluation`.
    fn selecting_in(
        mut evaluation: Evaluation,
        value: &Value,
        limits: StructuralLimits,
        input: &SelectorInput,
    ) -> (Evaluation, Admitted<String>) {
        let bytes: Arc<[u8]> = Arc::from(serde_json::to_vec(value).unwrap());
        let document = materialize_file(Arc::clone(&bytes), input_limits()).unwrap();
        // The capability's `policyComparable` bounds classify the file's
        // members as well as bound Phase 3 (D15:776).
        let mut comparable = fixture_limits(100);
        comparable.configuration_input.structural = limits;
        let fixture = FileFixture::new(&comparable);
        let file = fixture.file(bytes);
        let analysis = admit_structure(
            &mut evaluation,
            &file,
            document,
            &Schema::for_model().unwrap(),
            limits,
            Ok(()),
        )
        .unwrap();
        let selected = select_profile(
            &mut evaluation,
            &file,
            analysis.as_ref().ok(),
            input,
            limits,
        )
        .unwrap()
        .map(|selection| selection.id().as_str().to_owned());
        (evaluation, selected)
    }

    /// Phases 2 and 3 over `value` under `limits`, the selector naming `app`.
    fn selected(value: &Value, limits: StructuralLimits) -> Evaluation {
        let input = SelectorInput::string("app", limits.max_profile_id_bytes);
        selecting(value, limits, &input).0
    }

    /// The configuration with a copy of its one declaration under each of
    /// `keys`, in place of `app`.
    fn declared(keys: &[&str]) -> Value {
        let mut config = minimal_config();
        let declaration = config["profiles"]["app"].clone();
        config["profiles"] = keys
            .iter()
            .map(|&key| (key.to_owned(), declaration.clone()))
            .collect();
        config
    }

    fn id(key: &str) -> SelectionSubject {
        SelectionSubject::ProfileId(serde_json::from_value::<ProfileId>(json!(key)).unwrap())
    }

    /// Each unit of the declarations' checks with its status.
    fn units(evaluation: &Evaluation) -> Vec<(Unit, Status)> {
        evaluation
            .statuses()
            .filter(|(unit, _)| {
                unit.phase == Phase::ProfileSelection
                    && unit.subject != Subject::Selection(SelectionSubject::Selector)
            })
            .map(|(unit, status)| (unit.clone(), status.clone()))
            .collect()
    }

    /// Each unit of the selector's checks, with the units of its causes when
    /// it was not evaluated.
    fn selector_units(evaluation: &Evaluation) -> Vec<(Check, Option<Vec<Unit>>)> {
        evaluation
            .statuses()
            .filter(|(unit, _)| unit.subject == Subject::Selection(SelectionSubject::Selector))
            .map(|(unit, status)| {
                let causes = match status {
                    Status::Evaluated => None,
                    Status::NotEvaluated(causes) => {
                        Some(causes.keys().map(IssueKey::unit).cloned().collect())
                    }
                };
                (unit.check, causes)
            })
            .collect()
    }

    /// Each Phase 3 Finding, as its unit and code.
    fn found(evaluation: &Evaluation) -> Vec<(Unit, Code)> {
        evaluation
            .issues()
            .filter(|key| key.unit().phase == Phase::ProfileSelection)
            .map(|key| (key.unit().clone(), key.code()))
            .collect()
    }

    /// Each Phase 3 bound exceeded, as its subject and the bound.
    fn exceeded(evaluation: &Evaluation) -> Vec<(Subject, LimitExceeded)> {
        evaluation
            .findings()
            .iter()
            .filter(|finding| finding.key.unit().phase == Phase::ProfileSelection)
            .filter_map(|finding| match finding.details {
                Some(Details::Limit(exceeded)) => {
                    Some((finding.key.unit().subject.clone(), *exceeded))
                }
                _ => None,
            })
            .collect()
    }

    fn over(bound: BoundId, limit: u64, actual: u64) -> LimitExceeded {
        LimitExceeded {
            bound,
            limit: Bound::new(limit).unwrap(),
            actual: Actual::Exact(actual),
        }
    }

    /// The units of the causes of `unit`, which was not evaluated.
    fn causes(evaluation: &Evaluation, unit: &Unit) -> Vec<Unit> {
        match evaluation.status(unit) {
            Some(Status::NotEvaluated(causes)) => {
                causes.keys().map(IssueKey::unit).cloned().collect()
            }
            status => panic!("{unit:?}: {status:?}"),
        }
    }

    /// The units of what held the selection back.
    fn held(selected: Admitted<String>) -> BTreeSet<Unit> {
        selected
            .unwrap_err()
            .keys()
            .map(IssueKey::unit)
            .cloned()
            .collect()
    }

    #[test]
    fn the_declarations_count_under_the_capability_with_their_exact_count() {
        let config = declared(&["app", "web"]);
        let evaluation = selected(&config, bounds(2, 64));
        assert_eq!(
            units(&evaluation),
            [
                (capability(SelectionSubject::Profiles), Status::Evaluated),
                (capability(id("app")), Status::Evaluated),
                (capability(id("web")), Status::Evaluated),
            ]
        );
        assert_eq!(found(&evaluation), []);
        let evaluation = selected(&config, bounds(1, 64));
        assert_eq!(
            exceeded(&evaluation),
            [(
                Subject::Selection(SelectionSubject::Profiles),
                over(MAX_PROFILES, 1, 2)
            )]
        );
    }

    /// Each declared ID reports its own exact bytes, and the count's bound
    /// and the IDs' bound never suppress each other (D15:1886, 2008).
    #[test]
    fn each_declared_profile_id_reports_its_own_exact_bytes() {
        let config = declared(&["app", "storefront", "web"]);
        let evaluation = selected(&config, bounds(1, 5));
        assert_eq!(
            exceeded(&evaluation),
            [
                (
                    Subject::Selection(SelectionSubject::Profiles),
                    over(MAX_PROFILES, 1, 3)
                ),
                (
                    Subject::Selection(id("storefront")),
                    over(MAX_PROFILE_ID_BYTES, 5, 10)
                ),
            ]
        );
        assert_eq!(units(&evaluation).len(), 4);
    }

    /// A declaration whose key lacks the identifier syntax counts, but has no
    /// Profile ID subject (D15:1885, 2184).
    #[test]
    fn a_declaration_whose_key_fails_counts_without_an_id_subject() {
        let config = declared(&["Admin Site", "app"]);
        let evaluation = selected(&config, bounds(1, 64));
        assert_eq!(
            units(&evaluation)
                .into_iter()
                .map(|(unit, _)| unit)
                .collect::<Vec<_>>(),
            [
                capability(SelectionSubject::Profiles),
                capability(id("app"))
            ]
        );
        assert_eq!(
            exceeded(&evaluation),
            [(
                Subject::Selection(SelectionSubject::Profiles),
                over(MAX_PROFILES, 1, 2)
            )]
        );
    }

    /// Without a `profiles` object the structural checks admitted, nothing
    /// is counted: the count waits on their Findings (D15:2131).
    #[test]
    fn without_an_admitted_profiles_object_the_count_waits_on_its_findings() {
        let field = |check| Unit {
            phase: Phase::StructuralAdmission,
            subject: Subject::Structural(StructuralSubject::root(&["profiles"])),
            check,
        };
        let mut missing = minimal_config();
        missing.as_object_mut().unwrap().remove("profiles");
        let mut array = minimal_config();
        array["profiles"] = json!([]);
        for (config, check) in [(missing, Check::RequiredField), (array, Check::FieldType)] {
            let evaluation = selected(&config, bounds(1, 64));
            let count = capability(SelectionSubject::Profiles);
            assert_eq!(units(&evaluation).len(), 1);
            assert_eq!(causes(&evaluation, &count), [field(check)]);
        }
    }

    /// A failure elsewhere in the root holds back no count, which waits on
    /// the `profiles` object alone (D15:2131, 2367).
    #[test]
    fn a_failure_elsewhere_in_the_root_holds_back_no_count() {
        let mut config = declared(&["app", "web"]);
        config["$schema"] = json!(1);
        let evaluation = selected(&config, bounds(1, 64));
        assert!(evaluation
            .issues()
            .any(|key| key.unit().phase == Phase::StructuralAdmission));
        assert_eq!(
            exceeded(&evaluation),
            [(
                Subject::Selection(SelectionSubject::Profiles),
                over(MAX_PROFILES, 1, 2)
            )]
        );
    }

    /// Without the analysis, the count waits on the causes of its absence,
    /// and no Profile ID has a subject (D15:2131, 2184).
    #[test]
    fn without_an_analysis_the_count_waits_on_its_causes() {
        let mut config = minimal_config();
        config["schemaVersion"] = json!("1");
        let evaluation = selected(&config, bounds(1, 64));
        let count = capability(SelectionSubject::Profiles);
        assert_eq!(units(&evaluation).len(), 1);
        assert_eq!(
            causes(&evaluation, &count),
            [selection(Check::SupportedSchemaVersion)]
        );
    }

    /// The selector's checks are recorded only where the selector has what
    /// they examine: the requirement always, the bound for a string, and
    /// its admission and its declaration for a supplied selector (Decision
    /// 015-196).
    #[test]
    fn a_selector_check_is_recorded_only_with_what_it_examines() {
        let limits = bounds(1, 64);
        let bound = limits.max_profile_id_bytes;
        let config = minimal_config();
        let checks = |input: &SelectorInput| -> Vec<Check> {
            let (evaluation, _) = selecting(&config, limits, input);
            selector_units(&evaluation)
                .into_iter()
                .map(|(check, _)| check)
                .collect()
        };
        assert_eq!(
            checks(&SelectorInput::absent(bound)),
            [Check::RequiredSelector]
        );
        assert_eq!(
            checks(&SelectorInput::string("app", bound)),
            [
                Check::WithinCapability,
                Check::RequiredSelector,
                Check::ValidSelector,
                Check::DeclaredSelector,
            ]
        );
        assert_eq!(
            checks(&SelectorInput::invalid_type(
                InvalidSelectorType::Number,
                bound
            )),
            [
                Check::RequiredSelector,
                Check::ValidSelector,
                Check::DeclaredSelector,
            ]
        );
    }

    /// An omitted selector selects the only declaration, and a string one the
    /// declaration it names, with no Finding.
    #[test]
    fn a_selector_selects_the_declaration_it_names_or_the_only_one() {
        let limits = bounds(2, 64);
        let bound = limits.max_profile_id_bytes;
        let (evaluation, selected) =
            selecting(&minimal_config(), limits, &SelectorInput::absent(bound));
        assert_eq!(selected, Ok("app".to_owned()));
        assert_eq!(found(&evaluation), []);
        let input = SelectorInput::string("web", bound);
        let (evaluation, selected) = selecting(&declared(&["app", "web"]), limits, &input);
        assert_eq!(selected, Ok("web".to_owned()));
        assert_eq!(found(&evaluation), []);
    }

    /// More than one declaration and no selector is `selector-required`, and
    /// nothing is selected (D15:2295).
    #[test]
    fn more_than_one_declaration_requires_a_selector() {
        let limits = bounds(2, 64);
        let input = SelectorInput::absent(limits.max_profile_id_bytes);
        let (evaluation, selected) = selecting(&declared(&["app", "web"]), limits, &input);
        let required = selector(Check::RequiredSelector);
        assert_eq!(
            found(&evaluation),
            [(required.clone(), Code::SelectorRequired)]
        );
        assert_eq!(held(selected), BTreeSet::from([required]));
    }

    /// A selector of another type, or a string without the identifier
    /// syntax, is `selector-invalid`, and its declaration is not looked up
    /// (D15:2296).
    #[test]
    fn a_selector_without_a_profile_id_is_invalid() {
        let limits = bounds(1, 64);
        let bound = limits.max_profile_id_bytes;
        let valid = selector(Check::ValidSelector);
        for input in [
            SelectorInput::invalid_type(InvalidSelectorType::Object, bound),
            SelectorInput::string("Admin Site", bound),
        ] {
            let (evaluation, selected) = selecting(&minimal_config(), limits, &input);
            assert_eq!(found(&evaluation), [(valid.clone(), Code::SelectorInvalid)]);
            assert_eq!(
                causes(&evaluation, &selector(Check::DeclaredSelector)),
                std::slice::from_ref(&valid)
            );
            assert_eq!(held(selected), BTreeSet::from([valid.clone()]));
        }
    }

    /// An over-limit selector reports the bound with the smallest first-over
    /// witness instead of `selector-invalid`, and nothing after it is
    /// evaluated (D15:1886, 2296).
    #[test]
    fn an_over_limit_selector_reports_only_the_bound() {
        let limits = bounds(1, 4);
        let input = SelectorInput::string("storefront", limits.max_profile_id_bytes);
        let (evaluation, selected) = selecting(&minimal_config(), limits, &input);
        let bounded = selector(Check::WithinCapability);
        assert_eq!(
            exceeded(&evaluation),
            [(
                Subject::Selection(SelectionSubject::Selector),
                LimitExceeded {
                    bound: MAX_PROFILE_ID_BYTES,
                    limit: Bound::new(4).unwrap(),
                    actual: Actual::AtLeast(5),
                }
            )]
        );
        assert_eq!(
            selector_units(&evaluation),
            [
                (Check::WithinCapability, None),
                (Check::RequiredSelector, None),
                (Check::ValidSelector, Some(vec![bounded.clone()])),
                (Check::DeclaredSelector, Some(vec![bounded.clone()])),
            ]
        );
        assert_eq!(held(selected), BTreeSet::from([bounded]));
    }

    /// A selector naming no declaration is `selector-unknown` (D15:2297).
    #[test]
    fn a_selector_naming_no_declaration_is_unknown() {
        let limits = bounds(1, 64);
        let input = SelectorInput::string("admin", limits.max_profile_id_bytes);
        let (evaluation, selected) = selecting(&minimal_config(), limits, &input);
        let named = selector(Check::DeclaredSelector);
        assert_eq!(found(&evaluation), [(named.clone(), Code::SelectorUnknown)]);
        assert_eq!(held(selected), BTreeSet::from([named]));
    }

    /// The checks comparing the selector with the declarations wait on their
    /// count and on a `profiles` object holding one (D15:2363).
    #[test]
    fn the_comparisons_wait_on_the_declarations() {
        let limits = bounds(1, 64);
        let input = SelectorInput::string("app", limits.max_profile_id_bytes);
        let count = capability(SelectionSubject::Profiles);
        let (evaluation, _) = selecting(&declared(&["app", "web"]), limits, &input);
        assert_eq!(
            selector_units(&evaluation),
            [
                (Check::WithinCapability, None),
                (Check::RequiredSelector, Some(vec![count.clone()])),
                (Check::ValidSelector, None),
                (Check::DeclaredSelector, Some(vec![count])),
            ]
        );
        let empty = Unit {
            phase: Phase::StructuralAdmission,
            subject: Subject::Structural(StructuralSubject::root(&["profiles"])),
            check: Check::FieldValue,
        };
        let mut config = minimal_config();
        config["profiles"] = json!({});
        let (evaluation, _) = selecting(&config, limits, &input);
        assert_eq!(
            selector_units(&evaluation),
            [
                (Check::WithinCapability, None),
                (Check::RequiredSelector, Some(vec![empty.clone()])),
                (Check::ValidSelector, None),
                (Check::DeclaredSelector, Some(vec![empty])),
            ]
        );
    }

    /// The only declaration whose key lacks the identifier syntax, or whose
    /// Profile ID exceeds the bound, is selected by no omitted selector: its
    /// key's Finding, or its ID's, holds the selection back (D15:2184, 3341).
    #[test]
    fn the_only_declaration_without_an_admitted_id_is_not_selected() {
        let key = Unit {
            phase: Phase::StructuralAdmission,
            subject: Subject::Structural(StructuralSubject::ProfileKey(
                crate::resolver::structural_admission::EntryKey::Content(None),
            )),
            check: Check::FieldValue,
        };
        let limits = bounds(1, 5);
        let input = SelectorInput::absent(limits.max_profile_id_bytes);
        let (_, selected) = selecting(&declared(&["Admin Site"]), limits, &input);
        assert_eq!(held(selected), BTreeSet::from([key]));
        let (_, selected) = selecting(&declared(&["storefront"]), limits, &input);
        assert_eq!(
            held(selected),
            BTreeSet::from([capability(id("storefront"))])
        );
    }

    /// A structural failure inside the selected declaration holds the
    /// selection back, and one outside it does not (plan 025 interpretation
    /// #205, D15:2367, 3434).
    #[test]
    fn only_a_declaration_admitted_whole_is_selected() {
        let limits = bounds(2, 64);
        let input = SelectorInput::string("app", limits.max_profile_id_bytes);
        let mut config = declared(&["app", "web"]);
        config["profiles"]["web"]["projectId"] = json!(1);
        let (_, selected) = selecting(&config, limits, &input);
        assert_eq!(selected, Ok("app".to_owned()));
        config["profiles"]["app"]["projectId"] = json!(1);
        let (_, selected) = selecting(&config, limits, &input);
        assert!(held(selected)
            .iter()
            .all(|unit| unit.phase == Phase::StructuralAdmission));
    }

    /// A key the structural checks did not admit leaves the selection no
    /// subject, whatever the selector names (D15:2184).
    #[test]
    fn a_selected_key_the_checks_did_not_admit_holds_the_selection_back() {
        let limits = bounds(1, 64);
        let input = SelectorInput::string("app", limits.max_profile_id_bytes);
        let key = profile_key("app");
        let blocked = Evaluation::blocking(key.clone());
        let (evaluation, selected) = selecting_in(blocked, &minimal_config(), limits, &input);
        assert_eq!(units(&evaluation).len(), 1);
        assert_eq!(held(selected), BTreeSet::from([key]));
    }

    /// The comparisons wait on the closed values of `profiles` alone: another
    /// closed value of the root that holds back nothing else holds them back
    /// no more (D15:2363, 2367).
    #[test]
    fn the_comparisons_wait_on_no_other_closed_value() {
        let limits = bounds(1, 64);
        let input = SelectorInput::string("app", limits.max_profile_id_bytes);
        let version = Unit {
            phase: Phase::StructuralAdmission,
            subject: Subject::Structural(StructuralSubject::root(&["schemaVersion"])),
            check: Check::FieldValue,
        };
        let blocked = Evaluation::blocking(version);
        let (evaluation, selected) = selecting_in(blocked, &minimal_config(), limits, &input);
        assert_eq!(
            selector_units(&evaluation),
            [
                (Check::WithinCapability, None),
                (Check::RequiredSelector, None),
                (Check::ValidSelector, None),
                (Check::DeclaredSelector, None),
            ]
        );
        assert_eq!(selected, Ok("app".to_owned()));
    }

    /// A selector observed under another bound is a resolver bug.
    #[test]
    fn a_selector_observed_under_another_bound_is_a_resolver_bug() {
        let limits = bounds(1, 64);
        let input = SelectorInput::string("app", Bound::new(65).unwrap());
        let mut evaluation = Evaluation::default();
        let fixture = FileFixture::new(&fixture_limits(100));
        let file = fixture.file(Arc::from(&b"{}"[..]));
        let analysis: Option<
            &crate::structural::StructuralAnalysis<FixturePolicyReference, FixtureTargetReference>,
        > = None;
        assert_eq!(
            select_profile(&mut evaluation, &file, analysis, &input, limits).err(),
            Some(ProfileSelectionInvariant::Selection(
                SelectionInvariant::BootstrapBoundMismatch
            ))
        );
    }

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// A Finding Occurrence's unit, with the views of its primary and related
    /// Evidence.
    type Observed = (Unit, Value, Vec<Value>);

    /// What Phase 3 found over `value` under `limits` with `input`, each
    /// Finding Occurrence in 015's order and checked against its Finding
    /// Registry row, and the file's bytes.
    fn observed(
        value: &Value,
        limits: StructuralLimits,
        input: &SelectorInput,
    ) -> (Vec<u8>, Vec<Observed>) {
        let (evaluation, _) = selecting(value, limits, input);
        let candidates = evaluation.occurrence_candidates();
        let tokens = EvidenceTokens::assign(PROFILE, redacted(candidates), &Sha256);
        let found = occurrences(candidates, PROFILE, &tokens)
            .unwrap()
            .iter()
            .filter(|occurrence| occurrence.owner().phase == Phase::ProfileSelection)
            .map(|occurrence| {
                (
                    occurrence.owner().clone(),
                    occurrence.primary().view(),
                    occurrence.related().iter().map(Evidence::view).collect(),
                )
            })
            .collect();
        (serde_json::to_vec(value).unwrap(), found)
    }

    /// The view of the span `text` occupies in `source`, at its first
    /// occurrence.
    fn span(source: &[u8], text: &[u8]) -> Value {
        let start = source
            .windows(text.len())
            .position(|window| window == text)
            .unwrap();
        json!({ "start": start.to_string(), "end": (start + text.len()).to_string() })
    }

    /// Where the configuration-value Evidence `view` locates its value, and
    /// its span.
    fn located(view: &Value) -> (Value, Value) {
        assert_eq!(view["kind"], "configuration-value", "{view}");
        assert_eq!(view["origin"], "configuration-file", "{view}");
        (view["value"].clone(), view["span"].clone())
    }

    /// Where `config`'s declarations are in `source`: the `profiles` object
    /// at its token.
    fn at_declarations(source: &[u8], config: &Value) -> (Value, Value) {
        (
            json!({ "exact-json-pointer": "/profiles" }),
            span(source, &serde_json::to_vec(&config["profiles"]).unwrap()),
        )
    }

    /// The view of the capability's `bound`, which a bound's Finding
    /// relates.
    fn bound_view(bound: BoundId) -> Value {
        let tokens = EvidenceTokens::assign(PROFILE, redacted(&[]), &Sha256);
        EvidenceProjection::resolver_input(
            ComponentPath::capability(CapabilityBound::PolicyComparable(bound)).unwrap(),
        )
        .retained(PROFILE, &tokens)
        .unwrap()
        .view()
    }

    /// The declarations' count is observed at the `profiles` object, beside
    /// the capability's bound (D15:790, 836).
    #[test]
    fn the_count_is_observed_at_the_declarations_beside_the_bound() {
        let config = declared(&["app", "web"]);
        let limits = bounds(1, 64);
        let input = SelectorInput::string("app", limits.max_profile_id_bytes);
        let (source, found) = observed(&config, limits, &input);
        assert_eq!(found.len(), 1);
        let (unit, primary, related) = &found[0];
        assert_eq!(*unit, capability(SelectionSubject::Profiles));
        assert_eq!(located(primary), at_declarations(&source, &config));
        assert_eq!(*related, [bound_view(MAX_PROFILES)]);
    }

    /// A declared Profile ID past the bound is observed at its key, which the
    /// same bound leaves unsafe and so redacted, beside the capability's
    /// bound (D15:776, 790; plan 025 interpretation #216).
    #[test]
    fn a_profile_id_past_the_bound_is_observed_at_its_redacted_key() {
        let config = declared(&["catalog"]);
        let limits = bounds(1, 3);
        let input = SelectorInput::absent(limits.max_profile_id_bytes);
        let (source, found) = observed(&config, limits, &input);
        assert_eq!(found.len(), 1);
        let (unit, primary, related) = &found[0];
        assert_eq!(*unit, capability(id("catalog")));
        let (path, at) = located(primary);
        let segments = path["redacted-logical-path"].as_array().unwrap();
        assert_eq!(segments.len(), 2, "{path}");
        assert_eq!(segments[0], json!({ "safe-member": "profiles" }));
        assert!(segments[1].get("redacted-member").is_some(), "{path}");
        assert_eq!(at, span(&source, br#""catalog""#));
        assert_eq!(*related, [bound_view(MAX_PROFILE_ID_BYTES)]);
    }

    /// Each selector check is observed at the selector a file entry's
    /// `profile-selector-cli` slot supplies, projected under the capability's
    /// bound, beside the declarations, and the bound's beside the
    /// capability's bound too (D15:804-812; plan 025 interpretations #215,
    /// #217).
    #[test]
    fn each_selector_check_observes_the_selector_beside_the_declarations() {
        let pair = declared(&["app", "web"]);
        let one = declared(&["app"]);
        let limits = bounds(2, 3);
        let bound = limits.max_profile_id_bytes;
        let capability = bound_view(MAX_PROFILE_ID_BYTES);
        let cases = [
            (
                &pair,
                SelectorInput::absent(bound),
                Check::RequiredSelector,
                json!({ "absent": null }),
            ),
            (
                &one,
                SelectorInput::string("catalog", bound),
                Check::WithinCapability,
                json!({ "redacted-value": { "type": "string", "length": "over-limit" } }),
            ),
            (
                &one,
                SelectorInput::invalid_type(InvalidSelectorType::Number, bound),
                Check::ValidSelector,
                json!({ "redacted-value": { "type": "number" } }),
            ),
            (
                &one,
                SelectorInput::string("A b", bound),
                Check::ValidSelector,
                json!({ "redacted-value": { "type": "string", "length": { "exact": "3" } } }),
            ),
            (
                &one,
                SelectorInput::string("web", bound),
                Check::DeclaredSelector,
                json!({ "redacted-value": { "type": "string", "length": { "exact": "3" } } }),
            ),
        ];
        for (config, input, check, projection) in cases {
            let (source, found) = observed(config, limits, &input);
            assert_eq!(found.len(), 1, "{check:?}");
            let (unit, primary, related) = &found[0];
            assert_eq!(*unit, selector(check));
            assert_eq!(primary["kind"], "profile-selector");
            assert_eq!(primary["origin"], "profile-selector-cli");
            // A redacted selector's token stands for its type and length
            // alone.
            let mut kept = primary["selector"].clone();
            if let Some(value) = kept.get_mut("redacted-value") {
                assert!(value.as_object_mut().unwrap().remove("token").is_some());
            }
            assert_eq!(kept, projection, "{check:?}");
            let (values, bounds): (Vec<&Value>, Vec<&Value>) = related
                .iter()
                .partition(|view| view["kind"] == "configuration-value");
            assert_eq!(
                values.into_iter().map(located).collect::<Vec<_>>(),
                [at_declarations(&source, config)]
            );
            let expected = if check == Check::WithinCapability {
                vec![&capability]
            } else {
                Vec::new()
            };
            assert_eq!(bounds, expected, "{check:?}");
        }
    }

    /// The declarations are related only once structural admission admitted
    /// the `profiles` object (D15:812; plan 025 interpretation #215).
    #[test]
    fn the_declarations_are_related_only_once_admitted() {
        let mut config = minimal_config();
        config["profiles"] = json!([]);
        let limits = bounds(1, 3);
        let bound = limits.max_profile_id_bytes;
        let number = SelectorInput::invalid_type(InvalidSelectorType::Number, bound);
        let (_, found) = observed(&config, limits, &number);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, selector(Check::ValidSelector));
        assert!(found[0].2.is_empty());
        let over = SelectorInput::string("catalog", bound);
        let (_, found) = observed(&config, limits, &over);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, selector(Check::WithinCapability));
        assert_eq!(found[0].2, [bound_view(MAX_PROFILE_ID_BYTES)]);
    }
}
