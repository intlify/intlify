// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::prepare::prepare;
use super::*;
use crate::benchmark::operation::{Operation, Output};
use crate::locale::{CanonicalizationFailure, ProviderFailure};

#[test]
fn canonicalization_cases_extend_the_catalog_without_changing_the_original_inventory() {
    let cases = declarations();
    assert_eq!(cases.len(), 160);
    assert!(cases[..77]
        .iter()
        .all(|case| case.operation != Operation::LocaleCanonicalization));
    assert!(cases[77..94]
        .iter()
        .all(|case| case.operation == Operation::LocaleCanonicalization));
    assert_eq!(
        cases[77..94]
            .iter()
            .filter(|case| case.limit.is_some())
            .count(),
        4
    );
    for case in &cases[77..94] {
        let candidate = prepare(case).unwrap();
        assert!(
            candidate.input_limits.is_none(),
            "no fabricated file preparation"
        );
        match candidate.output {
            Output::Locale(Ok(_)) => {
                assert_eq!(case.expected_kind, ExpectedKind::LocaleCanonicalized);
            }
            Output::Locale(Err(_)) => assert_eq!(case.expected_kind, ExpectedKind::LocaleRejected),
            _ => panic!("wrong measured boundary"),
        }
    }
}

#[test]
fn equivalent_locale_spellings_share_only_the_semantic_observation() {
    let observed = |recipe| {
        let case = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::Locale(recipe) && case.limit.is_none())
            .unwrap();
        prepare(&case).unwrap().observation
    };
    let canonical = observed(LocaleRecipe::Region);
    let alias = observed(LocaleRecipe::Casing);
    assert_eq!(canonical.shared, alias.shared);
    assert_ne!(canonical.entry, alias.entry);
    assert_ne!(observed(LocaleRecipe::Language).shared, canonical.shared);
    for (alias, canonical) in [
        (LocaleRecipe::Alias, LocaleRecipe::CanonicalAlias),
        (
            LocaleRecipe::ExtensionOrdering,
            LocaleRecipe::CanonicalExtensions,
        ),
        (
            LocaleRecipe::ExpandingAlias,
            LocaleRecipe::CanonicalExpandedAlias,
        ),
    ] {
        assert_eq!(observed(alias).shared, observed(canonical).shared);
        assert_ne!(observed(alias).entry, observed(canonical).entry);
    }
}

#[test]
fn locale_declarations_match_their_exact_independent_semantic_expectations() {
    let mut checked = 0;
    for case in declarations()
        .into_iter()
        .filter(|case| case.operation == Operation::LocaleCanonicalization && case.limit.is_none())
    {
        // Both providers answer to the same kind of owner declaration.
        let (spelling, canonical) = match case.fixture {
            Recipe::Locale(recipe) => (recipe.spelling(), recipe.canonical()),
            Recipe::Icu4xLocale { input, .. } => (input.spelling(), input.canonical()),
            _ => panic!("locale recipe required"),
        };
        let Output::Locale(result) = prepare(&case).unwrap().output else {
            panic!("locale output required")
        };
        checked += 1;
        match canonical {
            Some(expected) => {
                let result = result.unwrap();
                assert_eq!(result.locale().as_str(), expected);
                assert_eq!(
                    result.suggested_replacement(),
                    (spelling != expected).then_some(expected)
                );
            }
            None => assert_eq!(
                result.unwrap_err(),
                CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier)
            ),
        }
    }
    // 13 finite inputs, and 7 through the ICU4X provider, each with two engines.
    assert_eq!(checked, 27);
}

#[test]
fn locale_context_requires_actual_declared_input_and_no_fabricated_file_preparation() {
    use super::context::{observe, ContextFailure};
    use crate::benchmark::operation::Prepared;
    use std::sync::Arc;

    let case = declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::Locale(LocaleRecipe::Region) && case.limit.is_none())
        .unwrap();
    let mut candidate = prepare(&case).unwrap();
    assert!(observe(&candidate).is_ok());
    candidate.input_limits = Some(crate::materialize_tests::limits());
    assert_eq!(
        observe(&candidate),
        Err(ContextFailure::PreparationLimitsMismatch)
    );
    candidate.input_limits = None;
    let Prepared::Locale { input, .. } = &mut candidate.prepared else {
        unreachable!()
    };
    *input = Arc::from("not-the-declared-fixture-input");
    assert_eq!(
        observe(&candidate),
        Err(ContextFailure::LocaleInputMismatch)
    );

    let mut file = prepare(&declarations()[0]).unwrap();
    file.input_limits = None;
    assert_eq!(
        observe(&file),
        Err(ContextFailure::PreparationLimitsMismatch)
    );
}

#[test]
fn provider_unavailability_and_invariants_never_become_completed_locale_observations() {
    use crate::benchmark::operation::OutputFailure;
    for failure in [
        CanonicalizationFailure::Provider(ProviderFailure::UnsupportedInput),
        CanonicalizationFailure::Provider(ProviderFailure::Unavailable),
    ] {
        assert_eq!(
            Output::Locale(Err(failure)).observe(),
            Err(OutputFailure::LocaleProviderUnavailable)
        );
    }
    for failure in [
        CanonicalizationFailure::UnrepresentableByteCount,
        CanonicalizationFailure::ProviderBindingChanged,
        CanonicalizationFailure::ProviderOutputInvariant,
    ] {
        assert_eq!(
            Output::Locale(Err(failure)).observe(),
            Err(OutputFailure::LocaleProviderInvariant)
        );
    }
}

