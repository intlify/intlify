// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Phase 7's counting bounds: `maxLocaleOccurrences` over every locale
//! spelling of the declaration, checked before any is canonicalized,
//! `maxRequestedLocales` over the project's canonical requested set, and the
//! two target bounds 015 fixes in this phase, `maxTargetProfiles` over the
//! Target ID map and the `maxTargetIdBytes` of each Target ID key, which every
//! later check of the targets waits on. Counts use checked arithmetic, and a
//! count past `u64` is past every bound. The per-spelling
//! `maxLocaleIdentifierBytes` is the canonicalizer's check.
//!
//! A bound's Finding is observed where its table's Scope sits in the
//! declaration, beside the confirmed policy, which supplies the bound
//! (D15:1896-1897, 1907, 1930-1931, 2249): the declaration for the
//! occurrences, which every locale-bearing field holds, the
//! `requestedLocales` field for the requested set, the `targetProfiles` map,
//! and each Target ID at its key.

use std::collections::BTreeMap;

use crate::model::{
    FallbackCandidate, NonEmptyMap, ProfileDeclaration, TargetDeclaration, TargetId,
};

use super::super::declaration_observer::{beside, DeclarationObserver};
use super::super::evaluation::{both, Actual, Admitted, Causes, Check, Evaluation, Found};
use super::super::limits::{BoundId, ConfirmedPolicy, Group};
use super::super::tally::{actual, total, within, Tally};
use super::{unit, KnownSet, LocaleFields, LocaleInvariant, LocaleSubject};

const fn locale_bound(name: &'static str) -> BoundId {
    BoundId {
        group: Group::LocaleResolution,
        name,
    }
}

pub(super) const MAX_LOCALE_IDENTIFIER_BYTES: BoundId = locale_bound("maxLocaleIdentifierBytes");
const MAX_LOCALE_OCCURRENCES: BoundId = locale_bound("maxLocaleOccurrences");
const MAX_REQUESTED_LOCALES: BoundId = locale_bound("maxRequestedLocales");

const fn grouping_bound(name: &'static str) -> BoundId {
    BoundId {
        group: Group::TargetGrouping,
        name,
    }
}

const MAX_TARGET_PROFILES: BoundId = grouping_bound("maxTargetProfiles");
const MAX_TARGET_ID_BYTES: BoundId = grouping_bound("maxTargetIdBytes");

/// The locale spellings of a declaration structural admission admitted
/// whole, as `maxLocaleOccurrences` counts them: an alias key and its
/// destination, a fallback key and its literal candidates (not the Intent
/// source-locale marker), and every coverage locale selector item, though
/// Phase 7 canonicalizes neither a candidate nor a selector.
pub(super) fn declared_spellings<Policy, Target>(
    declaration: &ProfileDeclaration<Policy, Target>,
) -> Tally {
    let aliases = Tally::of(
        declaration
            .locale_negotiation
            .as_option()
            .and_then(|negotiation| negotiation.aliases.as_option())
            .map_or(0, BTreeMap::len),
    );
    let fallback = declaration
        .message_fallback
        .as_option()
        .into_iter()
        .flat_map(BTreeMap::values)
        .map(|sequence| {
            let literals = sequence
                .as_slice()
                .iter()
                .filter(|candidate| matches!(candidate, FallbackCandidate::Locale(_)))
                .count();
            Tally::of(1).plus(Tally::of(literals))
        });
    let selectors = declaration
        .coverage
        .as_option()
        .and_then(|coverage| coverage.rules.as_option())
        .into_iter()
        .flatten()
        .map(|rule| Tally::of(rule.requested_locales().map_or(0, <[String]>::len)));
    let targets = declaration.target_profiles.values().map(|target| {
        Tally::of(target.requested_locales.as_slice().len()).plus(Tally::of(usize::from(
            !target.default_requested_locale.is_absent(),
        )))
    });
    total([
        Tally::of(declaration.requested_locales.as_slice().len()),
        Tally::of(1),
        Tally::of(usize::from(!declaration.default_source_locale.is_absent())),
        aliases.plus(aliases),
        total(fallback),
        total(selectors),
        total(targets),
    ])
}

