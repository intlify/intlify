// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 11 (`target-resolution`) over the selected declaration's
//! targets: the Target ID map under `maxTargetProfiles`, each Target ID under
//! `maxTargetIdBytes`, each target's requested locales as a subset of the
//! project's, its effective default, which is its own override or else the
//! project's default and must be one of its requested locales, and the
//! directions its Target Profile has for its requested locales. No default is
//! inferred: a target without an override takes the project's default, never
//! the first, only, or smallest of its own locales. No direction is inferred
//! either: only an entry spelled as the canonical locale covers it.
//!
//! Not the canonicalization of a target's locales or the repeats among them,
//! which Phase 7 did; not the exact resolution of its Target Profile reference,
//! which Phase 9 did and this phase only joins; and not the groups (Phase 12)
//! or the hydration relations (Phase 13). A subject is a Target ID, a semantic
//! identity the declaration's map admitted.

use std::collections::BTreeMap;

use crate::artifacts::content::Content;
use crate::locale::CanonicalLocale;
use crate::model::{NonEmptyMap, TargetDeclaration, TargetId};

use super::artifact_resolution::AdmittedArtifact;
use super::evaluation::{
    and, both, within, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant,
    Phase, Reason, Subject, Unit,
};
use super::exact_resolution::{Named, ReferenceResults};
use super::limits::{BoundId, ConfirmedPolicy, Group};
use super::locales::{LocaleSet, ResolvedLocales, TargetLocales};
use super::resource_policy::ReferenceField;

const MAX_TARGET_PROFILES: BoundId = BoundId {
    group: Group::TargetGrouping,
    name: "maxTargetProfiles",
};

const MAX_TARGET_ID_BYTES: BoundId = BoundId {
    group: Group::TargetGrouping,
    name: "maxTargetIdBytes",
};

/// What one Phase 11 unit checks. The map's bound, which every target waits
/// on, comes first.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TargetSubject {
    /// The submitted Target ID map, under `maxTargetProfiles`.
    Targets,
    /// One Target ID: its bytes under `maxTargetIdBytes`, then its requested
    /// locales, its effective default, and its Target Profile's directions.
    Target(TargetId),
}

/// The unit Phase 11 records for `check` over `subject`.
pub(crate) fn unit(subject: TargetSubject, check: Check) -> Unit {
    Unit {
        phase: Phase::TargetResolution,
        subject: Subject::Target(subject),
        check,
    }
}

/// One target as Phase 11 resolved it: the Set's Target Profile its reference
/// names, its requested locales, and its effective default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedTarget<'s> {
    pub(crate) profile: Named<'s>,
    pub(crate) requested: LocaleSet,
    pub(crate) effective_default: CanonicalLocale,
}

/// What Phase 11 established of one declared target. A later check reads
/// only the facts it needs, so one that failed leaves only those checks
/// unevaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TargetFacts<'s> {
    /// The Target ID within the map's bound and its own: what a later check
    /// that names the target needs, whatever its locales and Target Profile.
    pub(crate) admitted: Admitted<()>,
    /// Its requested locales, once found a subset of the project's.
    pub(crate) requested: Admitted<LocaleSet>,
    /// Its effective default, once found one of its requested locales.
    pub(crate) effective_default: Admitted<CanonicalLocale>,
    /// The Set's Target Profile its reference names.
    pub(crate) profile: Admitted<Named<'s>>,
    /// The target, once every fact above and its repeats check held.
    pub(crate) resolved: Admitted<ResolvedTarget<'s>>,
}

/// Every declared target's facts, in Target ID order, and the map's bound,
/// which a later check over the set of Target IDs needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedTargets<'s> {
    bounded: Admitted<()>,
    facts: BTreeMap<TargetId, TargetFacts<'s>>,
}

impl<'s> ResolvedTargets<'s> {
    /// The map within `maxTargetProfiles`: the set of Target IDs is admitted.
    pub(crate) const fn bounded(&self) -> &Admitted<()> {
        &self.bounded
    }

    pub(crate) fn get(&self, id: &TargetId) -> Option<&TargetFacts<'s>> {
        self.facts.get(id)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&TargetId, &TargetFacts<'s>)> {
        self.facts.iter()
    }

    /// Whether every target resolved, or the causes of each that did not.
    pub(crate) fn complete(&self) -> Admitted<()> {
        let mut holds = Ok(());
        for facts in self.facts.values() {
            let resolved = facts.resolved.as_ref().map(|_| ()).map_err(Causes::clone);
            holds = and(holds, resolved);
        }
        holds
    }
}

