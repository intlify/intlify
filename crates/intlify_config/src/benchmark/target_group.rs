// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015 Phases 11 and 12 over the selected declaration's targets and groups,
//! under a fixture-owned confirmed policy: each target's checked locales,
//! effective default, and Target Profile, each group's members, and whether
//! the groups partition the targets. Phase 7 runs through the finite fixture
//! provider, and the collection admission and Phase 9's exact resolution run
//! after it, all before the interval and on an evaluation of their own; the
//! interval runs Phase 11 and then Phase 12 over a fresh one. Neither the
//! result nor its observation is a Profile or a shared artifact.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::envelope::ExactTuple;
use crate::locale::CanonicalLocale;
use crate::model::{ProfileDeclaration, ProfileId, TargetId};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::resolver::artifact_resolution::{admit_collection, ArtifactSet};
use crate::resolver::evaluation::{
    both, Actual, Admitted, Causes, Check, Evaluation, EvaluationInvariant, IssueKey,
    LimitExceeded, Status, Subject, Unit,
};
use crate::resolver::exact_resolution::{resolve_fields, ReferenceResults};
use crate::resolver::groups::{resolve_groups, GroupSubject, ResolvedGroups};
use crate::resolver::limits::ConfirmedPolicy;
use crate::resolver::locales::{resolve_locales, LocaleFields, ResolvedLocales};
use crate::resolver::resource_policy::DeclarationReferences;
use crate::resolver::targets::{resolve_targets, ResolvedTargets, TargetSubject};

use super::observation::{Frame, Observation};
use super::operation::{Analysis, Config, OutputFailure};
use super::quantity::Quantity;
use super::{artifact_collection, locale, locale_resolution, resolved_binding};

type Declaration = ProfileDeclaration<PolicyReference, TargetProfileReference>;

/// A case's complete configuration and selected declaration, the confirmed
/// policy every phase runs under, the finite provider Phase 7 canonicalizes
/// through, and the collection past 015's pre-invocation boundary.
pub(super) struct PreparedTargets {
    pub(super) analysis: Analysis,
    pub(super) config: Config,
    pub(super) selected: ProfileId,
    pub(super) policy: ConfirmedPolicy,
    pub(super) canonicalizer: Arc<locale::Core>,
    pub(super) collection: FormalCollection,
}

/// What the interval borrows, built before it: the selected declaration, the
/// locales Phase 7 resolved, and what Phase 9 resolved each reference to.
pub(super) struct TargetInputs<'a> {
    pub(super) declaration: &'a Declaration,
    pub(super) policy: &'a ConfirmedPolicy,
    pub(super) locales: ResolvedLocales,
    pub(super) references: ReferenceResults<'a>,
}

impl PreparedTargets {
    /// The selected declaration, when the configuration is complete.
    pub(super) fn declaration(&self) -> Option<&Declaration> {
        if !self.analysis.is_complete() {
            return None;
        }
        self.config.profiles().get(&self.selected)
    }

    /// The Set the collection admission builds or why there is none, when
    /// the admission runs without a defect. Its own record is no part of the
    /// interval's.
    pub(super) fn set(&self) -> Option<Admitted<ArtifactSet<'_>>> {
        admit_collection(
            &mut Evaluation::default(),
            Ok(&self.policy),
            &self.collection,
        )
        .ok()
    }

    /// The interval's inputs over `set`, when the configuration is complete
    /// and Phases 7 and 9 run without a defect.
    pub(super) fn inputs<'a>(
        &'a self,
        set: &'a Admitted<ArtifactSet<'a>>,
    ) -> Option<TargetInputs<'a>> {
        let declaration = self.declaration()?;
        let mut preparation = Evaluation::default();
        let locales = resolve_locales(
            &mut preparation,
            Ok(&self.policy),
            Ok(&*self.canonicalizer),
            &LocaleFields::declared(declaration),
        )
        .ok()?;
        let references = resolve_fields(
            &mut preparation,
            set.as_ref(),
            &DeclarationReferences {
                policies: Ok(&declaration.policies),
                target_profiles: Ok(&declaration.target_profiles),
            },
        )
        .ok()?;
        Some(TargetInputs {
            declaration,
            policy: &self.policy,
            locales,
            references,
        })
    }

    /// One untimed invocation, retained as a timed one is.
    pub(super) fn resolve(&self) -> Option<Resolved> {
        let set = self.set()?;
        let inputs = self.inputs(&set)?;
        Some(Resolved::retain(construct(&inputs)))
    }
}

/// What one invocation returns: the evaluation, and Phase 11's targets, which
/// borrow the Set, with Phase 12's groups.
pub(super) struct Construction<'s> {
    evaluation: Evaluation,
    result: Result<(Admitted<ResolvedTargets<'s>>, Admitted<ResolvedGroups>), EvaluationInvariant>,
}

