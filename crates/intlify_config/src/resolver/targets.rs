// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 11 (`target-resolution`) over the selected declaration's
//! targets: each target's requested locales as a subset of the project's, its
//! effective default, which is its own override or else the project's default
//! and must be one of its requested locales, and the directions its Target
//! Profile has for its requested locales. Every check of a target waits on
//! the Target ID map's `maxTargetProfiles` and the key's `maxTargetIdBytes`,
//! which 015 has Phase 7 check. No default is inferred: a target without an
//! override takes the project's default, never the first, only, or smallest of
//! its own locales. No direction is inferred either: only an entry spelled as
//! the canonical locale covers it.
//!
//! Not the canonicalization of a target's locales or the repeats among them,
//! which Phase 7 did; not the exact resolution of its Target Profile reference,
//! which Phase 9 did and this phase only joins; and not the groups (Phase 12)
//! or the hydration relations (Phase 13). A subject is a Target ID, a semantic
//! identity the declaration's map admitted.
//!
//! A target whose locale set is not a subset of the project's is observed at
//! its locale set beside the project's, and one whose effective default is
//! outside its set at that default where it is written, its own override or
//! the project's default, beside its set (D15:2417). A requested locale its
//! Target Profile has no direction for is observed at each item that names
//! it, a repeat included, beside the Target Profile, a member of the Set
//! (D15:835, 1406, 2388).

use std::collections::BTreeMap;

use crate::artifacts::content::Content;
use crate::locale::CanonicalLocale;
use crate::model::TargetId;

use super::artifact_resolution::{self, AdmittedArtifact};
use super::declaration_observer::{DeclarationObserver, ObserverInvariant};
use super::evaluation::{
    and, both, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant, Found,
    Observation, Phase, Reason, Subject, Unit,
};
use super::evidence::EvidenceProjection;
use super::evidence_component::ComponentInvariant;
use super::exact_resolution::{Named, ReferenceResults};
use super::locales::{LocaleKey, LocaleSet, Location, ResolvedLocales, TargetLocales};
use super::resource_policy::ReferenceField;

/// A resolver bug Phase 11 refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TargetInvariant {
    Evaluation(EvaluationInvariant),
    /// The Target Profile a check compared has no component path.
    Component(ComponentInvariant),
    /// What a check looked at in the declaration has no Evidence.
    Observer(ObserverInvariant),
}

impl From<ComponentInvariant> for TargetInvariant {
    fn from(invariant: ComponentInvariant) -> Self {
        Self::Component(invariant)
    }
}

impl From<EvaluationInvariant> for TargetInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

impl From<ObserverInvariant> for TargetInvariant {
    fn from(invariant: ObserverInvariant) -> Self {
        Self::Observer(invariant)
    }
}

/// What one Phase 11 unit checks.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TargetSubject {
    /// One Target ID: its requested locales, its effective default, and its
    /// Target Profile's directions.
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
    /// The Target ID within the map's bound and its own, as Phase 7 checked
    /// them: what a later check that names the target needs, whatever its
    /// locales and Target Profile.
    pub(crate) admitted: Admitted<()>,
    /// Its requested locales, once found a subset of the project's.
    pub(crate) requested: Admitted<LocaleSet>,
    /// Its effective default, once found one of its requested locales.
    pub(crate) effective_default: Admitted<CanonicalLocale>,
    /// Where its effective default comes from, once there is one: its own
    /// override or the project's default, where a later check that compares
    /// the default observes it.
    pub(crate) default_origin: Option<Reason>,
    /// The Set's Target Profile its reference names.
    pub(crate) profile: Admitted<Named<'s>>,
    /// The target, once every fact above and its repeats check held.
    pub(crate) resolved: Admitted<ResolvedTarget<'s>>,
}

