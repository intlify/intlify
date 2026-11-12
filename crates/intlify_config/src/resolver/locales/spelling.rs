// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Phase 7's canonicalization of locale spellings: one unit per subject, the
//! spellings that share it, the canonical form a differing spelling should
//! take, and the duplicates in a set. Where a spelling sits decides its
//! subject's kind and owner; its canonical identity, or its content when it
//! has none, decides the rest.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::locale::{
    CanonicalLocale, CanonicalizationFailure, Canonicalizer, Provider, ProviderFailure,
};
use crate::model::{FallbackCandidate, NonEmptyMap, NonEmptyVec, TargetDeclaration, TargetId};

use super::super::evaluation::{
    both, Actual, Admitted, Blocking, Check, Code, Evaluation, EvaluationInvariant, LimitExceeded,
    Notice,
};
use super::bound::MAX_LOCALE_IDENTIFIER_BYTES;
use super::{
    issue, unit, AliasLocales, Content, LocaleInvariant, LocaleKey, LocaleOccurrence, LocaleSet,
    LocaleSubject, Location, Spelled, TargetLocales,
};

/// Where a spelling sits before its own key is known: its subject's kind and
/// the owner it belongs to.
pub(super) enum Scope {
    RequestedLocale,
    DefaultSourceLocale,
    DefaultRequestedLocale,
    NegotiationAliasKey,
    NegotiationAliasValue(LocaleKey),
    MessageFallbackKey,
    TargetRequestedLocale(TargetId),
    TargetDefaultRequestedLocale(TargetId),
}

impl Scope {
    /// Whether a spelling's own key names its subject here, which several
    /// spellings can share: those of one canonical locale, or, once content
    /// keys are projected, of one projection. A single-valued field's path
    /// names its one spelling.
    const fn keyed_by_spelling(&self) -> bool {
        !matches!(
            self,
            Self::DefaultSourceLocale
                | Self::DefaultRequestedLocale
                | Self::TargetDefaultRequestedLocale(_)
        )
    }

    /// The subject of a spelling with `key` here. A single-valued field's
    /// subject is its path, whatever the spelling names.
    fn subject(self, key: LocaleKey) -> LocaleSubject {
        match self {
            Self::RequestedLocale => LocaleSubject::RequestedLocale(key),
            Self::DefaultSourceLocale => LocaleSubject::DefaultSourceLocale,
            Self::DefaultRequestedLocale => LocaleSubject::DefaultRequestedLocale,
            Self::NegotiationAliasKey => LocaleSubject::NegotiationAliasKey(key),
            Self::NegotiationAliasValue(owner) => LocaleSubject::NegotiationAliasValue(owner, key),
            Self::MessageFallbackKey => LocaleSubject::MessageFallbackKey(key),
            Self::TargetRequestedLocale(id) => LocaleSubject::TargetRequestedLocale(id, key),
            Self::TargetDefaultRequestedLocale(id) => {
                LocaleSubject::TargetDefaultRequestedLocale(id)
            }
        }
    }
}

/// The canonicalization of every spelling, which waits on the canonicalizer
/// and on the occurrence bound.
pub(super) struct Pass<'e, 'c, P: Provider> {
    pub(super) evaluation: &'e mut Evaluation,
    canonicalizer: Admitted<&'c Canonicalizer<P>>,
    occurrences: Vec<LocaleOccurrence>,
}

impl<'e, 'c, P: Provider> Pass<'e, 'c, P> {
    pub(super) fn new(
        evaluation: &'e mut Evaluation,
        canonicalizer: Admitted<&'c Canonicalizer<P>>,
    ) -> Self {
        Self {
            evaluation,
            canonicalizer,
            occurrences: Vec::new(),
        }
    }

    /// Every spelling the pass reached, in location order.
    pub(super) fn into_occurrences(mut self) -> Vec<LocaleOccurrence> {
        self.occurrences
            .sort_by(|left, right| left.location.cmp(&right.location));
        self.occurrences
    }

    /// Canonicalizes `spellings` and records one unit per subject: spellings
    /// of one subject share its result, and a spelling that differs from the
    /// canonical form makes the unit suggest it. Returns each spelling's
    /// result in order.
    pub(super) fn spell(
        &mut self,
        spellings: Vec<(Location, Scope, &str)>,
    ) -> Result<Vec<Spelled>, LocaleInvariant> {
        self.spell_after(spellings, &Ok(()))
    }

