// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 8 for the selected declaration's negotiation input: the alias
//! map under `maxNegotiationAliases`, a key two entries name, and each
//! destination's membership in the project's requested locales, all under the
//! admitted Locale Negotiation Specification. Not portable lookup itself, which
//! runs per invocation over one target's supported subset and the application's
//! preferences, and not the canonicalization of keys and destinations, which
//! Phase 7 did. A subject names a key or destination as Phase 7 did: by its
//! canonical identity, or by its spelling when it has none.

use std::collections::BTreeMap;

use crate::locale::{CanonicalLocale, VersionedIdentity};

use super::evaluation::{
    both, within, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant, Phase,
    Reason, Subject, Unit,
};
use super::limits::{BoundId, ConfirmedPolicy, Group};
use super::locales::{AliasLocales, LocaleKey, ResolvedLocales};

const MAX_NEGOTIATION_ALIASES: BoundId = BoundId {
    group: Group::LocaleResolution,
    name: "maxNegotiationAliases",
};

/// What one negotiation unit checks. Kinds are declared so that the bound,
/// which every other unit waits on, comes first.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NegotiationSubject {
    /// The submitted alias map, under `maxNegotiationAliases`.
    Aliases,
    /// An alias key.
    Key(LocaleKey),
    /// An alias destination, under its key's subject key.
    Destination(LocaleKey, LocaleKey),
}

/// The unit Phase 8 records for `check` over `subject`.
pub(crate) fn unit(subject: NegotiationSubject, check: Check) -> Unit {
    Unit {
        phase: Phase::LocalePolicyResolution,
        subject: Subject::Negotiation(subject),
        check,
    }
}

/// The checked Locale Negotiation Profile: the admitted specification and the
/// alias map, its canonical keys in ascending unsigned UTF-8 byte order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NegotiationProfile<Id> {
    specification: VersionedIdentity<Id>,
    aliases: BTreeMap<CanonicalLocale, CanonicalLocale>,
}

impl<Id> NegotiationProfile<Id> {
    pub(crate) const fn specification(&self) -> &VersionedIdentity<Id> {
        &self.specification
    }

    pub(crate) const fn aliases(&self) -> &BTreeMap<CanonicalLocale, CanonicalLocale> {
        &self.aliases
    }
}

/// Phase 8's negotiation checks over what Phase 7 resolved. Every unit waits
/// on the alias bound, which counts each submitted entry before any collision
/// is looked for; the profile exists only when every unit passed.
pub(crate) fn resolve_negotiation<Id: Clone>(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    specification: &Admitted<VersionedIdentity<Id>>,
    locales: &ResolvedLocales,
) -> Result<Admitted<NegotiationProfile<Id>>, EvaluationInvariant> {
    let aliases = locales.negotiation_aliases.as_ref().map_err(Causes::clone);
    let bounded = evaluation.check(
        unit(NegotiationSubject::Aliases, Check::WithinBound),
        both(
            both(policy.map_err(Causes::clone), specification.clone()),
            aliases.clone(),
        ),
        |((policy, _), aliases)| {
            within(
                MAX_NEGOTIATION_ALIASES,
                policy.limits().locale_resolution.max_negotiation_aliases,
                aliases.len(),
            )
        },
    )?;
    let entries = aliases.map_or(&[][..], Vec::as_slice);
    let mut checked = bounded.clone();
    for (key, sharing) in by_key(entries) {
        let found = unique(evaluation, &bounded, key, &sharing)?;
        checked = both(checked, found).map(|((), ())| ());
    }
    let requested = locales.requested_set.as_ref().map_err(Causes::clone);
    let mut destinations = BTreeMap::new();
    for alias in entries {
        destinations
            .entry((&alias.key.key, &alias.value.key))
            .or_insert(&alias.value);
    }
    for ((key, value), destination) in destinations {
        let member = evaluation.check(
            unit(
                NegotiationSubject::Destination(key.clone(), value.clone()),
                Check::RequestedMember,
            ),
            both(
                both(bounded.clone(), destination.locale.clone()),
                requested.clone(),
            ),
            |(((), locale), requested)| {
                if requested.contains(&locale) {
                    Ok(())
                } else {
                    Err(issue(Code::NegotiationAliasDestinationNotRequested, None))
                }
            },
        )?;
        checked = both(checked, member).map(|((), ())| ());
    }
    let mut map = Ok(BTreeMap::new());
    for alias in entries {
        map = both(
            map,
            both(alias.key.locale.clone(), alias.value.locale.clone()),
        )
        .map(|(mut map, (key, value))| {
            map.insert(key, value);
            map
        });
    }
    Ok(
        both(both(specification.clone(), checked), map).map(|((specification, ()), aliases)| {
            NegotiationProfile {
                specification,
                aliases,
            }
        }),
    )
}

