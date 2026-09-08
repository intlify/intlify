// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::prepare::prepare;
use super::*;

fn case(recipe: LocaleCoreRecipe) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::LocaleCore(recipe) && case.limit.is_none())
        .unwrap()
}

#[test]
fn core_cases_extend_the_existing_94_cases_without_replacing_their_operations() {
    let cases = declarations();
    assert_eq!(cases.len(), 127);
    assert!(cases[..94]
        .iter()
        .all(|case| case.operation != Operation::LocaleCoreResolution));
    assert!(cases[94..]
        .iter()
        .all(|case| case.operation == Operation::LocaleCoreResolution));
    assert_eq!(
        cases[94..]
            .iter()
            .filter(|case| case.limit.is_some())
            .count(),
        8
    );
    for declaration in &cases[94..] {
        let candidate = prepare(declaration).unwrap();
        assert!(
            candidate.input_limits.is_some(),
            "complete config preparation is explicit"
        );
        let crate::benchmark::operation::Output::LocaleCore(result) = candidate.output else {
            panic!("wrong owner boundary")
        };
        assert_eq!(
            result.value().is_ok(),
            declaration.expected_kind == ExpectedKind::LocaleCoreResolved
        );
    }
}

#[test]
fn source_order_is_separate_from_shared_core_meaning_on_success_and_failure() {
    let observed = |recipe| prepare(&case(recipe)).unwrap().observation;
    for (left, right) in [
        (LocaleCoreRecipe::Multi, LocaleCoreRecipe::Reordered),
        (LocaleCoreRecipe::Multi, LocaleCoreRecipe::Aliased),
        (
            LocaleCoreRecipe::MultipleDuplicates,
            LocaleCoreRecipe::DuplicateReordered,
        ),
        (
            LocaleCoreRecipe::InvalidAndDuplicate,
            LocaleCoreRecipe::InvalidReordered,
        ),
        (
            LocaleCoreRecipe::ExpandingAlias,
            LocaleCoreRecipe::ExpandedAlias,
        ),
    ] {
        assert_eq!(
            observed(left).shared,
            observed(right).shared,
            "{left:?} / {right:?}"
        );
    }
    for (left, right) in [
        (LocaleCoreRecipe::Multi, LocaleCoreRecipe::Aliased),
        (
            LocaleCoreRecipe::MultipleDuplicates,
            LocaleCoreRecipe::DuplicateReordered,
        ),
        (
            LocaleCoreRecipe::InvalidAndDuplicate,
            LocaleCoreRecipe::InvalidReordered,
        ),
    ] {
        assert_ne!(observed(left).entry, observed(right).entry);
    }
    for changed in [
        LocaleCoreRecipe::ChangedSet,
        LocaleCoreRecipe::ChangedDefault,
        LocaleCoreRecipe::ChangedSource,
        LocaleCoreRecipe::AbsentSource,
    ] {
        assert_ne!(
            observed(LocaleCoreRecipe::Multi).shared,
            observed(changed).shared
        );
    }
    assert_ne!(
        observed(LocaleCoreRecipe::Minimal).shared,
        observed(LocaleCoreRecipe::SourceUnd).shared
    );
}

#[test]
fn successful_core_recipes_have_independent_complete_value_and_correction_expectations() {
    use crate::benchmark::operation::{Output, Schema};
    use crate::locale::core::Location::{Requested, RequestedDefault, SourceDefault};
    use LocaleCoreRecipe as R;

    let schema = Schema::for_model().unwrap();
    let oracle = jsonschema::draft7::new(schema.schema_body()).unwrap();
    for recipe in R::ALL {
        // Every recipe enters the measured core through a complete structural
        // model, even if this operation rejects its locale meaning afterwards.
        assert!(oracle.is_valid(&recipe.value()), "{recipe:?}");
        if !recipe.resolves() {
            continue;
        }
        let (source, requested, default): (Option<&str>, &[&str], &str) = match recipe {
            R::Minimal | R::OtherProfile => (None, &["en"], "en"),
            R::SourceUnd => (Some("und"), &["en"], "en"),
            R::SourceEn | R::UnrelatedFields => (Some("en"), &["en"], "en"),
            R::Multi | R::Reordered | R::Aliased => {
                (Some("en"), &["en", "en-US", "he-IL", "ja"], "en-US")
            }
            R::ChangedSet => (Some("en"), &["en-US", "ja"], "en-US"),
            R::ChangedDefault => (Some("en"), &["en", "en-US", "he-IL", "ja"], "ja"),
            R::ChangedSource => (Some("fr"), &["en", "en-US", "he-IL", "ja"], "en-US"),
            R::AbsentSource => (None, &["en", "en-US", "he-IL", "ja"], "en-US"),
            R::ExpandingAlias | R::ExpandedAlias => {
                (None, &["und-u-ca-islamic-civil"], "und-u-ca-islamic-civil")
            }
            _ => panic!("new success recipe needs an independent expectation"),
        };
        let Output::LocaleCore(result) = prepare(&case(recipe)).unwrap().output else {
            unreachable!()
        };
        let core = result.value().unwrap();
        assert_eq!(
            core.source_default()
                .map(crate::locale::CanonicalLocale::as_str),
            source,
            "{recipe:?}"
        );
        assert_eq!(
            core.requested()
                .iter()
                .map(crate::locale::CanonicalLocale::as_str)
                .collect::<Vec<_>>(),
            requested,
            "{recipe:?}"
        );
        assert_eq!(core.requested_default().as_str(), default, "{recipe:?}");
        assert_eq!(
            result.counts().active_occurrences,
            Some(requested.len() as u64 + 1 + u64::from(source.is_some()))
        );
        assert_eq!(
            result.counts().canonical_requested,
            Some(requested.len() as u64)
        );
        let corrections = result
            .corrections()
            .iter()
            .map(|item| (item.location, item.replacement.as_str()))
            .collect::<Vec<_>>();
        let expected = match recipe {
            R::Aliased => vec![
                (SourceDefault, "en"),
                (Requested(1), "he-IL"),
                (Requested(2), "en-US"),
                (Requested(3), "en"),
                (RequestedDefault, "en-US"),
            ],
            R::ExpandingAlias => vec![
                (Requested(0), "und-u-ca-islamic-civil"),
                (RequestedDefault, "und-u-ca-islamic-civil"),
            ],
            _ => vec![],
        };
        assert_eq!(corrections, expected, "{recipe:?}");
    }
    for (left, right) in [
        (R::Minimal, R::OtherProfile),
        (R::SourceEn, R::UnrelatedFields),
    ] {
        assert_eq!(
            prepare(&case(left)).unwrap().observation,
            prepare(&case(right)).unwrap().observation
        );
        assert_ne!(
            super::context::observe(&prepare(&case(left)).unwrap()).unwrap(),
            super::context::observe(&prepare(&case(right)).unwrap()).unwrap()
        );
    }
}