fn icu4x(input: Icu4xRecipe, engine: Engine) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::Icu4xLocale { input, engine })
        .unwrap()
}

#[test]
fn icu4x_cases_follow_the_original_catalog_with_both_engines_for_each_input() {
    use crate::benchmark::operation::Prepared;

    let cases = declarations();
    // Only Phase 7's later finite rows follow them, before its own ICU4X rows.
    assert!(cases[..127]
        .iter()
        .chain(&cases[141..150])
        .all(|case| case.fixture.icu4x_engine().is_none()));
    assert_eq!(cases[127..141].len(), 2 * Icu4xRecipe::ALL.len());
    for (pair, input) in cases[127..141].chunks(2).zip(Icu4xRecipe::ALL) {
        assert_eq!(
            pair[0].fixture,
            Recipe::Icu4xLocale {
                input,
                engine: Engine::Reused
            }
        );
        assert_eq!(
            pair[1].fixture,
            Recipe::Icu4xLocale {
                input,
                engine: Engine::Fresh
            }
        );
        for case in pair {
            assert_eq!(case.operation, Operation::LocaleCanonicalization);
            assert_eq!(case.limit, None);
            assert_eq!(case.selector, Selector::Absent);
            assert_eq!(&*case.fixture.source(), input.spelling().as_bytes());
            let candidate = prepare(case).unwrap();
            assert!(
                candidate.input_limits.is_none(),
                "no fabricated file preparation"
            );
            let Prepared::Icu4xLocale { engine, .. } = &candidate.prepared else {
                panic!("wrong provider")
            };
            assert_eq!(Some(engine.engine()), case.fixture.icu4x_engine());
        }
    }
}

#[test]
fn fresh_and_reused_engines_agree_on_every_semantic_and_work_observation() {
    use super::context::observe;

    let registry = super::registry::Registry::load().unwrap();
    for input in Icu4xRecipe::ALL {
        let reused = prepare(&icu4x(input, Engine::Reused)).unwrap();
        let fresh = prepare(&icu4x(input, Engine::Fresh)).unwrap();
        assert_eq!(reused.observation, fresh.observation);
        assert_eq!(reused.logical_work, fresh.logical_work);
        // The engine state is a fact of the input, so the two contexts differ.
        assert_ne!(observe(&reused).unwrap(), observe(&fresh).unwrap());
        // The checked-in rows pin the same equivalence.
        let reused = registry.prepare(&icu4x(input, Engine::Reused)).unwrap();
        let fresh = registry.prepare(&icu4x(input, Engine::Fresh)).unwrap();
        assert_eq!(reused.expected(), fresh.expected());
        assert_eq!(reused.expected_work(), fresh.expected_work());
        assert_ne!(reused.input_context(), fresh.input_context());
    }
}

#[test]
fn equal_answers_from_either_provider_are_one_semantic_observation() {
    let finite = |recipe| {
        let case = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::Locale(recipe) && case.limit.is_none())
            .unwrap();
        prepare(&case).unwrap().observation
    };
    for (input, recipe) in [
        (Icu4xRecipe::Small, LocaleRecipe::Region),
        (Icu4xRecipe::Alias, LocaleRecipe::Alias),
        (Icu4xRecipe::Correction, LocaleRecipe::ExpandingAlias),
        (Icu4xRecipe::InvalidSyntax, LocaleRecipe::InvalidLegacy),
        (Icu4xRecipe::InvalidValidity, LocaleRecipe::InvalidValidity),
    ] {
        assert_eq!(input.spelling(), recipe.spelling());
        assert_eq!(
            prepare(&icu4x(input, Engine::Reused)).unwrap().observation,
            finite(recipe)
        );
    }
}

#[test]
fn icu4x_context_requires_the_declared_input_and_engine() {
    use super::context::{observe, ContextFailure};
    use crate::benchmark::operation::Prepared;
    use std::sync::Arc;

    let mut candidate = prepare(&icu4x(Icu4xRecipe::Small, Engine::Reused)).unwrap();
    assert!(observe(&candidate).is_ok());
    candidate.input_limits = Some(crate::materialize_tests::limits());
    assert_eq!(
        observe(&candidate),
        Err(ContextFailure::PreparationLimitsMismatch)
    );
    candidate.input_limits = None;
    let fresh = prepare(&icu4x(Icu4xRecipe::Small, Engine::Fresh)).unwrap();
    let reused = std::mem::replace(&mut candidate.prepared, fresh.prepared);
    assert_eq!(
        observe(&candidate),
        Err(ContextFailure::LocaleInputMismatch)
    );
    candidate.prepared = reused;
    let Prepared::Icu4xLocale { input, .. } = &mut candidate.prepared else {
        unreachable!()
    };
    *input = Arc::from(Icu4xRecipe::Alias.spelling());
    assert_eq!(
        observe(&candidate),
        Err(ContextFailure::LocaleInputMismatch)
    );
    // A finite provider's declaration cannot claim an ICU4X engine.
    let mut finite = prepare(
        &declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::Locale(LocaleRecipe::Region))
            .unwrap(),
    )
    .unwrap();
    finite.prepared = prepare(&icu4x(Icu4xRecipe::Small, Engine::Reused))
        .unwrap()
        .prepared;
    assert_eq!(observe(&finite), Err(ContextFailure::LocaleInputMismatch));
}
