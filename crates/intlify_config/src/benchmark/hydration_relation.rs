// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015 Phase 13 over the selected declaration's hydration relations, under a
//! fixture-owned confirmed policy and fixture-owned answers to the role and
//! compatibility questions 024 has not defined yet (plan 023 ◆#1): each
//! group's checked relations, and the Locale Negotiation Profile every
//! relation shares. Phase 7 through the finite fixture provider, Phase 8's
//! negotiation, the collection admission, Phase 9, and Phases 11 and 12 all
//! run before the interval, on an evaluation of their own; the interval runs
//! Phase 13 over a fresh one. Neither the result nor its observation is a
//! Profile or a shared artifact.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::artifacts::collection::FormalCollection;
use crate::model::{ProfileDeclaration, ProfileId, TargetId};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::resolver::artifact_resolution::{admit_collection, AdmittedArtifact, ArtifactSet};
use crate::resolver::evaluation::{
    Actual, Admitted, Causes, Check, Evaluation, EvaluationInvariant, IssueKey, LimitExceeded,
    Status, Subject, Unit,
};
use crate::resolver::exact_resolution::resolve_fields;
use crate::resolver::groups::{resolve_groups, ResolvedGroups};
use crate::resolver::hydration::{
    validate_hydration, HydrationSubject, ResolvedHydration, TargetCapabilities,
};
use crate::resolver::limits::ConfirmedPolicy;
use crate::resolver::locales::{resolve_locales, LocaleFields};
use crate::resolver::negotiation::resolve_negotiation;
use crate::resolver::resource_policy::DeclarationReferences;
use crate::resolver::targets::{resolve_targets, ResolvedTargets};

use super::locale_resolution::PolicyInputs;
use super::observation::{Frame, Observation};
use super::operation::{Analysis, Config, OutputFailure};
use super::quantity::Quantity;
use super::{artifact_collection, locale, locale_resolution, resolved_binding, target_group};

type Declaration = ProfileDeclaration<PolicyReference, TargetProfileReference>;

/// Fixture-owned answers to what 015 asks 024 about Target Profiles, by each
/// profile's identity: which render server-side, which hydrate in the
/// browser, and which `(server, client)` pairs are statically incompatible.
/// They are no 024 capability, which 024 has not defined yet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(super) struct FixtureCapabilities {
    pub(super) renders: BTreeSet<String>,
    pub(super) hydrates: BTreeSet<String>,
    pub(super) incompatible: BTreeSet<(String, String)>,
}

/// The identity of an admitted Target Profile's envelope.
fn identity(profile: &AdmittedArtifact<'_>) -> Option<String> {
    profile
        .member
        .envelope()
        .map(|envelope| envelope.tuple().identity.as_str().to_owned())
}

impl TargetCapabilities for FixtureCapabilities {
    fn renders_server_side(&self, profile: &AdmittedArtifact<'_>) -> bool {
        identity(profile).is_some_and(|identity| self.renders.contains(&identity))
    }

    fn hydrates_in_browser(&self, profile: &AdmittedArtifact<'_>) -> bool {
        identity(profile).is_some_and(|identity| self.hydrates.contains(&identity))
    }

    fn statically_incompatible(
        &self,
        server: &AdmittedArtifact<'_>,
        client: &AdmittedArtifact<'_>,
    ) -> bool {
        identity(server)
            .zip(identity(client))
            .is_some_and(|pair| self.incompatible.contains(&pair))
    }
}

/// A case's complete configuration and selected declaration, the confirmed
/// policy every phase runs under, the finite provider Phase 7 canonicalizes
/// through, the collection past 015's pre-invocation boundary, and the
/// fixture's answers to 024's questions.
pub(super) struct PreparedHydration {
    pub(super) analysis: Analysis,
    pub(super) config: Config,
    pub(super) selected: ProfileId,
    pub(super) policy: ConfirmedPolicy,
    pub(super) canonicalizer: Arc<locale::Core>,
    pub(super) collection: FormalCollection,
    pub(super) capabilities: FixtureCapabilities,
}