#[test]
fn failing_core_recipes_keep_exact_reasons_all_related_indices_and_independent_checks() {
    use crate::benchmark::operation::Output;
    use crate::locale::core::{Failure, Issue, Location};
    use crate::locale::{CanonicalizationFailure, ProviderFailure};
    use LocaleCoreRecipe as R;
    use Location::{Requested, RequestedDefault, SourceDefault};

    #[derive(Debug, PartialEq, Eq)]
    enum Expected<'a> {
        Invalid(Location),
        Duplicate(&'a str, Vec<usize>),
        Outside(&'a str),
    }
    use Expected::{Duplicate, Invalid, Outside};
    for (recipe, occurrences, canonical, expected, corrections) in [
        (
            R::ExactDuplicate,
            3,
            Some(1),
            vec![Duplicate("en", vec![0, 1])],
            vec![],
        ),
        (
            R::AliasDuplicate,
            3,
            Some(1),
            vec![Duplicate("en", vec![0, 1])],
            vec![(Requested(0), "en")],
        ),
        (
            R::MultipleDuplicates,
            8,
            Some(3),
            vec![
                Duplicate("en", vec![2, 5]),
                Duplicate("en-US", vec![1, 3]),
                Duplicate("ja", vec![0, 4]),
            ],
            vec![(Requested(1), "en-US"), (Requested(2), "en")],
        ),
        (
            R::DuplicateReordered,
            8,
            Some(3),
            vec![
                Duplicate("en", vec![0, 3]),
                Duplicate("en-US", vec![2, 4]),
                Duplicate("ja", vec![1, 5]),
            ],
            vec![(Requested(3), "en"), (Requested(4), "en-US")],
        ),
        (
            R::DuplicateHeavy,
            33,
            Some(1),
            vec![Duplicate("en", (0..32).collect())],
            vec![],
        ),
        (
            R::InvalidSource,
            3,
            Some(1),
            vec![Invalid(SourceDefault)],
            vec![],
        ),
        (
            R::InvalidRequested,
            2,
            None,
            vec![Invalid(Requested(0))],
            vec![],
        ),
        (
            R::InvalidDefault,
            2,
            Some(1),
            vec![Invalid(RequestedDefault)],
            vec![],
        ),
        (
            R::InvalidAndDuplicate,
            5,
            None,
            vec![
                Invalid(SourceDefault),
                Invalid(Requested(0)),
                Duplicate("en-US", vec![1, 2]),
            ],
            vec![(Requested(1), "en-US")],
        ),
        (
            R::InvalidReordered,
            5,
            None,
            vec![
                Invalid(SourceDefault),
                Invalid(Requested(1)),
                Duplicate("en-US", vec![0, 2]),
            ],
            vec![(Requested(2), "en-US")],
        ),
        (R::DefaultNotMember, 2, Some(1), vec![Outside("fr")], vec![]),
    ] {
        let Output::LocaleCore(result) = prepare(&case(recipe)).unwrap().output else {
            unreachable!()
        };
        let Failure::Issues(issues) = result.value().unwrap_err() else {
            panic!("wrong failure {recipe:?}")
        };
        let actual = issues
            .iter()
            .map(|issue| match issue {
                Issue::Canonicalization { location, reason } => {
                    assert_eq!(
                        *reason,
                        CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier)
                    );
                    Invalid(*location)
                }
                Issue::Duplicate {
                    locale,
                    occurrences,
                } => Duplicate(locale.as_str(), occurrences.clone()),
                Issue::DefaultNotRequested { locale } => Outside(locale.as_str()),
                Issue::RequestedLimit { .. } => panic!("unexpected reason {recipe:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{recipe:?}");
        assert_eq!(result.counts().active_occurrences, Some(occurrences));
        assert_eq!(result.counts().canonical_requested, canonical);
        assert_eq!(
            result
                .corrections()
                .iter()
                .map(|item| (item.location, item.replacement.as_str()))
                .collect::<Vec<_>>(),
            corrections,
            "{recipe:?}"
        );
    }
}