/// Phase 11 and then Phase 12 over a fresh evaluation: what every timed
/// invocation runs.
pub(super) fn construct<'s>(inputs: &'s TargetInputs<'s>) -> Construction<'s> {
    let mut evaluation = Evaluation::default();
    let result = phases(&mut evaluation, inputs);
    Construction { evaluation, result }
}

fn phases<'s>(
    evaluation: &mut Evaluation,
    inputs: &'s TargetInputs<'s>,
) -> Result<(Admitted<ResolvedTargets<'s>>, Admitted<ResolvedGroups>), EvaluationInvariant> {
    let targets = resolve_targets(
        evaluation,
        Ok(inputs.policy),
        Ok(&inputs.declaration.target_profiles),
        &inputs.locales,
        &inputs.references,
    )?;
    let groups = resolve_groups(
        evaluation,
        Ok(inputs.policy),
        Ok(&inputs.declaration.deployment_groups),
        targets.as_ref(),
    )?;
    Ok((targets, groups))
}

/// One target as the output retains it: each fact, or why it is unavailable,
/// with the Target Profile as the tuple the Set holds it under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RetainedTarget {
    pub(super) id: String,
    pub(super) admitted: Admitted<()>,
    pub(super) requested: Admitted<Vec<String>>,
    pub(super) effective_default: Admitted<String>,
    pub(super) profile: Admitted<ExactTuple>,
    pub(super) resolved: Admitted<()>,
}

/// One group as the output retains it: its members, or why there are none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RetainedGroup {
    pub(super) id: String,
    pub(super) members: Admitted<Vec<String>>,
}

/// What Phases 11 and 12 established, in Target ID and Group ID order, copied
/// out of the Set's borrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Retained {
    pub(super) targets: Vec<RetainedTarget>,
    pub(super) groups: Vec<RetainedGroup>,
    pub(super) partition: Admitted<()>,
}

/// Phases 11 and 12's record of every unit they ran, and what they
/// established: the complete ordinary output, retained past the end read.
pub(super) struct Resolved {
    pub(super) evaluation: Evaluation,
    pub(super) result: Result<Admitted<Retained>, EvaluationInvariant>,
}

fn texts<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    values.map(str::to_owned).collect()
}