/// What the interval borrows, built before it: the selected declaration,
/// what Phases 11 and 12 established, and whether Phase 8 admitted the
/// Locale Negotiation Profile.
pub(super) struct HydrationInputs<'a> {
    pub(super) declaration: &'a Declaration,
    pub(super) policy: &'a ConfirmedPolicy,
    pub(super) targets: Admitted<ResolvedTargets<'a>>,
    pub(super) groups: Admitted<ResolvedGroups>,
    pub(super) negotiation: Admitted<()>,
    pub(super) capabilities: &'a FixtureCapabilities,
}

impl PreparedHydration {
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
    /// and every earlier phase runs without a defect.
    pub(super) fn inputs<'a>(
        &'a self,
        set: &'a Admitted<ArtifactSet<'a>>,
    ) -> Option<HydrationInputs<'a>> {
        let declaration = self.declaration()?;
        let mut preparation = Evaluation::default();
        let policy: Result<&ConfirmedPolicy, &Causes> = Ok(&self.policy);
        let locales = resolve_locales(
            &mut preparation,
            policy,
            Ok(&*self.canonicalizer),
            &LocaleFields::declared(declaration),
        )
        .ok()?;
        let negotiation = resolve_negotiation(
            &mut preparation,
            policy,
            &PolicyInputs::fixture_owned().negotiation,
            &locales,
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
        let targets = resolve_targets(
            &mut preparation,
            policy,
            Ok(&declaration.target_profiles),
            &locales,
            &references,
        )
        .ok()?;
        let groups = resolve_groups(
            &mut preparation,
            policy,
            Ok(&declaration.deployment_groups),
            targets.as_ref(),
        )
        .ok()?;
        Some(HydrationInputs {
            declaration,
            policy: &self.policy,
            targets,
            groups,
            negotiation: negotiation.as_ref().map(|_| ()).map_err(Causes::clone),
            capabilities: &self.capabilities,
        })
    }

    /// One untimed invocation, retained as a timed one is.
    pub(super) fn resolve(&self) -> Option<Resolved> {
        let set = self.set()?;
        let inputs = self.inputs(&set)?;
        Some(Resolved::retain(construct(&inputs)))
    }
}

/// What one invocation returns: the evaluation, and each group's relations
/// with the shared negotiation profile.
pub(super) struct Construction {
    evaluation: Evaluation,
    result: Result<Admitted<ResolvedHydration>, EvaluationInvariant>,
}

/// Phase 13 over a fresh evaluation: what every timed invocation runs.
pub(super) fn construct(inputs: &HydrationInputs<'_>) -> Construction {
    let mut evaluation = Evaluation::default();
    let result = validate_hydration(
        &mut evaluation,
        Ok(inputs.policy),
        Ok(&inputs.declaration.deployment_groups),
        inputs.targets.as_ref(),
        inputs.groups.as_ref(),
        inputs.negotiation.clone(),
        inputs.capabilities,
    );
    Construction { evaluation, result }
}

/// One group's relations as the output retains them: its `(server, client)`
/// pairs, or why they are unavailable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RetainedGroup {
    pub(super) id: String,
    pub(super) relations: Admitted<Vec<(String, String)>>,
}

/// What Phase 13 established, in Group ID and pair order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Retained {
    pub(super) groups: Vec<RetainedGroup>,
    pub(super) negotiation: Admitted<()>,
}

/// Phase 13's record of every unit it ran, and what it established: the
/// complete ordinary output.
pub(super) struct Resolved {
    pub(super) evaluation: Evaluation,
    pub(super) result: Result<Admitted<Retained>, EvaluationInvariant>,
}

