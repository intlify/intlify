// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::prepare::prepare;
use super::resolution_inputs::SCALE_LANGUAGES;
use super::*;
use crate::benchmark::locale_resolution::Resolved;
use crate::benchmark::operation::Output;
use crate::locale::CanonicalLocale;
use crate::resolver::evaluation::{Actual, Admitted, Code, Status, Subject};
use crate::resolver::locales::{FallbackLocale, LocaleKey, LocaleSubject, ResolvedLocales};

use LocaleResolutionRecipe as R;

fn case(recipe: LocaleResolutionRecipe) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::LocaleResolution(recipe) && case.limit.is_none())
        .unwrap()
}

fn icu4x_case(input: LocaleResolutionRecipe, engine: Engine) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::Icu4xLocaleResolution { input, engine })
        .unwrap()
}

fn resolved(declaration: &Declaration) -> Resolved {
    let Output::LocaleResolution(resolved) = prepare(declaration).unwrap().output else {
        panic!("wrong owner boundary")
    };
    resolved
}

fn locales(resolved: &Resolved) -> &ResolvedLocales {
    resolved.result.as_ref().unwrap()
}

fn names(locales: &[CanonicalLocale]) -> Vec<&str> {
    locales.iter().map(CanonicalLocale::as_str).collect()
}

/// A subject as its role, its owner, and its locale or rejected spelling.
fn describe(subject: &LocaleSubject) -> String {
    let key = |key: &LocaleKey| match key {
        LocaleKey::Canonical(locale) => locale.as_str().to_owned(),
        LocaleKey::Content(content) => content.as_str().to_owned(),
    };
    match subject {
        LocaleSubject::Occurrences => "occurrences".to_owned(),
        LocaleSubject::RequestedLocale(own) => format!("requested {}", key(own)),
        LocaleSubject::RequestedLocales => "requested set".to_owned(),
        LocaleSubject::DefaultSourceLocale(own) => format!("source {}", key(own)),
        LocaleSubject::DefaultRequestedLocale(own) => format!("default {}", key(own)),
        LocaleSubject::NegotiationAliasKey(own) => format!("alias {}", key(own)),
        LocaleSubject::NegotiationAliasValue(owner, own) => {
            format!("alias {} value {}", key(owner), key(own))
        }
        LocaleSubject::MessageFallbackKey(own) => format!("fallback {}", key(own)),
        LocaleSubject::MessageFallbackCandidate(owner, position, own) => {
            format!("fallback {} candidate {position} {}", key(owner), key(own))
        }
        LocaleSubject::TargetRequestedLocale(id, own) => {
            format!("{} requested {}", id.as_str(), key(own))
        }
        LocaleSubject::TargetDefaultRequestedLocale(id, own) => {
            format!("{} default {}", id.as_str(), key(own))
        }
    }
}

/// Each blocking result as its subject and its code, in 015's order.
fn issues(resolved: &Resolved) -> Vec<(String, Code)> {
    resolved
        .evaluation
        .issues()
        .map(|issue| {
            let Subject::Locale(subject) = &issue.unit().subject else {
                panic!("only Phase 7 runs in this boundary")
            };
            (describe(subject), issue.code())
        })
        .collect()
}

#[test]
fn resolution_cases_keep_their_rows_and_append_after_the_icu4x_rows() {
    let cases = declarations();
    assert_eq!(cases.len(), 213);
    let resolution = |cases: &[Declaration]| {
        cases
            .iter()
            .all(|case| case.operation == Operation::LocaleResolution)
    };
    assert!(cases[..94]
        .iter()
        .all(|case| case.operation != Operation::LocaleResolution));
    assert!(resolution(&cases[94..127]));
    assert!(cases[127..141]
        .iter()
        .all(|case| case.operation == Operation::LocaleCanonicalization));
    assert!(resolution(&cases[141..160]));
    for (rows, edges) in [
        (&cases[94..127], 8),
        (&cases[141..150], 2),
        (&cases[150..160], 0),
    ] {
        assert_eq!(
            rows.iter().filter(|case| case.limit.is_some()).count(),
            edges
        );
    }
    for declaration in cases
        .iter()
        .filter(|case| case.operation == Operation::LocaleResolution)
    {
        let candidate = prepare(declaration).unwrap();
        assert!(
            candidate.input_limits.is_some(),
            "complete config preparation is explicit"
        );
        let Output::LocaleResolution(resolved) = candidate.output else {
            panic!("wrong owner boundary")
        };
        assert!(resolved.result.is_ok());
        assert_eq!(
            resolved.evaluation.issues().next().is_none(),
            declaration.expected_kind == ExpectedKind::LocaleResolutionComplete,
            "{declaration:?}"
        );
    }
}

