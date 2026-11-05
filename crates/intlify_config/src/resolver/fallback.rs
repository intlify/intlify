// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 8 for the selected declaration's message fallback: the mapping
//! under `maxFallbackSources`, a source two keys name, and a source outside
//! the project's requested locales; then, for each key with an identity no
//! other key shares, its sequence under `maxFallbackCandidatesPerSource`,
//! each literal candidate validated and canonicalized, and a sequence that
//! names its own source, repeats a literal, or repeats the Intent
//! source-locale candidate, all under the admitted Message Locale Fallback
//! Specification. A candidate's own checks name it by its position in its
//! key's sequence, and the repeat checks by its canonical identity or, for
//! the Intent source-locale candidate, by its fixed kind. Colliding keys and
//! a key without an identity leave their sequences unchecked: one position
//! under such a key would merge distinct candidates. A sequence keeps the
//! order it was declared in: nothing here deduplicates, reorders, or expands
//! it through another source's sequence. Not the Linker's probing of
//! definitions, which 020 owns, and not the canonicalization of the keys,
//! which Phase 7 did.

use std::collections::BTreeMap;

use crate::locale::{CanonicalLocale, Canonicalizer, Provider, VersionedIdentity};
use crate::model::{FallbackCandidate, NonEmptyVec};

use super::evaluation::{
    and, both, within, Admitted, Blocking, Causes, Check, Code, Evaluation, Phase, Reason, Subject,
    Unit,
};
use super::limits::{BoundId, ConfirmedPolicy, Group};
use super::locales::{
    canonicalize, Content, LocaleInvariant, LocaleKey, LocaleSet, ResolvedLocales, Spelled,
};

const MAX_FALLBACK_SOURCES: BoundId = BoundId {
    group: Group::LocaleResolution,
    name: "maxFallbackSources",
};
const MAX_FALLBACK_CANDIDATES_PER_SOURCE: BoundId = BoundId {
    group: Group::LocaleResolution,
    name: "maxFallbackCandidatesPerSource",
};

/// What one fallback unit checks. Kinds are declared so that each unit's
/// prerequisites come first: the mapping's bound, each key, its sequence's
/// bound, each literal candidate at its position, then the repeats, which
/// need the candidates' identities.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FallbackSubject {
    /// The submitted mapping, under `maxFallbackSources`.
    Sources,
    /// A mapping key: the source its sequence falls back from.
    Source(LocaleKey),
    /// A key's sequence, under `maxFallbackCandidatesPerSource`.
    Sequence(LocaleKey),
    /// A literal candidate at its position in its key's sequence.
    Candidate(LocaleKey, usize),
    /// A literal candidate's locale, in its key's sequence.
    Literal(LocaleKey, LocaleKey),
    /// The Intent source-locale candidate, in its key's sequence.
    IntentSource(LocaleKey),
}

/// The unit Phase 8 records for `check` over `subject`.
pub(crate) fn unit(subject: FallbackSubject, check: Check) -> Unit {
    Unit {
        phase: Phase::LocalePolicyResolution,
        subject: Subject::Fallback(subject),
        check,
    }
}

/// One candidate of a checked sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Candidate {
    /// A definition locale, which need not be a requested locale.
    Literal(CanonicalLocale),
    /// The source locale of each Intent, which 020 resolves per Intent.
    IntentSource,
}

/// The checked message fallback: the admitted specification and each
/// source's complete sequence, sources in ascending unsigned UTF-8 byte order
/// and each sequence in its declared order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MessageFallbackPolicy<Id> {
    specification: VersionedIdentity<Id>,
    sequences: BTreeMap<CanonicalLocale, Vec<Candidate>>,
}

impl<Id> MessageFallbackPolicy<Id> {
    pub(crate) const fn specification(&self) -> &VersionedIdentity<Id> {
        &self.specification
    }

    pub(crate) const fn sequences(&self) -> &BTreeMap<CanonicalLocale, Vec<Candidate>> {
        &self.sequences
    }

    /// Every literal candidate once, as a set: the definition locales the
    /// policy may ask a Store for.
    pub(crate) fn definition_locales(&self) -> LocaleSet {
        LocaleSet::new(
            self.sequences
                .values()
                .flatten()
                .filter_map(|candidate| match candidate {
                    Candidate::Literal(locale) => Some(locale.clone()),
                    Candidate::IntentSource => None,
                })
                .collect(),
        )
    }
}

/// A submitted key as Phase 7 spelled it, with its sequence as submitted.
struct Entry<'a> {
    key: &'a Spelled,
    candidates: &'a [FallbackCandidate],
}

