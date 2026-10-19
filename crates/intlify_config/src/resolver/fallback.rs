// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 8 for the selected declaration's message fallback: the mapping
//! under `maxFallbackSources`, each sequence under
//! `maxFallbackCandidatesPerSource`, a source two keys name, a source outside
//! the project's requested locales, and a sequence that names its own source,
//! repeats a literal, or repeats the Intent source-locale candidate, all under
//! the admitted Message Locale Fallback Specification. A sequence keeps the
//! order it was declared in: nothing here deduplicates, reorders, or expands it
//! through another source's sequence. Not the Linker's probing of definitions,
//! which 020 owns, and not the canonicalization of sources and literals, which
//! Phase 7 did.

use std::collections::BTreeMap;

use crate::locale::{CanonicalLocale, VersionedIdentity};

use super::evaluation::{
    both, within, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant, Phase,
    Reason, Subject, Unit,
};
use super::limits::{BoundId, ConfirmedPolicy, Group};
use super::locales::{
    FallbackLocale, FallbackLocales, LocaleKey, LocaleSet, ResolvedLocales, Spelled,
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
/// prerequisites come first: the mapping's bound, then each source with its
/// sequence's bound, then the candidates that wait on it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FallbackSubject {
    /// The submitted mapping, under `maxFallbackSources`.
    Sources,
    /// A mapping key: the source its sequence falls back from.
    Source(LocaleKey),
    /// A literal candidate's locale, in its source's sequence.
    Literal(LocaleKey, LocaleKey),
    /// The Intent source-locale candidate, in its source's sequence.
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

/// Phase 8's fallback checks over what Phase 7 resolved. Every unit waits on
/// the mapping's bound, and every candidate on its sequence's; the policy
/// exists only when every unit passed.
pub(crate) fn resolve_fallback<Id: Clone>(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    specification: &Admitted<VersionedIdentity<Id>>,
    locales: &ResolvedLocales,
) -> Result<Admitted<MessageFallbackPolicy<Id>>, EvaluationInvariant> {
    let policy = policy.map_err(Causes::clone);
    let sources = locales.message_fallback.as_ref().map_err(Causes::clone);
    let bounded = evaluation.check(
        unit(FallbackSubject::Sources, Check::WithinBound),
        both(both(policy.clone(), specification.clone()), sources.clone()),
        |((policy, _), sources)| {
            within(
                MAX_FALLBACK_SOURCES,
                policy.limits().locale_resolution.max_fallback_sources,
                sources.len(),
            )
        },
    )?;
    let entries = sources.map_or(&[][..], Vec::as_slice);
    let requested = locales.requested_set.as_ref().map_err(Causes::clone);
    let mut checked = bounded.clone();
    for (key, sharing) in by_source(entries) {
        // Entries under one subject share their key's result.
        let source = &sharing[0].key;
        let sequence = evaluation.check(
            unit(FallbackSubject::Source(key.clone()), Check::WithinBound),
            both(bounded.clone(), policy.clone()),
            |((), policy)| {
                let longest = sharing
                    .iter()
                    .map(|entry| entry.candidates.len())
                    .fold(0, usize::max);
                within(
                    MAX_FALLBACK_CANDIDATES_PER_SOURCE,
                    policy
                        .limits()
                        .locale_resolution
                        .max_fallback_candidates_per_source,
                    longest,
                )
            },
        )?;
        let unique = evaluation.check(
            unit(FallbackSubject::Source(key.clone()), Check::UniqueLocale),
            both(bounded.clone(), source.locale.clone()),
            |_| {
                if sharing.len() == 1 {
                    Ok(())
                } else {
                    Err(issue(Code::LocaleDuplicate, None))
                }
            },
        )?;
        let member = evaluation.check(
            unit(FallbackSubject::Source(key.clone()), Check::RequestedMember),
            both(
                both(bounded.clone(), source.locale.clone()),
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
        let candidates = candidates(evaluation, &sequence, source, &sharing)?;
        checked = both(
            both(checked, sequence),
            both(both(unique, member), candidates),
        )
        .map(|_| ());
    }
    let mut sequences = Ok(BTreeMap::new());
    for entry in entries {
        sequences = both(sequences, both(entry.key.locale.clone(), resolved(entry))).map(
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

/// The submitted entries grouped by their key's subject, in subject order.
fn by_source(entries: &[FallbackLocales]) -> BTreeMap<&LocaleKey, Vec<&FallbackLocales>> {
    let mut sources: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for entry in entries {
        sources.entry(&entry.key.key).or_default().push(entry);
    }
    sources
}

/// The units over one source's candidates, which wait on its sequence's
/// bound: each literal's identity, never its source and never repeated, and
/// the Intent source-locale candidate, never repeated. A repeat counts within
/// one sequence, even when two keys name the source.
fn candidates(
    evaluation: &mut Evaluation,
    sequence: &Admitted<()>,
    source: &Spelled,
    sharing: &[&FallbackLocales],
) -> Result<Admitted<()>, EvaluationInvariant> {
    // Each literal's subject with the most times one sequence holds it.
    let mut literals: BTreeMap<&LocaleKey, (&Spelled, usize)> = BTreeMap::new();
    let mut intent_sources: Option<usize> = None;
    for entry in sharing {
        let mut counted: BTreeMap<&LocaleKey, (&Spelled, usize)> = BTreeMap::new();
        let mut intent = 0;
        for candidate in &entry.candidates {
            match candidate {
                FallbackLocale::Literal(literal) => {
                    counted.entry(&literal.key).or_insert((literal, 0)).1 += 1;
                }
                FallbackLocale::IntentSource => intent += 1,
            }
        }
        for (key, (literal, count)) in counted {
            let most = &mut literals.entry(key).or_insert((literal, 0)).1;
            *most = (*most).max(count);
        }
        if intent > 0 {
            intent_sources = Some(intent_sources.map_or(intent, |most| most.max(intent)));
        }
    }
    let mut checked = sequence.clone();
    for (key, (literal, most)) in literals {
        let subject = FallbackSubject::Literal(source.key.clone(), key.clone());
        let distinct = evaluation.check(
            unit(subject.clone(), Check::DistinctFromSource),
            both(
                both(sequence.clone(), source.locale.clone()),
                literal.locale.clone(),
            ),
            |(((), source), literal)| {
                if literal == source {
                    Err(sequence_invalid(Reason::LiteralSelfReference))
                } else {
                    Ok(())
                }
            },
        )?;
        let once = evaluation.check(
            unit(subject, Check::UniqueCandidate),
            both(sequence.clone(), literal.locale.clone()),
            |_| repeated(most, Reason::LiteralDuplicate),
        )?;
        checked = both(checked, both(distinct, once)).map(|_| ());
    }
    if let Some(most) = intent_sources {
        let once = evaluation.check(
            unit(
                FallbackSubject::IntentSource(source.key.clone()),
                Check::UniqueCandidate,
            ),
            sequence.clone(),
            |()| repeated(most, Reason::IntentSourceDuplicate),
        )?;
        checked = both(checked, once).map(|_| ());
    }
    Ok(checked)
}

/// A candidate's issue when one sequence holds it more than once.
fn repeated(most: usize, reason: Reason) -> Result<(), Blocking> {
    if most > 1 {
        Err(sequence_invalid(reason))
    } else {
        Ok(())
    }
}

/// One entry's sequence as checked candidates, in its declared order.
fn resolved(entry: &FallbackLocales) -> Admitted<Vec<Candidate>> {
    let mut candidates = Ok(Vec::with_capacity(entry.candidates.len()));
    for candidate in &entry.candidates {
        let checked = match candidate {
            FallbackLocale::Literal(literal) => literal.locale.clone().map(Candidate::Literal),
            FallbackLocale::IntentSource => Ok(Candidate::IntentSource),
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
        limit: None,
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
        canonical, canonicalizer, content, declaration, key, limits, stand_in, units, waits_on,
        Declaration,
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

    /// Phase 7 over `fields` under a policy with room for them, then Phase 8's
    /// fallback under `policy` and `specification`, into one evaluation.
    fn resolve_fields(
        fields: &LocaleFields<'_, FixtureTargetReference>,
        policy: Result<&ConfirmedPolicy, &Causes>,
        specification: &Admitted<VersionedIdentity<Id>>,
    ) -> Resolved {
        let mut evaluation = Evaluation::default();
        let locales = resolve_locales(
            &mut evaluation,
            Ok(&self::policy(100, 100)),
            Ok(&canonicalizer(64)),
            fields,
        )
        .unwrap();
        let policy = resolve_fallback(&mut evaluation, policy, specification, &locales).unwrap();
        Resolved { evaluation, policy }
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

    fn sources_unit() -> Unit {
        unit(FallbackSubject::Sources, Check::WithinBound)
    }

    fn source_unit(source: LocaleKey, check: Check) -> Unit {
        unit(FallbackSubject::Source(source), check)
    }

    fn literal_unit(source: LocaleKey, literal: LocaleKey, check: Check) -> Unit {
        unit(FallbackSubject::Literal(source, literal), check)
    }

    fn intent_unit(source: LocaleKey) -> Unit {
        unit(
            FallbackSubject::IntentSource(source),
            Check::UniqueCandidate,
        )
    }

    /// Every unit one source records with no candidates after it.
    fn source_units(source: &str) -> [Unit; 3] {
        [
            Check::WithinBound,
            Check::UniqueLocale,
            Check::RequestedMember,
        ]
        .map(|check| source_unit(key(source), check))
    }

    /// The two units of one literal.
    fn literal_units(source: &str, literal: &str) -> [Unit; 2] {
        [Check::DistinctFromSource, Check::UniqueCandidate]
            .map(|check| literal_unit(key(source), key(literal), check))
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
    fn subjects_order_the_bound_first_then_sources_then_their_candidates() {
        let ordered = [
            FallbackSubject::Sources,
            FallbackSubject::Source(key("de")),
            FallbackSubject::Source(key("ja")),
            FallbackSubject::Source(content("zz")),
            FallbackSubject::Literal(key("de"), key("ja")),
            FallbackSubject::Literal(key("ja"), key("de")),
            FallbackSubject::Literal(key("ja"), key("en")),
            FallbackSubject::IntentSource(key("de")),
            FallbackSubject::IntentSource(key("ja")),
        ];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
        // A sequence's bound runs before the checks of its source's key.
        let source = FallbackSubject::Source(key("ja"));
        assert!(unit(source.clone(), Check::WithinBound) < unit(source, Check::UniqueLocale));
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
            [
                [sources_unit()].as_slice(),
                &source_units("fr"),
                &literal_units("fr", "de"),
            ]
            .concat()
            .into_iter()
            .map(|unit| (unit, Status::Evaluated))
            .collect::<Vec<_>>()
        );
        assert_eq!(units(&resolved.policy.unwrap_err()), [outside]);
    }

    #[test]
    fn two_keys_naming_one_source_are_a_locale_duplicate_and_each_sequence_counts_alone() {
        // `de` and the Intent source once in each sequence are no repeat; the
        // longer sequence is the one its source's bound measures.
        let declaration = declared(
            &["en"],
            json!({"EN": ["de", intent()], "en": ["fr", "de", intent()]}),
        );
        let duplicate = (
            source_unit(key("en"), Check::UniqueLocale),
            "project-profile-locale-duplicate",
            None,
        );
        let resolved = resolve_under(&declaration, 100, 3);
        assert_eq!(
            issues(&resolved.evaluation),
            std::slice::from_ref(&duplicate)
        );
        assert_eq!(
            units(&resolved.policy.unwrap_err()),
            std::slice::from_ref(&duplicate.0)
        );

        let resolved = resolve_under(&declaration, 100, 2);
        let sequence = source_unit(key("en"), Check::WithinBound);
        assert_eq!(
            issues(&resolved.evaluation),
            [
                (
                    sequence.clone(),
                    "project-profile-resource-limit-exceeded",
                    None
                ),
                duplicate,
            ]
        );
        let found = resolved
            .evaluation
            .issues()
            .find(|key| key.unit() == &sequence);
        assert_eq!(
            resolved.evaluation.limit(found.unwrap()),
            Some(LimitExceeded {
                bound: MAX_FALLBACK_CANDIDATES_PER_SOURCE,
                limit: Bound::new(2).unwrap(),
                actual: Actual::Exact(3),
            })
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
                    literal_unit(key("ja"), key("en"), Check::UniqueCandidate),
                    invalid,
                    Some("literal-duplicate")
                ),
                (
                    literal_unit(key("ja"), key("ja"), Check::DistinctFromSource),
                    invalid,
                    Some("literal-self-reference")
                ),
                (
                    intent_unit(key("ja")),
                    invalid,
                    Some("intent-source-duplicate")
                ),
            ]
        );
        // `ja` names its source once; `en` is not the source.
        for evaluated in [
            literal_unit(key("ja"), key("ja"), Check::UniqueCandidate),
            literal_unit(key("ja"), key("en"), Check::DistinctFromSource),
        ] {
            assert_eq!(waits_on(&resolved.evaluation, &evaluated), None);
        }
        assert_eq!(units(&resolved.policy.unwrap_err()).len(), 3);
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
        // The source's three units and two for each of its literals wait.
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
                literal_unit(key("ja"), key("en"), Check::UniqueCandidate),
                "project-profile-message-fallback-sequence-invalid",
                Some("literal-duplicate")
            )]
        );

        let resolved = resolve_under(&declaration, 100, 2);
        let sequence = source_unit(key("ja"), Check::WithinBound);
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
            resolved
                .evaluation
                .limit(found)
                .map(|exceeded| exceeded.actual),
            Some(Actual::Exact(3))
        );
        for candidate in [
            literal_unit(key("ja"), key("en"), Check::DistinctFromSource),
            literal_unit(key("ja"), key("en"), Check::UniqueCandidate),
            intent_unit(key("ja")),
        ] {
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
    fn an_invalid_spelling_leaves_only_the_units_needing_its_identity_not_evaluated() {
        let resolved = resolve(&declared(
            &["ja"],
            json!({"zz": ["de", "de"], "ja": ["zz"]}),
        ));
        let invalid_source = locales::unit(
            LocaleSubject::MessageFallbackKey(content("zz")),
            Check::CanonicalLocale,
        );
        let invalid_literal = locales::unit(
            LocaleSubject::MessageFallbackCandidate(key("ja"), 0, content("zz")),
            Check::CanonicalLocale,
        );
        for (waiting, cause) in [
            (
                source_unit(content("zz"), Check::UniqueLocale),
                &invalid_source,
            ),
            (
                source_unit(content("zz"), Check::RequestedMember),
                &invalid_source,
            ),
            (
                literal_unit(content("zz"), key("de"), Check::DistinctFromSource),
                &invalid_source,
            ),
            (
                literal_unit(key("ja"), content("zz"), Check::DistinctFromSource),
                &invalid_literal,
            ),
            (
                literal_unit(key("ja"), content("zz"), Check::UniqueCandidate),
                &invalid_literal,
            ),
        ] {
            assert_eq!(
                waits_on(&resolved.evaluation, &waiting),
                Some(vec![cause.clone()])
            );
        }
        // A sequence's bound and a repeat need no source identity.
        assert_eq!(
            waits_on(
                &resolved.evaluation,
                &source_unit(content("zz"), Check::WithinBound)
            ),
            None
        );
        assert_eq!(
            issues(&resolved.evaluation),
            [(
                literal_unit(content("zz"), key("de"), Check::UniqueCandidate),
                "project-profile-message-fallback-sequence-invalid",
                Some("literal-duplicate")
            )]
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
            // The mapping, the source's three, the literal's two, the Intent
            // source's one.
            assert_eq!(recorded.len(), 7);
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