#[test]
fn set_and_member_order_change_only_the_entry_domain() {
    let observed = |recipe| prepare(&case(recipe)).unwrap().observation;
    for (left, right) in [
        (R::Multi, R::Reordered),
        (R::MultipleDuplicates, R::DuplicateReordered),
        (R::InvalidAndDuplicate, R::InvalidReordered),
        (R::EveryRole, R::EveryRoleReordered),
    ] {
        let (left, right) = (observed(left), observed(right));
        assert_eq!(left.shared, right.shared, "{left:?} / {right:?}");
        assert_ne!(left.entry, right.entry, "{left:?} / {right:?}");
    }
}

#[test]
fn aliases_add_suggestions_and_never_change_a_value_or_unit() {
    for (canonical, aliased, corrected) in [
        (R::Multi, R::Aliased, 5),
        (R::ExpandedAlias, R::ExpandingAlias, 2),
        (R::EveryRole, R::EveryRoleAliased, 11),
    ] {
        let observed = |recipe| prepare(&case(recipe)).unwrap().observation;
        assert_ne!(observed(canonical).shared, observed(aliased).shared);
        let (canonical, aliased) = (resolved(&case(canonical)), resolved(&case(aliased)));
        let (left, right) = (locales(&canonical), locales(&aliased));
        assert_eq!(left.requested_set, right.requested_set);
        assert_eq!(left.project, right.project);
        assert_eq!(left.negotiation_aliases, right.negotiation_aliases);
        assert_eq!(left.message_fallback, right.message_fallback);
        assert_eq!(left.targets, right.targets);
        assert!(canonical
            .evaluation
            .statuses()
            .eq(aliased.evaluation.statuses()));
        assert_eq!(canonical.evaluation.notices().count(), 0);
        assert_eq!(aliased.evaluation.notices().count(), corrected);
        assert_eq!(
            right
                .occurrences
                .iter()
                .filter(|occurrence| occurrence.corrected)
                .count(),
            corrected
        );
    }
}

#[test]
fn changed_meanings_change_the_shared_domain_and_unread_inputs_do_not() {
    let shared = |recipe| prepare(&case(recipe)).unwrap().observation.shared;
    for changed in [
        R::ChangedSet,
        R::ChangedDefault,
        R::ChangedSource,
        R::AbsentSource,
    ] {
        assert_ne!(shared(R::Multi), shared(changed), "{changed:?}");
    }
    assert_ne!(shared(R::Minimal), shared(R::SourceUnd));
    // An unselected profile, and a coverage selector Phase 7 only counts,
    // change the input and nothing Phase 7 decides.
    for (left, right) in [
        (R::Minimal, R::OtherProfile),
        (R::EveryRole, R::CoverageUncanonicalized),
    ] {
        let (left, right) = (
            prepare(&case(left)).unwrap(),
            prepare(&case(right)).unwrap(),
        );
        assert_eq!(left.observation, right.observation);
        assert_ne!(
            super::context::observe(&left).unwrap(),
            super::context::observe(&right).unwrap()
        );
    }
}