/// Phase 8's fallback checks over the keys Phase 7 spelled from `fallback`
/// and the sequences `fallback` holds, with the canonicalizer Phase 5 bound.
/// Every unit waits on the mapping's bound; a sequence's units wait on its
/// key holding alone with an identity, and its candidates on the sequence's
/// bound. The policy exists only when every unit passed.
pub(crate) fn resolve_fallback<Id: Clone, P: Provider>(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    specification: &Admitted<VersionedIdentity<Id>>,
    canonicalizer: Result<&Canonicalizer<P>, &Causes>,
    locales: &ResolvedLocales,
    fallback: Admitted<Option<&BTreeMap<String, NonEmptyVec<FallbackCandidate>>>>,
) -> Result<Admitted<MessageFallbackPolicy<Id>>, LocaleInvariant> {
    let policy = policy.map_err(Causes::clone);
    let keys = locales.fallback_keys.as_ref().map_err(Causes::clone);
    let bounded = evaluation.check(
        unit(FallbackSubject::Sources, Check::WithinBound),
        both(both(policy.clone(), specification.clone()), keys.clone()),
        |((policy, _), keys)| {
            within(
                MAX_FALLBACK_SOURCES,
                policy.limits().locale_resolution.max_fallback_sources,
                keys.len(),
            )
        },
    )?;
    let entries = entries(keys.map_or(&[][..], Vec::as_slice), fallback);
    // As in Phase 7, nothing is canonicalized unless the occurrence bound,
    // which counts every literal candidate, holds.
    let spelling = both(
        canonicalizer.map_err(Causes::clone),
        locales.occurrence_bound.clone(),
    )
    .map(|(canonicalizer, ())| canonicalizer);
    let requested = locales.requested_set.as_ref().map_err(Causes::clone);
    let mut checked = bounded.clone();
    let mut sequences = Ok(BTreeMap::new());
    for (key, sharing) in by_source(&entries) {
        // Entries under one subject share their key's result.
        let spelled = sharing[0].key;
        let alone = evaluation.check(
            unit(FallbackSubject::Source(key.clone()), Check::UniqueLocale),
            both(bounded.clone(), spelled.locale.clone()),
            |((), locale)| {
                if sharing.len() == 1 {
                    Ok(locale)
                } else {
                    Err(issue(Code::LocaleDuplicate, None))
                }
            },
        )?;
        let member = evaluation.check(
            unit(FallbackSubject::Source(key.clone()), Check::RequestedMember),
            both(
                both(bounded.clone(), spelled.locale.clone()),
                requested.clone(),
            ),
            |(((), locale), requested)| {
                if requested.contains(&locale) {
                    Ok(())
                } else {
                    Err(issue(Code::MessageFallbackSourceNotRequested, None))
                }
            },
        )?;
        // A key holding alone has one sequence, the one its bound measures.
        let sequence = evaluation.check(
            unit(FallbackSubject::Sequence(key.clone()), Check::WithinBound),
            both(alone.clone(), policy.clone()),
            |(source, policy)| {
                within(
                    MAX_FALLBACK_CANDIDATES_PER_SOURCE,
                    policy
                        .limits()
                        .locale_resolution
                        .max_fallback_candidates_per_source,
                    sharing[0].candidates.len(),
                )
                .map(|()| source)
            },
        )?;
        let (candidates, resolved) = candidates(evaluation, &spelling, key, &sequence, &sharing)?;
        checked = and(and(checked, and(alone.map(|_| ()), member)), candidates);
        sequences = both(sequences, both(sequence, resolved)).map(
            |(mut sequences, (source, candidates))| {
                sequences.insert(source, candidates);
                sequences
            },
        );
    }
    Ok(both(both(specification.clone(), checked), sequences).map(
        |((specification, ()), sequences)| MessageFallbackPolicy {
            specification,
            sequences,
        },
    ))
}

/// Each key Phase 7 spelled with its sequence as submitted, in the map's
/// order: Phase 7 spelled the keys of this same map.
fn entries<'a>(
    keys: &'a [Spelled],
    fallback: Admitted<Option<&'a BTreeMap<String, NonEmptyVec<FallbackCandidate>>>>,
) -> Vec<Entry<'a>> {
    let sequences: Vec<&'a [FallbackCandidate]> =
        fallback.ok().flatten().map_or_else(Vec::new, |map| {
            map.values().map(NonEmptyVec::as_slice).collect()
        });
    assert_eq!(
        keys.len(),
        sequences.len(),
        "Phase 7 spelled every key of the map"
    );
    keys.iter()
        .zip(sequences)
        .map(|(key, candidates)| Entry { key, candidates })
        .collect()
}

/// The submitted entries grouped by their key's subject, in subject order.
fn by_source<'e, 'a>(entries: &'e [Entry<'a>]) -> BTreeMap<&'a LocaleKey, Vec<&'e Entry<'a>>> {
    let mut sources: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for entry in entries {
        sources.entry(&entry.key.key).or_default().push(entry);
    }
    sources
}