/// Every target with an admitted Target ID's facts, in Target ID order; the
/// map's bound as Phase 7 checked it, which a later check over the set of
/// Target IDs needs; whether every key of the map was admitted; and whether
/// everything at and below the map passed, which what needs every target
/// waits on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedTargets<'s> {
    bounded: Admitted<()>,
    keys: Admitted<()>,
    whole: Admitted<()>,
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

    /// Whether every key of the map was admitted, or why not: the set of
    /// Target IDs is complete only then.
    pub(crate) const fn keys(&self) -> &Admitted<()> {
        &self.keys
    }

    /// Whether `id` names a declared Target ID, once the map is within its
    /// bound: an admitted key proves it, and naming none is proved only beside
    /// every key, a value the set lacks being unproved while a key failed
    /// (D15:2366, 2368).
    pub(crate) fn declares(&self, id: &TargetId) -> Admitted<bool> {
        self.bounded.clone()?;
        if self.facts.contains_key(id) {
            Ok(true)
        } else {
            self.keys.clone().map(|()| false)
        }
    }

    /// Whether the whole map was admitted and every target resolved, or the
    /// causes of what was not and of each that did not.
    pub(crate) fn complete(&self) -> Admitted<()> {
        let mut holds = self.whole.clone();
        for facts in self.facts.values() {
            let resolved = facts.resolved.as_ref().map(|_| ()).map_err(Causes::clone);
            holds = and(holds, resolved);
        }
        holds
    }
}

/// Phase 11 over the targets Phase 7 read, each with its locales and bounds,
/// and the references Phase 9 resolved, observed through `observer`, the
/// selected declaration's, when there is one. Each check of a target waits on
/// its Target ID's bounds. The subset, the effective default, and the
/// directions are checked independently of each other, over the values of its
/// set that were admitted (D15:2185, 3490); only the directions wait on the
/// Target Profile, so an invalid one stops what needs it alone (D15:2368), and
/// the resolved target needs every one. Without a map to read there is no
/// target, and no unit.
///
/// # Errors
///
/// Returns the resolver bug of a unit recorded against its phase's rules, or
/// of a declared value without Evidence.
pub(crate) fn resolve_targets<'s>(
    evaluation: &mut Evaluation,
    locales: &ResolvedLocales,
    (references, observer): (&ReferenceResults<'s>, Option<&DeclarationObserver<'_>>),
) -> Result<Admitted<ResolvedTargets<'s>>, TargetInvariant> {
    let targets = match &locales.targets {
        Ok(targets) => targets,
        Err(causes) => return Ok(Err(causes.clone())),
    };
    let mut facts = BTreeMap::new();
    for own in targets {
        let id = &own.id;
        facts.insert(
            id.clone(),
            Checks { id, observer }.resolve(evaluation, own, locales, references)?,
        );
    }
    Ok(Ok(ResolvedTargets {
        bounded: locales.target_bound.clone(),
        keys: locales.target_keys.clone(),
        whole: locales.target_whole.clone(),
        facts,
    }))
}

/// One target's checks, and where they observe the declaration.
struct Checks<'a, 'o> {
    id: &'a TargetId,
    observer: Option<&'a DeclarationObserver<'o>>,
}

impl Checks<'_, '_> {
    fn unit(&self, check: Check) -> Unit {
        unit(TargetSubject::Target(self.id.clone()), check)
    }

