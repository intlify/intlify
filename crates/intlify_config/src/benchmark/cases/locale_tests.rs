// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::prepare::prepare;
use super::*;
use crate::benchmark::operation::{Operation, Output};
use crate::locale::{CanonicalizationFailure, ProviderFailure};

#[test]
fn canonicalization_cases_extend_the_catalog_without_changing_the_original_inventory() {
    let cases = declarations();
    assert_eq!(cases.len(), 94);
    assert!(cases[..77]
        .iter()
        .all(|case| case.operation != Operation::LocaleCanonicalization));
    assert!(cases[77..]
        .iter()
        .all(|case| case.operation == Operation::LocaleCanonicalization));
    assert_eq!(
        cases[77..]
            .iter()
            .filter(|case| case.limit.is_some())
            .count(),
        4
    );
    for case in &cases[77..] {
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
    for case in declarations()
        .into_iter()
        .filter(|case| case.operation == Operation::LocaleCanonicalization && case.limit.is_none())
    {
        let Recipe::Locale(recipe) = case.fixture else {
            panic!("locale recipe required")
        };
        let Output::Locale(result) = prepare(&case).unwrap().output else {
            panic!("locale output required")
        };
        match recipe.canonical() {
            Some(expected) => {
                let result = result.unwrap();
                assert_eq!(result.locale().as_str(), expected);
                assert_eq!(
                    result.suggested_replacement(),
                    (recipe.spelling() != expected).then_some(expected)
                );
            }
            None => assert_eq!(
                result.unwrap_err(),
                CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier)
            ),
        }
    }
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