/// Phase 11 under the confirmed policy, over the declared targets, the locales
/// Phase 7 resolved, and the references Phase 9 resolved. Every target waits
/// on the map's bound, and each check of a target on its ID's bound. The
/// subset, the effective default, and the directions are checked
/// independently of each other; only the directions wait on the Target
/// Profile, and the resolved target needs every one.
pub(crate) fn resolve_targets<'s, Target>(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    declared: Admitted<&NonEmptyMap<TargetId, TargetDeclaration<Target>>>,
    locales: &ResolvedLocales,
    references: &ReferenceResults<'s>,
) -> Result<Admitted<ResolvedTargets<'s>>, EvaluationInvariant> {
    let policy = policy.map_err(Causes::clone);
    let bounded = evaluation.check(
        unit(TargetSubject::Targets, Check::WithinBound),
        both(policy.clone(), declared.clone()),
        |(policy, declared)| {
            within(
                MAX_TARGET_PROFILES,
                policy.limits().target_grouping.max_target_profiles,
                declared.len(),
            )
        },
    )?;
    let declared = match declared {
        Ok(declared) => declared,
        Err(causes) => return Ok(Err(causes)),
    };
    let by_id: Admitted<BTreeMap<&TargetId, &TargetLocales>> = locales
        .targets
        .as_ref()
        .map(|targets| targets.iter().map(|target| (&target.id, target)).collect())
        .map_err(Causes::clone);
    let mut facts = BTreeMap::new();
    for (id, _) in declared.iter() {
        let own = by_id.as_ref().map_err(Causes::clone).map(|by_id| {
            *by_id
                .get(id)
                .expect("Phase 7 resolves the locales of every declared target")
        });
        let checks = Checks {
            policy: &policy,
            bounded: &bounded,
            id,
        };
        facts.insert(
            id.clone(),
            checks.resolve(evaluation, own, locales, references)?,
        );
    }
    Ok(Ok(ResolvedTargets { bounded, facts }))
}

/// One target's checks and what they share.
struct Checks<'a, 'p> {
    policy: &'a Admitted<&'p ConfirmedPolicy>,
    bounded: &'a Admitted<()>,
    id: &'a TargetId,
}

impl Checks<'_, '_> {
    fn unit(&self, check: Check) -> Unit {
        unit(TargetSubject::Target(self.id.clone()), check)
    }

    /// The ID's bound, then the subset, the effective default, and the
    /// directions, then the target when everything it needs was admitted.
    fn resolve<'s>(
        &self,
        evaluation: &mut Evaluation,
        own: Admitted<&TargetLocales>,
        locales: &ResolvedLocales,
        references: &ReferenceResults<'s>,
    ) -> Result<TargetFacts<'s>, EvaluationInvariant> {
        let admitted = evaluation.check(
            self.unit(Check::WithinBound),
            both(self.policy.clone(), self.bounded.clone()),
            |(policy, ())| {
                within(
                    MAX_TARGET_ID_BYTES,
                    policy.limits().target_grouping.max_target_id_bytes,
                    self.id.as_str().len(),
                )
            },
        )?;
        let requested = own.clone().and_then(|own| own.requested.clone());
        let subset = evaluation.check(
            self.unit(Check::ProjectSubset),
            both(
                both(admitted.clone(), requested.clone()),
                locales.requested_set.clone(),
            ),
            |(((), requested), project)| {
                if requested
                    .as_slice()
                    .iter()
                    .all(|locale| project.contains(locale))
                {
                    Ok(())
                } else {
                    Err(issue(Code::TargetLocalesNotSubset, None))
                }
            },
        )?;
        let default = evaluation.check(
            self.unit(Check::SupportedDefault),
            both(
                both(admitted.clone(), requested.clone()),
                effective_default(own.clone(), locales),
            ),
            |(((), requested), (locale, reason))| {
                if requested.contains(&locale) {
                    Ok(locale)
                } else {
                    Err(issue(Code::TargetDefaultLocaleNotSupported, Some(reason)))
                }
            },
        )?;
        let profile = profile(references, self.id);
        let directed = evaluation.check(
            self.unit(Check::DirectedLocales),
            both(both(admitted.clone(), requested.clone()), profile.clone()),
            |(((), requested), (_, artifact))| {
                if directs(artifact, &requested) {
                    Ok(())
                } else {
                    Err(issue(Code::TargetLocaleDirectionMissing, None))
                }
            },
        )?;
        let distinct = own.and_then(|own| own.distinct.clone());
        let requested = both(requested, subset).map(|(requested, ())| requested);
        let resolved = both(
            both(requested.clone(), distinct),
            both(default.clone(), both(profile.clone(), directed)),
        )
        .map(
            |((requested, ()), (effective_default, (profile, ())))| ResolvedTarget {
                profile,
                requested,
                effective_default,
            },
        );
        Ok(TargetFacts {
            admitted,
            requested,
            effective_default: default,
            profile,
            resolved,
        })
    }
}