    /// Each item of the target's requested locales Phase 7 canonicalized, by
    /// its position and its canonical locale, in the declaration's order, a
    /// repeat included.
    fn items<'l>(&self, locales: &'l ResolvedLocales) -> Vec<(usize, &'l CanonicalLocale)> {
        locales
            .occurrences
            .iter()
            .filter_map(|occurrence| match (&occurrence.location, &occurrence.key) {
                (Location::TargetRequestedLocale(id, index), LocaleKey::Canonical(locale))
                    if id == self.id =>
                {
                    Some((*index, locale))
                }
                _ => None,
            })
            .collect()
    }

    /// The path of the target's `field` in the declaration.
    fn path<'f>(&'f self, field: &'f str) -> [&'f str; 3] {
        ["targetProfiles", self.id.as_str(), field]
    }

    /// The subset, the effective default, and the directions, then the target
    /// when everything it needs was admitted. Each waits on the Target ID's
    /// bounds.
    fn resolve<'s>(
        &self,
        evaluation: &mut Evaluation,
        own: &TargetLocales,
        locales: &ResolvedLocales,
        references: &ReferenceResults<'s>,
    ) -> Result<TargetFacts<'s>, TargetInvariant> {
        let admitted = own.admitted.clone();
        // The identities of the target's set that were admitted: a check
        // over them waits only when every element is excluded (D15:2185,
        // 3490).
        let counted = own.requested.counted().cloned();
        // A target locale the project's set holds is in it whatever else
        // the set lacks, and one it does not hold is unproved while it lacks
        // any (D15:2366, 2368).
        let covered = match &counted {
            Ok(own) => locales.requested_set.covers(own.as_slice()),
            Err(_) => locales.requested_set.complete().map(|_| true),
        };
        let subset_input = both(both(admitted.clone(), counted.clone()), covered);
        // What each check observes, made before its unit records it, and only
        // once it finds what it observes, which the declaration then holds.
        let beside_project = match (self.observer, &subset_input) {
            (Some(observer), Ok((_, false))) => Some(Observation {
                primary: observer.value(&self.path("requestedLocales"))?,
                related: vec![observer.value(&["requestedLocales"])?],
            }),
            _ => None,
        };
        let subset = evaluation.check_found(
            self.unit(Check::ProjectSubset),
            subset_input,
            |(_, covered)| {
                if covered {
                    Ok(())
                } else {
                    let outside = issue(Code::TargetLocalesNotSubset, None);
                    Err(Found::through(outside, beside_project))
                }
            },
        )?;
        let origin = effective_default(own, locales);
        let default_origin = origin.as_ref().ok().map(|&(_, reason)| reason);
        // The default is among the admitted values, or else unproved beside
        // an element without an identity (D15:2366, 2368).
        let held = own
            .requested
            .holds(&origin.clone().map(|(locale, _)| locale));
        let default_input = both(both(admitted.clone(), held), origin);
        let beside_set = match (self.observer, &default_input) {
            (Some(observer), Ok((((), false), (_, reason)))) => {
                let written = if *reason == Reason::ExplicitOverride {
                    observer.value(&self.path("defaultRequestedLocale"))?
                } else {
                    observer.value(&["defaultRequestedLocale"])?
                };
                Some(Observation {
                    primary: written,
                    related: vec![observer.value(&self.path("requestedLocales"))?],
                })
            }
            _ => None,
        };
        let checked = evaluation.check_found(
            self.unit(Check::SupportedDefault),
            default_input,
            |(((), held), (locale, reason))| {
                if held {
                    Ok(locale)
                } else {
                    let unsupported = issue(Code::TargetDefaultLocaleNotSupported, Some(reason));
                    Err(Found::through(unsupported, beside_set))
                }
            },
        )?;
        // The check read the values admitted, while what a later phase reads
        // also waits on those excluded: an invalid target emits no effective
        // default (D15:2185, 2417).
        let default =
            both(checked, own.requested.complete().map(|_| ())).map(|(locale, ())| locale);
        let profile = profile(references, self.id);
        let directed_input = both(both(admitted.clone(), counted), profile.clone());
        let sight = match (self.observer, &directed_input) {
            (Some(observer), Ok((_, (tuple, _)))) => Some(Directions {
                items: self
                    .items(locales)
                    .into_iter()
                    .map(|(index, locale)| {
                        Ok((
                            locale.clone(),
                            observer.element(&self.path("requestedLocales"), index)?,
                        ))
                    })
                    .collect::<Result<_, ObserverInvariant>>()?,
                profile: artifact_resolution::admitted(tuple)?,
            }),
            _ => None,
        };
        let directed = evaluation.check_found(
            self.unit(Check::DirectedLocales),
            directed_input,
            |(((), requested), (_, artifact))| {
                let missing = undirected(artifact, &requested);
                if missing.is_empty() {
                    Ok(())
                } else {
                    let blocking = issue(Code::TargetLocaleDirectionMissing, None);
                    let observed = sight.map_or_else(Vec::new, |sight| sight.at(&missing));
                    Err(Found::each(blocking, observed))
                }
            },
        )?;
        let requested =
            both(own.requested.complete().cloned(), subset).map(|(requested, ())| requested);
        let resolved = both(
            both(requested.clone(), own.distinct.clone()),
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
            default_origin,
            profile,
            resolved,
        })
    }
}

