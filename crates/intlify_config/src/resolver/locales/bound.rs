// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Phase 7's two counting bounds: `maxLocaleOccurrences` over every locale
//! spelling of the declaration, checked before any is canonicalized, and
//! `maxRequestedLocales` over the project's canonical requested set. Counts
//! use checked arithmetic, and a count past `u64` is past every bound. The
//! per-spelling `maxLocaleIdentifierBytes` is the canonicalizer's check.

use std::collections::BTreeMap;

use crate::input_limits::Bound;
use crate::model::FallbackCandidate;

use super::super::evaluation::{
    both, Actual, Admitted, Blocking, Causes, Check, Evaluation, EvaluationInvariant, LimitExceeded,
};
use super::super::limits::{BoundId, ConfirmedPolicy, Group};
use super::{unit, LocaleFields, LocaleSet, LocaleSubject};

const fn locale_bound(name: &'static str) -> BoundId {
    BoundId {
        group: Group::LocaleResolution,
        name,
    }
}

pub(super) const MAX_LOCALE_IDENTIFIER_BYTES: BoundId = locale_bound("maxLocaleIdentifierBytes");
const MAX_LOCALE_OCCURRENCES: BoundId = locale_bound("maxLocaleOccurrences");
const MAX_REQUESTED_LOCALES: BoundId = locale_bound("maxRequestedLocales");

/// A count kept with checked arithmetic. `None` is a count past `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tally(Option<u64>);

impl Tally {
    fn of(count: usize) -> Self {
        Self(u64::try_from(count).ok())
    }

    fn plus(self, other: Self) -> Self {
        Self(
            self.0
                .zip(other.0)
                .and_then(|(left, right)| left.checked_add(right)),
        )
    }
}

fn total(tallies: impl IntoIterator<Item = Tally>) -> Tally {
    tallies.into_iter().fold(Tally::of(0), Tally::plus)
}

/// A total, or, past `u64`, the smallest witness that it exceeds `limit`.
fn actual(tally: Tally, limit: Bound) -> Actual {
    // `Bound` excludes `u64::MAX`, so the witness is representable.
    tally
        .0
        .map_or(Actual::AtLeast(limit.get() + 1), Actual::Exact)
}

/// What a count proves against `limit` while part of what it counts may be
/// unavailable: an exact total when nothing is, a witness when what is
/// visible already exceeds `limit` (the rest can only add), and otherwise
/// nothing, with the causes of the missing part.
fn measure(tally: Tally, unavailable: Option<Causes>, limit: Bound) -> Admitted<Actual> {
    match (actual(tally, limit), unavailable) {
        (actual, None) => Ok(actual),
        (Actual::Exact(total), Some(causes)) if total <= limit.get() => Err(causes),
        (_, Some(_)) => Ok(Actual::AtLeast(limit.get() + 1)),
    }
}

/// The bound's issue when `actual` exceeds `limit`.
fn within(bound: BoundId, limit: Bound, actual: Actual) -> Result<(), Blocking> {
    match actual {
        Actual::Exact(total) if total <= limit.get() => Ok(()),
        actual => Err(Blocking::limit(LimitExceeded {
            bound,
            limit,
            actual,
        })),
    }
}