/// The target's effective default and where it came from: its own override,
/// or else the project's default.
fn effective_default(
    own: Admitted<&TargetLocales>,
    locales: &ResolvedLocales,
) -> Admitted<(CanonicalLocale, Reason)> {
    own.and_then(|own| match &own.default {
        Ok(Some(locale)) => Ok((locale.clone(), Reason::ExplicitOverride)),
        Ok(None) => locales
            .requested_default
            .clone()
            .map(|locale| (locale, Reason::InheritedProjectDefault)),
        Err(causes) => Err(causes.clone()),
    })
}

/// Whether the Target Profile has a direction for each of `requested`: an
/// entry whose locale is the canonical spelling, byte for byte. An entry for
/// another locale is unused, and an artifact that is no Target Profile has
/// none.
fn directs(artifact: &AdmittedArtifact<'_>, requested: &LocaleSet) -> bool {
    let Content::TargetProfile(profile) = &artifact.content else {
        return false;
    };
    requested.as_slice().iter().all(|locale| {
        profile
            .directions
            .iter()
            .any(|direction| direction.locale == locale.as_str())
    })
}

/// The Set's Target Profile the target's reference names, or why it names
/// none.
fn profile<'s>(references: &ReferenceResults<'s>, id: &TargetId) -> Admitted<Named<'s>> {
    match references.get(&ReferenceField::TargetProfile(id.clone())) {
        Some(found) => found.clone(),
        None => Err(references
            .unavailable()
            .cloned()
            .expect("Phase 9 resolves or suppresses every declared target's reference")),
    }
}