/// The target's effective default and where it came from: its own override,
/// or else the project's default.
fn effective_default(
    own: &TargetLocales,
    locales: &ResolvedLocales,
) -> Admitted<(CanonicalLocale, Reason)> {
    match &own.default {
        Ok(Some(locale)) => Ok((locale.clone(), Reason::ExplicitOverride)),
        Ok(None) => locales
            .requested_default
            .clone()
            .map(|locale| (locale, Reason::InheritedProjectDefault)),
        Err(causes) => Err(causes.clone()),
    }
}

/// What the directions check observes, made before its unit records it:
/// each item of the target's locales, by its canonical locale, at its token,
/// and the Target Profile the check compared them with.
struct Directions {
    items: Vec<(CanonicalLocale, EvidenceProjection)>,
    profile: EvidenceProjection,
}

impl Directions {
    /// One observation for each item that names one of `missing`, a repeat
    /// included, beside the Target Profile (D15:2388).
    fn at(&self, missing: &[&CanonicalLocale]) -> Vec<Option<Observation>> {
        self.items
            .iter()
            .filter(|(locale, _)| missing.contains(&locale))
            .map(|(_, at)| {
                Some(Observation {
                    primary: at.clone(),
                    related: vec![self.profile.clone()],
                })
            })
            .collect()
    }
}

/// The requested locales the Target Profile has no direction for: those
/// without an entry whose locale is the canonical spelling, byte for byte. An
/// entry for another locale is unused, and an artifact that is no Target
/// Profile has none.
fn undirected<'l>(
    artifact: &AdmittedArtifact<'_>,
    requested: &'l LocaleSet,
) -> Vec<&'l CanonicalLocale> {
    let Content::TargetProfile(profile) = &artifact.content else {
        return requested.as_slice().iter().collect();
    };
    requested
        .as_slice()
        .iter()
        .filter(|locale| {
            !profile
                .directions
                .iter()
                .any(|direction| direction.locale == locale.as_str())
        })
        .collect()
}