/// The submitted entries grouped by their key's subject, in subject order.
fn by_key(entries: &[AliasLocales]) -> BTreeMap<&LocaleKey, Vec<&AliasLocales>> {
    let mut keys: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for alias in entries {
        keys.entry(&alias.key.key).or_default().push(alias);
    }
    keys
}

/// The key's unit: two entries naming one canonical key are a duplicate whose
/// reason their destinations give.
fn unique(
    evaluation: &mut Evaluation,
    bounded: &Admitted<()>,
    key: &LocaleKey,
    sharing: &[&AliasLocales],
) -> Result<Admitted<()>, EvaluationInvariant> {
    // Entries under one subject share their key's result.
    let spelled = &sharing[0].key;
    let reason = if sharing.len() == 1 {
        Ok(None)
    } else {
        duplicate(sharing.iter().map(|alias| &alias.value.locale)).map(Some)
    };
    evaluation.check(
        unit(NegotiationSubject::Key(key.clone()), Check::UniqueLocale),
        both(both(bounded.clone(), spelled.locale.clone()), reason),
        |(_, reason)| match reason {
            None => Ok(()),
            Some(reason) => Err(issue(Code::NegotiationAliasDuplicate, Some(reason))),
        },
    )
}

/// Why several entries share a key, from their destinations. Two admitted
/// destinations that differ prove a conflict whatever the others are; the
/// same destination needs every one admitted, and is otherwise unproved.
fn duplicate<'a>(
    destinations: impl Iterator<Item = &'a Admitted<CanonicalLocale>>,
) -> Admitted<Reason> {
    let mut seen: Option<&CanonicalLocale> = None;
    let mut unavailable: Option<Causes> = None;
    for destination in destinations {
        match (destination, seen) {
            (Ok(locale), Some(first)) if locale != first => {
                return Ok(Reason::ConflictingDestination);
            }
            (Ok(locale), _) => seen = Some(locale),
            (Err(causes), _) => {
                unavailable = Some(match unavailable {
                    Some(earlier) => earlier.union(causes.clone()),
                    None => causes.clone(),
                });
            }
        }
    }
    unavailable.map_or(Ok(Reason::SameDestination), Err)
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
    use serde_json::{json, Value};

    use super::*;
    use crate::fixtures::minimal_config;
    use crate::input_limits::Bound;
    use crate::resolver::evaluation::{Actual, LimitExceeded, Status};
    use crate::resolver::locales::fixtures::{
        canonical, canonicalizer, content, declaration, key, limits, stand_in, target, units,
        waits_on, Declaration,
    };
    use crate::resolver::locales::{self, resolve_locales, LocaleFields, LocaleSubject};
    use crate::resolver::specification::Slot;

    type Id = &'static str;

    const SPECIFICATION: VersionedIdentity<Id> = VersionedIdentity {
        identity: "intlify-locale-negotiation",
        revision: "0",
    };

    /// A confirmed policy with `aliases` as its alias bound and room for
    /// everything Phase 7 bounds.
    fn policy(aliases: u64) -> ConfirmedPolicy {
        let mut limits = limits(64, 100, 100);
        limits.locale_resolution.max_negotiation_aliases = Bound::new(aliases).unwrap();
        ConfirmedPolicy::fixture_owned(limits)
    }

    /// The minimal declaration with these requested locales, the first its
    /// default, and these aliases.
    fn declared(requested: &[&str], aliases: Value) -> Declaration {
        declaration(&minimal_config(), |app| {
            app["requestedLocales"] = json!(requested);
            app["defaultRequestedLocale"] = json!(requested[0]);
            app["localeNegotiation"]["aliases"] = aliases;
        })
    }

    struct Resolved {
        evaluation: Evaluation,
        profile: Admitted<NegotiationProfile<Id>>,
    }

    /// Phase 7 over `fields` under a policy with room for them, then Phase 8's
    /// negotiation under `policy` and `specification`, into one evaluation.
    fn resolve_fields(
        fields: &LocaleFields<'_, crate::fixtures::FixtureTargetReference>,
        policy: Result<&ConfirmedPolicy, &Causes>,
        specification: &Admitted<VersionedIdentity<Id>>,
    ) -> Resolved {
        let mut evaluation = Evaluation::default();
        let locales = resolve_locales(
            &mut evaluation,
            Ok(&self::policy(100)),
            Ok(&canonicalizer(64)),
            fields,
        )
        .unwrap();
        let profile =
            resolve_negotiation(&mut evaluation, policy, specification, &locales).unwrap();
        Resolved {
            evaluation,
            profile,
        }
    }

    fn resolve_under(declaration: &Declaration, aliases: u64) -> Resolved {
        resolve_fields(
            &LocaleFields::declared(declaration),
            Ok(&policy(aliases)),
            &Ok(SPECIFICATION),
        )
    }

    fn resolve(declaration: &Declaration) -> Resolved {
        resolve_under(declaration, 100)
    }

    /// Phase 8's units and their statuses, in 015's order.
    fn statuses(evaluation: &Evaluation) -> Vec<(Unit, Status)> {
        evaluation
            .statuses()
            .filter(|(unit, _)| unit.phase == Phase::LocalePolicyResolution)
            .map(|(unit, status)| (unit.clone(), status.clone()))
            .collect()
    }

    /// Phase 8's issues as their unit, code, and reason.
    fn issues(evaluation: &Evaluation) -> Vec<(Unit, &'static str, Option<&'static str>)> {
        evaluation
            .issues()
            .filter(|key| key.unit().phase == Phase::LocalePolicyResolution)
            .map(|key| {
                (
                    key.unit().clone(),
                    key.code().as_str(),
                    key.reason().map(Reason::as_str),
                )
            })
            .collect()
    }

    fn bound_unit() -> Unit {
        unit(NegotiationSubject::Aliases, Check::WithinBound)
    }

    fn key_unit(key: LocaleKey) -> Unit {
        unit(NegotiationSubject::Key(key), Check::UniqueLocale)
    }

    fn destination_unit(owner: LocaleKey, own: LocaleKey) -> Unit {
        unit(
            NegotiationSubject::Destination(owner, own),
            Check::RequestedMember,
        )
    }

    /// Phase 7's unit for a spelling it could not canonicalize.
    fn spelling_unit(subject: LocaleSubject) -> Unit {
        locales::unit(subject, Check::CanonicalLocale)
    }

    #[test]
    fn subjects_order_the_bound_first_then_keys_then_destinations() {
        let ordered = [
            NegotiationSubject::Aliases,
            NegotiationSubject::Key(key("de")),
            NegotiationSubject::Key(key("fr")),
            NegotiationSubject::Key(content("zz")),
            NegotiationSubject::Destination(key("de"), key("fr")),
            NegotiationSubject::Destination(key("fr"), key("de")),
            NegotiationSubject::Destination(content("zz"), key("de")),
        ];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
        // Phase 8 commits after every Phase 7 unit.
        let last_of_phase_7 = locales::unit(
            LocaleSubject::TargetDefaultRequestedLocale(target("server"), content("zz")),
            Check::RequestedMember,
        );
        assert!(last_of_phase_7 < bound_unit());
    }

    #[test]
    fn a_valid_alias_map_resolves_to_canonical_keys_in_byte_order() {
        // `EN` sorts first as authored and second once canonical.
        let declaration = declared(
            &["en", "fr-FR", "de-DE"],
            json!({"fr": "fr-FR", "EN": "en", "de": "de-DE"}),
        );
        let resolved = resolve(&declaration);
        assert_eq!(issues(&resolved.evaluation), []);
        assert_eq!(
            statuses(&resolved.evaluation),
            [
                bound_unit(),
                key_unit(key("de")),
                key_unit(key("en")),
                key_unit(key("fr")),
                destination_unit(key("de"), key("de-DE")),
                destination_unit(key("en"), key("en")),
                destination_unit(key("fr"), key("fr-FR")),
            ]
            .map(|unit| (unit, Status::Evaluated))
        );
        let profile = resolved.profile.unwrap();
        assert_eq!(profile.specification(), &SPECIFICATION);
        assert_eq!(
            profile
                .aliases()
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect::<Vec<_>>(),
            [("de", "de-DE"), ("en", "en"), ("fr", "fr-FR")]
        );
    }

    #[test]
    fn two_keys_naming_one_locale_are_a_duplicate_with_the_reason_their_destinations_give() {
        for (aliases, reason, destinations) in [
            (
                json!({"EN": "fr-FR", "en": "fr-FR"}),
                "same-destination",
                vec![key("fr-FR")],
            ),
            (
                json!({"EN": "fr-FR", "en": "de-DE"}),
                "conflicting-destination",
                vec![key("de-DE"), key("fr-FR")],
            ),
        ] {
            let resolved = resolve(&declared(&["en", "fr-FR", "de-DE"], aliases));
            let duplicate = key_unit(key("en"));
            assert_eq!(
                issues(&resolved.evaluation),
                [(
                    duplicate.clone(),
                    "project-profile-negotiation-alias-duplicate",
                    Some(reason)
                )]
            );
            // A destination both entries share is one subject.
            assert_eq!(
                statuses(&resolved.evaluation)
                    .into_iter()
                    .filter_map(|(unit, _)| match unit.subject {
                        Subject::Negotiation(NegotiationSubject::Destination(_, own)) => Some(own),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                destinations
            );
            assert_eq!(units(&resolved.profile.unwrap_err()), [duplicate]);
        }
        // Aliases of one locale collide like equal spellings.
        let resolved = resolve(&declared(
            &["en", "he-IL"],
            json!({"iw-IL": "en", "he-IL": "en"}),
        ));
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                key_unit(key("he-IL")),
                "project-profile-negotiation-alias-duplicate",
                Some("same-destination")
            )]
        );
    }

    #[test]
    fn a_duplicate_whose_reason_waits_on_an_invalid_destination_is_not_evaluated() {
        let resolved = resolve(&declared(
            &["en", "fr-FR"],
            json!({"EN": "zz", "en": "fr-FR"}),
        ));
        let invalid = spelling_unit(LocaleSubject::NegotiationAliasValue(
            key("en"),
            content("zz"),
        ));
        assert_eq!(
            waits_on(&resolved.evaluation, &key_unit(key("en"))),
            Some(vec![invalid.clone()])
        );
        assert_eq!(issues(&resolved.evaluation), []);
        assert_eq!(units(&resolved.profile.unwrap_err()), [invalid]);
    }

    #[test]
    fn a_conflict_needs_two_admitted_destinations_and_sameness_needs_every_one() {
        let missing = stand_in(Slot::Coverage);
        let other = stand_in(Slot::DeliveryGraph);
        let (de, fr) = (Ok(canonical("de")), Ok(canonical("fr")));
        let unavailable = Err(missing.clone());
        let reason =
            |destinations: &[&Admitted<CanonicalLocale>]| duplicate(destinations.iter().copied());
        assert_eq!(reason(&[&de, &de]), Ok(Reason::SameDestination));
        assert_eq!(reason(&[&de, &fr]), Ok(Reason::ConflictingDestination));
        // Two that differ prove a conflict wherever the unavailable one is.
        for destinations in [
            [&unavailable, &de, &fr],
            [&de, &unavailable, &fr],
            [&de, &fr, &unavailable],
        ] {
            assert_eq!(reason(&destinations), Ok(Reason::ConflictingDestination));
        }
        assert_eq!(reason(&[&de, &unavailable, &de]), Err(missing.clone()));
        assert_eq!(
            reason(&[&unavailable, &Err(other.clone())]),
            Err(missing.union(other))
        );
    }

    #[test]
    fn a_destination_outside_the_project_set_blocks_but_a_key_outside_it_does_not() {
        // Neither key is requested; `fr-FR` is, even with `en` named twice.
        let resolved = resolve(&declared(
            &["en", "EN", "fr-FR"],
            json!({"fr": "fr-FR", "de": "de-DE"}),
        ));
        let outside = destination_unit(key("de"), key("de-DE"));
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                outside.clone(),
                "project-profile-negotiation-alias-destination-not-requested",
                None
            )]
        );
        assert_eq!(
            waits_on(
                &resolved.evaluation,
                &destination_unit(key("fr"), key("fr-FR"))
            ),
            None
        );
        assert_eq!(units(&resolved.profile.unwrap_err()), [outside]);
    }

    #[test]
    fn the_alias_bound_counts_every_submitted_entry_before_any_collision() {
        let declaration = declared(&["en"], json!({"EN": "en", "en": "en"}));
        let resolved = resolve_under(&declaration, 2);
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                key_unit(key("en")),
                "project-profile-negotiation-alias-duplicate",
                Some("same-destination")
            )]
        );
        assert_eq!(waits_on(&resolved.evaluation, &bound_unit()), None);

        let resolved = resolve_under(&declaration, 1);
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                bound_unit(),
                "project-profile-resource-limit-exceeded",
                None
            )]
        );
        let found = resolved.evaluation.issues().last().unwrap();
        assert_eq!(
            resolved.evaluation.limit(found),
            Some(LimitExceeded {
                bound: MAX_NEGOTIATION_ALIASES,
                limit: Bound::new(1).unwrap(),
                actual: Actual::Exact(2),
            })
        );
        for waiting in [key_unit(key("en")), destination_unit(key("en"), key("en"))] {
            assert_eq!(
                waits_on(&resolved.evaluation, &waiting),
                Some(vec![bound_unit()])
            );
        }
        assert_eq!(units(&resolved.profile.unwrap_err()), [bound_unit()]);
    }

    #[test]
    fn an_unadmitted_specification_policy_or_field_leaves_every_unit_not_evaluated() {
        let declaration = declared(&["en", "fr-FR"], json!({"fr": "fr-FR"}));
        let fields = LocaleFields::declared(&declaration);
        let unsupported = stand_in(Slot::LocaleNegotiation);
        let unconfirmed = stand_in(Slot::Coverage);
        let confirmed = policy(100);
        for (policy, specification, causes) in [
            (
                Ok(&confirmed),
                Err(unsupported.clone()),
                unsupported.clone(),
            ),
            (Err(&unconfirmed), Ok(SPECIFICATION), unconfirmed.clone()),
            (
                Err(&unconfirmed),
                Err(unsupported.clone()),
                unsupported.clone().union(unconfirmed.clone()),
            ),
        ] {
            let resolved = resolve_fields(&fields, policy, &specification);
            assert_eq!(issues(&resolved.evaluation), []);
            let recorded = statuses(&resolved.evaluation);
            assert_eq!(recorded.len(), 3);
            assert!(recorded
                .iter()
                .all(|(_, status)| status == &Status::NotEvaluated(causes.clone())));
            assert_eq!(resolved.profile, Err(causes));
        }
        // A field structural admission left unavailable has nothing to count.
        let mut fields = LocaleFields::declared(&declaration);
        let rejected = stand_in(Slot::DeliveryGraph);
        fields.locale_negotiation = Err(rejected.clone());
        let resolved = resolve_fields(&fields, Ok(&confirmed), &Ok(SPECIFICATION));
        assert_eq!(
            statuses(&resolved.evaluation),
            [(bound_unit(), Status::NotEvaluated(rejected.clone()))]
        );
        assert_eq!(resolved.profile, Err(rejected));
    }

    #[test]
    fn an_invalid_spelling_leaves_only_the_units_needing_its_identity_not_evaluated() {
        let resolved = resolve(&declared(
            &["en", "fr-FR"],
            json!({"zz": "fr-FR", "fr": "zz"}),
        ));
        let invalid_key = spelling_unit(LocaleSubject::NegotiationAliasKey(content("zz")));
        let invalid_value = spelling_unit(LocaleSubject::NegotiationAliasValue(
            key("fr"),
            content("zz"),
        ));
        assert_eq!(
            waits_on(&resolved.evaluation, &key_unit(content("zz"))),
            Some(vec![invalid_key.clone()])
        );
        assert_eq!(
            waits_on(
                &resolved.evaluation,
                &destination_unit(key("fr"), content("zz"))
            ),
            Some(vec![invalid_value.clone()])
        );
        // Each one's other units need nothing it lacks.
        for evaluated in [
            key_unit(key("fr")),
            destination_unit(content("zz"), key("fr-FR")),
        ] {
            assert_eq!(waits_on(&resolved.evaluation, &evaluated), None);
        }
        assert_eq!(issues(&resolved.evaluation), []);
        assert_eq!(
            units(&resolved.profile.unwrap_err()),
            [invalid_key, invalid_value]
        );
    }

    #[test]
    fn a_requested_set_phase_7_could_not_form_leaves_only_destinations_not_evaluated() {
        let resolved = resolve(&declared(
            &["en", "zz"],
            json!({"fr": "fr-FR", "de": "fr-FR"}),
        ));
        let invalid = spelling_unit(LocaleSubject::RequestedLocale(content("zz")));
        for destination in [
            destination_unit(key("de"), key("fr-FR")),
            destination_unit(key("fr"), key("fr-FR")),
        ] {
            assert_eq!(
                waits_on(&resolved.evaluation, &destination),
                Some(vec![invalid.clone()])
            );
        }
        for key_ in ["de", "fr"] {
            assert_eq!(waits_on(&resolved.evaluation, &key_unit(key(key_))), None);
        }
        assert_eq!(units(&resolved.profile.unwrap_err()), [invalid]);
    }

    #[test]
    fn no_aliases_resolve_to_an_empty_map() {
        for edit in [
            (|app: &mut Value| {
                app.as_object_mut().unwrap().remove("localeNegotiation");
            }) as fn(&mut Value),
            |app| app["localeNegotiation"] = json!({}),
            |app| app["localeNegotiation"] = json!({"aliases": {}}),
        ] {
            let declaration = declaration(&minimal_config(), edit);
            let resolved = resolve(&declaration);
            assert_eq!(
                statuses(&resolved.evaluation),
                [(bound_unit(), Status::Evaluated)]
            );
            let profile = resolved.profile.unwrap();
            assert_eq!(profile.specification(), &SPECIFICATION);
            assert!(profile.aliases().is_empty());
        }
    }
}