/// Each field's locale spellings as `maxLocaleOccurrences` counts them: an
/// alias key and its destination, a fallback key and its literal candidates
/// (not the Intent source-locale marker), and every coverage locale selector
/// item, though Phase 7 canonicalizes no selector.
fn field_tallies<'f, Target>(
    fields: &'f LocaleFields<'_, Target>,
) -> [Result<Tally, &'f Causes>; 7] {
    [
        fields
            .requested_locales
            .as_ref()
            .map(|requested| Tally::of(requested.len())),
        fields
            .default_requested_locale
            .as_ref()
            .map(|_| Tally::of(1)),
        fields
            .default_source_locale
            .as_ref()
            .map(|source| Tally::of(usize::from(source.is_some()))),
        fields.locale_negotiation.as_ref().map(|negotiation| {
            let aliases = Tally::of(
                negotiation
                    .and_then(|negotiation| negotiation.aliases.as_option())
                    .map_or(0, BTreeMap::len),
            );
            aliases.plus(aliases)
        }),
        fields.message_fallback.as_ref().map(|fallback| {
            total(
                fallback
                    .iter()
                    .flat_map(|fallback| fallback.values())
                    .map(|sequence| {
                        let literals = sequence
                            .as_slice()
                            .iter()
                            .filter(|candidate| matches!(candidate, FallbackCandidate::Locale(_)))
                            .count();
                        Tally::of(1).plus(Tally::of(literals))
                    }),
            )
        }),
        fields.coverage.as_ref().map(|coverage| {
            total(
                coverage
                    .and_then(|coverage| coverage.rules.as_option())
                    .into_iter()
                    .flatten()
                    .map(|rule| Tally::of(rule.requested_locales().map_or(0, <[String]>::len))),
            )
        }),
        fields.target_profiles.as_ref().map(|targets| {
            total(targets.values().map(|target| {
                Tally::of(target.requested_locales.as_slice().len()).plus(Tally::of(usize::from(
                    !target.default_requested_locale.is_absent(),
                )))
            }))
        }),
    ]
}

/// The occurrence bound's unit: what the admitted fields prove against
/// `maxLocaleOccurrences`, and whether canonicalization may run.
pub(super) fn occurrences<Target>(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    fields: &LocaleFields<'_, Target>,
) -> Result<(Option<Actual>, Admitted<()>), EvaluationInvariant> {
    let mut visible = Tally::of(0);
    let mut unavailable: Option<Causes> = None;
    for field in field_tallies(fields) {
        match field {
            Ok(tally) => visible = visible.plus(tally),
            Err(causes) => unavailable = Some(join(unavailable, causes)),
        }
    }
    let measured = match policy {
        Ok(policy) => {
            let limit = policy.limits().locale_resolution.max_locale_occurrences;
            measure(visible, unavailable, limit).map(|actual| (limit, actual))
        }
        Err(causes) => Err(join(unavailable, causes)),
    };
    let total = measured.as_ref().ok().map(|&(_, actual)| actual);
    let bounded = evaluation.check(
        unit(LocaleSubject::Occurrences, Check::WithinBound),
        measured,
        |(limit, actual)| within(MAX_LOCALE_OCCURRENCES, limit, actual),
    )?;
    Ok((total, bounded))
}

fn join(earlier: Option<Causes>, causes: &Causes) -> Causes {
    match earlier {
        Some(earlier) => earlier.union(causes.clone()),
        None => causes.clone(),
    }
}