#[test]
fn complete_recipes_have_independent_value_and_correction_expectations() {
    let schema = super::super::operation::Schema::for_model().unwrap();
    let oracle = jsonschema::draft7::new(schema.schema_body()).unwrap();
    let multi = ["en", "en-US", "he-IL", "ja"];
    let every = ["en", "he-IL", "ja"];
    let fixture = [
        "ar-EG-u-nu-latn",
        "de",
        "de-DE",
        "en",
        "en-US",
        "en-u-ca-gregory-nu-latn",
        "fr",
        "fr-FR",
        "he-IL",
        "ja",
        "und",
        "und-u-ca-islamic-civil",
        "zh-Hant-TW",
    ];
    let expanded = ["und-u-ca-islamic-civil"];
    for recipe in R::ALL {
        // Every recipe enters Phase 7 through a complete structural model,
        // even when Phase 7 blocks its locales afterwards.
        assert!(oracle.is_valid(&recipe.value()), "{recipe:?}");
        if !recipe.completes() {
            continue;
        }
        let (source, requested, default, occurrences, corrected): (
            Option<&str>,
            &[&str],
            &str,
            u64,
            usize,
        ) = match recipe {
            R::Minimal | R::OtherProfile => (None, &["en"], "en", 3, 0),
            R::SourceUnd => (Some("und"), &["en"], "en", 4, 0),
            R::SourceEn => (Some("en"), &["en"], "en", 4, 0),
            R::Multi | R::Reordered => (Some("en"), &multi, "en-US", 7, 0),
            R::Aliased => (Some("en"), &multi, "en-US", 7, 5),
            R::ChangedSet => (Some("en"), &["en-US", "ja"], "en-US", 5, 0),
            R::ChangedDefault => (Some("en"), &multi, "ja", 7, 0),
            R::ChangedSource => (Some("fr"), &multi, "en-US", 7, 0),
            R::AbsentSource => (None, &multi, "en-US", 6, 0),
            R::EveryRole | R::EveryRoleReordered | R::CoverageUncanonicalized => {
                (Some("en"), &every, "ja", 21, 0)
            }
            R::EveryRoleAliased => (Some("en"), &every, "ja", 21, 11),
            R::ExpandingAlias => (None, &expanded, expanded[0], 3, 2),
            R::ExpandedAlias => (None, &expanded, expanded[0], 3, 0),
            R::ManyTargets => (None, &["en"], "en", 34, 0),
            R::EveryFixtureLocale => (None, &fixture, "en", 27, 0),
            _ => panic!("new complete recipe needs an independent expectation"),
        };
        let resolved = resolved(&case(recipe));
        let locales = locales(&resolved);
        let project = locales.project.as_ref().unwrap();
        assert_eq!(
            project.source_default().map(CanonicalLocale::as_str),
            source,
            "{recipe:?}"
        );
        assert_eq!(
            names(project.requested().as_slice()),
            requested,
            "{recipe:?}"
        );
        assert_eq!(project.requested_default().as_str(), default, "{recipe:?}");
        assert_eq!(
            locales.occurrence_total,
            Some(Actual::Exact(occurrences)),
            "{recipe:?}"
        );
        assert_eq!(
            locales
                .occurrences
                .iter()
                .filter(|occurrence| occurrence.corrected)
                .count(),
            corrected,
            "{recipe:?}"
        );
    }
}