    /// The same, once `after` also holds: a spelling is canonicalized only
    /// then, and otherwise stands for itself and waits on its causes.
    fn spell_after(
        &mut self,
        spellings: Vec<(Location, Scope, &str)>,
        after: &Admitted<()>,
    ) -> Result<Vec<Spelled>, LocaleInvariant> {
        type Attempt = Admitted<Result<CanonicalLocale, Blocking>>;
        let mut groups: BTreeMap<LocaleSubject, (Attempt, bool)> = BTreeMap::new();
        let mut subjects = Vec::with_capacity(spellings.len());
        for (location, scope, raw) in spellings {
            // Past the byte bound, a subject several spellings can share
            // reports only the first-over witness (Decision 015-157).
            let shared = scope.keyed_by_spelling();
            let attempt = match both(self.canonicalizer.clone(), after.clone()) {
                Ok((canonicalizer, ())) => Ok(canonicalize(canonicalizer, raw)?.map_err(|issue| {
                    if shared {
                        issue.first_over()
                    } else {
                        issue
                    }
                })),
                Err(causes) => Err(causes),
            };
            let (key, corrected) = match &attempt {
                Ok(Ok((locale, corrected))) => (LocaleKey::Canonical(locale.clone()), *corrected),
                _ => (LocaleKey::Content(Content(Arc::from(raw))), false),
            };
            let subject = scope.subject(key.clone());
            self.occurrences.push(LocaleOccurrence {
                location,
                subject: subject.clone(),
                key: key.clone(),
                corrected,
            });
            let attempt = attempt.map(|attempt| attempt.map(|(locale, _)| locale));
            groups.entry(subject.clone()).or_insert((attempt, false)).1 |= corrected;
            subjects.push((subject, key));
        }
        let mut results = BTreeMap::new();
        for (subject, (attempt, corrected)) in groups {
            let unit = unit(subject.clone(), Check::CanonicalLocale);
            let locale = self
                .evaluation
                .check(unit.clone(), attempt, |attempt| attempt)?;
            if let (true, Ok(canonical)) = (corrected, &locale) {
                self.evaluation
                    .notice(unit, Notice::non_canonical(canonical.clone()))?;
            }
            results.insert(subject, locale);
        }
        Ok(subjects
            .into_iter()
            .map(|(subject, key)| Spelled {
                key,
                locale: results[&subject].clone(),
            })
            .collect())
    }

    pub(super) fn spell_one(
        &mut self,
        location: Location,
        scope: Scope,
        raw: &str,
    ) -> Result<Spelled, LocaleInvariant> {
        Ok(self
            .spell(vec![(location, scope, raw)])?
            .pop()
            .expect("one result per spelling"))
    }

    /// Alias keys, then each destination under its key's subject key.
    pub(super) fn aliases(
        &mut self,
        aliases: Option<&BTreeMap<String, String>>,
    ) -> Result<Vec<AliasLocales>, LocaleInvariant> {
        let Some(aliases) = aliases else {
            return Ok(Vec::new());
        };
        let keys = self.spell(
            aliases
                .keys()
                .enumerate()
                .map(|(rank, key)| {
                    (
                        Location::NegotiationAliasKey(rank),
                        Scope::NegotiationAliasKey,
                        key.as_str(),
                    )
                })
                .collect(),
        )?;
        let values = self.spell(
            aliases
                .values()
                .zip(&keys)
                .enumerate()
                .map(|(rank, (value, key))| {
                    (
                        Location::NegotiationAliasValue(rank),
                        Scope::NegotiationAliasValue(key.key.clone()),
                        value.as_str(),
                    )
                })
                .collect(),
        )?;
        Ok(keys
            .into_iter()
            .zip(values)
            .map(|(key, value)| AliasLocales { key, value })
            .collect())
    }

