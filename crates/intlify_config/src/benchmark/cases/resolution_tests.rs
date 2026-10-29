// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::prepare::prepare;
use super::resolution_inputs::SCALE_LANGUAGES;
use super::*;
use crate::benchmark::locale_resolution::Resolved;
use crate::benchmark::operation::Output;
use crate::locale::CanonicalLocale;
use crate::model::CoverageMode;
use crate::resolver::coverage::{CoverageSubject, Decision};
use crate::resolver::evaluation::{Actual, Admitted, Check, Code, Status, Subject};
use crate::resolver::fallback::{Candidate, FallbackSubject};
use crate::resolver::locales::{FallbackLocale, LocaleKey, LocaleSubject, ResolvedLocales};
use crate::resolver::negotiation::NegotiationSubject;

use LocalePolicyResolutionRecipe as R;

fn case(recipe: LocalePolicyResolutionRecipe) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::LocalePolicyResolution(recipe) && case.limit.is_none())
        .unwrap()
}

fn icu4x_case(input: LocalePolicyResolutionRecipe, engine: Engine) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::Icu4xLocalePolicyResolution { input, engine })
        .unwrap()
}

fn resolved(declaration: &Declaration) -> Resolved {
    let Output::LocalePolicyResolution(resolved) = prepare(declaration).unwrap().output else {
        panic!("wrong owner boundary")
    };
    resolved
}

fn locales(resolved: &Resolved) -> &ResolvedLocales {
    &resolved.result.as_ref().unwrap().locales
}

fn names(locales: &[CanonicalLocale]) -> Vec<&str> {
    locales.iter().map(CanonicalLocale::as_str).collect()
}

/// A subject key's locale or rejected spelling.
fn key(key: &LocaleKey) -> String {
    match key {
        LocaleKey::Canonical(locale) => locale.as_str().to_owned(),
        LocaleKey::Content(content) => content.as_str().to_owned(),
    }
}