#[test]
fn every_role_hands_each_locale_policy_input_to_the_later_phases() {
    fn canonical(locale: &Admitted<CanonicalLocale>) -> &str {
        locale.as_ref().unwrap().as_str()
    }
    let resolved = resolved(&case(R::EveryRole));
    let locales = locales(&resolved);
    let aliases = locales
        .negotiation_aliases
        .as_ref()
        .unwrap()
        .iter()
        .map(|alias| (canonical(&alias.key.locale), canonical(&alias.value.locale)))
        .collect::<Vec<_>>();
    assert_eq!(aliases, [("en-US", "en"), ("fr", "fr-FR")]);
    let fallback = locales
        .message_fallback
        .as_ref()
        .unwrap()
        .iter()
        .map(|source| {
            let candidates = source
                .candidates
                .iter()
                .map(|candidate| match candidate {
                    FallbackLocale::Literal(literal) => canonical(&literal.locale),
                    FallbackLocale::IntentSource => "intent-source-locale",
                })
                .collect::<Vec<_>>();
            (canonical(&source.key.locale), candidates)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        fallback,
        [
            ("he-IL", vec!["en"]),
            ("ja", vec!["en", "intent-source-locale"])
        ]
    );
    let targets = locales
        .targets
        .as_ref()
        .unwrap()
        .iter()
        .map(|target| {
            (
                target.id.as_str(),
                names(target.requested.as_ref().unwrap().as_slice()),
                target
                    .default
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .map(CanonicalLocale::as_str),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        [
            ("browser", vec!["en", "ja"], Some("ja")),
            ("server", vec!["en", "he-IL"], None)
        ]
    );
}

#[test]
fn blocked_recipes_keep_exact_subjects_codes_and_independent_checks() {
    let duplicate = Code::LocaleDuplicate;
    let invalid = Code::LocaleInvalid;
    for (recipe, occurrences, requested, expected) in [
        (
            R::ExactDuplicate,
            4,
            Some(1),
            vec![("requested en", duplicate)],
        ),
        (
            R::AliasDuplicate,
            4,
            Some(1),
            vec![("requested en", duplicate)],
        ),
        (
            R::MultipleDuplicates,
            9,
            Some(3),
            vec![
                ("requested en", duplicate),
                ("requested en-US", duplicate),
                ("requested ja", duplicate),
            ],
        ),
        (
            R::DuplicateReordered,
            9,
            Some(3),
            vec![
                ("requested en", duplicate),
                ("requested en-US", duplicate),
                ("requested ja", duplicate),
            ],
        ),
        (
            R::DuplicateHeavy,
            34,
            Some(1),
            vec![("requested en", duplicate)],
        ),
        (
            R::InvalidSource,
            4,
            Some(1),
            vec![("source en_US", invalid)],
        ),
        // Without the set, the default's membership waits instead of failing.
        (
            R::InvalidRequested,
            3,
            None,
            vec![("requested zz", invalid)],
        ),
        (
            R::InvalidDefault,
            3,
            Some(1),
            vec![("default en_US", invalid)],
        ),
        (
            R::InvalidAndDuplicate,
            6,
            None,
            vec![
                ("requested en-US", duplicate),
                ("requested zz", invalid),
                ("source en_US", invalid),
            ],
        ),
        (
            R::InvalidReordered,
            6,
            None,
            vec![
                ("requested en-US", duplicate),
                ("requested zz", invalid),
                ("source en_US", invalid),
            ],
        ),
        (
            R::DefaultNotMember,
            3,
            Some(1),
            vec![("default fr", Code::DefaultRequestedLocaleNotInProject)],
        ),
        (
            R::EveryRoleInvalid,
            21,
            Some(3),
            vec![
                ("alias root", invalid),
                ("alias root value zz", invalid),
                ("fallback en_US", invalid),
                ("fallback en_US candidate 0 en-x-brand", invalid),
                ("browser requested Latn", invalid),
                ("browser default en-u-ca-madeup", invalid),
            ],
        ),
        (
            R::TargetDuplicate,
            22,
            Some(3),
            vec![("server requested he-IL", duplicate)],
        ),
    ] {
        let resolved = resolved(&case(recipe));
        assert_eq!(
            issues(&resolved),
            expected
                .into_iter()
                .map(|(subject, code)| (subject.to_owned(), code))
                .collect::<Vec<_>>(),
            "{recipe:?}"
        );
        let locales = locales(&resolved);
        assert_eq!(
            locales.occurrence_total,
            Some(Actual::Exact(occurrences)),
            "{recipe:?}"
        );
        assert_eq!(
            locales
                .requested_set
                .as_ref()
                .ok()
                .map(|set| set.as_slice().len()),
            requested,
            "{recipe:?}"
        );
        // A role beyond the project's blocks nothing the project resolved.
        assert_eq!(
            locales.project.is_ok(),
            matches!(recipe, R::EveryRoleInvalid | R::TargetDuplicate),
            "{recipe:?}"
        );
    }
}

#[test]
fn the_occurrence_bound_counts_every_role_at_its_exact_value_and_first_over() {
    for edge in [LimitEdge::Exact, LimitEdge::FirstOver] {
        let declaration = declarations()
            .into_iter()
            .find(|case| {
                case.fixture == Recipe::LocaleResolution(R::EveryRole)
                    && case.limit == Some((LimitKind::ResolutionLocaleOccurrences, edge))
            })
            .unwrap();
        let resolved = resolved(&declaration);
        assert_eq!(locales(&resolved).occurrence_total, Some(Actual::Exact(21)));
        if edge == LimitEdge::Exact {
            assert!(issues(&resolved).is_empty());
            continue;
        }
        assert_eq!(
            issues(&resolved),
            [("occurrences".to_owned(), Code::ResourceLimitExceeded)]
        );
        let key = resolved.evaluation.issues().next().unwrap();
        let exceeded = resolved.evaluation.limit(key).unwrap();
        assert_eq!(
            (exceeded.bound.name, exceeded.limit.get(), exceeded.actual),
            ("maxLocaleOccurrences", 20, Actual::Exact(21))
        );
        // Nothing past the bound is canonicalized or checked.
        assert!(resolved.evaluation.statuses().all(|(unit, status)| {
            unit.subject == Subject::Locale(LocaleSubject::Occurrences)
                || matches!(status, Status::NotEvaluated(_))
        }));
    }
}

#[test]
fn the_input_context_refuses_a_candidate_bound_to_another_input() {
    use std::sync::Arc;

    use super::context::{observe, ContextFailure};
    use crate::benchmark::operation::{Prepared, Schema};
    use crate::input_limits::Bound;
    use crate::materialize::materialize_file;
    use crate::structural::StructuralLimits;

    for change in 0..4 {
        let mut candidate = prepare(&case(R::Minimal)).unwrap();
        let Prepared::LocaleResolution(prepared) = &mut candidate.prepared else {
            unreachable!()
        };
        match change {
            0 => candidate.declaration.fixture = Recipe::Minimal,
            1 => candidate.declaration.selector = Selector::Absent,
            2 => prepared.selected = serde_json::from_value(serde_json::json!("other")).unwrap(),
            // The selected declaration's fields cannot be borrowed from an
            // incomplete root.
            _ => {
                let doc =
                    materialize_file(Arc::from(&b"{}"[..]), crate::materialize_tests::limits())
                        .unwrap();
                let limits = StructuralLimits {
                    max_profiles: Bound::new(64).unwrap(),
                    max_profile_id_bytes: Bound::new(256).unwrap(),
                    max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
                };
                prepared.analysis = Schema::for_model()
                    .unwrap()
                    .analyze(Arc::new(doc), limits)
                    .unwrap();
            }
        }
        assert_eq!(
            observe(&candidate),
            Err(ContextFailure::LocaleInputMismatch),
            "change {change}"
        );
    }
}

#[test]
fn icu4x_resolution_cases_follow_every_finite_row_with_both_engines_for_each_input() {
    let cases = declarations();
    let expected = LocaleResolutionRecipe::ICU4X
        .into_iter()
        .flat_map(|input| {
            [Engine::Reused, Engine::Fresh]
                .map(|engine| Recipe::Icu4xLocaleResolution { input, engine })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cases[150..160]
            .iter()
            .map(|case| case.fixture)
            .collect::<Vec<_>>(),
        expected
    );
    for case in &cases[150..160] {
        assert_eq!(case.operation, Operation::LocaleResolution);
        assert_eq!(case.selector, Selector::App);
        let Recipe::Icu4xLocaleResolution { input, .. } = case.fixture else {
            unreachable!()
        };
        // The scales are no finite rows; every other input is one.
        assert_eq!(
            LocaleResolutionRecipe::ALL.contains(&input),
            !input.scales(),
            "{input:?}"
        );
    }
    assert!(cases[..150]
        .iter()
        .all(|case| !matches!(case.fixture, Recipe::Icu4xLocaleResolution { .. })));
}

/// The finite table agrees with the ICU4X provider on every row (PR 4d), and
/// these recipes spell only its rows: either provider and either engine give
/// one semantic observation and one logical work.
#[test]
fn either_provider_resolves_a_finite_recipe_to_one_observation_and_work() {
    for input in [R::EveryRole, R::EveryRoleAliased, R::EveryRoleInvalid] {
        let finite = prepare(&case(input)).unwrap();
        for engine in [Engine::Reused, Engine::Fresh] {
            let icu4x = prepare(&icu4x_case(input, engine)).unwrap();
            assert_eq!(
                icu4x.observation, finite.observation,
                "{input:?}, {engine:?}"
            );
            assert_eq!(
                icu4x.logical_work, finite.logical_work,
                "{input:?}, {engine:?}"
            );
        }
    }
}

/// The engine state is a fact of the input, never of the result: for every
/// input, the scales included, both engines give one semantic observation and
/// one logical work, and the checked-in rows pin the same equivalence.
#[test]
fn fresh_and_reused_engines_agree_on_phase_7_for_every_input() {
    use super::context::observe;

    let registry = super::registry::Registry::load().unwrap();
    for input in R::ICU4X {
        let reused = prepare(&icu4x_case(input, Engine::Reused)).unwrap();
        let fresh = prepare(&icu4x_case(input, Engine::Fresh)).unwrap();
        assert_eq!(reused.observation, fresh.observation, "{input:?}");
        assert_eq!(reused.logical_work, fresh.logical_work, "{input:?}");
        assert_ne!(observe(&reused).unwrap(), observe(&fresh).unwrap());
        let reused = registry
            .prepare(&icu4x_case(input, Engine::Reused))
            .unwrap();
        let fresh = registry.prepare(&icu4x_case(input, Engine::Fresh)).unwrap();
        assert_eq!(reused.expected(), fresh.expected(), "{input:?}");
        assert_eq!(reused.expected_work(), fresh.expected_work(), "{input:?}");
        assert_ne!(reused.input_context(), fresh.input_context());
    }
}

/// 256 targets of one locale scale the spellings alone; 128 languages
/// requested twice scale the canonical locales with them. Each language is
/// its own canonical form, so neither scale suggests a correction.
#[test]
fn the_scales_grow_spellings_and_canonical_locales_through_icu4x() {
    let languages = SCALE_LANGUAGES.split(' ').collect::<Vec<_>>();
    for engine in [Engine::Reused, Engine::Fresh] {
        let spellings = resolved(&icu4x_case(R::OccurrenceScale, engine));
        let canonical = resolved(&icu4x_case(R::CanonicalScale, engine));
        for (scale, total, project_requested, target_requested, target_count) in [
            (&spellings, 258, vec!["en"], vec!["en"], 256),
            (&canonical, 257, languages.clone(), languages.clone(), 1),
        ] {
            let locales = locales(scale);
            assert!(issues(scale).is_empty());
            assert_eq!(scale.evaluation.notices().count(), 0);
            assert_eq!(locales.occurrence_total, Some(Actual::Exact(total)));
            let project = locales.project.as_ref().unwrap();
            assert_eq!(names(project.requested().as_slice()), project_requested);
            assert_eq!(project.requested_default().as_str(), "en");
            let targets = locales.targets.as_ref().unwrap();
            assert_eq!(targets.len(), target_count);
            for target in targets {
                assert_eq!(
                    names(target.requested.as_ref().unwrap().as_slice()),
                    target_requested
                );
            }
        }
    }
}

#[test]
fn the_input_context_refuses_a_provider_or_engine_other_than_the_declared_one() {
    use super::context::{observe, ContextFailure};

    let finite = prepare(&case(R::EveryRole)).unwrap();
    let icu4x = |engine| prepare(&icu4x_case(R::EveryRole, engine)).unwrap();
    for (mut candidate, declared) in [
        // ICU4X prepared, the finite recipe declared.
        (icu4x(Engine::Reused), finite.declaration.clone()),
        // A reused engine prepared, a fresh one declared.
        (
            icu4x(Engine::Reused),
            icu4x_case(R::EveryRole, Engine::Fresh),
        ),
        (
            icu4x(Engine::Fresh),
            icu4x_case(R::EveryRole, Engine::Reused),
        ),
        // The finite provider prepared, ICU4X declared.
        (
            prepare(&case(R::EveryRole)).unwrap(),
            icu4x_case(R::EveryRole, Engine::Reused),
        ),
    ] {
        assert!(observe(&candidate).is_ok());
        candidate.declaration = declared;
        assert_eq!(
            observe(&candidate),
            Err(ContextFailure::LocaleInputMismatch)
        );
    }
}