/// The units over one key's candidates, which wait on its sequence: each
/// literal canonicalized at its position and never its source, and each
/// literal identity and the Intent source-locale candidate never repeated. A
/// key several entries share has no sequence checked, so its positions merge
/// their candidates into units none of which is evaluated. Returns whether
/// every unit passed and the sequence as checked candidates.
fn candidates<P: Provider>(
    evaluation: &mut Evaluation,
    spelling: &Admitted<&Canonicalizer<P>>,
    key: &LocaleKey,
    sequence: &Admitted<CanonicalLocale>,
    sharing: &[&Entry<'_>],
) -> Result<(Admitted<()>, Admitted<Vec<Candidate>>), LocaleInvariant> {
    let canonicalizing =
        both(spelling.clone(), sequence.clone()).map(|(canonicalizer, _)| canonicalizer);
    let after = sequence.clone().map(|_| ());
    // The literals at each position, and the Intent source-locale candidates.
    let mut positions: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
    let mut intents = 0;
    for entry in sharing {
        for (position, candidate) in entry.candidates.iter().enumerate() {
            match candidate {
                FallbackCandidate::Locale(raw) => positions.entry(position).or_default().push(raw),
                FallbackCandidate::IntentSource(_) => intents += 1,
            }
        }
    }
    let mut checked = after.clone();
    let mut found = BTreeMap::new();
    // Each literal's subject key, what its repeat check waits on, and how
    // many candidates hold it: the one sequence's, whenever the check runs.
    let mut literals: BTreeMap<LocaleKey, (Admitted<()>, usize)> = BTreeMap::new();
    for (position, raws) in positions {
        let subject = FallbackSubject::Candidate(key.clone(), position);
        let at = literal_at(evaluation, &canonicalizing, &subject, &raws, sequence)?;
        let identified = at.locale.clone().map(|_| ());
        for identity in at.identities {
            let (waits, holding) = literals.entry(identity).or_insert((Ok(()), 0));
            *waits = and(waits.clone(), identified.clone());
            *holding += 1;
        }
        checked = and(checked, at.checked);
        found.insert(position, at.locale);
    }
    for (identity, (waits, holding)) in literals {
        let once = evaluation.check(
            unit(
                FallbackSubject::Literal(key.clone(), identity),
                Check::UniqueCandidate,
            ),
            and(after.clone(), waits),
            |()| repeated(holding, Reason::LiteralDuplicate),
        )?;
        checked = and(checked, once);
    }
    if intents > 0 {
        let once = evaluation.check(
            unit(
                FallbackSubject::IntentSource(key.clone()),
                Check::UniqueCandidate,
            ),
            after,
            |()| repeated(intents, Reason::IntentSourceDuplicate),
        )?;
        checked = and(checked, once);
    }
    let resolved = match sharing {
        [entry] => resolved(entry, &found),
        // Keys several entries share check no sequence.
        _ => sequence.clone().map(|_| Vec::new()),
    };
    Ok((checked, resolved))
}

/// What the units at one position found.
struct LiteralAt {
    /// The literal's canonical locale, or why it has none.
    locale: Admitted<CanonicalLocale>,
    /// Whether both units passed.
    checked: Admitted<()>,
    /// The subject keys the repeat check names the position's literals by:
    /// the canonical identity, or each spelling when it has none.
    identities: Vec<LocaleKey>,
}

/// The two units at one position: the literal canonicalized, its canonical
/// form suggested when it is spelled otherwise, then compared with its
/// source. They are evaluated only under a key holding alone, which has one
/// literal at the position.
fn literal_at<P: Provider>(
    evaluation: &mut Evaluation,
    canonicalizing: &Admitted<&Canonicalizer<P>>,
    subject: &FallbackSubject,
    raws: &[&str],
    sequence: &Admitted<CanonicalLocale>,
) -> Result<LiteralAt, LocaleInvariant> {
    let attempt = match canonicalizing {
        Ok(canonicalizer) => {
            let &[raw] = raws else {
                unreachable!("a key holding alone has one sequence");
            };
            Ok(canonicalize(canonicalizer, raw)?)
        }
        Err(causes) => Err(causes.clone()),
    };
    let (identities, corrected) = match &attempt {
        Ok(Ok((locale, corrected))) => (vec![LocaleKey::Canonical(locale.clone())], *corrected),
        // A literal without a canonical locale stands for itself.
        _ => (
            raws.iter()
                .map(|raw| LocaleKey::Content(Content::new(raw)))
                .collect(),
            false,
        ),
    };
    let own = |check| unit(subject.clone(), check);
    let locale = evaluation.check(
        own(Check::CanonicalLocale),
        attempt.map(|attempt| attempt.map(|(locale, _)| locale)),
        |attempt| attempt,
    )?;
    if corrected {
        evaluation.notice(own(Check::CanonicalLocale), Code::LocaleNonCanonical)?;
    }
    let distinct = evaluation.check(
        own(Check::DistinctFromSource),
        both(locale.clone(), sequence.clone()),
        |(literal, source)| {
            if literal == source {
                Err(sequence_invalid(Reason::LiteralSelfReference))
            } else {
                Ok(())
            }
        },
    )?;
    Ok(LiteralAt {
        checked: and(locale.clone().map(|_| ()), distinct),
        locale,
        identities,
    })
}

/// A candidate's issue when one sequence holds it more than once.
fn repeated(holding: usize, reason: Reason) -> Result<(), Blocking> {
    if holding > 1 {
        Err(sequence_invalid(reason))
    } else {
        Ok(())
    }
}

/// One entry's sequence as checked candidates, in its declared order.
fn resolved(
    entry: &Entry<'_>,
    found: &BTreeMap<usize, Admitted<CanonicalLocale>>,
) -> Admitted<Vec<Candidate>> {
    let mut candidates = Ok(Vec::with_capacity(entry.candidates.len()));
    for (position, candidate) in entry.candidates.iter().enumerate() {
        let checked = match candidate {
            FallbackCandidate::Locale(_) => found[&position].clone().map(Candidate::Literal),
            FallbackCandidate::IntentSource(_) => Ok(Candidate::IntentSource),
        };
        candidates = both(candidates, checked).map(|(mut candidates, candidate)| {
            candidates.push(candidate);
            candidates
        });
    }
    candidates
}

const fn issue(code: Code, reason: Option<Reason>) -> Blocking {
    Blocking {
        code,
        reason,
        details: None,
    }
}

const fn sequence_invalid(reason: Reason) -> Blocking {
    issue(Code::MessageFallbackSequenceInvalid, Some(reason))
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::fixtures::{minimal_config, FixtureTargetReference};
    use crate::input_limits::Bound;
    use crate::resolver::evaluation::{Actual, LimitExceeded, Status};
    use crate::resolver::locales::fixtures::{
        canonical, canonicalizer, content, declaration, key, limits, notices, stand_in, units,
        waits_on, Declaration,
    };
    use crate::resolver::locales::{self, resolve_locales, LocaleFields, LocaleSubject};
    use crate::resolver::specification::Slot;

    type Id = &'static str;

    const SPECIFICATION: VersionedIdentity<Id> = VersionedIdentity {
        identity: "intlify-message-locale-fallback",
        revision: "0",
    };

    /// A confirmed policy with these two fallback bounds and room for
    /// everything Phase 7 bounds.
    fn policy(sources: u64, candidates: u64) -> ConfirmedPolicy {
        let mut limits = limits(64, 100, 100);
        let locale = &mut limits.locale_resolution;
        locale.max_fallback_sources = Bound::new(sources).unwrap();
        locale.max_fallback_candidates_per_source = Bound::new(candidates).unwrap();
        ConfirmedPolicy::fixture_owned(limits)
    }

    /// The minimal declaration with these requested locales, the first its
    /// default, and this message fallback.
    fn declared(requested: &[&str], fallback: Value) -> Declaration {
        declaration(&minimal_config(), |app| {
            app["requestedLocales"] = json!(requested);
            app["defaultRequestedLocale"] = json!(requested[0]);
            app["messageFallback"] = fallback;
        })
    }

    fn intent() -> Value {
        json!({"kind": "intent-source-locale"})
    }

    struct Resolved {
        evaluation: Evaluation,
        policy: Admitted<MessageFallbackPolicy<Id>>,
    }

    /// What Phase 8 canonicalizes candidates under: the finite provider's
    /// identifier bound, which Phase 7 shares, and the causes standing in for
    /// an unavailable canonicalizer or for an occurrence bound Phase 7 found
    /// exceeded, when a test gives them.
    struct Spelling<'c> {
        identifier_bytes: u64,
        unbound: Option<&'c Causes>,
        uncounted: Option<&'c Causes>,
    }

    const ROOM: Spelling<'static> = Spelling {
        identifier_bytes: 64,
        unbound: None,
        uncounted: None,
    };

    /// Phase 7 over `fields` under a policy with room for them, then Phase 8's
    /// fallback under `policy` and `specification`, into one evaluation.
    fn resolve_with(
        fields: &LocaleFields<'_, FixtureTargetReference>,
        policy: Result<&ConfirmedPolicy, &Causes>,
        specification: &Admitted<VersionedIdentity<Id>>,
        spelling: &Spelling<'_>,
    ) -> Resolved {
        let mut evaluation = Evaluation::default();
        let canonicalizer = canonicalizer(spelling.identifier_bytes);
        let mut locales = resolve_locales(
            &mut evaluation,
            Ok(&self::policy(100, 100)),
            Ok(&canonicalizer),
            fields,
        )
        .unwrap();
        if let Some(uncounted) = spelling.uncounted {
            locales.occurrence_bound = Err(uncounted.clone());
        }
        let policy = resolve_fallback(
            &mut evaluation,
            policy,
            specification,
            spelling.unbound.map_or(Ok(&canonicalizer), Err),
            &locales,
            fields.message_fallback.clone(),
        )
        .unwrap();
        Resolved { evaluation, policy }
    }

    fn resolve_fields(
        fields: &LocaleFields<'_, FixtureTargetReference>,
        policy: Result<&ConfirmedPolicy, &Causes>,
        specification: &Admitted<VersionedIdentity<Id>>,
    ) -> Resolved {
        resolve_with(fields, policy, specification, &ROOM)
    }

    fn resolve_under(declaration: &Declaration, sources: u64, candidates: u64) -> Resolved {
        resolve_fields(
            &LocaleFields::declared(declaration),
            Ok(&policy(sources, candidates)),
            &Ok(SPECIFICATION),
        )
    }

    fn resolve(declaration: &Declaration) -> Resolved {
        resolve_under(declaration, 100, 100)
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

    fn evaluated(units: &[&[Unit]]) -> Vec<(Unit, Status)> {
        units
            .concat()
            .into_iter()
            .map(|unit| (unit, Status::Evaluated))
            .collect()
    }

    fn sources_unit() -> Unit {
        unit(FallbackSubject::Sources, Check::WithinBound)
    }

    fn source_unit(source: LocaleKey, check: Check) -> Unit {
        unit(FallbackSubject::Source(source), check)
    }

    fn sequence_unit(source: LocaleKey) -> Unit {
        unit(FallbackSubject::Sequence(source), Check::WithinBound)
    }

    fn candidate_unit(source: LocaleKey, position: usize, check: Check) -> Unit {
        unit(FallbackSubject::Candidate(source, position), check)
    }

    fn literal_unit(source: LocaleKey, literal: LocaleKey) -> Unit {
        unit(
            FallbackSubject::Literal(source, literal),
            Check::UniqueCandidate,
        )
    }

    fn intent_unit(source: LocaleKey) -> Unit {
        unit(
            FallbackSubject::IntentSource(source),
            Check::UniqueCandidate,
        )
    }

    /// Both units of one key.
    fn source_units(source: &str) -> [Unit; 2] {
        [Check::UniqueLocale, Check::RequestedMember].map(|check| source_unit(key(source), check))
    }

    /// Both units of the literal candidate at `position` in `source`'s
    /// sequence.
    fn candidate_units(source: &LocaleKey, position: usize) -> [Unit; 2] {
        [Check::CanonicalLocale, Check::DistinctFromSource]
            .map(|check| candidate_unit(source.clone(), position, check))
    }

    /// Each source's sequence as spelled, sources in their order.
    fn sequences(policy: &MessageFallbackPolicy<Id>) -> Vec<(&str, Vec<&str>)> {
        policy
            .sequences()
            .iter()
            .map(|(source, candidates)| {
                (
                    source.as_str(),
                    candidates
                        .iter()
                        .map(|candidate| match candidate {
                            Candidate::Literal(locale) => locale.as_str(),
                            Candidate::IntentSource => "(intent source)",
                        })
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn subjects_order_the_bound_then_keys_sequences_candidates_and_repeats() {
        let ordered = [
            FallbackSubject::Sources,
            FallbackSubject::Source(key("de")),
            FallbackSubject::Source(key("ja")),
            FallbackSubject::Source(content("zz")),
            FallbackSubject::Sequence(key("ja")),
            FallbackSubject::Sequence(content("zz")),
            FallbackSubject::Candidate(key("de"), 1),
            FallbackSubject::Candidate(key("ja"), 0),
            FallbackSubject::Candidate(key("ja"), 2),
            // A position is a number, not its decimal spelling.
            FallbackSubject::Candidate(key("ja"), 10),
            FallbackSubject::Literal(key("de"), key("ja")),
            FallbackSubject::Literal(key("ja"), key("de")),
            FallbackSubject::Literal(key("ja"), key("en")),
            FallbackSubject::IntentSource(key("de")),
            FallbackSubject::IntentSource(key("ja")),
        ];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
        // A key's collision check runs before its membership, and a
        // candidate's identity before the comparison with its source.
        let source = FallbackSubject::Source(key("ja"));
        assert!(unit(source.clone(), Check::UniqueLocale) < unit(source, Check::RequestedMember));
        let candidate = FallbackSubject::Candidate(key("ja"), 0);
        assert!(
            unit(candidate.clone(), Check::CanonicalLocale)
                < unit(candidate, Check::DistinctFromSource)
        );
        // Negotiation's units commit before fallback's.
        assert!(
            crate::resolver::negotiation::unit(
                crate::resolver::negotiation::NegotiationSubject::Destination(
                    content("zz"),
                    content("zz")
                ),
                Check::RequestedMember
            ) < sources_unit()
        );
    }

    #[test]
    fn sequences_keep_their_declared_order_while_sources_and_literals_form_sorted_sets() {
        // `EN-us` sorts before `de` as authored and after it once canonical.
        let authored = |ja: Value| {
            declared(
                &["en", "en-US", "de", "ja"],
                json!({"ja": ja, "EN-us": ["en"], "de": ["en-US", "en"]}),
            )
        };
        let resolved = resolve(&authored(json!(["en", intent(), "de"])));
        assert_eq!(issues(&resolved.evaluation), []);
        let policy = resolved.policy.unwrap();
        assert_eq!(policy.specification(), &SPECIFICATION);
        assert_eq!(
            sequences(&policy),
            [
                ("de", vec!["en-US", "en"]),
                ("en-US", vec!["en"]),
                ("ja", vec!["en", "(intent source)", "de"]),
            ]
        );
        assert_eq!(
            policy.definition_locales().as_slice(),
            ["de", "en", "en-US"].map(canonical)
        );
        // Reordering a sequence changes the policy, and its literals do not.
        let reordered = resolve(&authored(json!(["de", intent(), "en"])))
            .policy
            .unwrap();
        assert_eq!(
            sequences(&reordered)[2],
            ("ja", vec!["de", "(intent source)", "en"])
        );
        assert_ne!(reordered, policy);
        assert_eq!(reordered.definition_locales(), policy.definition_locales());
    }

    #[test]
    fn a_sequence_is_never_expanded_through_another_sources_sequence() {
        let resolved = resolve(&declared(
            &["en", "ja"],
            json!({"ja": ["en"], "en": ["fr"]}),
        ));
        assert_eq!(issues(&resolved.evaluation), []);
        assert_eq!(
            sequences(&resolved.policy.unwrap()),
            [("en", vec!["fr"]), ("ja", vec!["en"])]
        );
    }

    #[test]
    fn a_literal_may_name_any_locale_but_a_source_must_be_requested() {
        let resolved = resolve(&declared(&["en"], json!({"fr": ["de"]})));
        let outside = source_unit(key("fr"), Check::RequestedMember);
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                outside.clone(),
                "project-profile-message-fallback-source-not-requested",
                None
            )]
        );
        assert_eq!(
            statuses(&resolved.evaluation),
            evaluated(&[
                &[sources_unit()],
                &source_units("fr"),
                &[sequence_unit(key("fr"))],
                &candidate_units(&key("fr"), 0),
                &[literal_unit(key("fr"), key("de"))],
            ])
        );
        assert_eq!(units(&resolved.policy.unwrap_err()), [outside]);
    }

    #[test]
    fn each_literal_is_canonicalized_at_its_position_and_a_differing_spelling_corrected() {
        let resolved = resolve(&declared(
            &["ja"],
            json!({"ja": ["EN", intent(), "de", "EN-us"]}),
        ));
        assert_eq!(issues(&resolved.evaluation), []);
        // The Intent source at position 1 spells no locale, so the position
        // has no unit; the repeat checks name each literal by its identity.
        assert_eq!(
            statuses(&resolved.evaluation),
            evaluated(&[
                &[sources_unit()],
                &source_units("ja"),
                &[sequence_unit(key("ja"))],
                &candidate_units(&key("ja"), 0),
                &candidate_units(&key("ja"), 2),
                &candidate_units(&key("ja"), 3),
                &[
                    literal_unit(key("ja"), key("de")),
                    literal_unit(key("ja"), key("en")),
                    literal_unit(key("ja"), key("en-US")),
                    intent_unit(key("ja")),
                ],
            ])
        );
        assert_eq!(
            notices(&resolved.evaluation),
            [0, 3].map(|position| candidate_unit(key("ja"), position, Check::CanonicalLocale))
        );
        assert_eq!(
            sequences(&resolved.policy.unwrap()),
            [("ja", vec!["en", "(intent source)", "de", "en-US"])]
        );
    }

    #[test]
    fn two_keys_naming_one_source_are_a_duplicate_whose_sequences_are_never_checked() {
        // The longer sequence is over its bound, and `de` sits in both, but
        // neither sequence is measured or checked: one position under the
        // shared key would merge the candidates of two sequences.
        let declaration = declared(
            &["en"],
            json!({"EN": ["de", intent()], "en": ["fr", "de", intent()]}),
        );
        let duplicate = source_unit(key("en"), Check::UniqueLocale);
        let resolved = resolve_under(&declaration, 100, 2);
        assert_eq!(
            issues(&resolved.evaluation),
            [(duplicate.clone(), "project-profile-locale-duplicate", None)]
        );
        // Position 0 holds `de` and `fr`, position 1 `de`, and position 2 only
        // the Intent source. No literal has an identity, so each stands for
        // its spelling.
        let waiting = [
            [sequence_unit(key("en"))].as_slice(),
            &candidate_units(&key("en"), 0),
            &candidate_units(&key("en"), 1),
            &[
                literal_unit(key("en"), content("de")),
                literal_unit(key("en"), content("fr")),
                intent_unit(key("en")),
            ],
        ]
        .concat();
        for unit in &waiting {
            assert_eq!(
                waits_on(&resolved.evaluation, unit),
                Some(vec![duplicate.clone()]),
                "{unit:?}"
            );
        }
        // The mapping, the key's two, and every unit that waits.
        assert_eq!(statuses(&resolved.evaluation).len(), 3 + waiting.len());
        assert_eq!(
            waits_on(
                &resolved.evaluation,
                &source_unit(key("en"), Check::RequestedMember)
            ),
            None
        );
        assert_eq!(units(&resolved.policy.unwrap_err()), [duplicate]);
    }

    #[test]
    fn a_key_without_an_identity_leaves_its_sequence_never_checked() {
        let resolved = resolve(&declared(
            &["ja"],
            json!({"zz": ["de", "de"], "ja": ["zz"]}),
        ));
        let invalid_key = locales::unit(
            LocaleSubject::MessageFallbackKey(content("zz")),
            Check::CanonicalLocale,
        );
        // The repeated `de` is never found: one position under a key without
        // an identity could merge the candidates of distinct keys.
        for waiting in [
            source_unit(content("zz"), Check::UniqueLocale),
            source_unit(content("zz"), Check::RequestedMember),
            sequence_unit(content("zz")),
            candidate_unit(content("zz"), 0, Check::CanonicalLocale),
            candidate_unit(content("zz"), 0, Check::DistinctFromSource),
            candidate_unit(content("zz"), 1, Check::CanonicalLocale),
            candidate_unit(content("zz"), 1, Check::DistinctFromSource),
            literal_unit(content("zz"), content("de")),
        ] {
            assert_eq!(
                waits_on(&resolved.evaluation, &waiting),
                Some(vec![invalid_key.clone()]),
                "{waiting:?}"
            );
        }
        // A literal without an identity is Phase 8's issue, and only what
        // needs its identity waits on it.
        let invalid_literal = candidate_unit(key("ja"), 0, Check::CanonicalLocale);
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                invalid_literal.clone(),
                "project-profile-locale-invalid",
                None
            )]
        );
        for waiting in [
            candidate_unit(key("ja"), 0, Check::DistinctFromSource),
            literal_unit(key("ja"), content("zz")),
        ] {
            assert_eq!(
                waits_on(&resolved.evaluation, &waiting),
                Some(vec![invalid_literal.clone()])
            );
        }
        assert_eq!(
            waits_on(&resolved.evaluation, &sequence_unit(key("ja"))),
            None
        );
    }

    #[test]
    fn each_sequence_defect_is_an_issue_with_its_own_reason() {
        let resolved = resolve(&declared(
            &["ja", "en"],
            json!({"ja": ["ja", "en", "EN", intent(), intent()]}),
        ));
        let invalid = "project-profile-message-fallback-sequence-invalid";
        assert_eq!(
            issues(&resolved.evaluation),
            [
                (
                    candidate_unit(key("ja"), 0, Check::DistinctFromSource),
                    invalid,
                    Some("literal-self-reference")
                ),
                (
                    literal_unit(key("ja"), key("en")),
                    invalid,
                    Some("literal-duplicate")
                ),
                (
                    intent_unit(key("ja")),
                    invalid,
                    Some("intent-source-duplicate")
                ),
            ]
        );
        // `ja` is named once, and neither `en` nor `EN` is the source.
        for evaluated in [
            literal_unit(key("ja"), key("ja")),
            candidate_unit(key("ja"), 1, Check::DistinctFromSource),
            candidate_unit(key("ja"), 2, Check::DistinctFromSource),
        ] {
            assert_eq!(waits_on(&resolved.evaluation, &evaluated), None);
        }
        assert_eq!(
            notices(&resolved.evaluation),
            [candidate_unit(key("ja"), 2, Check::CanonicalLocale)]
        );
        assert_eq!(units(&resolved.policy.unwrap_err()).len(), 3);
    }

    #[test]
    fn each_position_naming_its_source_is_its_own_self_reference() {
        // Two spellings of the source: a self-reference at each position,
        // and one repeat of the literal they share.
        let resolved = resolve(&declared(&["en"], json!({"en": ["en", "EN"]})));
        let invalid = "project-profile-message-fallback-sequence-invalid";
        assert_eq!(
            issues(&resolved.evaluation),
            [
                (
                    candidate_unit(key("en"), 0, Check::DistinctFromSource),
                    invalid,
                    Some("literal-self-reference")
                ),
                (
                    candidate_unit(key("en"), 1, Check::DistinctFromSource),
                    invalid,
                    Some("literal-self-reference")
                ),
                (
                    literal_unit(key("en"), key("en")),
                    invalid,
                    Some("literal-duplicate")
                ),
            ]
        );
    }

    #[test]
    fn the_source_bound_counts_every_submitted_key_before_any_collision() {
        let declaration = declared(&["en"], json!({"EN": ["de"], "en": ["fr"]}));
        let resolved = resolve_under(&declaration, 2, 100);
        assert_eq!(waits_on(&resolved.evaluation, &sources_unit()), None);
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                source_unit(key("en"), Check::UniqueLocale),
                "project-profile-locale-duplicate",
                None
            )]
        );

        let resolved = resolve_under(&declaration, 1, 100);
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                sources_unit(),
                "project-profile-resource-limit-exceeded",
                None
            )]
        );
        let found = resolved.evaluation.issues().last().unwrap();
        assert_eq!(
            resolved.evaluation.limit(found),
            Some(LimitExceeded {
                bound: MAX_FALLBACK_SOURCES,
                limit: Bound::new(1).unwrap(),
                actual: Actual::Exact(2),
            })
        );
        let over = resolved.policy.unwrap_err();
        assert_eq!(units(&over), [sources_unit()]);
        // The key's two units, its sequence, the two of the position both
        // literals share, and a repeat check for each literal's spelling wait.
        let recorded = statuses(&resolved.evaluation);
        assert_eq!(recorded.len(), 8);
        assert!(recorded[1..]
            .iter()
            .all(|(_, status)| status == &Status::NotEvaluated(over.clone())));
    }

    #[test]
    fn the_candidate_bound_counts_both_kinds_before_any_duplicate_check() {
        let declaration = declared(&["ja"], json!({"ja": ["en", intent(), "en"]}));
        let resolved = resolve_under(&declaration, 100, 3);
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                literal_unit(key("ja"), key("en")),
                "project-profile-message-fallback-sequence-invalid",
                Some("literal-duplicate")
            )]
        );

        let resolved = resolve_under(&declaration, 100, 2);
        let sequence = sequence_unit(key("ja"));
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                sequence.clone(),
                "project-profile-resource-limit-exceeded",
                None
            )]
        );
        let found = resolved.evaluation.issues().last().unwrap();
        assert_eq!(
            resolved.evaluation.limit(found),
            Some(LimitExceeded {
                bound: MAX_FALLBACK_CANDIDATES_PER_SOURCE,
                limit: Bound::new(2).unwrap(),
                actual: Actual::Exact(3),
            })
        );
        // Nothing past the bound is canonicalized, so `en` stands for its
        // spelling.
        for candidate in [
            &candidate_units(&key("ja"), 0)[..],
            &candidate_units(&key("ja"), 2),
            &[
                literal_unit(key("ja"), content("en")),
                intent_unit(key("ja")),
            ],
        ]
        .concat()
        {
            assert_eq!(
                waits_on(&resolved.evaluation, &candidate),
                Some(vec![sequence.clone()])
            );
        }
        // The key's own checks never wait on its sequence.
        for check in [Check::UniqueLocale, Check::RequestedMember] {
            assert_eq!(
                waits_on(&resolved.evaluation, &source_unit(key("ja"), check)),
                None
            );
        }
    }

    #[test]
    fn a_literal_waits_on_the_canonicalizer_and_phase_7_s_count_and_is_bounded_in_bytes() {
        let declaration = declared(&["ja"], json!({"ja": ["en-US", intent(), "en"]}));
        let fields = LocaleFields::declared(&declaration);
        let confirmed = policy(100, 100);
        let unbound = stand_in(Slot::LocaleCanonicalization);
        // Phase 7's occurrence bound, exceeded in another run.
        let uncounted = resolve_locales(
            &mut Evaluation::default(),
            Ok(&locales::fixtures::policy(64, 1, 100)),
            Ok(&canonicalizer(64)),
            &fields,
        )
        .unwrap()
        .occurrence_bound
        .unwrap_err();
        for (spelling, causes) in [
            (
                Spelling {
                    unbound: Some(&unbound),
                    ..ROOM
                },
                &unbound,
            ),
            (
                Spelling {
                    uncounted: Some(&uncounted),
                    ..ROOM
                },
                &uncounted,
            ),
        ] {
            let resolved = resolve_with(&fields, Ok(&confirmed), &Ok(SPECIFICATION), &spelling);
            for literal in [
                &candidate_units(&key("ja"), 0)[..],
                &candidate_units(&key("ja"), 2),
                &[
                    literal_unit(key("ja"), content("en-US")),
                    literal_unit(key("ja"), content("en")),
                ],
            ]
            .concat()
            {
                assert_eq!(
                    waits_on(&resolved.evaluation, &literal),
                    Some(units(causes)),
                    "{literal:?}"
                );
            }
            // The key, its sequence, and the Intent source need no locale
            // read.
            for evaluated in [
                source_unit(key("ja"), Check::UniqueLocale),
                sequence_unit(key("ja")),
                intent_unit(key("ja")),
            ] {
                assert_eq!(waits_on(&resolved.evaluation, &evaluated), None);
            }
            assert_eq!(resolved.policy, Err(causes.clone()));
        }
        // Each literal is measured in bytes before the provider reads it.
        let short = resolve_with(
            &fields,
            Ok(&confirmed),
            &Ok(SPECIFICATION),
            &Spelling {
                identifier_bytes: 4,
                ..ROOM
            },
        );
        let long = candidate_unit(key("ja"), 0, Check::CanonicalLocale);
        assert_eq!(
            issues(&short.evaluation),
            [(
                long.clone(),
                "project-profile-resource-limit-exceeded",
                None
            )]
        );
        let found = short.evaluation.issues().last().unwrap();
        assert_eq!(
            short.evaluation.limit(found),
            Some(LimitExceeded {
                bound: BoundId {
                    group: Group::LocaleResolution,
                    name: "maxLocaleIdentifierBytes",
                },
                limit: Bound::new(4).unwrap(),
                actual: Actual::Exact(5),
            })
        );
        assert_eq!(
            waits_on(
                &short.evaluation,
                &literal_unit(key("ja"), content("en-US"))
            ),
            Some(vec![long])
        );
        assert_eq!(
            waits_on(&short.evaluation, &literal_unit(key("ja"), key("en"))),
            None
        );
    }

    #[test]
    fn a_requested_set_phase_7_could_not_form_leaves_only_membership_not_evaluated() {
        let resolved = resolve(&declared(&["ja", "zz"], json!({"ja": ["en"]})));
        let invalid = locales::unit(
            LocaleSubject::RequestedLocale(content("zz")),
            Check::CanonicalLocale,
        );
        assert_eq!(
            waits_on(
                &resolved.evaluation,
                &source_unit(key("ja"), Check::RequestedMember)
            ),
            Some(vec![invalid.clone()])
        );
        assert_eq!(
            statuses(&resolved.evaluation)
                .into_iter()
                .filter(|(_, status)| status != &Status::Evaluated)
                .count(),
            1
        );
        assert_eq!(units(&resolved.policy.unwrap_err()), [invalid]);
    }

    #[test]
    fn omission_and_an_empty_mapping_are_one_empty_policy() {
        let omitted = resolve(&declaration(&minimal_config(), |_| {}));
        let empty = resolve(&declared(&["en"], json!({})));
        for resolved in [&omitted, &empty] {
            assert_eq!(
                statuses(&resolved.evaluation),
                [(sources_unit(), Status::Evaluated)]
            );
        }
        let policy = omitted.policy.unwrap();
        assert_eq!(empty.policy, Ok(policy.clone()));
        assert!(policy.sequences().is_empty());
        assert_eq!(policy.definition_locales().as_slice(), []);
    }

    #[test]
    fn an_unadmitted_specification_policy_or_field_leaves_every_unit_not_evaluated() {
        let declaration = declared(&["ja"], json!({"ja": ["en", intent()]}));
        let fields = LocaleFields::declared(&declaration);
        let unsupported = stand_in(Slot::MessageLocaleFallback);
        let unconfirmed = stand_in(Slot::Coverage);
        let confirmed = policy(100, 100);
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
            // The mapping, the key's two, its sequence, the two of its
            // literal's position, the literal's repeat, and the Intent
            // source's.
            assert_eq!(recorded.len(), 8);
            assert!(recorded
                .iter()
                .all(|(_, status)| status == &Status::NotEvaluated(causes.clone())));
            assert_eq!(resolved.policy, Err(causes));
        }
        let mut fields = LocaleFields::declared(&declaration);
        let rejected = stand_in(Slot::DeliveryGraph);
        fields.message_fallback = Err(rejected.clone());
        let resolved = resolve_fields(&fields, Ok(&confirmed), &Ok(SPECIFICATION));
        assert_eq!(
            statuses(&resolved.evaluation),
            [(sources_unit(), Status::NotEvaluated(rejected.clone()))]
        );
        assert_eq!(resolved.policy, Err(rejected));
    }
}
