// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Phase 3, `profile-selection`, its checks of the declarations
//! under the capability's `policyComparable.configurationInput` bounds
//! (D15:2131). Once structural admission admitted the root's `profiles`
//! object, its declarations are checked against `maxProfiles`, every
//! declaration counted, with their exact count (D15:1885). Each declared
//! Profile ID is a subject keyed by its identity (D15:2181) and is checked
//! against `maxProfileIdBytes` with its exact bytes (D15:1886). A
//! declaration whose key structural admission did not admit counts, but has
//! no Profile ID subject (D15:2184). One bound's violation suppresses no
//! other (D15:2008).
//!
//! The selector's checks, and the selection they gate, come with the
//! selector's units (plan 025 PR 5f-3b). Until then the core's provisional
//! selection gates the later phases.

use crate::model::ProfileId;
use crate::structural::{AdmissionInvariant, StructuralAnalysis, StructuralLimits};

use super::evaluation::{Admitted, Check, Evaluation, EvaluationInvariant, Phase, Subject, Unit};
use super::limits::{BoundId, Group};
use super::structural_admission::{profile_key, profiles};
use super::tally::{actual, within, Tally};

/// A subject of profile selection's checks under the capability.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SelectionSubject {
    /// The root `profiles` object's declarations, under `maxProfiles`.
    Profiles,
    /// One declared Profile ID, its own key, under `maxProfileIdBytes`.
    ProfileId(ProfileId),
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

/// A resolver bug Phase 3 found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProfileSelectionInvariant {
    Evaluation(EvaluationInvariant),
    /// The analysis found a key with the identifier syntax that is no
    /// Profile ID.
    Admission(AdmissionInvariant),
    /// The checks admitted the root's `profiles` object, and the analysis
    /// found no object there.
    UnreportedFailure,
}

impl From<EvaluationInvariant> for ProfileSelectionInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

/// Phase 3's checks of the declarations under `limits`, the capability's
/// values, over `analysis`, what Phase 2 admitted, if anything (D15:2131).
/// While the structural checks have not admitted the `profiles` object, the
/// declarations' unit is not evaluated, with their causes. Each Profile ID
/// is checked on its own once they admitted its key, and has no subject
/// otherwise (D15:1886, 2184).
///
/// # Errors
///
/// Returns the resolver bug Phase 3 found.
pub(crate) fn bound_declarations<Policy, Target>(
    evaluation: &mut Evaluation,
    analysis: Option<&StructuralAnalysis<Policy, Target>>,
    limits: StructuralLimits,
) -> Result<(), ProfileSelectionInvariant> {
    let declarations = analysis
        .map(StructuralAnalysis::declarations)
        .transpose()
        .map_err(ProfileSelectionInvariant::Admission)?
        .flatten();
    let count = match profiles(evaluation) {
        // The checks decide, so an object they admit and the analysis did
        // not find is a resolver bug.
        Ok(()) => Ok(declarations
            .as_ref()
            .ok_or(ProfileSelectionInvariant::UnreportedFailure)?
            .0),
        Err(causes) => Err(causes),
    };
    let _: Admitted<()> =
        evaluation.check(capability(SelectionSubject::Profiles), count, |count| {
            let limit = limits.max_profiles;
            within(MAX_PROFILES, limit, actual(Tally::exactly(count), limit))
        })?;
    let Some((_, ids)) = declarations else {
        return Ok(());
    };
    for id in ids {
        // A declaration whose key the checks did not admit has no subject in
        // a later phase (D15:2184); the analysis checks every key it finds.
        let key = profile_key(id.as_str());
        if evaluation.admitted_within(|unit| *unit == key).is_err() {
            continue;
        }
        let bytes = Tally::of(id.as_str().len());
        let _: Admitted<()> =
            evaluation.check(capability(SelectionSubject::ProfileId(id)), Ok(()), |()| {
                let limit = limits.max_profile_id_bytes;
                within(MAX_PROFILE_ID_BYTES, limit, actual(bytes, limit))
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{json, Value};

    use crate::fixtures::{minimal_config, FixturePolicyReference, FixtureTargetReference};
    use crate::input_limits::Bound;
    use crate::materialize::materialize_file;
    use crate::materialize_tests::limits as input_limits;
    use crate::model::ProfileId;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::evaluation::{
        Actual, Check, Details, Evaluation, IssueKey, LimitExceeded, Phase, Status, Subject, Unit,
    };
    use crate::resolver::limits::{fixture_limits, BoundId};
    use crate::resolver::structural_admission::{admit_structure, selection, StructuralSubject};
    use crate::structural::{AuthoringSchema, StructuralLimits};

    use super::{
        bound_declarations, capability, SelectionSubject, MAX_PROFILES, MAX_PROFILE_ID_BYTES,
    };

    type Schema = AuthoringSchema<FixturePolicyReference, FixtureTargetReference>;

    fn bounds(profiles: u64, id_bytes: u64) -> StructuralLimits {
        StructuralLimits {
            max_profiles: Bound::new(profiles).unwrap(),
            max_profile_id_bytes: Bound::new(id_bytes).unwrap(),
            max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
        }
    }

    /// Phases 2 and 3 over `value` under `limits`.
    fn selected(value: &Value, limits: StructuralLimits) -> Evaluation {
        let mut evaluation = Evaluation::default();
        let bytes: Arc<[u8]> = Arc::from(serde_json::to_vec(value).unwrap());
        let document = materialize_file(Arc::clone(&bytes), input_limits()).unwrap();
        let fixture = FileFixture::new(&fixture_limits(100));
        let analysis = admit_structure(
            &mut evaluation,
            &fixture.file(bytes),
            document,
            &Schema::for_model().unwrap(),
            limits,
            Ok(()),
        )
        .unwrap();
        bound_declarations(&mut evaluation, analysis.as_ref().ok(), limits).unwrap();
        evaluation
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

    /// Each Phase 3 unit with its status.
    fn units(evaluation: &Evaluation) -> Vec<(Unit, Status)> {
        evaluation
            .statuses()
            .filter(|(unit, _)| unit.phase == Phase::ProfileSelection)
            .map(|(unit, status)| (unit.clone(), status.clone()))
            .collect()
    }

    /// Each Phase 3 Finding, as its subject and the bound it exceeded.
    fn exceeded(evaluation: &Evaluation) -> Vec<(Subject, LimitExceeded)> {
        evaluation
            .findings()
            .iter()
            .filter(|finding| finding.key.unit().phase == Phase::ProfileSelection)
            .map(|finding| match finding.details {
                Some(Details::Limit(exceeded)) => (finding.key.unit().subject.clone(), *exceeded),
                details => panic!("{details:?}"),
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
        assert_eq!(exceeded(&evaluation), []);
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
}