impl Resolved {
    /// The targets borrow the Set, and with it the prepared collection, so
    /// after the end read their facts are copied out. The copy is
    /// observation, not work of the interval.
    pub(super) fn retain(construction: Construction<'_>) -> Self {
        let result =
            construction.result.map(|(targets, groups)| {
                both(targets, groups).map(|(targets, groups)| Retained {
                    targets: targets
                        .iter()
                        .map(|(id, facts)| RetainedTarget {
                            id: id.as_str().into(),
                            admitted: facts.admitted.clone(),
                            requested: facts.requested.as_ref().map_err(Causes::clone).map(|set| {
                                texts(set.as_slice().iter().map(CanonicalLocale::as_str))
                            }),
                            effective_default: facts
                                .effective_default
                                .as_ref()
                                .map(|locale| locale.as_str().to_owned())
                                .map_err(Causes::clone),
                            profile: facts
                                .profile
                                .as_ref()
                                .map(|(tuple, _)| (*tuple).clone())
                                .map_err(Causes::clone),
                            resolved: facts.resolved.as_ref().map(|_| ()).map_err(Causes::clone),
                        })
                        .collect(),
                    groups: groups
                        .iter()
                        .map(|(id, members)| RetainedGroup {
                            id: id.as_str().into(),
                            members: members
                                .as_ref()
                                .map_err(Causes::clone)
                                .map(|members| texts(members.iter().map(TargetId::as_str))),
                        })
                        .collect(),
                    partition: groups.partition().clone(),
                })
            });
        Self {
            evaluation: construction.evaluation,
            result,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InputFacts {
    scope: String,
    selected_profile: String,
    policy: String,
    canonicalizer: String,
    submitted_members: Quantity,
    prerequisites: String,
    input_storage: String,
    structural_preparation: String,
    ordinary_output: String,
}

impl InputFacts {
    pub(super) fn observe(prepared: &PreparedTargets) -> Self {
        Self {
            scope: "every-target-and-group-of-the-selected-declaration".into(),
            selected_profile: prepared.selected.as_str().into(),
            policy: "fixture-owned-confirmed-policy-not-an-admitted-resource-limit-policy".into(),
            canonicalizer: "finite-fixture-provider-bound-under-the-policy-identifier-bound".into(),
            submitted_members: Quantity::new(
                u64::try_from(prepared.collection.members().len()).unwrap_or(u64::MAX),
            ),
            prerequisites:
                "complete-configuration-selected-declaration-phase-7-locales-and-phase-9-references"
                    .into(),
            input_storage: "borrowed-selected-declaration-locales-and-set-over-resident-collection"
                .into(),
            structural_preparation:
                "complete-model-selection-phase-7-collection-admission-and-phase-9-before-interval"
                    .into(),
            ordinary_output:
                "fresh-evaluation-with-target-and-group-facts-copied-after-end-read-or-resolver-defect"
                    .into(),
        }
    }
}

/// A resolver defect is no blocked outcome: the case fails rather than
/// completing as an expected failure. Over a complete configuration both maps
/// are admitted, Phases 11 and 12 record only their own units, and of their
/// blocking results only a bound's has details and only the effective
/// default's a reason; there is no notice. Anything else is a defect too.
/// Logical work uses this screen.
pub(super) fn require_observable(resolved: &Resolved) -> Result<&Retained, OutputFailure> {
    let evaluation = &resolved.evaluation;
    let Ok(Ok(retained)) = &resolved.result else {
        return Err(OutputFailure::TargetAndGroupResolutionInvariant);
    };
    let own = evaluation
        .statuses()
        .all(|(unit, _)| matches!(unit.subject, Subject::Target(_) | Subject::Group(_)));
    let plain = evaluation.issues().all(|key| {
        let check = key.unit().check;
        evaluation.shortfall(key).is_none()
            && evaluation.limit(key).is_some() == (check == Check::WithinBound)
            && key.reason().is_some() == (check == Check::SupportedDefault)
    });
    if own && plain && evaluation.notices().next().is_none() {
        Ok(retained)
    } else {
        Err(OutputFailure::TargetAndGroupResolutionInvariant)
    }
}

const fn defect<T>() -> Result<T, OutputFailure> {
    Err(OutputFailure::TargetAndGroupResolutionInvariant)
}

fn count(value: usize) -> Result<u64, OutputFailure> {
    u64::try_from(value).map_err(|_| OutputFailure::TargetAndGroupResolutionInvariant)
}

fn target_subject(frame: &mut Frame, subject: &TargetSubject) {
    match subject {
        TargetSubject::Targets => frame.uint(0),
        TargetSubject::Target(id) => {
            frame.uint(1);
            frame.text(id.as_str());
        }
    }
}

fn group_subject(frame: &mut Frame, subject: &GroupSubject) {
    match subject {
        GroupSubject::Groups => frame.uint(0),
        GroupSubject::Memberships => frame.uint(1),
        GroupSubject::Group(id) => {
            frame.uint(2);
            frame.text(id.as_str());
        }
        GroupSubject::Members(id) => {
            frame.uint(3);
            frame.text(id.as_str());
        }
        GroupSubject::Member(id, member) => {
            frame.uint(4);
            frame.text(id.as_str());
            frame.text(member.as_str());
        }
        GroupSubject::Assignment(target) => {
            frame.uint(5);
            frame.text(target.as_str());
        }
    }
}

/// Only Phase 11's and Phase 12's own units run in the interval: a target's
/// or a group's subject under one of their six checks.
fn unit(frame: &mut Frame, unit: &Unit) -> Result<(), OutputFailure> {
    frame.text(unit.phase.id());
    match &unit.subject {
        Subject::Target(own) => {
            frame.uint(0);
            target_subject(frame, own);
        }
        Subject::Group(own) => {
            frame.uint(1);
            group_subject(frame, own);
        }
        Subject::Slot(_)
        | Subject::Vocabulary
        | Subject::CanonicalizationData
        | Subject::ResourcePolicy(_)
        | Subject::DeclarationField(_)
        | Subject::Locale(_)
        | Subject::Negotiation(_)
        | Subject::Fallback(_)
        | Subject::Coverage(_)
        | Subject::ArtifactCollection(_)
        | Subject::ArtifactReference(_)
        | Subject::DeliveryPlacement
        | Subject::Hydration(_) => return defect(),
    }
    frame.uint(match unit.check {
        Check::WithinBound => 0,
        Check::ProjectSubset => 1,
        Check::SupportedDefault => 2,
        Check::KnownMember => 3,
        Check::UniqueMember => 4,
        Check::SingleGroup => 5,
        Check::CanonicalLocale
        | Check::UniqueLocale
        | Check::RequestedMember
        | Check::DistinctFromSource
        | Check::UniqueCandidate
        | Check::VocabularyMember
        | Check::UniqueSurfaceClass
        | Check::UniqueDomain
        | Check::EffectiveMode
        | Check::SupportedAssertion
        | Check::MatchingData
        | Check::IntactArtifact
        | Check::AssertedArtifact
        | Check::OpaqueIdentity
        | Check::WithinCapability
        | Check::BootstrapCandidate
        | Check::CompatiblePolicy
        | Check::AdmissibleMember
        | Check::SingleOccurrence
        | Check::ConsistentContent
        | Check::ResolvedReference
        | Check::KnownEndpoints
        | Check::DistinctEndpoints
        | Check::SameGroupEndpoints
        | Check::UniqueRelation
        | Check::ServerRole
        | Check::ClientRole
        | Check::SameRequestedLocales
        | Check::SameEffectiveDefault
        | Check::StaticCompatibility
        | Check::SingleRole => return defect(),
    });
    Ok(())
}

/// A blocking result of Phase 11 or 12: its unit, code, and reason.
fn issue(frame: &mut Frame, key: &IssueKey) -> Result<(), OutputFailure> {
    unit(frame, key.unit())?;
    frame.text(key.code().as_str());
    frame.flag(key.reason().is_some());
    if let Some(reason) = key.reason() {
        frame.text(reason.as_str());
    }
    Ok(())
}

/// What a unit or a fact waits on: a blocking result of Phase 11 or 12, or
/// one Phase 7, the collection admission, or Phase 9 found before the
/// interval, framed as its own boundary frames it.
fn causes(frame: &mut Frame, causes: &Causes) -> Result<(), OutputFailure> {
    let keys = causes.keys().collect::<Vec<_>>();
    frame.uint(count(keys.len())?);
    for key in keys {
        let framed = match key.unit().subject {
            Subject::Target(_) | Subject::Group(_) => {
                frame.uint(0);
                issue(frame, key)
            }
            Subject::Locale(_) => {
                frame.uint(1);
                locale_resolution::issue(frame, key)
            }
            Subject::ArtifactCollection(_) => {
                frame.uint(2);
                artifact_collection::issue(frame, key)
            }
            Subject::ArtifactReference(_) => {
                frame.uint(3);
                resolved_binding::issue(frame, key)
            }
            Subject::Slot(_)
            | Subject::Vocabulary
            | Subject::CanonicalizationData
            | Subject::ResourcePolicy(_)
            | Subject::DeclarationField(_)
            | Subject::Negotiation(_)
            | Subject::Fallback(_)
            | Subject::Coverage(_)
            | Subject::DeliveryPlacement
            | Subject::Hydration(_) => return defect(),
        };
        framed.map_err(|_| OutputFailure::TargetAndGroupResolutionInvariant)?;
    }
    Ok(())
}

fn status(frame: &mut Frame, status: &Status) -> Result<(), OutputFailure> {
    match status {
        Status::Evaluated => {
            frame.uint(0);
            Ok(())
        }
        Status::NotEvaluated(why) => {
            frame.uint(1);
            causes(frame, why)
        }
    }
}

/// A bound's details: the bound, its value, and the total.
fn limit(frame: &mut Frame, exceeded: Option<LimitExceeded>) {
    frame.flag(exceeded.is_some());
    if let Some(exceeded) = exceeded {
        frame.text(exceeded.bound.group.path());
        frame.text(exceeded.bound.name);
        frame.uint(exceeded.limit.get());
        match exceeded.actual {
            Actual::Exact(value) => {
                frame.uint(0);
                frame.uint(value);
            }
            Actual::AtLeast(value) => {
                frame.uint(1);
                frame.uint(value);
            }
        }
    }
}

/// A fact, or the causes it is unavailable.
fn admitted<T>(
    frame: &mut Frame,
    value: &Admitted<T>,
    present: impl FnOnce(&mut Frame, &T) -> Result<(), OutputFailure>,
) -> Result<(), OutputFailure> {
    match value {
        Ok(value) => {
            frame.uint(0);
            present(frame, value)
        }
        Err(why) => {
            frame.uint(1);
            causes(frame, why)
        }
    }
}

fn names(frame: &mut Frame, values: &[String]) -> Result<(), OutputFailure> {
    frame.uint(count(values.len())?);
    for value in values {
        frame.text(value);
    }
    Ok(())
}

/// Each target's facts, each group's members, and the partition.
fn retained(frame: &mut Frame, retained: &Retained) -> Result<(), OutputFailure> {
    frame.uint(count(retained.targets.len())?);
    for target in &retained.targets {
        frame.text(&target.id);
        admitted(frame, &target.admitted, |_, ()| Ok(()))?;
        admitted(frame, &target.requested, |frame, locales| {
            names(frame, locales)
        })?;
        admitted(frame, &target.effective_default, |frame, locale| {
            frame.text(locale);
            Ok(())
        })?;
        admitted(frame, &target.profile, |frame, tuple| {
            artifact_collection::tuple(frame, tuple);
            Ok(())
        })?;
        admitted(frame, &target.resolved, |_, ()| Ok(()))?;
    }
    frame.uint(count(retained.groups.len())?);
    for group in &retained.groups {
        frame.text(&group.id);
        admitted(frame, &group.members, |frame, members| {
            names(frame, members)
        })?;
    }
    admitted(frame, &retained.partition, |_, ()| Ok(()))
}

/// The shared domain is what Phases 11 and 12 decided, none of it positional:
/// every unit and its status, each blocking result with a bound's details,
/// and each target's facts and each group's members with the partition. The
/// order of the declaration's members, a group's members, and the collection
/// is no part of the input, so reordering any of them changes nothing here.
/// A target or a group is no entry's own, so there is no entry domain.
pub(super) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let established = require_observable(resolved)?;
    let evaluation = &resolved.evaluation;
    let mut shared = Frame::new("target-and-group-resolution-result");
    shared.uint(count(evaluation.statuses().count())?);
    for (recorded, recorded_status) in evaluation.statuses() {
        unit(&mut shared, recorded)?;
        status(&mut shared, recorded_status)?;
    }
    shared.uint(count(evaluation.issues().count())?);
    for key in evaluation.issues() {
        issue(&mut shared, key)?;
        limit(&mut shared, evaluation.limit(key));
    }
    retained(&mut shared, established)?;
    Ok(Observation {
        shared: shared.finish(),
        entry: None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::benchmark::cases::prepare::prepare;
    use crate::benchmark::cases::{declarations, Recipe, TargetAndGroupRecipe as R};
    use crate::benchmark::observation::Digest;
    use crate::benchmark::operation::Prepared;
    use crate::input_limits::Bound;
    use crate::model::DeploymentGroupId;
    use crate::resolver::artifact_resolution::{unit as collection_unit, CollectionSubject};
    use crate::resolver::evaluation::{Blocking, Code, Details, Phase, Reason, Shortfall};
    use crate::resolver::exact_resolution::unit as reference_unit;
    use crate::resolver::groups::unit as group_unit;
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::resource_policy::{PolicySlot, ReferenceField};
    use crate::resolver::targets::unit as target_unit;

    fn prepared(recipe: R) -> PreparedTargets {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::TargetAndGroup(recipe) && case.limit.is_none())
            .unwrap();
        let Prepared::TargetAndGroup(prepared) = prepare(&declaration).unwrap().prepared else {
            unreachable!()
        };
        *prepared
    }

    fn resolved(recipe: R) -> Resolved {
        prepared(recipe).resolve().unwrap()
    }

    fn retained_of(recipe: R) -> Retained {
        let Ok(Ok(retained)) = resolved(recipe).result else {
            unreachable!()
        };
        retained
    }

    fn digest(frame_with: impl FnOnce(&mut Frame)) -> Digest {
        let mut frame = Frame::new("target-and-group-test");
        frame_with(&mut frame);
        frame.finish()
    }

    fn apart<T: PartialEq>(values: &[T]) -> bool {
        values
            .iter()
            .enumerate()
            .all(|(index, left)| values[index + 1..].iter().all(|right| right != left))
    }

    fn target(id: &str) -> TargetId {
        serde_json::from_value(json!(id)).unwrap()
    }

    fn group(id: &str) -> DeploymentGroupId {
        serde_json::from_value(json!(id)).unwrap()
    }

    fn of_browser(check: Check) -> Unit {
        target_unit(TargetSubject::Target(target("browser")), check)
    }

    const ID_BYTES: BoundId = BoundId {
        group: Group::TargetGrouping,
        name: "maxTargetIdBytes",
    };

    /// An evaluation whose one unit, `recorded`, `blocking` blocks, and the
    /// causes that leaves.
    fn blocked_unit(recorded: Unit, blocking: Blocking) -> (Evaluation, Causes) {
        let mut evaluation = Evaluation::default();
        let Err(causes) = evaluation
            .check(recorded, Ok(()), |()| Err::<(), _>(blocking))
            .unwrap()
        else {
            unreachable!()
        };
        (evaluation, causes)
    }

    fn blocked(
        recorded: Unit,
        code: Code,
        reason: Option<Reason>,
        details: Option<Details>,
    ) -> Evaluation {
        blocked_unit(
            recorded,
            Blocking {
                code,
                reason,
                details,
            },
        )
        .0
    }

    fn exceeded(limit: u64, actual: Actual) -> LimitExceeded {
        LimitExceeded {
            bound: ID_BYTES,
            limit: Bound::new(limit).unwrap(),
            actual,
        }
    }

    /// The causes a bound of Phase 11 leaves.
    fn own_causes() -> Causes {
        blocked_unit(
            of_browser(Check::WithinBound),
            Blocking::limit(exceeded(6, Actual::Exact(7))),
        )
        .1
    }

    #[test]
    fn the_retained_facts_are_the_ones_the_interval_made() {
        let prepared = prepared(R::Storefront);
        let set = prepared.set().unwrap();
        let inputs = prepared.inputs(&set).unwrap();
        let construction = construct(&inputs);
        let (targets, groups) = {
            let (Ok(targets), Ok(groups)) = construction.result.as_ref().unwrap() else {
                unreachable!()
            };
            (
                targets
                    .iter()
                    .map(|(id, facts)| {
                        let (tuple, _) = facts.profile.as_ref().unwrap();
                        let requested = facts.requested.as_ref().unwrap().as_slice();
                        (
                            id.as_str().to_owned(),
                            (*tuple).clone(),
                            texts(requested.iter().map(CanonicalLocale::as_str)),
                            facts
                                .effective_default
                                .as_ref()
                                .unwrap()
                                .as_str()
                                .to_owned(),
                        )
                    })
                    .collect::<Vec<_>>(),
                groups
                    .iter()
                    .map(|(id, members)| {
                        (
                            id.as_str().to_owned(),
                            texts(members.as_ref().unwrap().iter().map(TargetId::as_str)),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let statuses = construction.evaluation.statuses().count();
        let retained = Resolved::retain(construction);
        assert_eq!(retained.evaluation.statuses().count(), statuses);
        let retained = retained.result.unwrap().unwrap();
        assert_eq!(
            retained
                .targets
                .iter()
                .map(|target| (
                    target.id.clone(),
                    target.profile.clone().unwrap(),
                    target.requested.clone().unwrap(),
                    target.effective_default.clone().unwrap(),
                ))
                .collect::<Vec<_>>(),
            targets
        );
        assert_eq!(
            retained
                .groups
                .iter()
                .map(|group| (group.id.clone(), group.members.clone().unwrap()))
                .collect::<Vec<_>>(),
            groups
        );
        // Each target names the Target Profile the Set holds under its
        // tuple, and both request the example's locales with its default.
        let Ok(set) = &set else { unreachable!() };
        for target in &retained.targets {
            let tuple = target.profile.as_ref().unwrap();
            assert!(set.get(tuple).is_some());
            assert_eq!(tuple.identity.as_str(), format!("storefront-{}", target.id));
            assert_eq!(
                target.requested,
                Ok(vec!["en".into(), "fr-FR".into(), "ja".into()])
            );
            assert_eq!(target.effective_default, Ok("en".into()));
            assert!(target.admitted.is_ok() && target.resolved.is_ok());
        }
        assert_eq!(
            retained.groups[0].members,
            Ok(vec!["browser".into(), "ssr".into()])
        );
        assert!(retained.partition.is_ok());
        assert_eq!(prepared.resolve().unwrap().result, Ok(Ok(retained)));
    }

    #[test]
    fn a_resolver_defect_fails_the_case() {
        let resolved = Resolved {
            evaluation: Evaluation::default(),
            result: Err(EvaluationInvariant::UnitRecordedTwice(target_unit(
                TargetSubject::Targets,
                Check::WithinBound,
            ))),
        };
        assert_eq!(
            require_observable(&resolved).err(),
            Some(OutputFailure::TargetAndGroupResolutionInvariant)
        );
        assert_eq!(
            observe(&resolved).err(),
            Some(OutputFailure::TargetAndGroupResolutionInvariant)
        );
    }

    /// What Phases 11 and 12 cannot record over a complete configuration
    /// fails the case: an unavailable map, another phase's unit, details on
    /// a check other than a bound or none on a bound, a shortfall, a reason
    /// on a check other than the effective default or none on it, and a
    /// non-blocking result.
    #[test]
    fn only_what_phases_11_and_12_record_is_observed() {
        let storefront = retained_of(R::Storefront);
        let observed = |evaluation| {
            observe(&Resolved {
                evaluation,
                result: Ok(Ok(storefront.clone())),
            })
        };
        let limit = || Some(Details::Limit(exceeded(6, Actual::Exact(7))));
        // The controls: a complete case, a default's reason, and a bound's
        // details are observed.
        assert!(observe(&resolved(R::Storefront)).is_ok());
        assert!(observe(&resolved(R::ExplicitDefaultNotRequested)).is_ok());
        assert!(observed(blocked(
            of_browser(Check::WithinBound),
            Code::ResourceLimitExceeded,
            None,
            limit(),
        ))
        .is_ok());
        assert!(observed(blocked(
            of_browser(Check::SupportedDefault),
            Code::TargetDefaultLocaleNotSupported,
            Some(Reason::InheritedProjectDefault),
            None,
        ))
        .is_ok());
        let foreign = {
            let mut evaluation = Evaluation::default();
            let _ = evaluation
                .check(
                    reference_unit(ReferenceField::Policy(PolicySlot::Trust)),
                    Ok(()),
                    |()| Ok::<(), Blocking>(()),
                )
                .unwrap();
            evaluation
        };
        let shortfall = Details::Shortfall(Shortfall {
            bound: ID_BYTES,
            required: Bound::new(7).unwrap(),
            available: Bound::new(6).unwrap(),
        });
        let noticed = {
            let mut evaluation = resolved(R::Storefront).evaluation;
            evaluation
                .notice(
                    of_browser(Check::ProjectSubset),
                    Code::TargetLocalesNotSubset,
                )
                .unwrap();
            evaluation
        };
        let member = group_unit(
            GroupSubject::Member(group("web"), target("tablet")),
            Check::KnownMember,
        );
        for (name, evaluation) in [
            ("another phase's unit", foreign),
            (
                "details beside another check",
                blocked(
                    of_browser(Check::ProjectSubset),
                    Code::TargetLocalesNotSubset,
                    None,
                    limit(),
                ),
            ),
            (
                "a bound without its details",
                blocked(
                    of_browser(Check::WithinBound),
                    Code::ResourceLimitExceeded,
                    None,
                    None,
                ),
            ),
            (
                "a shortfall",
                blocked(
                    of_browser(Check::ProjectSubset),
                    Code::TargetLocalesNotSubset,
                    None,
                    Some(shortfall),
                ),
            ),
            (
                "a reason beside another check",
                blocked(
                    member,
                    Code::GroupMemberUnknown,
                    Some(Reason::ExplicitOverride),
                    None,
                ),
            ),
            (
                "a default without its reason",
                blocked(
                    of_browser(Check::SupportedDefault),
                    Code::TargetDefaultLocaleNotSupported,
                    None,
                    None,
                ),
            ),
            ("notice", noticed),
        ] {
            // Logical work reads the screen without framing a unit.
            let resolved = Resolved {
                evaluation,
                result: Ok(Ok(storefront.clone())),
            };
            for failure in [
                require_observable(&resolved).err(),
                observe(&resolved).err(),
            ] {
                assert_eq!(
                    failure,
                    Some(OutputFailure::TargetAndGroupResolutionInvariant),
                    "{name}"
                );
            }
        }
        // Over a complete configuration both maps are admitted.
        let unavailable = Resolved {
            evaluation: resolved(R::Storefront).evaluation,
            result: Ok(Err(own_causes())),
        };
        assert_eq!(
            observe(&unavailable).err(),
            Some(OutputFailure::TargetAndGroupResolutionInvariant)
        );
    }

    #[test]
    fn only_target_and_group_units_frame_and_other_phases_only_as_causes() {
        let mut frame = Frame::new("target-and-group-test");
        let foreign = [
            reference_unit(ReferenceField::Policy(PolicySlot::Trust)),
            collection_unit(CollectionSubject::Member(0), Check::AdmissibleMember),
            Unit {
                check: Check::ResolvedReference,
                ..target_unit(TargetSubject::Targets, Check::WithinBound)
            },
            Unit {
                check: Check::KnownEndpoints,
                ..group_unit(GroupSubject::Groups, Check::WithinBound)
            },
        ];
        for recorded in &foreign {
            assert_eq!(
                unit(&mut frame, recorded),
                Err(OutputFailure::TargetAndGroupResolutionInvariant),
                "{recorded:?}"
            );
        }
        // Each subject frames apart, a group apart from a target of its
        // name, and a member by its group and its Target ID.
        let subjects = [
            target_unit(TargetSubject::Targets, Check::WithinBound),
            target_unit(TargetSubject::Target(target("web")), Check::WithinBound),
            target_unit(TargetSubject::Target(target("ssr")), Check::WithinBound),
            group_unit(GroupSubject::Groups, Check::WithinBound),
            group_unit(GroupSubject::Memberships, Check::WithinBound),
            group_unit(GroupSubject::Group(group("web")), Check::WithinBound),
            group_unit(GroupSubject::Group(group("edge")), Check::WithinBound),
            group_unit(GroupSubject::Members(group("web")), Check::WithinBound),
            group_unit(GroupSubject::Members(group("edge")), Check::WithinBound),
            group_unit(
                GroupSubject::Member(group("web"), target("web")),
                Check::WithinBound,
            ),
            group_unit(
                GroupSubject::Member(group("web"), target("ssr")),
                Check::WithinBound,
            ),
            group_unit(
                GroupSubject::Member(group("edge"), target("web")),
                Check::WithinBound,
            ),
            group_unit(GroupSubject::Assignment(target("web")), Check::WithinBound),
            group_unit(GroupSubject::Assignment(target("ssr")), Check::WithinBound),
        ]
        .map(|recorded| digest(|frame| unit(frame, &recorded).unwrap()));
        assert!(apart(&subjects));
        // Each check frames apart, and the phase is part of the unit.
        let checks = [
            Check::WithinBound,
            Check::ProjectSubset,
            Check::SupportedDefault,
            Check::KnownMember,
            Check::UniqueMember,
            Check::SingleGroup,
        ]
        .map(|check| {
            digest(|frame| unit(frame, &target_unit(TargetSubject::Targets, check)).unwrap())
        });
        assert!(apart(&checks));
        let other_phase = Unit {
            phase: Phase::GroupResolution,
            ..target_unit(TargetSubject::Targets, Check::WithinBound)
        };
        assert_ne!(
            digest(|frame| unit(frame, &other_phase).unwrap()),
            checks[0]
        );
        // A cause frames as its own boundary frames it: one of Phases 11 and
        // 12, Phase 7's, the collection's, or Phase 9's.
        let locale = retained_of(R::TargetLocaleInvalid).targets[1]
            .requested
            .clone()
            .unwrap_err();
        let reference = retained_of(R::TargetProfileMissing).targets[1]
            .profile
            .clone()
            .unwrap_err();
        let collection = retained_of(R::CollectionNotAdmitted).targets[0]
            .profile
            .clone()
            .unwrap_err();
        let group_causes = blocked_unit(
            group_unit(GroupSubject::Groups, Check::WithinBound),
            Blocking::limit(exceeded(1, Actual::Exact(2))),
        )
        .1;
        for (why, subject) in [
            (&locale, "locale"),
            (&reference, "reference"),
            (&collection, "collection"),
        ] {
            assert!(why.keys().all(|key| match key.unit().subject {
                Subject::Locale(_) => subject == "locale",
                Subject::ArtifactReference(_) => subject == "reference",
                Subject::ArtifactCollection(_) => subject == "collection",
                _ => false,
            }));
        }
        let framed = [own_causes(), group_causes, locale, reference, collection]
            .map(|why| digest(|frame| causes(frame, &why).unwrap()));
        assert!(apart(&framed));
        // A cause of any other phase is a defect, and so is one its own
        // boundary would refuse.
        let (_, vocabulary) = blocked_unit(
            Unit {
                phase: Phase::ArtifactReferenceResolution,
                subject: Subject::Vocabulary,
                check: Check::VocabularyMember,
            },
            Blocking {
                code: Code::ArtifactReferenceNotAdmitted,
                reason: Some(Reason::NotFound),
                details: None,
            },
        );
        let (_, unexplained) = blocked_unit(
            reference_unit(ReferenceField::Policy(PolicySlot::Trust)),
            Blocking {
                code: Code::ArtifactReferenceNotAdmitted,
                reason: None,
                details: None,
            },
        );
        for why in [vocabulary, unexplained] {
            assert_eq!(
                causes(&mut frame, &why),
                Err(OutputFailure::TargetAndGroupResolutionInvariant)
            );
        }
    }

    #[test]
    fn every_part_of_a_result_frames_apart() {
        let storefront = retained_of(R::Storefront);
        let why = own_causes();
        let with = |change: &dyn Fn(&mut Retained)| {
            let mut changed = storefront.clone();
            change(&mut changed);
            changed
        };
        let framed = [
            storefront.clone(),
            with(&|retained| {
                retained.targets.pop();
            }),
            with(&|retained| retained.targets[0].id = "tablet".into()),
            with(&|retained| retained.targets[0].admitted = Err(why.clone())),
            with(&|retained| retained.targets[0].requested = Ok(vec!["en".into()])),
            with(&|retained| retained.targets[0].requested = Err(why.clone())),
            with(&|retained| retained.targets[0].effective_default = Ok("ja".into())),
            with(&|retained| retained.targets[0].effective_default = Err(why.clone())),
            with(&|retained| retained.targets[0].profile = retained.targets[1].profile.clone()),
            with(&|retained| retained.targets[0].profile = Err(why.clone())),
            with(&|retained| retained.targets[0].resolved = Err(why.clone())),
            with(&|retained| {
                retained.groups.pop();
            }),
            with(&|retained| retained.groups[0].id = "edge".into()),
            with(&|retained| retained.groups[0].members = Ok(vec!["browser".into()])),
            with(&|retained| retained.groups[0].members = Err(why.clone())),
            with(&|retained| retained.partition = Err(why.clone())),
        ]
        .map(|changed| digest(|frame| retained(frame, &changed).unwrap()));
        assert!(apart(&framed));
        // A status frames whether the unit was checked and what it waits on.
        let statuses = [
            Status::Evaluated,
            Status::NotEvaluated(why.clone()),
            Status::NotEvaluated(
                retained_of(R::TargetLocaleInvalid).targets[1]
                    .requested
                    .clone()
                    .unwrap_err(),
            ),
        ]
        .map(|recorded| digest(|frame| status(frame, &recorded).unwrap()));
        assert!(apart(&statuses));
        // A blocking result frames its code and whether it has a reason.
        let key = |code, reason| {
            let evaluation = blocked(of_browser(Check::SupportedDefault), code, reason, None);
            let recorded = evaluation.issues().next().unwrap().clone();
            recorded
        };
        let issues = [
            key(Code::TargetDefaultLocaleNotSupported, None),
            key(
                Code::TargetDefaultLocaleNotSupported,
                Some(Reason::ExplicitOverride),
            ),
            key(
                Code::TargetDefaultLocaleNotSupported,
                Some(Reason::InheritedProjectDefault),
            ),
            key(Code::TargetLocalesNotSubset, None),
        ]
        .map(|recorded| digest(|frame| issue(frame, &recorded).unwrap()));
        assert!(apart(&issues));
        // A bound's details frame the bound, its value, and the total.
        let other = |group, name| LimitExceeded {
            bound: BoundId { group, name },
            ..exceeded(6, Actual::Exact(7))
        };
        let limits = [
            None,
            Some(exceeded(6, Actual::Exact(7))),
            Some(exceeded(6, Actual::AtLeast(7))),
            Some(exceeded(6, Actual::Exact(8))),
            Some(exceeded(5, Actual::Exact(7))),
            Some(other(Group::TargetGrouping, "maxGroupIdBytes")),
            Some(other(Group::ArtifactAdmission, "maxTargetIdBytes")),
        ]
        .map(|exceeded| digest(|frame| limit(frame, exceeded)));
        assert!(apart(&limits));
    }
}