const fn issue(code: Code, reason: Option<Reason>) -> Blocking {
    Blocking {
        code,
        reason,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::{json, Map, Value};

    use super::*;
    use crate::artifacts::collection::{read_collection, CollectionLimits};
    use crate::artifacts::content::sample;
    use crate::artifacts::envelope::{envelope_of, ArtifactKind, ExactTuple, FormalMember};
    use crate::input_limits::Bound;
    use crate::model::ProfileDeclaration;
    use crate::profile_fixtures::minimal_config;
    use crate::references::{PolicyReference, TargetProfileReference};
    use crate::resolver::artifact_resolution::admit_collection;
    use crate::resolver::evaluation::{Actual, IssueKey, LimitExceeded, Status};
    use crate::resolver::exact_resolution::{self, resolve_fields};
    use crate::resolver::limits::{fixture_limits, ResolutionLimits};
    use crate::resolver::locales::fixtures::{
        canonical, canonicalizer, content, key, stand_in, target, waits_on,
    };
    use crate::resolver::locales::{self, resolve_locales, LocaleFields, LocaleSubject};
    use crate::resolver::resource_policy::DeclarationReferences;
    use crate::resolver::specification::Slot;

    type Declaration = ProfileDeclaration<PolicyReference, TargetProfileReference>;

    /// Collection limits no test reaches.
    const ROOM: CollectionLimits = CollectionLimits {
        members: u64::MAX,
        member_bytes: u64::MAX,
        total_bytes: u64::MAX,
        depth: u64::MAX,
    };

    /// The Target Profile the target `id` names.
    fn profile_of(id: &str) -> Value {
        envelope_of(
            ArtifactKind::TargetProfile,
            &format!("storefront-{id}"),
            "1",
            sample(ArtifactKind::TargetProfile),
        )
    }

    fn tuple_of(member: &Value) -> ExactTuple {
        FormalMember::new(member.clone())
            .unwrap()
            .envelope()
            .unwrap()
            .tuple()
            .clone()
    }

    /// The reference a declaration writes for `tuple`.
    fn reference_value(tuple: &ExactTuple) -> Value {
        json!({
            "kind": tuple.kind.spelling(),
            "identity": tuple.identity.as_str(),
            "revision": tuple.revision.as_str(),
            "specificationRevision": tuple.specification_revision.as_str(),
            "semanticDigest": tuple.semantic_digest.as_str(),
        })
    }

    /// What Phase 11 reads, which a test edits: the declaration, the Target
    /// Profiles submitted, the policy's bounds, and whether a policy was
    /// confirmed and structural admission admitted the Target ID map.
    struct Inputs {
        app: Value,
        members: Vec<Value>,
        limits: ResolutionLimits,
        policy: Admitted<()>,
        declared: Admitted<()>,
    }

    impl Inputs {
        /// Project `en`, `ja`, and `fr` with the default `en`; target
        /// `browser` requesting `ja` and `en` with its own default `ja`, and
        /// target `server` requesting `en` with no default of its own. Each
        /// target names its own submitted Target Profile.
        fn new() -> Self {
            let mut app = minimal_config()["profiles"]["app"].clone();
            app["requestedLocales"] = json!(["en", "ja", "fr"]);
            app["defaultRequestedLocale"] = json!("en");
            let mut members = Vec::new();
            let mut targets = Map::new();
            for (id, requested, default) in [
                ("browser", json!(["ja", "en"]), Some("ja")),
                ("server", json!(["en"]), None),
            ] {
                let member = profile_of(id);
                let mut declared = json!({
                    "profile": reference_value(&tuple_of(&member)),
                    "requestedLocales": requested,
                });
                if let Some(default) = default {
                    declared["defaultRequestedLocale"] = json!(default);
                }
                targets.insert(id.into(), declared);
                members.push(member);
            }
            app["targetProfiles"] = Value::Object(targets);
            app["deploymentGroups"] = json!({"web": {"members": ["browser", "server"]}});
            Self {
                app,
                members,
                limits: fixture_limits(100),
                policy: Ok(()),
                declared: Ok(()),
            }
        }

        fn target(&mut self, id: &str) -> &mut Value {
            &mut self.app["targetProfiles"][id]
        }

        /// Submits, in place of the Target Profile the target `id` names, one
        /// with `directions`, and names it.
        fn direct(&mut self, id: &str, directions: Value) {
            let mut content = sample(ArtifactKind::TargetProfile);
            content["locale"]["directions"] = directions;
            let member = envelope_of(
                ArtifactKind::TargetProfile,
                &format!("storefront-{id}"),
                "1",
                content,
            );
            let replaced = profile_of(id);
            for submitted in &mut self.members {
                if *submitted == replaced {
                    *submitted = member.clone();
                }
            }
            self.target(id)["profile"] = reference_value(&tuple_of(&member));
        }

        /// Phase 7, Phase 9's exact resolution of the targets' references,
        /// and Phase 11, into one evaluation; `inspect` reads it and what
        /// Phase 11 returned.
        fn resolve<R>(
            &self,
            inspect: impl FnOnce(Evaluation, Admitted<ResolvedTargets<'_>>) -> R,
        ) -> R {
            let declaration: Declaration = serde_json::from_value(self.app.clone()).unwrap();
            let confirmed = ConfirmedPolicy::fixture_owned(self.limits.clone());
            let mut evaluation = Evaluation::default();
            let locales = resolve_locales(
                &mut evaluation,
                Ok(&confirmed),
                Ok(&canonicalizer(64)),
                &LocaleFields::declared(&declaration),
            )
            .unwrap();
            let texts: Vec<String> = self.members.iter().map(Value::to_string).collect();
            let bytes: Vec<&[u8]> = texts.iter().map(String::as_bytes).collect();
            let collection = read_collection(Some(&bytes), &ROOM).unwrap();
            let set = admit_collection(&mut evaluation, Ok(&confirmed), &collection).unwrap();
            // The policies' references are no concern of Phase 11's.
            let references = resolve_fields(
                &mut evaluation,
                set.as_ref(),
                &DeclarationReferences {
                    policies: Err(stand_in(Slot::ProjectProfile)),
                    target_profiles: Ok(&declaration.target_profiles),
                },
            )
            .unwrap();
            let unconfirmed = self.policy.clone().err();
            let policy = unconfirmed.as_ref().map_or(Ok(&confirmed), Err);
            let declared = self.declared.clone().map(|()| &declaration.target_profiles);
            let targets =
                resolve_targets(&mut evaluation, policy, declared, &locales, &references).unwrap();
            inspect(evaluation, targets)
        }
    }

    fn bound(value: u64) -> Bound {
        Bound::new(value).unwrap()
    }

    fn target_unit(id: &str, check: Check) -> Unit {
        unit(TargetSubject::Target(target(id)), check)
    }

    fn map_unit() -> Unit {
        unit(TargetSubject::Targets, Check::WithinBound)
    }

    /// Every unit Phase 11 records for the two targets, in 015's order.
    fn every_unit() -> Vec<Unit> {
        let mut units = vec![map_unit()];
        for id in ["browser", "server"] {
            for check in [
                Check::WithinBound,
                Check::ProjectSubset,
                Check::SupportedDefault,
                Check::DirectedLocales,
            ] {
                units.push(target_unit(id, check));
            }
        }
        units
    }

    /// Phase 11's units and their statuses, in 015's order.
    fn statuses(evaluation: &Evaluation) -> Vec<(Unit, Status)> {
        evaluation
            .statuses()
            .filter(|(unit, _)| unit.phase == Phase::TargetResolution)
            .map(|(unit, status)| (unit.clone(), status.clone()))
            .collect()
    }

    /// Every blocking result, as its unit, code, and reason.
    fn issues(evaluation: &Evaluation) -> Vec<(Unit, Code, Option<Reason>)> {
        evaluation
            .issues()
            .map(|key| (key.unit().clone(), key.code(), key.reason()))
            .collect()
    }

    fn keys(causes: &Causes) -> BTreeSet<IssueKey> {
        causes.keys().cloned().collect()
    }

    fn facts<'t, 's>(targets: &'t ResolvedTargets<'s>, id: &str) -> &'t TargetFacts<'s> {
        targets.get(&target(id)).unwrap()
    }

    fn spellings(set: &LocaleSet) -> Vec<&str> {
        set.as_slice().iter().map(CanonicalLocale::as_str).collect()
    }

    #[test]
    fn subjects_order_the_map_first_then_each_target_after_every_phase_10_unit() {
        let ordered = [
            map_unit(),
            target_unit("browser", Check::WithinBound),
            target_unit("browser", Check::ProjectSubset),
            target_unit("browser", Check::SupportedDefault),
            target_unit("browser", Check::DirectedLocales),
            target_unit("server", Check::WithinBound),
        ];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
        let placement = Unit {
            phase: Phase::DeliveryResolution,
            subject: Subject::DeliveryPlacement,
            check: Check::EffectiveMode,
        };
        assert!(placement < map_unit());
    }

    #[test]
    fn each_target_resolves_its_subset_effective_default_and_target_profile() {
        Inputs::new().resolve(|evaluation, targets| {
            assert_eq!(issues(&evaluation), []);
            assert_eq!(
                statuses(&evaluation),
                every_unit()
                    .into_iter()
                    .map(|unit| (unit, Status::Evaluated))
                    .collect::<Vec<_>>()
            );
            let targets = targets.unwrap();
            assert_eq!(targets.complete(), Ok(()));
            assert_eq!(
                targets
                    .iter()
                    .map(|(id, _)| id.as_str())
                    .collect::<Vec<_>>(),
                ["browser", "server"]
            );
            // The browser's own default, and the project's for the server.
            for (id, requested, default) in [
                ("browser", vec!["en", "ja"], "ja"),
                ("server", vec!["en"], "en"),
            ] {
                let resolved = facts(&targets, id).resolved.as_ref().unwrap();
                assert_eq!(spellings(&resolved.requested), requested);
                assert_eq!(resolved.effective_default.as_str(), default);
                let (tuple, artifact) = resolved.profile;
                assert_eq!(tuple, &tuple_of(&profile_of(id)));
                assert_eq!(artifact.member.envelope().unwrap().tuple(), tuple);
            }
        });
    }

    /// The sample Target Profile directs `de`, `en`, `fr-FR`, and `ja`, more
    /// than either target requests: an entry for another locale is unused.
    #[test]
    fn a_requested_locale_without_a_direction_blocks_only_that_target() {
        let mut inputs = Inputs::new();
        // `JA` is no canonical spelling, so it directs nothing, and `fr` is
        // no locale the browser requests.
        inputs.direct(
            "browser",
            json!([
                {"locale": "JA", "direction": "LTR"},
                {"locale": "en", "direction": "LTR"},
                {"locale": "fr", "direction": "LTR"},
            ]),
        );
        inputs.resolve(|evaluation, targets| {
            assert_eq!(
                issues(&evaluation),
                [(
                    target_unit("browser", Check::DirectedLocales),
                    Code::TargetLocaleDirectionMissing,
                    None
                )]
            );
            assert!(statuses(&evaluation)
                .iter()
                .all(|(_, status)| *status == Status::Evaluated));
            let key = evaluation.issues().next().unwrap().clone();
            let targets = targets.unwrap();
            // What a later phase reads of the browser holds all the same.
            let browser = facts(&targets, "browser");
            assert_eq!(browser.admitted, Ok(()));
            assert_eq!(spellings(browser.requested.as_ref().unwrap()), ["en", "ja"]);
            assert_eq!(browser.effective_default.as_ref().unwrap().as_str(), "ja");
            assert!(browser.profile.is_ok());
            assert_eq!(
                keys(browser.resolved.as_ref().unwrap_err()),
                BTreeSet::from([key.clone()])
            );
            assert!(facts(&targets, "server").resolved.is_ok());
            assert_eq!(
                keys(&targets.complete().unwrap_err()),
                BTreeSet::from([key])
            );
        });
        // Exactly the browser's two locales are enough.
        let mut exact = Inputs::new();
        exact.direct(
            "browser",
            json!([
                {"locale": "en", "direction": "LTR"},
                {"locale": "ja", "direction": "RTL"},
            ]),
        );
        exact.resolve(|evaluation, targets| {
            assert_eq!(issues(&evaluation), []);
            assert_eq!(targets.unwrap().complete(), Ok(()));
        });
    }

    /// Only an entry spelled as the canonical locale directs it, and an
    /// artifact that is no Target Profile directs nothing.
    #[test]
    fn only_a_target_profiles_canonical_entry_directs_a_locale() {
        let set = |spellings: &[&str]| {
            LocaleSet::new(
                spellings
                    .iter()
                    .map(|spelling| canonical(spelling))
                    .collect(),
            )
        };
        let profile = FormalMember::new(profile_of("browser")).unwrap();
        let trust = FormalMember::new(envelope_of(
            ArtifactKind::TrustPolicy,
            "storefront-trust",
            "1",
            sample(ArtifactKind::TrustPolicy),
        ))
        .unwrap();
        let admitted = |member| AdmittedArtifact {
            member,
            content: member.envelope().unwrap().admit().unwrap(),
        };
        let (profile, trust) = (admitted(&profile), admitted(&trust));
        assert!(directs(&profile, &set(&["en"])));
        assert!(directs(&profile, &set(&["de", "en", "fr-FR", "ja"])));
        assert!(!directs(&profile, &set(&["en", "fr"])));
        assert!(!directs(&trust, &set(&["en"])));
    }

    #[test]
    fn a_locale_outside_the_project_is_no_subset_and_the_other_checks_go_on() {
        let mut inputs = Inputs::new();
        inputs.target("browser")["requestedLocales"] = json!(["ja", "de", "en"]);
        inputs.resolve(|evaluation, targets| {
            let subset = target_unit("browser", Check::ProjectSubset);
            assert_eq!(
                issues(&evaluation),
                [(subset.clone(), Code::TargetLocalesNotSubset, None)]
            );
            // The browser's default and directions are still checked, against
            // its own locales.
            for check in [Check::SupportedDefault, Check::DirectedLocales] {
                assert_eq!(waits_on(&evaluation, &target_unit("browser", check)), None);
            }
            let targets = targets.unwrap();
            let key = evaluation.issues().next().unwrap().clone();
            assert_eq!(
                keys(facts(&targets, "browser").resolved.as_ref().unwrap_err()),
                BTreeSet::from([key.clone()])
            );
            assert!(facts(&targets, "server").resolved.is_ok());
            assert_eq!(
                keys(&targets.complete().unwrap_err()),
                BTreeSet::from([key])
            );
        });
    }

    #[test]
    fn an_effective_default_outside_the_subset_says_where_it_came_from() {
        let mut inputs = Inputs::new();
        // `fr` is the project's but not the browser's; the server has only
        // `en` and inherits `ja`, and no locale of its own stands in.
        inputs.target("browser")["defaultRequestedLocale"] = json!("fr");
        inputs.app["defaultRequestedLocale"] = json!("ja");
        inputs.resolve(|evaluation, targets| {
            assert_eq!(
                issues(&evaluation),
                [
                    (
                        target_unit("browser", Check::SupportedDefault),
                        Code::TargetDefaultLocaleNotSupported,
                        Some(Reason::ExplicitOverride)
                    ),
                    (
                        target_unit("server", Check::SupportedDefault),
                        Code::TargetDefaultLocaleNotSupported,
                        Some(Reason::InheritedProjectDefault)
                    ),
                ]
            );
            let targets = targets.unwrap();
            for id in ["browser", "server"] {
                assert!(facts(&targets, id).resolved.is_err(), "{id}");
                assert_eq!(facts(&targets, id).admitted, Ok(()), "{id}");
            }
        });
    }

    #[test]
    fn the_map_holds_at_its_bound_and_one_target_more_leaves_every_target_unchecked() {
        for (limit, over) in [(2, false), (1, true)] {
            let mut inputs = Inputs::new();
            inputs.limits.target_grouping.max_target_profiles = bound(limit);
            inputs.resolve(|evaluation, targets| {
                if !over {
                    assert_eq!(issues(&evaluation), [], "{limit}");
                    return;
                }
                let key = evaluation.issues().next().unwrap().clone();
                assert_eq!(
                    issues(&evaluation),
                    [(map_unit(), Code::ResourceLimitExceeded, None)]
                );
                assert_eq!(
                    evaluation.limit(&key),
                    Some(LimitExceeded {
                        bound: MAX_TARGET_PROFILES,
                        limit: bound(1),
                        actual: Actual::Exact(2),
                    })
                );
                for unit in &every_unit()[1..] {
                    assert_eq!(
                        waits_on(&evaluation, unit),
                        Some(vec![map_unit()]),
                        "{unit:?}"
                    );
                }
                let targets = targets.unwrap();
                for id in ["browser", "server"] {
                    assert_eq!(
                        keys(facts(&targets, id).admitted.as_ref().unwrap_err()),
                        BTreeSet::from([key.clone()])
                    );
                }
            });
        }
    }

    #[test]
    fn each_target_id_holds_at_its_bound_and_one_byte_more_blocks_only_that_target() {
        // `browser` is seven bytes and `server` six.
        for (limit, over) in [(7, false), (6, true)] {
            let mut inputs = Inputs::new();
            inputs.limits.target_grouping.max_target_id_bytes = bound(limit);
            inputs.resolve(|evaluation, targets| {
                if !over {
                    assert_eq!(issues(&evaluation), [], "{limit}");
                    return;
                }
                let id = target_unit("browser", Check::WithinBound);
                assert_eq!(
                    issues(&evaluation),
                    [(id.clone(), Code::ResourceLimitExceeded, None)]
                );
                let key = evaluation.issues().next().unwrap().clone();
                assert_eq!(
                    evaluation.limit(&key),
                    Some(LimitExceeded {
                        bound: MAX_TARGET_ID_BYTES,
                        limit: bound(6),
                        actual: Actual::Exact(7),
                    })
                );
                for check in [
                    Check::ProjectSubset,
                    Check::SupportedDefault,
                    Check::DirectedLocales,
                ] {
                    assert_eq!(
                        waits_on(&evaluation, &target_unit("browser", check)),
                        Some(vec![id.clone()])
                    );
                    assert_eq!(waits_on(&evaluation, &target_unit("server", check)), None);
                }
                let targets = targets.unwrap();
                assert!(facts(&targets, "browser").admitted.is_err());
                assert!(facts(&targets, "server").resolved.is_ok());
            });
        }
    }

    #[test]
    fn a_target_profile_phase_9_could_not_resolve_leaves_only_that_target_unresolved() {
        let mut inputs = Inputs::new();
        let server = profile_of("server");
        inputs.members.retain(|member| *member != server);
        inputs.resolve(|evaluation, targets| {
            let reference = exact_resolution::unit(ReferenceField::TargetProfile(target("server")));
            let [(unit, code, _)] = issues(&evaluation).try_into().unwrap();
            assert_eq!(
                (unit, code),
                (reference.clone(), Code::ArtifactReferenceNotAdmitted)
            );
            // Phase 11 checks the server all the same, but for the
            // directions, which wait on its Target Profile.
            let directed = target_unit("server", Check::DirectedLocales);
            assert_eq!(
                waits_on(&evaluation, &directed),
                Some(vec![reference.clone()])
            );
            assert!(statuses(&evaluation)
                .iter()
                .filter(|(unit, _)| *unit != directed)
                .all(|(_, status)| *status == Status::Evaluated));
            let targets = targets.unwrap();
            let key = evaluation.issues().next().unwrap().clone();
            assert_eq!(
                keys(facts(&targets, "server").resolved.as_ref().unwrap_err()),
                BTreeSet::from([key])
            );
            assert!(facts(&targets, "browser").resolved.is_ok());
        });
    }

    #[test]
    fn invalid_target_locales_leave_the_checks_that_need_them_not_evaluated() {
        let mut inputs = Inputs::new();
        inputs.target("browser")["requestedLocales"] = json!(["ja", "en-x-brand"]);
        inputs.target("browser")["defaultRequestedLocale"] = json!("en_US");
        inputs.resolve(|evaluation, targets| {
            let member = locales::unit(
                LocaleSubject::TargetRequestedLocale(target("browser"), content("en-x-brand")),
                Check::CanonicalLocale,
            );
            let default = locales::unit(
                LocaleSubject::TargetDefaultRequestedLocale(target("browser"), content("en_US")),
                Check::CanonicalLocale,
            );
            for check in [Check::ProjectSubset, Check::DirectedLocales] {
                assert_eq!(
                    waits_on(&evaluation, &target_unit("browser", check)),
                    Some(vec![member.clone()])
                );
            }
            assert_eq!(
                waits_on(
                    &evaluation,
                    &target_unit("browser", Check::SupportedDefault)
                ),
                Some(vec![member, default])
            );
            // The ID was checked, and the server is untouched.
            assert_eq!(
                waits_on(&evaluation, &target_unit("browser", Check::WithinBound)),
                None
            );
            let targets = targets.unwrap();
            assert_eq!(facts(&targets, "browser").admitted, Ok(()));
            assert!(facts(&targets, "server").resolved.is_ok());
        });
    }

    #[test]
    fn an_inherited_default_waits_on_the_project_default_it_inherits() {
        let mut inputs = Inputs::new();
        inputs.app["defaultRequestedLocale"] = json!("en_US");
        inputs.resolve(|evaluation, targets| {
            let project_default = locales::unit(
                LocaleSubject::DefaultRequestedLocale(content("en_US")),
                Check::CanonicalLocale,
            );
            assert_eq!(
                waits_on(&evaluation, &target_unit("server", Check::SupportedDefault)),
                Some(vec![project_default])
            );
            // The browser has its own default.
            assert_eq!(
                waits_on(
                    &evaluation,
                    &target_unit("browser", Check::SupportedDefault)
                ),
                None
            );
            assert!(facts(&targets.unwrap(), "browser").resolved.is_ok());
        });
    }

    #[test]
    fn a_repeated_target_locale_is_still_checked_but_leaves_the_target_unresolved() {
        let mut inputs = Inputs::new();
        inputs.target("server")["requestedLocales"] = json!(["en", "EN"]);
        inputs.resolve(|evaluation, targets| {
            let repeat = locales::unit(
                LocaleSubject::TargetRequestedLocale(target("server"), key("en")),
                Check::UniqueLocale,
            );
            assert_eq!(
                issues(&evaluation),
                [(repeat.clone(), Code::LocaleDuplicate, None)]
            );
            // Its identities form a subset with the default in it.
            assert!(statuses(&evaluation)
                .iter()
                .all(|(_, status)| *status == Status::Evaluated));
            let targets = targets.unwrap();
            let causes = facts(&targets, "server").resolved.clone().unwrap_err();
            assert_eq!(
                causes
                    .keys()
                    .map(|key| key.unit().clone())
                    .collect::<Vec<_>>(),
                [repeat]
            );
        });
    }

    #[test]
    fn without_a_confirmed_policy_no_target_is_checked() {
        let mut inputs = Inputs::new();
        let causes = stand_in(Slot::Coverage);
        inputs.policy = Err(causes.clone());
        inputs.resolve(|evaluation, targets| {
            assert_eq!(
                statuses(&evaluation),
                every_unit()
                    .into_iter()
                    .map(|unit| (unit, Status::NotEvaluated(causes.clone())))
                    .collect::<Vec<_>>()
            );
            let targets = targets.unwrap();
            assert_eq!(targets.complete(), Err(causes));
        });
    }

    #[test]
    fn a_map_structural_admission_did_not_admit_records_only_its_bound() {
        let mut inputs = Inputs::new();
        let causes = stand_in(Slot::DeliveryGraph);
        inputs.declared = Err(causes.clone());
        inputs.resolve(|evaluation, targets| {
            assert_eq!(
                statuses(&evaluation),
                [(map_unit(), Status::NotEvaluated(causes.clone()))]
            );
            assert_eq!(targets, Err(causes));
        });
    }

    #[test]
    fn reordering_targets_and_their_locales_changes_nothing_phase_11_records() {
        let complete = Inputs::new();
        let mut blocked = Inputs::new();
        blocked.target("browser")["requestedLocales"] = json!(["ja", "de", "en"]);
        blocked.target("browser")["defaultRequestedLocale"] = json!("fr");
        blocked.direct("server", json!([{"locale": "ja", "direction": "LTR"}]));
        for authored in [complete, blocked] {
            let mut reordered = Inputs::new();
            reordered.limits = authored.limits.clone();
            let mut targets: Vec<(String, Value)> = authored.app["targetProfiles"]
                .as_object()
                .unwrap()
                .clone()
                .into_iter()
                .collect();
            targets.reverse();
            let mut map = Map::new();
            for (id, mut declared) in targets {
                declared["requestedLocales"]
                    .as_array_mut()
                    .unwrap()
                    .reverse();
                map.insert(id, declared);
            }
            reordered.app = authored.app.clone();
            reordered.app["targetProfiles"] = Value::Object(map);
            reordered.members = authored.members.iter().rev().cloned().collect();
            authored.resolve(|first_evaluation, first| {
                reordered.resolve(|second_evaluation, second| {
                    assert_eq!(statuses(&first_evaluation), statuses(&second_evaluation));
                    assert_eq!(issues(&first_evaluation), issues(&second_evaluation));
                    assert_eq!(first, second);
                });
            });
        }
    }
}