/// The occurrence bound's unit: the spellings against `maxLocaleOccurrences`,
/// and whether canonicalization may run. Past it, the declaration is observed:
/// it holds every field the bound counts.
pub(super) fn occurrences<Target>(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    fields: &LocaleFields<'_, Target>,
    observer: Option<&DeclarationObserver<'_>>,
) -> Result<(Option<Actual>, Admitted<()>), LocaleInvariant> {
    let measured =
        both(policy.map_err(Causes::clone), fields.spellings.clone()).map(|(policy, spellings)| {
            let limit = policy.limits().locale_resolution.max_locale_occurrences;
            (limit, actual(spellings, limit))
        });
    let total = measured.as_ref().ok().map(|&(_, actual)| actual);
    let observed = beside(policy, observer, |observer| observer.value(&[]))?;
    let bounded = evaluation.check_found(
        unit(LocaleSubject::Occurrences, Check::WithinBound),
        measured,
        |(limit, actual)| {
            within(MAX_LOCALE_OCCURRENCES, limit, actual)
                .map_err(|blocking| Found::through(blocking, observed))
        },
    )?;
    Ok((total, bounded))
}

/// The requested-set bound's unit: the canonical size of what the set holds,
/// which a repeated locale does not grow and an element without an identity
/// has none to add to, within `maxRequestedLocales`, observed past it at the
/// project's `requestedLocales` (D15:1897, 2184, 3354).
pub(super) fn requested(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    set: &KnownSet,
    observer: Option<&DeclarationObserver<'_>>,
) -> Result<Admitted<()>, LocaleInvariant> {
    let counted = set.counted();
    // The field is there once an element of it has an identity.
    let observed = match &counted {
        Ok(_) => beside(policy, observer, |observer| {
            observer.value(&["requestedLocales"])
        })?,
        Err(_) => None,
    };
    Ok(evaluation.check_found(
        unit(LocaleSubject::RequestedLocales, Check::WithinBound),
        both(policy.map_err(Causes::clone), counted),
        |(policy, set)| {
            let limit = policy.limits().locale_resolution.max_requested_locales;
            within(
                MAX_REQUESTED_LOCALES,
                limit,
                actual(Tally::of(set.as_slice().len()), limit),
            )
            .map_err(|blocking| Found::through(blocking, observed))
        },
    )?)
}

/// What the target bounds' units settle: the map's result, and each Target
/// ID's own, which includes the map's.
pub(super) struct TargetBounds {
    pub(super) map: Admitted<()>,
    pub(super) targets: BTreeMap<TargetId, Admitted<()>>,
}