/// A subject as its role, its owner, and its locale or rejected spelling.
fn describe(subject: &LocaleSubject) -> String {
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

/// Each blocking result as its subject and its code, in 015's order. No
/// recipe here blocks a Phase 8 unit but a coverage selector's locale.
fn issues(resolved: &Resolved) -> Vec<(String, Code)> {
    resolved
        .evaluation
        .issues()
        .map(|issue| {
            let subject = match &issue.unit().subject {
                Subject::Locale(subject) => describe(subject),
                Subject::Coverage(CoverageSubject::Locale(_, own)) => {
                    format!("coverage {}", key(own))
                }
                other => panic!("no recipe blocks {other:?}"),
            };
            (subject, issue.code())
        })
        .collect()
}

#[test]
fn resolution_cases_keep_their_rows_and_append_after_the_icu4x_rows() {
    let cases = declarations();
    assert_eq!(cases.len(), 303);
    let resolution = |cases: &[Declaration]| {
        cases
            .iter()
            .all(|case| case.operation == Operation::LocalePolicyResolution)
    };
    assert!(cases[..94]
        .iter()
        .all(|case| case.operation != Operation::LocalePolicyResolution));
    assert!(resolution(&cases[94..127]));
    assert!(cases[127..141]
        .iter()
        .all(|case| case.operation == Operation::LocaleCanonicalization));
    assert!(resolution(&cases[141..160]));
    assert!(cases[160..213]
        .iter()
        .all(|case| case.operation != Operation::LocalePolicyResolution));
    // Phase 8's workloads follow the Phase 9 rows: the finite recipes, the
    // eight bounds' edges on every role, and the ICU4X scales.
    assert!(resolution(&cases[213..252]));
    assert!(cases[244..252].iter().all(|case| matches!(
        case.fixture,
        Recipe::Icu4xLocalePolicyResolution { input, .. }
            if R::POLICY_ICU4X.contains(&input)
    )));
    for (rows, edges) in [
        (&cases[94..127], 8),
        (&cases[141..150], 2),
        (&cases[150..160], 0),
        (&cases[213..228], 0),
        (&cases[228..244], 16),
        (&cases[244..252], 0),
    ] {
        assert_eq!(
            rows.iter().filter(|case| case.limit.is_some()).count(),
            edges
        );
    }
    for declaration in cases
        .iter()
        .filter(|case| case.operation == Operation::LocalePolicyResolution)
    {
        let candidate = prepare(declaration).unwrap();
        assert!(
            candidate.input_limits.is_some(),
            "complete config preparation is explicit"
        );
        let Output::LocalePolicyResolution(resolved) = candidate.output else {
            panic!("wrong owner boundary")
        };
        assert!(resolved.result.is_ok());
        assert_eq!(
            resolved.evaluation.issues().next().is_none(),
            declaration.expected_kind == ExpectedKind::LocalePolicyResolutionComplete,
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
        (R::EveryRole, R::EveryRoleAliased, 12),
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
        let (before, after) = (
            canonical.result.as_ref().unwrap(),
            aliased.result.as_ref().unwrap(),
        );
        assert_eq!(before.negotiation, after.negotiation);
        assert_eq!(before.fallback, after.fallback);
        assert_eq!(before.coverage, after.coverage);
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
    // An unselected profile changes the input and nothing Phases 7 and 8
    // decide.
    let (left, right) = (
        prepare(&case(R::Minimal)).unwrap(),
        prepare(&case(R::OtherProfile)).unwrap(),
    );
    assert_eq!(left.observation, right.observation);
    assert_ne!(
        super::context::observe(&left).unwrap(),
        super::context::observe(&right).unwrap()
    );
}

/// Phase 7 counts a coverage selector and leaves it to Phase 8, which
/// rejects `zz`: the coverage policy is unavailable while negotiation and
/// message fallback resolve, and no spelling Phase 7 placed moves.
#[test]
fn a_coverage_selector_phase_7_counts_is_phase_8s_to_reject() {
    let (every, rejected) = (
        prepare(&case(R::EveryRole)).unwrap(),
        prepare(&case(R::CoverageUncanonicalized)).unwrap(),
    );
    assert_ne!(every.observation.shared, rejected.observation.shared);
    assert_eq!(every.observation.entry, rejected.observation.entry);
    let resolved = resolved(&case(R::CoverageUncanonicalized));
    assert_eq!(
        issues(&resolved),
        [("coverage zz".to_owned(), Code::LocaleInvalid)]
    );
    let policies = resolved.result.as_ref().unwrap();
    assert!(policies.locales.project.is_ok());
    assert!(policies.negotiation.is_ok());
    assert!(policies.fallback.is_ok());
    assert!(policies.coverage.is_err());
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
            R::EveryRole | R::EveryRoleReordered => (Some("en"), &every, "ja", 21, 0),
            R::EveryRoleAliased => (Some("en"), &every, "ja", 21, 12),
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
fn every_role_hands_each_locale_policy_input_to_phase_8_which_resolves_each_policy() {
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
    assert_eq!(aliases, [("en-US", "en"), ("fr", "en")]);
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
    // Phase 8 resolves each policy under the fixture-owned specifications
    // and vocabulary.
    let policies = resolved.result.as_ref().unwrap();
    let negotiation = policies.negotiation.as_ref().unwrap();
    assert_eq!(
        negotiation.specification().identity,
        "intlify-locale-negotiation"
    );
    assert_eq!(
        negotiation
            .aliases()
            .iter()
            .map(|(key, destination)| (key.as_str(), destination.as_str()))
            .collect::<Vec<_>>(),
        [("en-US", "en"), ("fr", "en")]
    );
    let fallback = policies.fallback.as_ref().unwrap();
    assert_eq!(
        fallback.specification().identity,
        "intlify-message-locale-fallback"
    );
    assert_eq!(
        fallback
            .sequences()
            .iter()
            .map(|(source, candidates)| {
                let candidates = candidates
                    .iter()
                    .map(|candidate| match candidate {
                        Candidate::Literal(literal) => literal.as_str(),
                        Candidate::IntentSource => "intent-source-locale",
                    })
                    .collect::<Vec<_>>();
                (source.as_str(), candidates)
            })
            .collect::<Vec<_>>(),
        [
            ("he-IL", vec!["en"]),
            ("ja", vec!["en", "intent-source-locale"])
        ]
    );
    // The default, the `ja` rule, the `checkout` rule, and the rule over
    // both: it outranks the `checkout` rule, and shares `ja`/`checkout` with
    // the `ja` rule, since neither of those two is narrower.
    let coverage = policies.coverage.as_ref().unwrap();
    assert_eq!(
        coverage.specification().identity,
        "intlify-localization-coverage"
    );
    let (direct, fallback_allowed) = (CoverageMode::DirectRequired, CoverageMode::FallbackAllowed);
    assert_eq!(
        coverage
            .basis()
            .iter()
            .map(|((locale, surface), basis)| {
                let rules = match &basis.decision {
                    Decision::Default => None,
                    Decision::Rules(domains) => Some(domains.len()),
                };
                (locale.as_str(), surface.as_str(), basis.mode, rules)
            })
            .collect::<Vec<_>>(),
        [
            ("en", "checkout", direct, Some(1)),
            ("en", "internal-tool", direct, None),
            ("en", "nav", direct, None),
            ("he-IL", "checkout", fallback_allowed, Some(1)),
            ("he-IL", "internal-tool", direct, None),
            ("he-IL", "nav", direct, None),
            ("ja", "checkout", fallback_allowed, Some(2)),
            ("ja", "internal-tool", fallback_allowed, Some(1)),
            ("ja", "nav", fallback_allowed, Some(1)),
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
                case.fixture == Recipe::LocalePolicyResolution(R::EveryRole)
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
        // Nothing past the bound is canonicalized or checked. Phase 8's
        // bounds over what was declared, the Intent source candidate, and the
        // two surface items naming a vocabulary member need no canonical
        // locale, so they still run; every other unit waits on the bound.
        let runs = |unit: &crate::resolver::evaluation::Unit| match &unit.subject {
            Subject::Locale(subject) => *subject == LocaleSubject::Occurrences,
            Subject::Negotiation(NegotiationSubject::Aliases)
            | Subject::Fallback(FallbackSubject::Sources | FallbackSubject::IntentSource(_))
            | Subject::Coverage(
                CoverageSubject::Rules
                | CoverageSubject::LocaleSelectors
                | CoverageSubject::SurfaceSelectors
                | CoverageSubject::SurfaceClass(..),
            ) => true,
            Subject::Fallback(FallbackSubject::Source(_)) => unit.check == Check::WithinBound,
            _ => false,
        };
        assert!(resolved
            .evaluation
            .statuses()
            .all(|(unit, status)| runs(unit) == (*status == Status::Evaluated)));
        assert_eq!(
            resolved
                .evaluation
                .statuses()
                .filter(|(unit, _)| runs(unit))
                .count(),
            13
        );
        // Evaluated and waiting, per policy.
        let mut policies = [[0; 2]; 3];
        for (unit, status) in resolved.evaluation.statuses() {
            let policy = match unit.subject {
                Subject::Negotiation(_) => 0,
                Subject::Fallback(_) => 1,
                Subject::Coverage(_) => 2,
                _ => continue,
            };
            policies[policy][usize::from(*status != Status::Evaluated)] += 1;
        }
        assert_eq!(policies, [[1, 4], [4, 8], [7, 11]]);
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
        let Prepared::LocalePolicyResolution(prepared) = &mut candidate.prepared else {
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
    let expected = LocalePolicyResolutionRecipe::ICU4X
        .into_iter()
        .flat_map(|input| {
            [Engine::Reused, Engine::Fresh]
                .map(|engine| Recipe::Icu4xLocalePolicyResolution { input, engine })
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
        assert_eq!(case.operation, Operation::LocalePolicyResolution);
        assert_eq!(case.selector, Selector::App);
        let Recipe::Icu4xLocalePolicyResolution { input, .. } = case.fixture else {
            unreachable!()
        };
        // The scales are no finite rows; every other input is one.
        assert_eq!(
            LocalePolicyResolutionRecipe::ALL.contains(&input),
            !input.scales(),
            "{input:?}"
        );
    }
    assert!(cases[..150]
        .iter()
        .all(|case| !matches!(case.fixture, Recipe::Icu4xLocalePolicyResolution { .. })));
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
fn fresh_and_reused_engines_agree_on_phases_7_and_8_for_every_input() {
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

/// A Phase 8 subject as its policy, its key, and its locale or spelling.
fn describe_policy(subject: &Subject) -> String {
    use crate::resolver::coverage::SurfaceKey;
    use crate::resolver::fallback::FallbackSubject as F;
    use crate::resolver::negotiation::NegotiationSubject as N;

    match subject {
        Subject::Locale(own) => describe(own),
        Subject::Negotiation(N::Aliases) => "aliases".to_owned(),
        Subject::Negotiation(N::Key(own)) => format!("alias {}", key(own)),
        Subject::Negotiation(N::Destination(owner, own)) => {
            format!("alias {} to {}", key(owner), key(own))
        }
        Subject::Fallback(F::Sources) => "fallback sources".to_owned(),
        Subject::Fallback(F::Source(own)) => format!("fallback {}", key(own)),
        Subject::Fallback(F::Literal(owner, own)) => {
            format!("fallback {} literal {}", key(owner), key(own))
        }
        Subject::Fallback(F::IntentSource(owner)) => {
            format!("fallback {} intent source", key(owner))
        }
        Subject::Coverage(CoverageSubject::Locale(_, own)) => format!("coverage {}", key(own)),
        Subject::Coverage(CoverageSubject::SurfaceClass(_, own)) => match own {
            SurfaceKey::Member(member) => format!("surface {}", member.as_str()),
            SurfaceKey::Content(content) => format!("surface {}", content.as_str()),
        },
        Subject::Coverage(CoverageSubject::Domain(domain)) => format!(
            "domain {} by {}",
            names(domain.locales()).join(","),
            domain
                .surfaces()
                .iter()
                .map(intlify_shared_json::token::Token::as_str)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Subject::Coverage(CoverageSubject::Cell(locale, surface)) => {
            format!("cell {} {}", locale.as_str(), surface.as_str())
        }
        Subject::Coverage(other) => format!("{other:?}"),
        other => panic!("not a Phase 7 or Phase 8 subject: {other:?}"),
    }
}

/// Each blocking result as its subject, code, and reason, in 015's order.
fn policy_issues(
    resolved: &Resolved,
) -> Vec<(String, Code, Option<crate::resolver::evaluation::Reason>)> {
    resolved
        .evaluation
        .issues()
        .map(|issue| {
            (
                describe_policy(&issue.unit().subject),
                issue.code(),
                issue.reason(),
            )
        })
        .collect()
}

/// One change to every role blocks the one Phase 8 check it names, with its
/// exact subject, code, and reason; the dense declaration blocks four at
/// once. Its conflicting rule normalizes, but every cell waits on the rule
/// naming an unknown surface class, so the conflict is never decided.
#[test]
fn phase_8_workloads_block_exactly_the_check_each_change_names() {
    use crate::resolver::evaluation::Reason;

    let duplicate = Code::NegotiationAliasDuplicate;
    let sequence = Code::MessageFallbackSequenceInvalid;
    let rule = Code::CoverageRuleInvalid;
    for (recipe, expected) in [
        (
            R::AliasConflictingKeys,
            vec![(
                "alias en-US",
                duplicate,
                Some(Reason::ConflictingDestination),
            )],
        ),
        (
            R::AliasRepeatedKeys,
            vec![("alias en-US", duplicate, Some(Reason::SameDestination))],
        ),
        (
            R::AliasUnrequestedDestination,
            vec![(
                "alias fr to fr-FR",
                Code::NegotiationAliasDestinationNotRequested,
                None,
            )],
        ),
        (
            R::FallbackUnrequestedSource,
            vec![("fallback de", Code::MessageFallbackSourceNotRequested, None)],
        ),
        (
            R::FallbackSelfReference,
            vec![(
                "fallback he-IL literal he-IL",
                sequence,
                Some(Reason::LiteralSelfReference),
            )],
        ),
        (
            R::FallbackRepeatedLiteral,
            vec![(
                "fallback he-IL literal en",
                sequence,
                Some(Reason::LiteralDuplicate),
            )],
        ),
        (
            R::FallbackRepeatedIntentSource,
            vec![(
                "fallback ja intent source",
                sequence,
                Some(Reason::IntentSourceDuplicate),
            )],
        ),
        (
            R::CoverageUnrequestedLocale,
            vec![("coverage fr", rule, Some(Reason::LocaleNotRequested))],
        ),
        (
            R::CoverageRepeatedLocale,
            vec![("coverage ja", Code::LocaleDuplicate, None)],
        ),
        (
            R::CoverageUnknownSurface,
            vec![("surface footer", rule, Some(Reason::SurfaceClassUnknown))],
        ),
        (
            R::CoverageRepeatedSurface,
            vec![(
                "surface checkout",
                rule,
                Some(Reason::SurfaceClassDuplicate),
            )],
        ),
        (
            R::CoverageRepeatedDomain,
            vec![(
                "domain ja by checkout,internal-tool,nav",
                Code::CoverageRuleDuplicate,
                None,
            )],
        ),
        (
            R::CoverageConflict,
            vec![("cell ja nav", Code::CoverageRuleConflict, None)],
        ),
        (
            R::PolicyDense,
            vec![
                (
                    "alias en-US",
                    duplicate,
                    Some(Reason::ConflictingDestination),
                ),
                (
                    "fallback he-IL literal he-IL",
                    sequence,
                    Some(Reason::LiteralSelfReference),
                ),
                (
                    "fallback ja intent source",
                    sequence,
                    Some(Reason::IntentSourceDuplicate),
                ),
                ("surface footer", rule, Some(Reason::SurfaceClassUnknown)),
            ],
        ),
    ] {
        let resolved = resolved(&case(recipe));
        let expected = expected
            .into_iter()
            .map(|(subject, code, reason)| (subject.to_owned(), code, reason))
            .collect::<Vec<_>>();
        assert_eq!(policy_issues(&resolved), expected, "{recipe:?}");
        // Phase 7 resolved the project; what Phase 8 blocked is unavailable.
        assert!(locales(&resolved).project.is_ok(), "{recipe:?}");
    }
    let dense = resolved(&case(R::PolicyDense));
    let (mut domains, mut cells) = (Vec::new(), 0);
    for (unit, status) in dense.evaluation.statuses() {
        match &unit.subject {
            Subject::Coverage(CoverageSubject::Domain(_)) => {
                assert_eq!(*status, Status::Evaluated);
                domains.push(describe_policy(&unit.subject));
            }
            Subject::Coverage(CoverageSubject::Cell(..)) => {
                assert_ne!(*status, Status::Evaluated);
                cells += 1;
            }
            _ => {}
        }
    }
    // The conflicting rule's domain is among them; the rule naming `footer`
    // has none.
    assert_eq!(
        domains,
        [
            "domain en,ja by nav",
            "domain he-IL,ja by checkout",
            "domain ja by checkout,internal-tool,nav"
        ]
    );
    assert_eq!(cells, 9);
}

/// The rules and their selector items in another order change neither the
/// shared nor the entry domain: the authoring order decides nothing, and
/// Phase 7 places no coverage selector. Only the input differs.
#[test]
fn reordered_rules_and_selector_items_change_no_observation() {
    let (every, reordered) = (
        prepare(&case(R::EveryRole)).unwrap(),
        prepare(&case(R::PolicyReordered)).unwrap(),
    );
    assert_eq!(every.observation, reordered.observation);
    assert_ne!(
        super::context::observe(&every).unwrap(),
        super::context::observe(&reordered).unwrap()
    );
}

/// Each of Phase 8's eight bounds counts every role at its exact value, and
/// one under it blocks with that bound's own name, limit, and count.
#[test]
fn phase_8_bounds_count_every_role_at_their_exact_values_and_first_over() {
    for (kind, name, exact) in [
        (
            LimitKind::ResolutionNegotiationAliases,
            "maxNegotiationAliases",
            2,
        ),
        (
            LimitKind::ResolutionFallbackSources,
            "maxFallbackSources",
            2,
        ),
        (
            LimitKind::ResolutionFallbackCandidatesPerSource,
            "maxFallbackCandidatesPerSource",
            2,
        ),
        (LimitKind::ResolutionCoverageRules, "maxCoverageRules", 3),
        (
            LimitKind::ResolutionCoverageLocaleSelectorOccurrences,
            "maxCoverageLocaleSelectorOccurrences",
            3,
        ),
        (
            LimitKind::ResolutionCoverageSurfaceSelectorOccurrences,
            "maxCoverageSurfaceSelectorOccurrences",
            2,
        ),
        (
            LimitKind::ResolutionCoverageDecisionCells,
            "maxCoverageDecisionCells",
            9,
        ),
        (
            LimitKind::ResolutionCoverageRuleCellComparisons,
            "maxCoverageRuleCellComparisons",
            27,
        ),
    ] {
        for edge in [LimitEdge::Exact, LimitEdge::FirstOver] {
            let declaration = declarations()
                .into_iter()
                .find(|case| {
                    case.fixture == Recipe::LocalePolicyResolution(R::EveryRole)
                        && case.limit == Some((kind, edge))
                })
                .unwrap();
            let resolved = resolved(&declaration);
            let issues = resolved.evaluation.issues().collect::<Vec<_>>();
            if edge == LimitEdge::Exact {
                assert!(issues.is_empty(), "{kind:?}");
                continue;
            }
            assert_eq!(issues.len(), 1, "{kind:?}");
            assert_eq!(issues[0].code(), Code::ResourceLimitExceeded, "{kind:?}");
            let exceeded = resolved.evaluation.limit(issues[0]).unwrap();
            assert_eq!(
                (exceeded.bound.name, exceeded.limit.get(), exceeded.actual),
                (name, exact - 1, Actual::Exact(exact)),
                "{kind:?}"
            );
        }
    }
}

/// Each ICU4X scale grows one Phase 8 dimension and resolves it whole at the
/// other recipes' Phase 7 capacity, through either engine.
#[test]
fn the_policy_scales_grow_one_dimension_each_through_icu4x() {
    for engine in [Engine::Reused, Engine::Fresh] {
        let policies = |input| {
            let resolved = resolved(&icu4x_case(input, engine));
            assert!(resolved.evaluation.issues().next().is_none(), "{input:?}");
            resolved.result.unwrap()
        };
        let aliases = policies(R::AliasScale);
        assert_eq!(aliases.negotiation.unwrap().aliases().len(), 24);
        let sources = policies(R::FallbackSourceScale).fallback.unwrap();
        assert_eq!(sources.sequences().len(), 40);
        assert!(sources
            .sequences()
            .values()
            .all(|sequence| sequence.len() == 1));
        let sequence = policies(R::FallbackSequenceScale).fallback.unwrap();
        assert_eq!(
            sequence
                .sequences()
                .values()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            [48]
        );
        let coverage = policies(R::CoverageRuleScale).coverage.unwrap();
        assert_eq!(coverage.basis().len(), 180);
        // Each language's rule decides its three cells alone.
        assert!(coverage.basis().values().all(|basis| matches!(
            &basis.decision,
            Decision::Rules(domains) if domains.len() == 1
        )));
    }
}