/// The Set's Target Profile the target's reference names, or why it names
/// none.
fn profile<'s>(references: &ReferenceResults<'s>, id: &TargetId) -> Admitted<Named<'s>> {
    references
        .get(&ReferenceField::TargetProfile(id.clone()))
        .expect("Phase 9 records every admitted target's reference")
        .clone()
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
    use crate::artifacts::collection::{read_collection, CollectionLimits, CollectionReport};
    use crate::artifacts::content::sample;
    use crate::artifacts::envelope::{envelope_of, ArtifactKind, ExactTuple, FormalMember};
    use crate::input_limits::Bound;
    use crate::model::ProfileDeclaration;
    use crate::profile_fixtures::minimal_config;
    use crate::references::{PolicyReference, TargetProfileReference};
    use crate::resolver::artifact_resolution::admit_collection;
    use crate::resolver::declaration_observer::fixtures::{view, Observed};
    use crate::resolver::evaluation::{Actual, IssueKey, LimitExceeded, Status};
    use crate::resolver::exact_resolution::{self, resolve_fields};
    use crate::resolver::limits::{
        fixture_limits, BoundId, ConfirmedPolicy, Group, ResolutionLimits,
    };
    use crate::resolver::locales::fixtures::{
        canonical, canonicalizer, content, key, rejected, stand_in, target, waits_on,
    };
    use crate::resolver::locales::{self, resolve_locales, LocaleFields, LocaleSubject};
    use crate::resolver::resource_policy::{
        DeclarationReferences, PolicyReferences, TargetReferences,
    };
    use crate::resolver::specification::Slot;

    type Declaration = ProfileDeclaration<PolicyReference, TargetProfileReference>;

    /// Collection limits no test reaches.
    const ROOM: CollectionLimits = CollectionLimits {
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
        /// Whether structural admission admitted everything under the map.
        structure: Admitted<()>,
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
                structure: Ok(()),
            }
        }

        fn target(&mut self, id: &str) -> &mut Value {
            &mut self.app["targetProfiles"][id]
        }

        /// Submits, in place of the Target Profile the target `id` names, one
        /// with `directions`, and names it.
        /// Gives the target `id` a Target Profile with these `directions`,
        /// and returns it.
        fn direct(&mut self, id: &str, directions: Value) -> Value {
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
            member
        }

        /// Phase 7, Phase 9's exact resolution of the targets' references,
        /// and Phase 11, into one evaluation; `inspect` reads it and what
        /// Phase 11 returned. Phase 7 checks the target bounds, under the
        /// policy if one was confirmed, and sees the map only when structural
        /// admission admitted it.
        fn resolve<R>(
            &self,
            inspect: impl FnOnce(Evaluation, Admitted<ResolvedTargets<'_>>) -> R,
        ) -> R {
            let declaration: Declaration = serde_json::from_value(self.app.clone()).unwrap();
            let confirmed = ConfirmedPolicy::fixture_owned(self.limits.clone());
            let unconfirmed = self.policy.clone().err();
            let policy = unconfirmed.as_ref().map_or(Ok(&confirmed), Err);
            let declared = self.declared.clone().map(|()| &declaration.target_profiles);
            let mut fields = LocaleFields::declared(&declaration);
            fields.target_entries = declared.clone().map(|targets| {
                let mut entries = locales::TargetMapFields::declared(targets);
                entries.whole = self.structure.clone();
                entries
            });
            let mut evaluation = Evaluation::default();
            let observed = Observed::declaration(&self.app);
            let locales = observed
                .app(|observer| {
                    resolve_locales(
                        &mut evaluation,
                        policy,
                        Ok(&canonicalizer(64)),
                        &fields,
                        Some(observer),
                    )
                })
                .unwrap();
            let texts: Vec<String> = self.members.iter().map(Value::to_string).collect();
            let bytes: Vec<&[u8]> = texts.iter().map(String::as_bytes).collect();
            let collection =
                read_collection(Some(CollectionReport::Formal(&bytes)), &ROOM).unwrap();
            let set = admit_collection(&mut evaluation, Ok(&confirmed), &collection).unwrap();
            // The policies' references are no concern of Phase 11's.
            let references = observed
                .app(|observer| {
                    resolve_fields(
                        &mut evaluation,
                        set.as_ref(),
                        &DeclarationReferences {
                            policies: PolicyReferences {
                                slots: Vec::new(),
                                object: Ok(()),
                            },
                            target_profiles: Ok(TargetReferences::declared(
                                &declaration.target_profiles,
                            )),
                        },
                        (locales.targets.as_deref(), Some(observer)),
                    )
                })
                .unwrap();
            let targets = observed
                .app(|observer| {
                    resolve_targets(&mut evaluation, &locales, (&references, Some(observer)))
                })
                .unwrap();
            inspect(evaluation, targets)
        }
    }

    impl Inputs {
        /// The view of the Evidence of the value at `path` in the declaration.
        fn at(&self, path: &[&str]) -> Value {
            Observed::declaration(&self.app).app(|observer| view(&observer.value(path).unwrap()))
        }

        /// The view of the Evidence of the element at `index` of the array at
        /// `path` in the declaration.
        fn element(&self, path: &[&str], index: usize) -> Value {
            Observed::declaration(&self.app)
                .app(|observer| view(&observer.element(path, index).unwrap()))
        }
    }

    /// The view of the Set's artifact `member`, under its exact-reference
    /// identity.
    fn artifact(member: &Value) -> Value {
        view(&artifact_resolution::admitted(&tuple_of(member)).unwrap())
    }

    /// Each Phase 11 Finding candidate: its unit, and the views of its
    /// primary and related Evidence.
    fn observed(evaluation: &Evaluation) -> Vec<(Unit, Value, Vec<Value>)> {
        evaluation
            .occurrence_candidates()
            .iter()
            .filter(|candidate| candidate.finding.unit().phase == Phase::TargetResolution)
            .map(|candidate| {
                let related = candidate.related.iter().map(view).collect();
                (
                    candidate.finding.unit().clone(),
                    view(&candidate.primary),
                    related,
                )
            })
            .collect()
    }

    fn bound(value: u64) -> Bound {
        Bound::new(value).unwrap()
    }

    fn target_unit(id: &str, check: Check) -> Unit {
        unit(TargetSubject::Target(target(id)), check)
    }

    /// Phase 7's unit over the Target ID map.
    fn map_unit() -> Unit {
        locales::unit(LocaleSubject::TargetProfiles, Check::WithinBound)
    }

    /// Phase 7's unit over one Target ID key.
    fn id_unit(id: &str) -> Unit {
        locales::unit(LocaleSubject::TargetId(target(id)), Check::WithinBound)
    }

    /// Phase 9's unit over the target's Target Profile reference.
    fn reference_unit(id: &str) -> Unit {
        exact_resolution::unit(ReferenceField::TargetProfile(target(id)))
    }

    const fn grouping_bound(name: &'static str) -> BoundId {
        BoundId {
            group: Group::TargetGrouping,
            name,
        }
    }

    /// Every unit Phase 11 records for the two targets, in 015's order.
    fn every_unit() -> Vec<Unit> {
        let mut units = Vec::new();
        for id in ["browser", "server"] {
            for check in [
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
    fn each_target_is_checked_after_its_bounds_in_phase_7_and_every_phase_10_unit() {
        let placement = Unit {
            phase: Phase::DeliveryResolution,
            subject: Subject::DeliveryPlacement,
            check: Check::EffectiveMode,
        };
        let ordered = [
            map_unit(),
            id_unit("browser"),
            id_unit("server"),
            placement,
            target_unit("browser", Check::ProjectSubset),
            target_unit("browser", Check::SupportedDefault),
            target_unit("browser", Check::DirectedLocales),
            target_unit("server", Check::ProjectSubset),
        ];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
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
        let profile = inputs.direct(
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
            // Observed at the item that names `ja`, beside the browser's
            // Target Profile (D15:1406).
            assert_eq!(
                observed(&evaluation),
                [(
                    target_unit("browser", Check::DirectedLocales),
                    inputs.element(&["targetProfiles", "browser", "requestedLocales"], 0),
                    vec![artifact(&profile)]
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

    /// Each item that names a locale without a direction is observed, a
    /// repeat included, beside the Target Profile (D15:1406, 2388).
    #[test]
    fn each_item_naming_an_undirected_locale_is_observed() {
        let mut inputs = Inputs::new();
        // The server names `en` twice, and its Target Profile directs only
        // `ja`, which the server does not request.
        inputs.target("server")["requestedLocales"] = json!(["en", "EN"]);
        let profile = inputs.direct("server", json!([{"locale": "ja", "direction": "LTR"}]));
        inputs.resolve(|evaluation, _| {
            let set = ["targetProfiles", "server", "requestedLocales"];
            let each = |index| {
                (
                    target_unit("server", Check::DirectedLocales),
                    inputs.element(&set, index),
                    vec![artifact(&profile)],
                )
            };
            assert_eq!(observed(&evaluation), [each(0), each(1)]);
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
        let (en, fr) = (canonical("en"), canonical("fr"));
        assert!(undirected(&profile, &set(&["en"])).is_empty());
        assert!(undirected(&profile, &set(&["de", "en", "fr-FR", "ja"])).is_empty());
        // Only the locale without its entry is missing.
        assert_eq!(undirected(&profile, &set(&["en", "fr"])), [&fr]);
        assert_eq!(undirected(&trust, &set(&["en"])), [&en]);
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
            // Observed at the browser's locale set, beside the project's
            // (D15:2417).
            assert_eq!(
                observed(&evaluation),
                [(
                    subset.clone(),
                    inputs.at(&["targetProfiles", "browser", "requestedLocales"]),
                    vec![inputs.at(&["requestedLocales"])]
                )]
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
            // Each at the default where it is written, the browser's own
            // override or the project's default the server inherits, beside
            // the target's locale set (D15:2417).
            let set = |id| inputs.at(&["targetProfiles", id, "requestedLocales"]);
            assert_eq!(
                observed(&evaluation),
                [
                    (
                        target_unit("browser", Check::SupportedDefault),
                        inputs.at(&["targetProfiles", "browser", "defaultRequestedLocale"]),
                        vec![set("browser")]
                    ),
                    (
                        target_unit("server", Check::SupportedDefault),
                        inputs.at(&["defaultRequestedLocale"]),
                        vec![set("server")]
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
                // Phase 7 finds it, and every check of a target waits on it,
                // Phase 9's resolution of its Target Profile reference too.
                let key = evaluation.issues().next().unwrap().clone();
                assert_eq!(
                    issues(&evaluation),
                    [(map_unit(), Code::ResourceLimitExceeded, None)]
                );
                assert_eq!(
                    evaluation.limit(&key),
                    Some(LimitExceeded {
                        bound: grouping_bound("maxTargetProfiles"),
                        limit: bound(1),
                        actual: Actual::Exact(2),
                    })
                );
                for unit in every_unit().iter().chain(&[
                    id_unit("browser"),
                    id_unit("server"),
                    reference_unit("browser"),
                    reference_unit("server"),
                ]) {
                    assert_eq!(
                        waits_on(&evaluation, unit),
                        Some(vec![map_unit()]),
                        "{unit:?}"
                    );
                }
                let targets = targets.unwrap();
                for id in ["browser", "server"] {
                    let facts = facts(&targets, id);
                    let profile = facts.profile.clone().map(|_| ());
                    for fact in [&facts.admitted, &profile] {
                        assert_eq!(
                            keys(fact.as_ref().unwrap_err()),
                            BTreeSet::from([key.clone()])
                        );
                    }
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
                let id = id_unit("browser");
                assert_eq!(
                    issues(&evaluation),
                    [(id.clone(), Code::ResourceLimitExceeded, None)]
                );
                let key = evaluation.issues().next().unwrap().clone();
                assert_eq!(
                    evaluation.limit(&key),
                    Some(LimitExceeded {
                        bound: grouping_bound("maxTargetIdBytes"),
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
                // Phase 9 resolves only the server's Target Profile reference.
                assert_eq!(
                    waits_on(&evaluation, &reference_unit("browser")),
                    Some(vec![id.clone()])
                );
                assert_eq!(waits_on(&evaluation, &reference_unit("server")), None);
                // Phase 7 canonicalizes none of the browser's spellings, which
                // have no identity, so share one unit, and wait on its ID, and
                // the server's as ever.
                for subject in [
                    LocaleSubject::TargetRequestedLocale(target("browser"), content("ja")),
                    LocaleSubject::TargetRequestedLocale(target("browser"), content("en")),
                    LocaleSubject::TargetDefaultRequestedLocale(target("browser")),
                ] {
                    assert_eq!(
                        waits_on(
                            &evaluation,
                            &locales::unit(subject.clone(), Check::CanonicalLocale)
                        ),
                        Some(vec![id.clone()]),
                        "{subject:?}"
                    );
                }
                let server = LocaleSubject::TargetRequestedLocale(
                    target("server"),
                    crate::resolver::locales::fixtures::key("en"),
                );
                assert_eq!(
                    waits_on(&evaluation, &locales::unit(server, Check::CanonicalLocale)),
                    None
                );
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
            // A spelling without an identity has the unit such spellings
            // share under its target.
            let member = locales::unit(
                LocaleSubject::TargetRequestedLocale(target("browser"), rejected()),
                Check::CanonicalLocale,
            );
            let default = locales::unit(
                LocaleSubject::TargetDefaultRequestedLocale(target("browser")),
                Check::CanonicalLocale,
            );
            // `ja` keeps its identity, so the checks over the set's values
            // are evaluated; the default names none and waits.
            for check in [Check::ProjectSubset, Check::DirectedLocales] {
                assert_eq!(waits_on(&evaluation, &target_unit("browser", check)), None);
            }
            assert_eq!(
                waits_on(
                    &evaluation,
                    &target_unit("browser", Check::SupportedDefault)
                ),
                Some(vec![member.clone(), default])
            );
            // The ID was checked in Phase 7, and the server is untouched.
            assert_eq!(waits_on(&evaluation, &id_unit("browser")), None);
            let targets = targets.unwrap();
            assert_eq!(facts(&targets, "browser").admitted, Ok(()));
            // The set still lacks an identity, so the target does not resolve.
            assert_eq!(
                facts(&targets, "browser")
                    .requested
                    .as_ref()
                    .map_err(|causes| causes
                        .keys()
                        .map(|key| key.unit().clone())
                        .collect::<Vec<_>>())
                    .unwrap_err(),
                vec![member]
            );
            assert!(facts(&targets, "browser").resolved.is_err());
            assert!(facts(&targets, "server").resolved.is_ok());
        });
    }

    /// A default the admitted values hold passes its check, and a later phase
    /// still reads no effective default from a target whose set lacks an
    /// identity (D15:2185, 2366, 2417).
    #[test]
    fn a_default_held_beside_a_failed_element_hands_on_no_effective_default() {
        let mut inputs = Inputs::new();
        inputs.target("browser")["requestedLocales"] = json!(["ja", "en-x-brand"]);
        inputs.resolve(|evaluation, targets| {
            let check = target_unit("browser", Check::SupportedDefault);
            assert_eq!(waits_on(&evaluation, &check), None);
            let member = locales::unit(
                LocaleSubject::TargetRequestedLocale(target("browser"), rejected()),
                Check::CanonicalLocale,
            );
            let targets = targets.unwrap();
            let browser = facts(&targets, "browser");
            let causes = browser.effective_default.as_ref().unwrap_err();
            assert_eq!(
                causes
                    .keys()
                    .map(|key| key.unit().clone())
                    .collect::<Vec<_>>(),
                [member]
            );
            assert_eq!(browser.default_origin, Some(Reason::ExplicitOverride));
            assert!(facts(&targets, "server").effective_default.is_ok());
        });
    }

    /// With no element of its set admitted, a target's checks over its
    /// values have none to read and wait on why (D15:2185, 3490).
    #[test]
    fn a_target_without_an_admitted_locale_waits_on_its_elements() {
        let mut inputs = Inputs::new();
        inputs.target("browser")["requestedLocales"] = json!(["en-x-brand"]);
        inputs.resolve(|evaluation, _| {
            let member = locales::unit(
                LocaleSubject::TargetRequestedLocale(target("browser"), rejected()),
                Check::CanonicalLocale,
            );
            for check in [
                Check::ProjectSubset,
                Check::DirectedLocales,
                Check::SupportedDefault,
            ] {
                assert_eq!(
                    waits_on(&evaluation, &target_unit("browser", check)),
                    Some(vec![member.clone()]),
                    "{check:?}"
                );
            }
        });
    }

    #[test]
    fn an_inherited_default_waits_on_the_project_default_it_inherits() {
        let mut inputs = Inputs::new();
        inputs.app["defaultRequestedLocale"] = json!("en_US");
        inputs.resolve(|evaluation, targets| {
            let project_default = locales::unit(
                LocaleSubject::DefaultRequestedLocale,
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
    fn a_map_structural_admission_did_not_admit_has_no_target_unit() {
        let mut inputs = Inputs::new();
        let causes = stand_in(Slot::DeliveryGraph);
        inputs.declared = Err(causes.clone());
        inputs.resolve(|evaluation, targets| {
            assert_eq!(statuses(&evaluation), []);
            // The map's bound is named by its path and waits on why.
            assert_eq!(
                evaluation.status(&map_unit()),
                Some(&Status::NotEvaluated(causes.clone()))
            );
            // Phase 7 bounds no target, so a Target Profile reference Phase 9
            // still sees waits on why.
            for id in ["browser", "server"] {
                assert_eq!(
                    waits_on(&evaluation, &reference_unit(id)),
                    Some(causes.keys().map(|key| key.unit().clone()).collect()),
                    "{id}"
                );
            }
            assert_eq!(targets, Err(causes));
        });
    }

    /// Beside a failure under the map, each target keeps its facts, while
    /// what needs every target waits on it, and every target's own causes
    /// join it (D15:2368).
    #[test]
    fn a_map_not_admitted_whole_keeps_each_targets_facts() {
        let mut inputs = Inputs::new();
        let causes = stand_in(Slot::DeliveryGraph);
        inputs.structure = Err(causes.clone());
        inputs.target("browser")["requestedLocales"] = json!(["ja", "de"]);
        inputs.resolve(|evaluation, targets| {
            let targets = targets.unwrap();
            assert!(facts(&targets, "server").resolved.is_ok());
            assert!(facts(&targets, "browser").requested.is_err());
            let found: BTreeSet<_> = evaluation.issues().cloned().collect();
            assert!(!found.is_empty());
            let every = found.into_iter().chain(causes.keys().cloned());
            assert_eq!(
                keys(&targets.complete().unwrap_err()),
                every.collect::<BTreeSet<_>>()
            );
        });
        Inputs::new().resolve(|_, targets| assert_eq!(targets.unwrap().complete(), Ok(())));
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