/// The target bounds' units: the Target ID map within `maxTargetProfiles`,
/// which every per-target check waits on, and then each Target ID key within
/// `maxTargetIdBytes`, which every other check of that target waits on. Past
/// a bound, the map is observed, or the Target ID at its key, which is
/// classified by facts fixed when the record is constructed (D15:776).
pub(super) fn targets<Target>(
    evaluation: &mut Evaluation,
    confirmed: Result<&ConfirmedPolicy, &Causes>,
    targets: &NonEmptyMap<TargetId, TargetDeclaration<Target>>,
    observer: Option<&DeclarationObserver<'_>>,
) -> Result<TargetBounds, LocaleInvariant> {
    let policy = confirmed.map_err(Causes::clone);
    let observed = beside(confirmed, observer, |observer| {
        observer.value(&["targetProfiles"])
    })?;
    let bounded = evaluation.check_found(
        unit(LocaleSubject::TargetProfiles, Check::WithinBound),
        policy.clone(),
        |policy| {
            let limit = policy.limits().target_grouping.max_target_profiles;
            within(
                MAX_TARGET_PROFILES,
                limit,
                actual(Tally::of(targets.len()), limit),
            )
            .map_err(|blocking| Found::through(blocking, observed))
        },
    )?;
    let mut admitted = BTreeMap::new();
    for (id, _) in targets.iter() {
        let observed = beside(confirmed, observer, |observer| {
            observer.member_key(&["targetProfiles"], id.as_str())
        })?;
        let own = evaluation.check_found(
            unit(LocaleSubject::TargetId(id.clone()), Check::WithinBound),
            both(policy.clone(), bounded.clone()),
            |(policy, ())| {
                let limit = policy.limits().target_grouping.max_target_id_bytes;
                within(
                    MAX_TARGET_ID_BYTES,
                    limit,
                    actual(Tally::of(id.as_str().len()), limit),
                )
                .map_err(|blocking| Found::through(blocking, observed))
            },
        )?;
        admitted.insert(id.clone(), own);
    }
    Ok(TargetBounds {
        map: bounded,
        targets: admitted,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::super::fixtures::{
        declaration, issues, limits, policy, spelled, stand_in, target, units, waits_on,
    };
    use super::*;
    use crate::fixtures::{complete_config, minimal_config};
    use crate::input_limits::Bound;
    use crate::resolver::declaration_observer::fixtures::Observed;
    use crate::resolver::evaluation::{Code, LimitExceeded, Observation, Unit};
    use crate::resolver::evidence::EvidenceProjection;
    use crate::resolver::specification::Slot;

    /// What each occurrence candidate `evaluation` recorded observed, with
    /// the unit of its Finding.
    fn observed(evaluation: &Evaluation) -> Vec<(Unit, Observation)> {
        evaluation
            .occurrence_candidates()
            .iter()
            .map(|candidate| {
                (
                    candidate.finding.unit().clone(),
                    Observation {
                        primary: candidate.primary.clone(),
                        related: candidate.related.clone(),
                    },
                )
            })
            .collect()
    }

    /// `at` in the declaration, beside `policy`, which supplies the bound.
    fn beside_policy(at: EvidenceProjection, policy: &ConfirmedPolicy) -> Observation {
        Observation {
            primary: at,
            related: vec![policy.evidence().clone()],
        }
    }

    /// One change to a declaration.
    type Edit = fn(&mut Value);

    fn bound(value: u64) -> Bound {
        Bound::new(value).unwrap()
    }

    fn occurrence_unit() -> Unit {
        unit(LocaleSubject::Occurrences, Check::WithinBound)
    }

    fn map_unit() -> Unit {
        unit(LocaleSubject::TargetProfiles, Check::WithinBound)
    }

    fn id_unit(id: &str) -> Unit {
        unit(LocaleSubject::TargetId(target(id)), Check::WithinBound)
    }

    /// A policy with the two target bounds as given.
    fn target_policy(profiles: u64, id_bytes: u64) -> ConfirmedPolicy {
        let mut limits = limits(64, 100, 100);
        limits.target_grouping.max_target_profiles = bound(profiles);
        limits.target_grouping.max_target_id_bytes = bound(id_bytes);
        ConfirmedPolicy::fixture_owned(limits)
    }

    /// The total `occurrences` measures for the `app` declaration of `config`
    /// under a bound no test reaches.
    fn counted(config: &Value, edit: impl FnOnce(&mut Value)) -> Option<Actual> {
        let declaration = declaration(config, edit);
        occurrences(
            &mut Evaluation::default(),
            Ok(&policy(64, 1_000, 1_000)),
            &LocaleFields::declared(&declaration),
            None,
        )
        .unwrap()
        .0
    }

    #[test]
    fn bounds_are_the_sections_locale_resolution_bounds() {
        let limits = limits(11, 12, 13);
        let bounds = limits.bounds();
        for (id, value) in [
            (MAX_LOCALE_IDENTIFIER_BYTES, 11),
            (MAX_LOCALE_OCCURRENCES, 12),
            (MAX_REQUESTED_LOCALES, 13),
        ] {
            assert!(
                bounds.contains(&(id, bound(value))),
                "{} is not in the section with {value}",
                id.name
            );
        }
    }

    #[test]
    fn every_role_counts_toward_the_occurrence_bound() {
        let minimal = minimal_config();
        // requestedLocales, defaultRequestedLocale, and one target's set.
        assert_eq!(counted(&minimal, |_| {}), Some(Actual::Exact(3)));
        // Repeats count: the bound protects work before duplicates collapse.
        assert_eq!(
            counted(&minimal, |app| app["requestedLocales"] =
                json!(["en", "en", "EN"])),
            Some(Actual::Exact(5))
        );
        let edits: [(Edit, u64); 6] = [
            (|app| app["defaultSourceLocale"] = json!("en"), 4),
            // A key and its destination each.
            (
                |app| app["localeNegotiation"] = json!({"aliases": {"de": "de-DE", "fr": "fr-FR"}}),
                8,
            ),
            // A key and its literals, never the Intent source-locale marker,
            // though Phase 7 canonicalizes no literal.
            (
                |app| {
                    app["messageFallback"] =
                        json!({"ja": ["en", {"kind": "intent-source-locale"}, "de"]});
                },
                11,
            ),
            // Selector items, though Phase 7 canonicalizes none of them.
            (
                |app| {
                    app["coverage"] = json!({"rules": [
                        {"requestedLocales": ["ja", "en"], "mode": "fallback-allowed"},
                        {"intentSurfaceClasses": ["checkout"], "mode": "direct-required"}
                    ]});
                },
                13,
            ),
            (
                |app| app["targetProfiles"]["browser"]["defaultRequestedLocale"] = json!("en"),
                14,
            ),
            (
                |app| {
                    app["targetProfiles"]["server"] = json!({
                        "profile": {"$testTarget": "server"},
                        "requestedLocales": ["en", "de"]
                    });
                },
                16,
            ),
        ];
        let mut config = minimal;
        for (edit, expected) in edits {
            edit(&mut config["profiles"]["app"]);
            assert_eq!(counted(&config, |_| {}), Some(Actual::Exact(expected)));
        }
        // Present but empty policy inputs have no spellings.
        assert_eq!(
            counted(&minimal_config(), |app| {
                app["localeNegotiation"] = json!({"aliases": {}});
                app["messageFallback"] = json!({});
                app["coverage"] = json!({"defaultMode": "direct-required"});
            }),
            Some(Actual::Exact(3))
        );
        assert_eq!(counted(&complete_config(), |_| {}), Some(Actual::Exact(12)));
    }

    /// Past the bound, the declaration is observed, which holds every field
    /// the bound counts, beside the policy, which supplies it (D15:1907,
    /// 2249).
    #[test]
    fn the_occurrence_bound_holds_at_its_limit_and_reports_the_first_over() {
        let config = complete_config();
        let declaration = declaration(&config, |_| {});
        let fields = LocaleFields::declared(&declaration);
        Observed::new(&config).app(|observer| {
            let mut evaluation = Evaluation::default();
            let (total, bounded) = occurrences(
                &mut evaluation,
                Ok(&policy(64, 12, 100)),
                &fields,
                Some(observer),
            )
            .unwrap();
            assert_eq!((total, bounded), (Some(Actual::Exact(12)), Ok(())));
            assert_eq!(waits_on(&evaluation, &occurrence_unit()), None);
            assert_eq!(issues(&evaluation), []);

            let mut evaluation = Evaluation::default();
            let confirmed = policy(64, 11, 100);
            let (total, bounded) =
                occurrences(&mut evaluation, Ok(&confirmed), &fields, Some(observer)).unwrap();
            assert_eq!(total, Some(Actual::Exact(12)));
            let exceeded = Some(LimitExceeded {
                bound: MAX_LOCALE_OCCURRENCES,
                limit: bound(11),
                actual: Actual::Exact(12),
            });
            assert_eq!(
                issues(&evaluation),
                [(
                    occurrence_unit(),
                    "project-profile-resource-limit-exceeded",
                    exceeded
                )]
            );
            assert_eq!(units(&bounded.unwrap_err()), [occurrence_unit()]);
            assert_eq!(
                observed(&evaluation),
                [(
                    occurrence_unit(),
                    beside_policy(observer.value(&[]).unwrap(), &confirmed)
                )]
            );
        });
    }

    /// A field that failed holds the count back no more: the count is of the
    /// spellings, which is what it is whatever a field's failure (D15:1896,
    /// 1907). What it reads is the declaration's own, here beside a field
    /// the caller marked unavailable.
    #[test]
    fn a_field_that_failed_leaves_the_bound_measured() {
        let config = complete_config();
        let declaration = declaration(&config, |_| {});
        let mut fields = LocaleFields::declared(&declaration);
        fields.coverage = Err(stand_in(Slot::Coverage));
        fields.requested_locales = Err(stand_in(Slot::DeliveryGraph));
        let mut evaluation = Evaluation::default();
        let (total, bounded) =
            occurrences(&mut evaluation, Ok(&policy(64, 12, 100)), &fields, None).unwrap();
        assert_eq!((total, bounded), (Some(Actual::Exact(12)), Ok(())));
        assert_eq!(waits_on(&evaluation, &occurrence_unit()), None);
    }

    /// Without a declaration there is nothing to count, and without a policy
    /// no bound: the unit waits on every cause there is (D15:2131).
    #[test]
    fn an_unconfirmed_policy_leaves_the_bound_not_evaluated_with_every_cause() {
        let declaration = declaration(&minimal_config(), |_| {});
        let mut fields = LocaleFields::declared(&declaration);
        let policy_causes = stand_in(Slot::Coverage);
        let mut evaluation = Evaluation::default();
        let (total, bounded) =
            occurrences(&mut evaluation, Err(&policy_causes), &fields, None).unwrap();
        assert_eq!((total, bounded), (None, Err(policy_causes.clone())));

        let unselected = stand_in(Slot::DeliveryPlacement);
        fields.spellings = Err(unselected.clone());
        let mut evaluation = Evaluation::default();
        let (total, bounded) =
            occurrences(&mut evaluation, Err(&policy_causes), &fields, None).unwrap();
        assert_eq!(
            (total, bounded),
            (None, Err(policy_causes.union(unselected.clone())))
        );
        let mut evaluation = Evaluation::default();
        let (total, bounded) =
            occurrences(&mut evaluation, Ok(&policy(64, 100, 100)), &fields, None).unwrap();
        assert_eq!((total, bounded), (None, Err(unselected)));
    }

    /// The set of `locales`, with `missing` as why an element has no identity.
    fn known(locales: &[&str], missing: Option<Causes>) -> KnownSet {
        let spelled: Vec<_> = locales.iter().map(|locale| spelled(locale)).collect();
        KnownSet::new(&spelled, missing)
    }

    /// Past the bound, the project's `requestedLocales` is observed beside
    /// the policy (D15:2249).
    #[test]
    fn the_requested_set_is_bounded_by_its_canonical_size() {
        let requested_unit = unit(LocaleSubject::RequestedLocales, Check::WithinBound);
        let set = known(&["en", "de", "fr"], None);
        let observed_config = Observed::new(&minimal_config());
        let mut evaluation = Evaluation::default();
        assert_eq!(
            requested(&mut evaluation, Ok(&policy(64, 100, 3)), &set, None).unwrap(),
            Ok(())
        );

        let mut evaluation = Evaluation::default();
        let confirmed = policy(64, 100, 2);
        let found = observed_config.app(|observer| {
            requested(&mut evaluation, Ok(&confirmed), &set, Some(observer)).unwrap()
        });
        assert_eq!(
            units(&found.unwrap_err()),
            std::slice::from_ref(&requested_unit)
        );
        observed_config.app(|observer| {
            assert_eq!(
                observed(&evaluation),
                [(
                    requested_unit.clone(),
                    beside_policy(observer.value(&["requestedLocales"]).unwrap(), &confirmed)
                )]
            );
        });
        let key = evaluation.issues().next().unwrap();
        assert_eq!(key.code(), Code::ResourceLimitExceeded);
        assert_eq!(
            evaluation.limit(key),
            Some(LimitExceeded {
                bound: MAX_REQUESTED_LOCALES,
                limit: bound(2),
                actual: Actual::Exact(3),
            })
        );

        // A field structural admission did not admit is not in the
        // declaration: with no element's identity, the unit observes nothing
        // there and waits.
        let mut unadmitted = minimal_config();
        unadmitted["profiles"]["app"]
            .as_object_mut()
            .unwrap()
            .remove("requestedLocales");
        let missing = stand_in(Slot::Coverage);
        let mut evaluation = Evaluation::default();
        let unproved = Observed::new(&unadmitted).app(|observer| {
            requested(
                &mut evaluation,
                Ok(&policy(64, 100, 2)),
                &known(&[], Some(missing.clone())),
                Some(observer),
            )
        });
        assert_eq!(unproved, Ok(Err(missing.clone())));
        let unconfirmed = stand_in(Slot::DeliveryGraph);
        let mut evaluation = Evaluation::default();
        assert_eq!(
            requested(&mut evaluation, Err(&unconfirmed), &set, None).unwrap(),
            Err(unconfirmed.clone())
        );
        assert_eq!(
            waits_on(&evaluation, &requested_unit),
            Some(units(&unconfirmed))
        );
    }

    /// An element without an identity has none to add: the bound holds over
    /// the others, exactly, and is found past them, observed at the field
    /// that still holds them (D15:1897, 2184, 3354).
    #[test]
    fn the_requested_set_is_bounded_by_what_it_holds_beside_a_failed_element() {
        let requested_unit = unit(LocaleSubject::RequestedLocales, Check::WithinBound);
        let failed = stand_in(Slot::Coverage);
        let set = known(&["en", "de"], Some(failed));
        let mut evaluation = Evaluation::default();
        assert_eq!(
            requested(&mut evaluation, Ok(&policy(64, 100, 2)), &set, None).unwrap(),
            Ok(())
        );
        assert_eq!(waits_on(&evaluation, &requested_unit), None);
        let observed_config = Observed::new(&minimal_config());
        let mut evaluation = Evaluation::default();
        let confirmed = policy(64, 100, 1);
        let found = observed_config.app(|observer| {
            requested(&mut evaluation, Ok(&confirmed), &set, Some(observer)).unwrap()
        });
        assert_eq!(
            units(&found.unwrap_err()),
            std::slice::from_ref(&requested_unit)
        );
        let key = evaluation.issues().next().unwrap();
        assert_eq!(
            evaluation.limit(key).map(|exceeded| exceeded.actual),
            Some(Actual::Exact(2))
        );
        observed_config.app(|observer| {
            assert_eq!(
                observed(&evaluation),
                [(
                    requested_unit.clone(),
                    beside_policy(observer.value(&["requestedLocales"]).unwrap(), &confirmed)
                )]
            );
        });
    }

    #[test]
    fn target_bounds_are_the_sections_target_grouping_bounds() {
        let policy = target_policy(21, 22);
        let bounds = policy.limits().bounds();
        for (id, value) in [(MAX_TARGET_PROFILES, 21), (MAX_TARGET_ID_BYTES, 22)] {
            assert!(
                bounds.contains(&(id, bound(value))),
                "{} is not in the section with {value}",
                id.name
            );
        }
    }

    /// Past a bound, the map is observed, or the Target ID at its key, beside
    /// the policy (D15:776, 1930-1931, 2249).
    #[test]
    fn the_target_bounds_hold_at_their_limits_and_the_ids_wait_on_the_map() {
        // `browser` is seven bytes and `server` six.
        let config = complete_config();
        let declaration = declaration(&config, |_| {});
        let declared = &declaration.target_profiles;
        let observed_config = Observed::new(&config);
        let mut evaluation = Evaluation::default();
        let bounds = observed_config.app(|observer| {
            targets(
                &mut evaluation,
                Ok(&target_policy(2, 7)),
                declared,
                Some(observer),
            )
            .unwrap()
        });
        assert_eq!(bounds.map, Ok(()));
        assert_eq!(
            bounds.targets,
            BTreeMap::from([(target("browser"), Ok(())), (target("server"), Ok(()))])
        );
        assert_eq!(issues(&evaluation), []);

        // One byte more blocks only that ID.
        let mut evaluation = Evaluation::default();
        let confirmed = target_policy(2, 6);
        let bounds = observed_config.app(|observer| {
            targets(&mut evaluation, Ok(&confirmed), declared, Some(observer)).unwrap()
        });
        let exceeded = Some(LimitExceeded {
            bound: MAX_TARGET_ID_BYTES,
            limit: bound(6),
            actual: Actual::Exact(7),
        });
        assert_eq!(
            issues(&evaluation),
            [(
                id_unit("browser"),
                "project-profile-resource-limit-exceeded",
                exceeded
            )]
        );
        assert_eq!(bounds.map, Ok(()));
        assert_eq!(
            units(bounds.targets[&target("browser")].as_ref().unwrap_err()),
            [id_unit("browser")]
        );
        assert_eq!(bounds.targets[&target("server")], Ok(()));
        observed_config.app(|observer| {
            assert_eq!(
                observed(&evaluation),
                [(
                    id_unit("browser"),
                    beside_policy(
                        observer.member_key(&["targetProfiles"], "browser").unwrap(),
                        &confirmed
                    )
                )]
            );
        });

        // One target more blocks the map, and no ID is checked.
        let mut evaluation = Evaluation::default();
        let confirmed = target_policy(1, 7);
        let bounds = observed_config.app(|observer| {
            targets(&mut evaluation, Ok(&confirmed), declared, Some(observer)).unwrap()
        });
        let exceeded = Some(LimitExceeded {
            bound: MAX_TARGET_PROFILES,
            limit: bound(1),
            actual: Actual::Exact(2),
        });
        assert_eq!(
            issues(&evaluation),
            [(
                map_unit(),
                "project-profile-resource-limit-exceeded",
                exceeded
            )]
        );
        assert_eq!(units(bounds.map.as_ref().unwrap_err()), [map_unit()]);
        observed_config.app(|observer| {
            assert_eq!(
                observed(&evaluation),
                [(
                    map_unit(),
                    beside_policy(observer.value(&["targetProfiles"]).unwrap(), &confirmed)
                )]
            );
        });
        for id in ["browser", "server"] {
            assert_eq!(waits_on(&evaluation, &id_unit(id)), Some(vec![map_unit()]));
            assert_eq!(
                units(bounds.targets[&target(id)].as_ref().unwrap_err()),
                [map_unit()]
            );
        }
    }

    #[test]
    fn an_unconfirmed_policy_leaves_every_target_bound_not_evaluated() {
        let declaration = declaration(&complete_config(), |_| {});
        let unconfirmed = stand_in(Slot::Coverage);
        let mut evaluation = Evaluation::default();
        let bounds = targets(
            &mut evaluation,
            Err(&unconfirmed),
            &declaration.target_profiles,
            None,
        )
        .unwrap();
        assert_eq!(bounds.map, Err(unconfirmed.clone()));
        assert_eq!(
            waits_on(&evaluation, &map_unit()),
            Some(units(&unconfirmed))
        );
        for id in ["browser", "server"] {
            assert_eq!(bounds.targets[&target(id)], Err(unconfirmed.clone()));
            assert_eq!(
                waits_on(&evaluation, &id_unit(id)),
                Some(units(&unconfirmed))
            );
        }
    }
}