impl Resolved {
    /// The relations as the output keeps them, by their Target IDs' spellings.
    pub(super) fn retain(construction: Construction) -> Self {
        let pair = |(server, client): &(TargetId, TargetId)| {
            (server.as_str().to_owned(), client.as_str().to_owned())
        };
        let result = construction.result.map(|hydration| {
            hydration.map(|hydration| Retained {
                groups: hydration
                    .iter()
                    .map(|(id, relations)| RetainedGroup {
                        id: id.as_str().into(),
                        relations: relations
                            .as_ref()
                            .map(|relations| relations.iter().map(pair).collect())
                            .map_err(Causes::clone),
                    })
                    .collect(),
                negotiation: hydration.negotiation().clone(),
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
    negotiation: String,
    capabilities: String,
    submitted_members: Quantity,
    prerequisites: String,
    input_storage: String,
    structural_preparation: String,
    ordinary_output: String,
}

impl InputFacts {
    pub(super) fn observe(prepared: &PreparedHydration) -> Self {
        Self {
            scope: "every-hydration-relation-of-the-selected-declaration".into(),
            selected_profile: prepared.selected.as_str().into(),
            policy: "fixture-owned-confirmed-policy-not-an-admitted-resource-limit-policy".into(),
            canonicalizer: "finite-fixture-provider-bound-under-the-policy-identifier-bound".into(),
            negotiation: "fixture-owned-locale-negotiation-specification-at-revision-0".into(),
            capabilities: "fixture-owned-role-and-compatibility-answers-not-024-capabilities"
                .into(),
            submitted_members: Quantity::new(
                u64::try_from(prepared.collection.members().len()).unwrap_or(u64::MAX),
            ),
            prerequisites: "complete-configuration-selected-declaration-phases-7-8-9-11-and-12"
                .into(),
            input_storage:
                "borrowed-selected-declaration-and-earlier-phase-results-over-resident-collection"
                    .into(),
            structural_preparation: "complete-model-selection-and-phases-7-to-12-before-interval"
                .into(),
            ordinary_output: "fresh-evaluation-with-relations-and-negotiation-or-resolver-defect"
                .into(),
        }
    }
}

/// A resolver defect is no blocked outcome: the case fails rather than
/// completing as an expected failure. Over a complete configuration the
/// groups are admitted, Phase 13 records only its own units, and of its
/// blocking results only a bound's has details and every one has a reason but
/// a bound's and the static incompatibility's; there is no notice. Anything
/// else is a defect too. Logical work uses this screen.
pub(super) fn require_observable(resolved: &Resolved) -> Result<&Retained, OutputFailure> {
    let evaluation = &resolved.evaluation;
    let Ok(Ok(retained)) = &resolved.result else {
        return Err(OutputFailure::HydrationRelationValidationInvariant);
    };
    let own = evaluation
        .statuses()
        .all(|(unit, _)| matches!(unit.subject, Subject::Hydration(_)));
    let plain = evaluation.issues().all(|key| {
        let check = key.unit().check;
        let bound = check == Check::WithinBound;
        evaluation.shortfall(key).is_none()
            && evaluation.limit(key).is_some() == bound
            && key.reason().is_some() != (bound || check == Check::StaticCompatibility)
    });
    if own && plain && evaluation.notices().next().is_none() {
        Ok(retained)
    } else {
        Err(OutputFailure::HydrationRelationValidationInvariant)
    }
}

const fn defect<T>() -> Result<T, OutputFailure> {
    Err(OutputFailure::HydrationRelationValidationInvariant)
}

fn count(value: usize) -> Result<u64, OutputFailure> {
    u64::try_from(value).map_err(|_| OutputFailure::HydrationRelationValidationInvariant)
}

/// Only Phase 13's own units run in the interval: the bound, a relation, or a
/// Target ID's roles, under one of its eleven checks.
fn unit(frame: &mut Frame, unit: &Unit) -> Result<(), OutputFailure> {
    frame.text(unit.phase.id());
    match &unit.subject {
        Subject::Hydration(HydrationSubject::Relations) => frame.uint(0),
        Subject::Hydration(HydrationSubject::Relation(group, server, client)) => {
            frame.uint(1);
            frame.text(group.as_str());
            frame.text(server.as_str());
            frame.text(client.as_str());
        }
        Subject::Hydration(HydrationSubject::Roles(group, target)) => {
            frame.uint(2);
            frame.text(group.as_str());
            frame.text(target.as_str());
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
        | Subject::Target(_)
        | Subject::Group(_) => return defect(),
    }
    frame.uint(match unit.check {
        Check::WithinBound => 0,
        Check::KnownEndpoints => 1,
        Check::DistinctEndpoints => 2,
        Check::SameGroupEndpoints => 3,
        Check::UniqueRelation => 4,
        Check::ServerRole => 5,
        Check::ClientRole => 6,
        Check::SameRequestedLocales => 7,
        Check::SameEffectiveDefault => 8,
        Check::StaticCompatibility => 9,
        Check::SingleRole => 10,
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
        | Check::ProjectSubset
        | Check::SupportedDefault
        | Check::KnownMember
        | Check::UniqueMember
        | Check::SingleGroup => return defect(),
    });
    Ok(())
}

/// A blocking result of Phase 13: its unit, code, and reason.
fn issue(frame: &mut Frame, key: &IssueKey) -> Result<(), OutputFailure> {
    unit(frame, key.unit())?;
    frame.text(key.code().as_str());
    frame.flag(key.reason().is_some());
    if let Some(reason) = key.reason() {
        frame.text(reason.as_str());
    }
    Ok(())
}

/// What a unit or a fact waits on: a blocking result of Phase 13, or one an
/// earlier phase found before the interval, framed as its own boundary frames
/// it.
fn causes(frame: &mut Frame, causes: &Causes) -> Result<(), OutputFailure> {
    let keys = causes.keys().collect::<Vec<_>>();
    frame.uint(count(keys.len())?);
    for key in keys {
        let framed = match key.unit().subject {
            Subject::Hydration(_) => {
                frame.uint(0);
                issue(frame, key)
            }
            Subject::Target(_) | Subject::Group(_) => {
                frame.uint(1);
                target_group::issue(frame, key)
            }
            Subject::Locale(_) | Subject::Negotiation(_) => {
                frame.uint(2);
                locale_resolution::issue(frame, key)
            }
            Subject::ArtifactCollection(_) => {
                frame.uint(3);
                artifact_collection::issue(frame, key)
            }
            Subject::ArtifactReference(_) => {
                frame.uint(4);
                resolved_binding::issue(frame, key)
            }
            Subject::Slot(_)
            | Subject::Vocabulary
            | Subject::CanonicalizationData
            | Subject::ResourcePolicy(_)
            | Subject::DeclarationField(_)
            | Subject::Fallback(_)
            | Subject::Coverage(_)
            | Subject::DeliveryPlacement => return defect(),
        };
        framed.map_err(|_| OutputFailure::HydrationRelationValidationInvariant)?;
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

/// Each group's relations, and the shared negotiation profile.
fn retained(frame: &mut Frame, retained: &Retained) -> Result<(), OutputFailure> {
    frame.uint(count(retained.groups.len())?);
    for group in &retained.groups {
        frame.text(&group.id);
        admitted(frame, &group.relations, |frame, relations| {
            frame.uint(count(relations.len())?);
            for (server, client) in relations {
                frame.text(server);
                frame.text(client);
            }
            Ok(())
        })?;
    }
    admitted(frame, &retained.negotiation, |_, ()| Ok(()))
}

/// The shared domain is what Phase 13 decided, none of it positional: every
/// unit and its status, each blocking result with a bound's details, each
/// group's relations, and the negotiation profile they share. The order of
/// the declaration's members, a group's members and relations, and the
/// collection is no part of the input, so reordering any of them changes
/// nothing here. A relation is no entry's own, so there is no entry domain.
pub(super) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let established = require_observable(resolved)?;
    let evaluation = &resolved.evaluation;
    let mut shared = Frame::new("hydration-relation-validation-result");
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
    use crate::benchmark::cases::{
        declarations, HydrationRecipe as R, Recipe, TargetAndGroupRecipe,
    };
    use crate::benchmark::observation::Digest;
    use crate::benchmark::operation::{Output, Prepared};
    use crate::input_limits::Bound;
    use crate::model::DeploymentGroupId;
    use crate::resolver::artifact_resolution::{unit as collection_unit, CollectionSubject};
    use crate::resolver::evaluation::{Blocking, Code, Details, Phase, Reason, Shortfall};
    use crate::resolver::exact_resolution::unit as reference_unit;
    use crate::resolver::hydration::unit as hydration_unit;
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::negotiation::{unit as negotiation_unit, NegotiationSubject};
    use crate::resolver::resource_policy::{PolicySlot, ReferenceField};
    use crate::resolver::targets::{unit as target_unit, TargetSubject};

    fn prepared(recipe: R) -> PreparedHydration {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::Hydration(recipe) && case.limit.is_none())
            .unwrap();
        let Prepared::Hydration(prepared) = prepare(&declaration).unwrap().prepared else {
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
        let mut frame = Frame::new("hydration-relation-test");
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

    fn relation(group_id: &str, server: &str, client: &str, check: Check) -> Unit {
        hydration_unit(
            HydrationSubject::Relation(group(group_id), target(server), target(client)),
            check,
        )
    }

    fn storefront(check: Check) -> Unit {
        relation("web", "ssr", "browser", check)
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

    /// The causes Phase 13's endpoint bound leaves.
    fn own_causes() -> Causes {
        blocked_unit(
            storefront(Check::WithinBound),
            Blocking::limit(exceeded(6, Actual::Exact(7))),
        )
        .1
    }

    /// The causes a target's locales leave in Phases 11 and 7.
    fn phase_causes(recipe: TargetAndGroupRecipe) -> Causes {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::TargetAndGroup(recipe) && case.limit.is_none())
            .unwrap();
        let Output::TargetAndGroup(resolved) = prepare(&declaration).unwrap().output else {
            unreachable!()
        };
        let Ok(Ok(retained)) = resolved.result else {
            unreachable!()
        };
        retained.targets[1].requested.clone().unwrap_err()
    }

    #[test]
    fn the_answers_are_read_by_each_profile_identity() {
        let prepared = prepared(R::Storefront);
        let set = prepared.set().unwrap();
        let Ok(set) = &set else { unreachable!() };
        let profile = |identity: &str| {
            set.iter()
                .find(|(tuple, _)| tuple.identity.as_str() == identity)
                .unwrap()
                .1
        };
        let (ssr, browser) = (profile("storefront-ssr"), profile("storefront-browser"));
        let mut answers = FixtureCapabilities::default();
        assert!(!answers.renders_server_side(ssr) && !answers.hydrates_in_browser(browser));
        assert!(!answers.statically_incompatible(ssr, browser));
        answers.renders.insert("storefront-ssr".into());
        answers.hydrates.insert("storefront-browser".into());
        answers
            .incompatible
            .insert(("storefront-ssr".into(), "storefront-browser".into()));
        assert!(answers.renders_server_side(ssr) && !answers.renders_server_side(browser));
        assert!(answers.hydrates_in_browser(browser) && !answers.hydrates_in_browser(ssr));
        assert!(answers.statically_incompatible(ssr, browser));
        assert!(!answers.statically_incompatible(browser, ssr));
    }

    #[test]
    fn the_retained_relations_are_the_ones_the_interval_made() {
        let prepared = prepared(R::FanOutFanIn);
        let set = prepared.set().unwrap();
        let inputs = prepared.inputs(&set).unwrap();
        let construction = construct(&inputs);
        let made = {
            let Ok(Ok(hydration)) = &construction.result else {
                unreachable!()
            };
            hydration
                .iter()
                .map(|(id, relations)| {
                    (
                        id.as_str().to_owned(),
                        relations
                            .as_ref()
                            .unwrap()
                            .iter()
                            .map(|(server, client)| {
                                (server.as_str().to_owned(), client.as_str().to_owned())
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let statuses = construction.evaluation.statuses().count();
        let retained = Resolved::retain(construction);
        assert_eq!(retained.evaluation.statuses().count(), statuses);
        let retained = retained.result.unwrap().unwrap();
        assert_eq!(
            retained
                .groups
                .iter()
                .map(|group| (group.id.clone(), group.relations.clone().unwrap()))
                .collect::<Vec<_>>(),
            made
        );
        // The pairs in (server, client) order.
        assert_eq!(
            made,
            [(
                "web".to_owned(),
                [("edge", "browser"), ("ssr", "app"), ("ssr", "browser")]
                    .map(|(server, client)| (server.to_owned(), client.to_owned()))
                    .to_vec()
            )]
        );
        assert!(retained.negotiation.is_ok());
        assert_eq!(prepared.resolve().unwrap().result, Ok(Ok(retained)));
    }

    #[test]
    fn a_resolver_defect_fails_the_case() {
        let resolved = Resolved {
            evaluation: Evaluation::default(),
            result: Err(EvaluationInvariant::UnitRecordedTwice(storefront(
                Check::WithinBound,
            ))),
        };
        assert_eq!(
            require_observable(&resolved).err(),
            Some(OutputFailure::HydrationRelationValidationInvariant)
        );
        assert_eq!(
            observe(&resolved).err(),
            Some(OutputFailure::HydrationRelationValidationInvariant)
        );
    }

    /// What Phase 13 cannot record over a complete configuration fails the
    /// case: unavailable groups, another phase's unit, details on a check
    /// other than a bound or none on a bound, a shortfall, a reason on a bound
    /// or on the static compatibility, none on another check, and a
    /// non-blocking result.
    #[test]
    fn only_what_phase_13_records_is_observed() {
        let storefront_retained = retained_of(R::Storefront);
        let observed = |evaluation| {
            observe(&Resolved {
                evaluation,
                result: Ok(Ok(storefront_retained.clone())),
            })
        };
        let limit = || Some(Details::Limit(exceeded(6, Actual::Exact(7))));
        let invalid = Code::HydrationRelationInvalid;
        // The controls: a complete case, a locale reason, an incompatibility
        // without a reason, and a bound's details are observed.
        assert!(observe(&resolved(R::Storefront)).is_ok());
        assert!(observe(&resolved(R::RequestedLocales)).is_ok());
        assert!(observe(&resolved(R::StaticIncompatibility)).is_ok());
        assert!(observed(blocked(
            storefront(Check::WithinBound),
            Code::ResourceLimitExceeded,
            None,
            limit(),
        ))
        .is_ok());
        let foreign = {
            let mut evaluation = Evaluation::default();
            let _ = evaluation
                .check(
                    target_unit(TargetSubject::Targets, Check::WithinBound),
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
                .notice(storefront(Check::KnownEndpoints), invalid)
                .unwrap();
            evaluation
        };
        for (name, evaluation) in [
            ("another phase's unit", foreign),
            (
                "details beside another check",
                blocked(
                    storefront(Check::KnownEndpoints),
                    invalid,
                    Some(Reason::UnknownEndpoint),
                    limit(),
                ),
            ),
            (
                "a bound without its details",
                blocked(
                    storefront(Check::WithinBound),
                    Code::ResourceLimitExceeded,
                    None,
                    None,
                ),
            ),
            (
                "a shortfall",
                blocked(
                    storefront(Check::KnownEndpoints),
                    invalid,
                    Some(Reason::UnknownEndpoint),
                    Some(shortfall),
                ),
            ),
            (
                "a reason on a bound",
                blocked(
                    storefront(Check::WithinBound),
                    Code::ResourceLimitExceeded,
                    Some(Reason::UnknownEndpoint),
                    limit(),
                ),
            ),
            (
                "a reason on the static compatibility",
                blocked(
                    storefront(Check::StaticCompatibility),
                    Code::HydrationCapabilityIncompatible,
                    Some(Reason::DualRole),
                    None,
                ),
            ),
            (
                "another check without its reason",
                blocked(storefront(Check::KnownEndpoints), invalid, None, None),
            ),
            ("notice", noticed),
        ] {
            // Logical work reads the screen without framing a unit.
            let resolved = Resolved {
                evaluation,
                result: Ok(Ok(storefront_retained.clone())),
            };
            for failure in [
                require_observable(&resolved).err(),
                observe(&resolved).err(),
            ] {
                assert_eq!(
                    failure,
                    Some(OutputFailure::HydrationRelationValidationInvariant),
                    "{name}"
                );
            }
        }
        // Over a complete configuration the groups are admitted.
        let unavailable = Resolved {
            evaluation: resolved(R::Storefront).evaluation,
            result: Ok(Err(own_causes())),
        };
        assert_eq!(
            observe(&unavailable).err(),
            Some(OutputFailure::HydrationRelationValidationInvariant)
        );
    }

    #[test]
    fn only_hydration_units_frame_and_other_phases_only_as_causes() {
        let mut frame = Frame::new("hydration-relation-test");
        let foreign = [
            target_unit(TargetSubject::Targets, Check::WithinBound),
            reference_unit(ReferenceField::Policy(PolicySlot::Trust)),
            Unit {
                check: Check::KnownMember,
                ..storefront(Check::WithinBound)
            },
        ];
        for recorded in &foreign {
            assert_eq!(
                unit(&mut frame, recorded),
                Err(OutputFailure::HydrationRelationValidationInvariant),
                "{recorded:?}"
            );
        }
        // Each subject frames apart, each endpoint and the group are part of
        // a relation, and a Target ID's roles are by group.
        let roles = |group_id: &str, id: &str| {
            hydration_unit(
                HydrationSubject::Roles(group(group_id), target(id)),
                Check::WithinBound,
            )
        };
        let subjects = [
            hydration_unit(HydrationSubject::Relations, Check::WithinBound),
            relation("web", "ssr", "browser", Check::WithinBound),
            relation("web", "browser", "ssr", Check::WithinBound),
            relation("web", "ssr", "app", Check::WithinBound),
            relation("edge", "ssr", "browser", Check::WithinBound),
            roles("web", "ssr"),
            roles("web", "browser"),
            roles("edge", "ssr"),
        ]
        .map(|recorded| digest(|frame| unit(frame, &recorded).unwrap()));
        assert!(apart(&subjects));
        // Each of the eleven checks frames apart, and the phase is part of
        // the unit.
        let checks = [
            Check::WithinBound,
            Check::KnownEndpoints,
            Check::DistinctEndpoints,
            Check::SameGroupEndpoints,
            Check::UniqueRelation,
            Check::ServerRole,
            Check::ClientRole,
            Check::SameRequestedLocales,
            Check::SameEffectiveDefault,
            Check::StaticCompatibility,
            Check::SingleRole,
        ]
        .map(|check| digest(|frame| unit(frame, &storefront(check)).unwrap()));
        assert!(apart(&checks));
        let other_phase = Unit {
            phase: Phase::GroupResolution,
            ..storefront(Check::WithinBound)
        };
        assert_ne!(
            digest(|frame| unit(frame, &other_phase).unwrap()),
            checks[0]
        );
        // A cause frames as its own boundary frames it: Phase 13's, Phase 11
        // or 12's, Phase 7 or 8's, the collection's, or Phase 9's.
        let targets = blocked_unit(
            target_unit(TargetSubject::Targets, Check::WithinBound),
            Blocking::limit(exceeded(1, Actual::Exact(2))),
        )
        .1;
        let locale = phase_causes(TargetAndGroupRecipe::TargetLocaleInvalid);
        let negotiation = blocked_unit(
            negotiation_unit(NegotiationSubject::Aliases, Check::WithinBound),
            Blocking::limit(exceeded(1, Actual::Exact(2))),
        )
        .1;
        let collection = blocked_unit(
            collection_unit(CollectionSubject::Member(0), Check::SingleOccurrence),
            Blocking {
                code: Code::ArtifactCollectionNotAdmitted,
                reason: Some(Reason::DuplicateEnvelope),
                details: None,
            },
        )
        .1;
        let reference = blocked_unit(
            reference_unit(ReferenceField::Policy(PolicySlot::Trust)),
            Blocking {
                code: Code::ArtifactReferenceNotAdmitted,
                reason: Some(Reason::NotFound),
                details: None,
            },
        )
        .1;
        let framed = [
            own_causes(),
            targets,
            locale,
            negotiation,
            collection,
            reference,
        ]
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
                Err(OutputFailure::HydrationRelationValidationInvariant)
            );
        }
    }

    #[test]
    fn every_part_of_a_result_frames_apart() {
        let fan = retained_of(R::FanOutFanIn);
        let why = own_causes();
        let with = |change: &dyn Fn(&mut Retained)| {
            let mut changed = fan.clone();
            change(&mut changed);
            changed
        };
        let framed = [
            fan.clone(),
            with(&|retained| {
                retained.groups.pop();
            }),
            with(&|retained| retained.groups[0].id = "edge".into()),
            with(&|retained| {
                if let Ok(relations) = &mut retained.groups[0].relations {
                    relations.pop();
                }
            }),
            with(&|retained| {
                if let Ok(relations) = &mut retained.groups[0].relations {
                    relations[0].0 = "app".into();
                }
            }),
            with(&|retained| {
                if let Ok(relations) = &mut retained.groups[0].relations {
                    relations[0].1 = "app".into();
                }
            }),
            with(&|retained| retained.groups[0].relations = Err(why.clone())),
            with(&|retained| retained.negotiation = Err(why.clone())),
        ]
        .map(|changed| digest(|frame| retained(frame, &changed).unwrap()));
        assert!(apart(&framed));
        // A status frames whether the unit was checked and what it waits on.
        let statuses = [
            Status::Evaluated,
            Status::NotEvaluated(why.clone()),
            Status::NotEvaluated(phase_causes(TargetAndGroupRecipe::TargetLocaleInvalid)),
        ]
        .map(|recorded| digest(|frame| status(frame, &recorded).unwrap()));
        assert!(apart(&statuses));
        // A blocking result frames its code and whether it has a reason.
        let key = |check, code, reason| {
            let evaluation = blocked(storefront(check), code, reason, None);
            let recorded = evaluation.issues().next().unwrap().clone();
            recorded
        };
        let issues = [
            key(
                Check::StaticCompatibility,
                Code::HydrationCapabilityIncompatible,
                None,
            ),
            key(
                Check::SameRequestedLocales,
                Code::HydrationLocalePolicyIncompatible,
                Some(Reason::RequestedLocales),
            ),
            key(
                Check::SameRequestedLocales,
                Code::HydrationLocalePolicyIncompatible,
                Some(Reason::EffectiveDefault),
            ),
            key(
                Check::SameRequestedLocales,
                Code::HydrationRelationInvalid,
                Some(Reason::RequestedLocales),
            ),
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
            Some(other(Group::TargetGrouping, "maxHydrationRelations")),
            Some(other(Group::ArtifactAdmission, "maxTargetIdBytes")),
        ]
        .map(|exceeded| digest(|frame| limit(frame, exceeded)));
        assert!(apart(&limits));
    }
}