    /// Fallback keys, in the order the map holds them. Their candidates are
    /// Phase 8's: it checks a key's sequence only once the key has an
    /// identity no other key shares.
    pub(super) fn fallback_keys(
        &mut self,
        fallback: Option<&BTreeMap<String, NonEmptyVec<FallbackCandidate>>>,
    ) -> Result<Vec<Spelled>, LocaleInvariant> {
        let Some(fallback) = fallback else {
            return Ok(Vec::new());
        };
        self.spell(
            fallback
                .keys()
                .enumerate()
                .map(|(rank, key)| {
                    (
                        Location::MessageFallbackKey(rank),
                        Scope::MessageFallbackKey,
                        key.as_str(),
                    )
                })
                .collect(),
        )
    }

    /// Each target's requested locales, checked for a locale named twice, and
    /// its default, once the target's bounds hold: `admitted` has each Target
    /// ID's result, which includes the map's.
    pub(super) fn targets<Target>(
        &mut self,
        targets: &NonEmptyMap<TargetId, TargetDeclaration<Target>>,
        admitted: &BTreeMap<TargetId, Admitted<()>>,
    ) -> Result<Vec<TargetLocales>, LocaleInvariant> {
        let mut resolved = Vec::with_capacity(targets.len());
        for (id, target) in targets.iter() {
            let own = admitted
                .get(id)
                .expect("every declared Target ID has its bound's result")
                .clone();
            let requested = self.spell_after(
                target
                    .requested_locales
                    .as_slice()
                    .iter()
                    .enumerate()
                    .map(|(index, raw)| {
                        (
                            Location::TargetRequestedLocale(id.clone(), index),
                            Scope::TargetRequestedLocale(id.clone()),
                            raw.as_str(),
                        )
                    })
                    .collect(),
                &own,
            )?;
            // A repeated member is an issue of its own; the set still holds
            // the canonical identities Phase 11 compares.
            let distinct = unique(self.evaluation, &requested, |key| {
                LocaleSubject::TargetRequestedLocale(id.clone(), key)
            })?;
            let default = match target.default_requested_locale.as_option() {
                Some(raw) => self
                    .spell_after(
                        vec![(
                            Location::TargetDefaultRequestedLocale(id.clone()),
                            Scope::TargetDefaultRequestedLocale(id.clone()),
                            raw,
                        )],
                        &own,
                    )?
                    .pop()
                    .expect("one result per spelling")
                    .locale
                    .map(Some),
                None => Ok(None),
            };
            resolved.push(TargetLocales {
                id: id.clone(),
                admitted: own,
                requested: set(&requested),
                distinct,
                default,
            });
        }
        Ok(resolved)
    }
}

/// Validates and canonicalizes one spelling: its canonical locale and whether
/// the spelling differs from it, or the issue an author can fix.
pub(crate) fn canonicalize<P: Provider>(
    canonicalizer: &Canonicalizer<P>,
    raw: &str,
) -> Result<Result<(CanonicalLocale, bool), Blocking>, LocaleInvariant> {
    match canonicalizer.canonicalize(raw) {
        Ok(canonicalized) => {
            let corrected = canonicalized.suggested_replacement().is_some();
            Ok(Ok((canonicalized.into_locale(), corrected)))
        }
        Err(CanonicalizationFailure::ByteLimit { limit, actual, .. }) => {
            Ok(Err(Blocking::limit(LimitExceeded {
                bound: MAX_LOCALE_IDENTIFIER_BYTES,
                limit,
                actual: Actual::Exact(actual),
            })))
        }
        Err(CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier)) => {
            Ok(Err(issue(Code::LocaleInvalid)))
        }
        Err(failure) => Err(LocaleInvariant::Canonicalization(failure)),
    }
}

/// One unit per subject of a set's spellings: two spellings naming one locale
/// are a duplicate, equal or aliases alike. A subject without a canonical
/// locale leaves its unit not evaluated.
pub(super) fn unique(
    evaluation: &mut Evaluation,
    spelled: &[Spelled],
    subject: impl Fn(LocaleKey) -> LocaleSubject,
) -> Result<Admitted<()>, EvaluationInvariant> {
    let mut members: BTreeMap<&LocaleKey, (usize, &Admitted<CanonicalLocale>)> = BTreeMap::new();
    for spelling in spelled {
        members
            .entry(&spelling.key)
            .or_insert((0, &spelling.locale))
            .0 += 1;
    }
    let mut all = Ok(());
    for (key, (spellings, locale)) in members {
        let checked = evaluation.check(
            unit(subject(key.clone()), Check::UniqueLocale),
            locale.clone(),
            |_| {
                if spellings == 1 {
                    Ok(())
                } else {
                    Err(issue(Code::LocaleDuplicate))
                }
            },
        )?;
        all = both(all, checked).map(|((), ())| ());
    }
    Ok(all)
}