/// The requested-set bound's unit: the set's canonical size, which a repeated
/// locale does not grow, within `maxRequestedLocales`.
pub(super) fn requested(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    set: &Admitted<LocaleSet>,
) -> Result<Admitted<()>, EvaluationInvariant> {
    evaluation.check(
        unit(LocaleSubject::RequestedLocales, Check::WithinBound),
        both(
            policy.map_err(Causes::clone),
            set.as_ref().map_err(Causes::clone),
        ),
        |(policy, set)| {
            let limit = policy.limits().locale_resolution.max_requested_locales;
            within(
                MAX_REQUESTED_LOCALES,
                limit,
                actual(Tally::of(set.as_slice().len()), limit),
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::super::fixtures::{
        canonical, declaration, issues, limits, policy, stand_in, units, waits_on,
    };
    use super::*;
    use crate::fixtures::{complete_config, minimal_config};
    use crate::resolver::evaluation::{Code, Unit};
    use crate::resolver::specification::Slot;

    /// One change to a declaration.
    type Edit = fn(&mut Value);

    fn bound(value: u64) -> Bound {
        Bound::new(value).unwrap()
    }

    fn occurrence_unit() -> Unit {
        unit(LocaleSubject::Occurrences, Check::WithinBound)
    }

    /// The total `occurrences` measures for the `app` declaration of `config`
    /// under a bound no test reaches.
    fn counted(config: &Value, edit: impl FnOnce(&mut Value)) -> Option<Actual> {
        let declaration = declaration(config, edit);
        occurrences(
            &mut Evaluation::default(),
            Ok(&policy(64, 1_000, 1_000)),
            &LocaleFields::declared(&declaration),
        )
        .unwrap()
        .0
    }

    #[test]
    fn counts_are_checked_and_one_past_u64_is_past_every_bound() {
        assert_eq!(Tally::of(2).plus(Tally::of(3)), Tally(Some(5)));
        assert_eq!(Tally(Some(u64::MAX)).plus(Tally::of(1)), Tally(None));
        assert_eq!(Tally(None).plus(Tally::of(0)), Tally(None));
        assert_eq!(Tally::of(0).plus(Tally(None)), Tally(None));
        assert_eq!(total([]), Tally(Some(0)));
        assert_eq!(
            total([Tally::of(1), Tally::of(2), Tally::of(3)]),
            Tally(Some(6))
        );
        assert_eq!(actual(Tally(Some(9)), bound(7)), Actual::Exact(9));
        assert_eq!(actual(Tally(None), bound(7)), Actual::AtLeast(8));
    }

    #[test]
    fn a_missing_part_is_measured_only_once_what_is_visible_exceeds_the_bound() {
        let causes = stand_in(Slot::Coverage);
        assert_eq!(
            measure(Tally(Some(3)), None, bound(5)),
            Ok(Actual::Exact(3))
        );
        assert_eq!(
            measure(Tally(Some(9)), None, bound(5)),
            Ok(Actual::Exact(9))
        );
        assert_eq!(
            measure(Tally(Some(5)), Some(causes.clone()), bound(5)),
            Err(causes.clone())
        );
        assert_eq!(
            measure(Tally(Some(6)), Some(causes.clone()), bound(5)),
            Ok(Actual::AtLeast(6))
        );
        assert_eq!(
            measure(Tally(None), Some(causes), bound(5)),
            Ok(Actual::AtLeast(6))
        );
        assert_eq!(measure(Tally(None), None, bound(5)), Ok(Actual::AtLeast(6)));
    }

    #[test]
    fn a_bound_admits_its_exact_value_and_reports_the_first_over() {
        assert_eq!(
            within(MAX_LOCALE_OCCURRENCES, bound(5), Actual::Exact(5)),
            Ok(())
        );
        for actual in [Actual::Exact(6), Actual::AtLeast(6)] {
            assert_eq!(
                within(MAX_LOCALE_OCCURRENCES, bound(5), actual),
                Err(Blocking::limit(LimitExceeded {
                    bound: MAX_LOCALE_OCCURRENCES,
                    limit: bound(5),
                    actual,
                }))
            );
        }
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
            // A key and its literals, never the Intent source-locale marker.
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

    #[test]
    fn the_occurrence_bound_holds_at_its_limit_and_reports_the_first_over() {
        let declaration = declaration(&complete_config(), |_| {});
        let fields = LocaleFields::declared(&declaration);
        let mut evaluation = Evaluation::default();
        let (total, bounded) =
            occurrences(&mut evaluation, Ok(&policy(64, 12, 100)), &fields).unwrap();
        assert_eq!((total, bounded), (Some(Actual::Exact(12)), Ok(())));
        assert_eq!(waits_on(&evaluation, &occurrence_unit()), None);
        assert_eq!(issues(&evaluation), []);

        let mut evaluation = Evaluation::default();
        let (total, bounded) =
            occurrences(&mut evaluation, Ok(&policy(64, 11, 100)), &fields).unwrap();
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
    }

    #[test]
    fn an_unavailable_field_leaves_the_bound_unproved_until_the_rest_exceeds_it() {
        let declaration = declaration(&complete_config(), |_| {});
        let mut fields = LocaleFields::declared(&declaration);
        let coverage = stand_in(Slot::Coverage);
        // The two coverage selector items are the unseen part: 10 are visible.
        fields.coverage = Err(coverage.clone());
        let mut evaluation = Evaluation::default();
        let (total, bounded) =
            occurrences(&mut evaluation, Ok(&policy(64, 10, 100)), &fields).unwrap();
        assert_eq!((total, bounded), (None, Err(coverage.clone())));
        assert_eq!(
            waits_on(&evaluation, &occurrence_unit()),
            Some(units(&coverage))
        );

        let mut evaluation = Evaluation::default();
        let (total, bounded) =
            occurrences(&mut evaluation, Ok(&policy(64, 9, 100)), &fields).unwrap();
        assert_eq!(total, Some(Actual::AtLeast(10)));
        assert_eq!(evaluation.issues().count(), 1);
        let key = evaluation.issues().next().unwrap();
        assert_eq!(
            evaluation.limit(key).map(|exceeded| exceeded.actual),
            Some(Actual::AtLeast(10))
        );
        assert_eq!(units(&bounded.unwrap_err()), [occurrence_unit()]);

        // Every unavailable field's causes, together.
        let requested = stand_in(Slot::DeliveryGraph);
        fields.requested_locales = Err(requested.clone());
        let mut evaluation = Evaluation::default();
        let (_, bounded) =
            occurrences(&mut evaluation, Ok(&policy(64, 100, 100)), &fields).unwrap();
        assert_eq!(bounded, Err(coverage.union(requested)));
    }

    #[test]
    fn an_unconfirmed_policy_leaves_the_bound_not_evaluated_with_every_cause() {
        let declaration = declaration(&minimal_config(), |_| {});
        let mut fields = LocaleFields::declared(&declaration);
        let policy_causes = stand_in(Slot::Coverage);
        let mut evaluation = Evaluation::default();
        let (total, bounded) = occurrences(&mut evaluation, Err(&policy_causes), &fields).unwrap();
        assert_eq!((total, bounded), (None, Err(policy_causes.clone())));

        let target = stand_in(Slot::DeliveryPlacement);
        fields.target_profiles = Err(target.clone());
        let mut evaluation = Evaluation::default();
        let (_, bounded) = occurrences(&mut evaluation, Err(&policy_causes), &fields).unwrap();
        assert_eq!(bounded, Err(policy_causes.union(target)));
    }

    #[test]
    fn the_requested_set_is_bounded_by_its_canonical_size() {
        let requested_unit = unit(LocaleSubject::RequestedLocales, Check::WithinBound);
        let set: Admitted<LocaleSet> = Ok(LocaleSet::new(vec![
            canonical("en"),
            canonical("de"),
            canonical("fr"),
        ]));
        let mut evaluation = Evaluation::default();
        assert_eq!(
            requested(&mut evaluation, Ok(&policy(64, 100, 3)), &set).unwrap(),
            Ok(())
        );

        let mut evaluation = Evaluation::default();
        let found = requested(&mut evaluation, Ok(&policy(64, 100, 2)), &set).unwrap();
        assert_eq!(
            units(&found.unwrap_err()),
            std::slice::from_ref(&requested_unit)
        );
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

        let missing = stand_in(Slot::Coverage);
        let mut evaluation = Evaluation::default();
        assert_eq!(
            requested(
                &mut evaluation,
                Ok(&policy(64, 100, 2)),
                &Err(missing.clone())
            )
            .unwrap(),
            Err(missing.clone())
        );
        let unconfirmed = stand_in(Slot::DeliveryGraph);
        let mut evaluation = Evaluation::default();
        assert_eq!(
            requested(&mut evaluation, Err(&unconfirmed), &set).unwrap(),
            Err(unconfirmed.clone())
        );
        assert_eq!(
            waits_on(&evaluation, &requested_unit),
            Some(units(&unconfirmed))
        );
    }
}