/// The set of the spellings' canonical locales, once each has one; otherwise
/// the causes of every spelling without one, not just the first.
pub(super) fn set(spelled: &[Spelled]) -> Admitted<LocaleSet> {
    let mut locales = Ok(Vec::with_capacity(spelled.len()));
    for spelling in spelled {
        locales = both(locales, spelling.locale.clone()).map(|(mut locales, locale)| {
            locales.push(locale);
            locales
        });
    }
    locales.map(LocaleSet::new)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::super::fixtures::{
        canonical, canonicalizer, content, issues, key, notices, rejected, spelled, stand_in,
        target, units, waits_on,
    };
    use super::*;
    use crate::input_limits::Bound;
    use crate::locale::fixtures::FixtureProvider;
    use crate::model::NonEmptyMap;
    use crate::resolver::evaluation::Unit;
    use crate::resolver::specification::Slot;

    fn requested(spellings: &[&'static str]) -> Vec<(Location, Scope, &'static str)> {
        spellings
            .iter()
            .enumerate()
            .map(|(index, raw)| {
                (
                    Location::RequestedLocale(index),
                    Scope::RequestedLocale,
                    *raw,
                )
            })
            .collect()
    }

    fn canonical_unit(subject: LocaleSubject) -> Unit {
        unit(subject, Check::CanonicalLocale)
    }

    fn requested_unit(key: LocaleKey) -> Unit {
        canonical_unit(LocaleSubject::RequestedLocale(key))
    }

    /// Spells `spellings` as project requested locales through `canonicalizer`.
    fn spell(
        evaluation: &mut Evaluation,
        canonicalizer: Admitted<&Canonicalizer<FixtureProvider>>,
        spellings: &[&'static str],
    ) -> (Result<Vec<Spelled>, LocaleInvariant>, Vec<LocaleOccurrence>) {
        let mut pass = Pass::new(evaluation, canonicalizer);
        let spelled = pass.spell(requested(spellings));
        (spelled, pass.into_occurrences())
    }

    fn keys(spelled: &[Spelled]) -> Vec<LocaleKey> {
        spelled
            .iter()
            .map(|spelling| spelling.key.clone())
            .collect()
    }

    #[test]
    fn spellings_of_one_locale_share_a_unit_and_a_differing_one_is_corrected() {
        let canonicalizer = canonicalizer(64);
        let mut evaluation = Evaluation::default();
        let (spelled, occurrences) =
            spell(&mut evaluation, Ok(&canonicalizer), &["EN", "de", "en"]);
        let spelled = spelled.unwrap();
        assert_eq!(keys(&spelled), [key("en"), key("de"), key("en")]);
        assert_eq!(
            spelled
                .iter()
                .map(|spelling| spelling.locale.clone())
                .collect::<Vec<_>>(),
            [
                Ok(canonical("en")),
                Ok(canonical("de")),
                Ok(canonical("en"))
            ]
        );
        assert_eq!(
            evaluation
                .statuses()
                .map(|(unit, _)| unit.clone())
                .collect::<Vec<_>>(),
            [requested_unit(key("de")), requested_unit(key("en"))]
        );
        assert_eq!(issues(&evaluation), []);
        assert_eq!(notices(&evaluation), [requested_unit(key("en"))]);
        assert_eq!(
            occurrences
                .iter()
                .map(|occurrence| (occurrence.location.clone(), occurrence.corrected))
                .collect::<Vec<_>>(),
            [
                (Location::RequestedLocale(0), true),
                (Location::RequestedLocale(1), false),
                (Location::RequestedLocale(2), false),
            ]
        );
    }

    /// Each rejected spelling keeps its own key, but every one joins the one
    /// unit of its scope that a spelling without an identity has (Decision
    /// 015-174), so they give one issue.
    #[test]
    fn rejected_spellings_share_one_unit_and_are_an_issue_an_author_can_fix() {
        let canonicalizer = canonicalizer(64);
        let mut evaluation = Evaluation::default();
        let (spelled, occurrences) = spell(
            &mut evaluation,
            Ok(&canonicalizer),
            &["zz", "en-x-brand", "zz"],
        );
        let spelled = spelled.unwrap();
        assert_eq!(
            keys(&spelled),
            [content("zz"), content("en-x-brand"), content("zz")]
        );
        assert_eq!(
            issues(&evaluation),
            [(
                requested_unit(rejected()),
                "project-profile-locale-invalid",
                None,
            )]
        );
        assert_eq!(
            units(spelled[0].locale.as_ref().unwrap_err()),
            [requested_unit(rejected())]
        );
        assert_eq!(spelled[0].locale, spelled[2].locale);
        assert_eq!(notices(&evaluation), []);
        assert!(occurrences.iter().all(|occurrence| !occurrence.corrected));
    }

    #[test]
    fn identifier_bytes_bound_the_raw_and_then_the_canonical_spelling() {
        let exceeded = |limit: u64, actual| {
            Some(LimitExceeded {
                bound: MAX_LOCALE_IDENTIFIER_BYTES,
                limit: Bound::new(limit).unwrap(),
                actual,
            })
        };
        // A requested locale's key can stand for several spellings, so past
        // the bound it reports only the first-over witness.
        let over = |limit: u64| {
            (
                requested_unit(rejected()),
                "project-profile-resource-limit-exceeded",
                exceeded(limit, Actual::AtLeast(limit + 1)),
            )
        };
        // The raw spelling is checked before the provider sees it.
        for (limit, found) in [(5, vec![]), (4, vec![over(4)]), (2, vec![over(2)])] {
            let canonicalizer = canonicalizer(limit);
            let mut evaluation = Evaluation::default();
            spell(&mut evaluation, Ok(&canonicalizer), &["EN-us"])
                .0
                .unwrap();
            assert_eq!(issues(&evaluation), found, "{limit}");
        }
        // 17 bytes become 22, and the canonical spelling is checked too.
        for (limit, found) in [(22, vec![]), (21, vec![over(21)])] {
            let canonicalizer = canonicalizer(limit);
            let mut evaluation = Evaluation::default();
            spell(&mut evaluation, Ok(&canonicalizer), &["und-u-ca-islamicc"])
                .0
                .unwrap();
            assert_eq!(issues(&evaluation), found, "{limit}");
        }
        // A single-valued field's path names its one spelling, whose exact
        // length it reports.
        let canonicalizer = canonicalizer(2);
        let mut evaluation = Evaluation::default();
        {
            let mut pass = Pass::new(&mut evaluation, Ok(&canonicalizer));
            pass.spell_one(
                Location::DefaultSourceLocale,
                Scope::DefaultSourceLocale,
                "EN-us",
            )
            .unwrap();
        }
        assert_eq!(
            issues(&evaluation),
            [(
                canonical_unit(LocaleSubject::DefaultSourceLocale),
                "project-profile-resource-limit-exceeded",
                exceeded(2, Actual::Exact(5)),
            )]
        );
    }

    /// Every spelling is then without an identity, so all of them are
    /// candidates of one unit, which none of them can evaluate.
    #[test]
    fn without_a_canonicalizer_every_spelling_shares_one_unit_not_evaluated() {
        let causes = stand_in(Slot::LocaleCanonicalization);
        let mut evaluation = Evaluation::default();
        let (spelled, occurrences) =
            spell(&mut evaluation, Err(causes.clone()), &["EN", "EN", "zz"]);
        let spelled = spelled.unwrap();
        assert_eq!(
            keys(&spelled),
            [content("EN"), content("EN"), content("zz")]
        );
        assert!(spelled
            .iter()
            .all(|spelling| spelling.locale == Err(causes.clone())));
        for raw in ["EN", "zz"] {
            assert_eq!(
                waits_on(&evaluation, &requested_unit(content(raw))),
                Some(units(&causes))
            );
        }
        assert_eq!(evaluation.statuses().count(), 1);
        assert_eq!(evaluation.candidates().count(), 2);
        assert_eq!(issues(&evaluation), []);
        assert_eq!(notices(&evaluation), []);
        assert!(occurrences.iter().all(|occurrence| !occurrence.corrected));
    }

    #[test]
    fn failures_no_author_can_fix_stop_the_pass() {
        // The finite provider does not know `pt-BR`: a gap in the fixture, not
        // an invalid locale.
        let canonicalizer = canonicalizer(64);
        let mut evaluation = Evaluation::default();
        let (spelled, _) = spell(&mut evaluation, Ok(&canonicalizer), &["en", "pt-BR"]);
        assert_eq!(
            spelled.err(),
            Some(LocaleInvariant::Canonicalization(
                CanonicalizationFailure::Provider(ProviderFailure::UnsupportedInput)
            ))
        );
    }

    #[test]
    fn a_locale_two_spellings_name_is_a_duplicate_whether_equal_or_aliases() {
        let canonicalizer = canonicalizer(64);
        let mut evaluation = Evaluation::default();
        let spelled = spell(
            &mut evaluation,
            Ok(&canonicalizer),
            &["en", "iw-IL", "EN", "de", "he-IL", "zz"],
        )
        .0
        .unwrap();
        let distinct = unique(&mut evaluation, &spelled, LocaleSubject::RequestedLocale).unwrap();
        let unique_unit = |key| unit(LocaleSubject::RequestedLocale(key), Check::UniqueLocale);
        let duplicate = |key| (unique_unit(key), "project-profile-locale-duplicate", None);
        let found: Vec<_> = issues(&evaluation)
            .into_iter()
            .filter(|(unit, _, _)| unit.check == Check::UniqueLocale)
            .collect();
        assert_eq!(found, [duplicate(key("en")), duplicate(key("he-IL"))]);
        assert_eq!(waits_on(&evaluation, &unique_unit(key("de"))), None);
        // Without a canonical identity, a spelling cannot repeat another.
        assert_eq!(
            waits_on(&evaluation, &unique_unit(content("zz"))),
            Some(vec![requested_unit(rejected())])
        );
        // Canonical keys order before contents, whatever the check.
        assert_eq!(
            units(&distinct.unwrap_err()),
            [
                unique_unit(key("en")),
                unique_unit(key("he-IL")),
                requested_unit(rejected()),
            ]
        );
    }

    /// A set without every identity waits on the one unit that every
    /// spelling without an identity shares.
    #[test]
    fn a_set_needs_every_canonical_locale_and_waits_on_the_spellings_without_one() {
        let canonicalizer = canonicalizer(64);
        let mut evaluation = Evaluation::default();
        let spelled = spell(
            &mut evaluation,
            Ok(&canonicalizer),
            &["fr", "en", "de", "EN"],
        )
        .0
        .unwrap();
        assert_eq!(
            set(&spelled).unwrap().as_slice(),
            ["de", "en", "fr"].map(canonical)
        );
        let mut evaluation = Evaluation::default();
        let spelled = spell(
            &mut evaluation,
            Ok(&canonicalizer),
            &["de", "zz", "en", "Latn"],
        )
        .0
        .unwrap();
        assert_eq!(
            units(&set(&spelled).unwrap_err()),
            [requested_unit(rejected())]
        );
        assert_eq!(set(&[]).map(|set| set.as_slice().len()), Ok(0));
    }

    #[test]
    fn alias_destinations_sit_under_their_keys_and_fallback_keys_are_spelled_alone() {
        let canonicalizer = canonicalizer(64);
        let mut evaluation = Evaluation::default();
        let mut pass = Pass::new(&mut evaluation, Ok(&canonicalizer));
        let aliases = BTreeMap::from([
            ("EN".to_string(), "fr-FR".to_string()),
            ("de".to_string(), "zz".to_string()),
        ]);
        let resolved = pass.aliases(Some(&aliases)).unwrap();
        assert_eq!(
            resolved,
            [
                AliasLocales {
                    key: spelled("en"),
                    value: spelled("fr-FR"),
                },
                AliasLocales {
                    key: spelled("de"),
                    value: resolved[1].value.clone(),
                },
            ]
        );
        // A rejected destination keeps its spelling as its subject key.
        assert_eq!(resolved[1].value.key, content("zz"));
        assert!(resolved[1].value.locale.is_err());
        let sequences: BTreeMap<String, NonEmptyVec<FallbackCandidate>> =
            serde_json::from_value(serde_json::json!({
                "ja": ["EN", {"kind": "intent-source-locale"}, "de"],
                "zz": ["en"]
            }))
            .unwrap();
        let keys = pass.fallback_keys(Some(&sequences)).unwrap();
        assert_eq!(pass.aliases(None).unwrap(), []);
        assert_eq!(pass.fallback_keys(None).unwrap(), []);
        let occurrences = pass.into_occurrences();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0], spelled("ja"));
        assert_eq!(keys[1].key, content("zz"));
        assert!(keys[1].locale.is_err());
        // The candidates are Phase 8's: none is spelled here, not even `EN`,
        // which differs from its canonical form.
        assert_eq!(
            occurrences
                .iter()
                .map(|occurrence| (
                    occurrence.location.clone(),
                    occurrence.subject.clone(),
                    occurrence.corrected
                ))
                .collect::<Vec<_>>(),
            [
                (
                    Location::NegotiationAliasKey(0),
                    LocaleSubject::NegotiationAliasKey(key("en")),
                    true
                ),
                (
                    Location::NegotiationAliasKey(1),
                    LocaleSubject::NegotiationAliasKey(key("de")),
                    false
                ),
                (
                    Location::NegotiationAliasValue(0),
                    LocaleSubject::NegotiationAliasValue(key("en"), key("fr-FR")),
                    false
                ),
                (
                    Location::NegotiationAliasValue(1),
                    LocaleSubject::NegotiationAliasValue(key("de"), content("zz")),
                    false
                ),
                (
                    Location::MessageFallbackKey(0),
                    LocaleSubject::MessageFallbackKey(key("ja")),
                    false
                ),
                (
                    Location::MessageFallbackKey(1),
                    LocaleSubject::MessageFallbackKey(content("zz")),
                    false
                ),
            ]
        );
        assert_eq!(
            notices(&evaluation),
            [canonical_unit(LocaleSubject::NegotiationAliasKey(key(
                "en"
            )))]
        );
        assert_eq!(evaluation.statuses().count(), 6);
    }

    #[test]
    fn colliding_keys_share_one_subject_and_phase_7_reports_no_collision() {
        let canonicalizer = canonicalizer(64);
        let mut evaluation = Evaluation::default();
        let mut pass = Pass::new(&mut evaluation, Ok(&canonicalizer));
        let aliases = BTreeMap::from([
            ("EN".to_string(), "fr".to_string()),
            ("en".to_string(), "de".to_string()),
        ]);
        let resolved = pass.aliases(Some(&aliases)).unwrap();
        assert_eq!(resolved.len(), 2);
        let occurrences = pass.into_occurrences();
        assert_eq!(
            occurrences[0].subject,
            LocaleSubject::NegotiationAliasKey(key("en"))
        );
        assert_eq!(occurrences[0].subject, occurrences[1].subject);
        // Phase 8 reports a key named twice; here each destination keeps its
        // own subject under the shared key.
        assert_eq!(issues(&evaluation), []);
        assert_eq!(
            evaluation
                .statuses()
                .map(|(unit, _)| unit.clone())
                .collect::<Vec<_>>(),
            [
                canonical_unit(LocaleSubject::NegotiationAliasKey(key("en"))),
                canonical_unit(LocaleSubject::NegotiationAliasValue(key("en"), key("de"))),
                canonical_unit(LocaleSubject::NegotiationAliasValue(key("en"), key("fr"))),
            ]
        );
    }

    #[test]
    fn each_target_keeps_its_own_set_and_default() {
        let targets: NonEmptyMap<TargetId, TargetDeclaration<serde_json::Value>> =
            serde_json::from_value(serde_json::json!({
                "server": {"profile": {}, "requestedLocales": ["de"]},
                "browser": {
                    "profile": {},
                    "requestedLocales": ["EN", "en", "de"],
                    "defaultRequestedLocale": "de"
                }
            }))
            .unwrap();
        let canonicalizer = canonicalizer(64);
        let mut evaluation = Evaluation::default();
        let mut pass = Pass::new(&mut evaluation, Ok(&canonicalizer));
        let admitted = targets.iter().map(|(id, _)| (id.clone(), Ok(()))).collect();
        let resolved = pass.targets(&targets, &admitted).unwrap();
        drop(pass);
        let duplicate = unit(
            LocaleSubject::TargetRequestedLocale(target("browser"), key("en")),
            Check::UniqueLocale,
        );
        // The browser's set still holds `en` once; the repeat leaves only the
        // set's distinctness unavailable.
        assert_eq!(
            resolved
                .iter()
                .map(|target| (
                    target.id.clone(),
                    target.requested.clone(),
                    target.distinct.as_ref().map_err(units).copied(),
                    target.default.clone(),
                ))
                .collect::<Vec<_>>(),
            [
                (
                    target("browser"),
                    Ok(LocaleSet::new(vec![canonical("de"), canonical("en")])),
                    Err(vec![duplicate]),
                    Ok(Some(canonical("de"))),
                ),
                (
                    target("server"),
                    Ok(LocaleSet::new(vec![canonical("de")])),
                    Ok(()),
                    Ok(None),
                ),
            ]
        );
        assert_eq!(
            issues(&evaluation),
            [(
                unit(
                    LocaleSubject::TargetRequestedLocale(target("browser"), key("en")),
                    Check::UniqueLocale
                ),
                "project-profile-locale-duplicate",
                None
            )]
        );
        // The same locale in another target's set is not a duplicate.
        assert_eq!(
            waits_on(
                &evaluation,
                &unit(
                    LocaleSubject::TargetRequestedLocale(target("server"), key("de")),
                    Check::UniqueLocale
                )
            ),
            None
        );
    }

    #[test]
    fn a_target_whose_bounds_do_not_hold_keeps_its_spellings_as_they_are() {
        let targets: NonEmptyMap<TargetId, TargetDeclaration<serde_json::Value>> =
            serde_json::from_value(serde_json::json!({
                "server": {"profile": {}, "requestedLocales": ["de"]},
                "browser": {
                    "profile": {},
                    "requestedLocales": ["EN", "en"],
                    "defaultRequestedLocale": "de"
                }
            }))
            .unwrap();
        // Stands in for the browser's Target ID bound.
        let causes = stand_in(Slot::DeliveryGraph);
        let admitted = BTreeMap::from([
            (target("browser"), Err(causes.clone())),
            (target("server"), Ok(())),
        ]);
        let canonicalizer = canonicalizer(64);
        let mut evaluation = Evaluation::default();
        let mut pass = Pass::new(&mut evaluation, Ok(&canonicalizer));
        let resolved = pass.targets(&targets, &admitted).unwrap();
        drop(pass);
        // Each of the browser's spellings stands for itself and waits on its
        // bound, so neither the repeat nor the casing is found.
        for subject in [
            LocaleSubject::TargetRequestedLocale(target("browser"), content("EN")),
            LocaleSubject::TargetRequestedLocale(target("browser"), content("en")),
            LocaleSubject::TargetDefaultRequestedLocale(target("browser")),
        ] {
            assert_eq!(
                waits_on(&evaluation, &canonical_unit(subject.clone())),
                Some(units(&causes)),
                "{subject:?}"
            );
        }
        assert_eq!(issues(&evaluation), []);
        assert_eq!(notices(&evaluation), []);
        let server = LocaleSubject::TargetRequestedLocale(target("server"), key("de"));
        assert_eq!(waits_on(&evaluation, &canonical_unit(server)), None);
        assert_eq!(
            resolved
                .iter()
                .map(|target| (
                    target.id.clone(),
                    target.admitted.clone(),
                    target.requested.clone(),
                    target.distinct.clone(),
                    target.default.clone(),
                ))
                .collect::<Vec<_>>(),
            [
                (
                    target("browser"),
                    Err(causes.clone()),
                    Err(causes.clone()),
                    Err(causes.clone()),
                    Err(causes),
                ),
                (
                    target("server"),
                    Ok(()),
                    Ok(LocaleSet::new(vec![canonical("de")])),
                    Ok(()),
                    Ok(None),
                ),
            ]
        );
    }
}
