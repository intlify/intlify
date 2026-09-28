// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Exclusion: `noIntent(value, reason)`.
//!
//! Every expected range is found in the source text by searching for the
//! bytes it covers, never read back from the analysis under test.

mod support;

use intlify_authoring::{OccurrenceRole, ReasonFamily, UnitOutcome};
use intlify_authoring_js::{detail, Grammar, JsLimitKind, ProducerFailure, UnitAnalysis};
use support::{
    analyze, at, diagnostics, expect, limits, occurrence, profile, try_analyze, PRELUDE,
};

fn source(body: &str) -> String {
    format!("{PRELUDE}{body}")
}

fn excluded(analysis: &UnitAnalysis) -> Vec<((u64, u64), String)> {
    analysis
        .inspection_facts()
        .exclusions()
        .iter()
        .map(|exclusion| {
            let (role, range) = occurrence(exclusion.occurrence());
            assert_eq!(role, OccurrenceRole::Exclusion);
            (range, exclusion.reason().to_owned())
        })
        .collect()
}

#[test]
fn an_exclusion_keeps_its_reason_and_makes_no_intent() {
    let text = source(
        "brand.textContent = noIntent('Intlify', 'Product name')\n\
         comment.textContent = noIntent(userComment, `User-authored content`)\n",
    );
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    assert_eq!(
        excluded(&analysis),
        [
            (
                at(&text, "noIntent('Intlify', 'Product name')"),
                "Product name".to_owned()
            ),
            (
                at(&text, "noIntent(userComment, `User-authored content`)"),
                "User-authored content".to_owned()
            ),
        ]
    );
    let facts = analysis.checked().expect("a checked unit");
    assert!(facts.declarations().is_empty());
    assert!(facts.references().is_empty());
}

#[test]
fn a_reason_that_is_missing_empty_or_computed_is_reported() {
    let text = source(
        "noIntent(first)\n\
         noIntent(second, '')\n\
         noIntent(third, why)\n\
         noIntent(fourth, `Brand ${name}`)\n\
         noIntent(fifth, 'Brand', extra)\n\
         noIntent(...args)\n",
    );
    let analysis = analyze(&text);
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    assert_eq!(
        diagnostics(&analysis),
        [
            expect(
                unsupported,
                detail::exclusion_reason_missing(),
                at(&text, "noIntent(first)")
            ),
            expect(
                unsupported,
                detail::exclusion_reason_empty(),
                at(&text, "''")
            ),
            expect(
                unsupported,
                detail::exclusion_reason_dynamic(),
                at(&text, "why")
            ),
            expect(
                unsupported,
                detail::exclusion_reason_dynamic(),
                at(&text, "`Brand ${name}`")
            ),
            expect(
                unsupported,
                detail::exclusion_arguments(),
                at(&text, "extra")
            ),
            expect(
                unsupported,
                detail::exclusion_arguments(),
                at(&text, "...args")
            ),
        ]
    );
    assert_eq!(excluded(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
}

#[test]
fn localizing_and_excluding_one_value_is_a_contradiction() {
    let text = source(
        "noIntent(intent('Pay now'), 'Brand')\n\
         intent(noIntent(value, 'Brand'))\n\
         noIntent(mf2`Pay later`, 'Brand')\n",
    );
    let analysis = analyze(&text);
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    // Reported once, at the outer call, and neither side becomes a fact: the
    // order the markers are written in does not decide between them.
    assert_eq!(
        diagnostics(&analysis),
        [
            expect(
                unsupported,
                detail::explicit_forms_nested(),
                at(&text, "noIntent(intent('Pay now'), 'Brand')")
            ),
            expect(
                unsupported,
                detail::explicit_forms_nested(),
                at(&text, "intent(noIntent(value, 'Brand'))")
            ),
            expect(
                unsupported,
                detail::explicit_forms_nested(),
                at(&text, "noIntent(mf2`Pay later`, 'Brand')")
            ),
        ]
    );
    let facts = analysis.inspection_facts();
    assert!(facts.declarations().is_empty());
    assert!(facts.references().is_empty());
    assert!(facts.exclusions().is_empty());
}

#[test]
fn a_message_inside_an_excluded_expression_is_not_a_contradiction() {
    // The excluded value is what `format` returns, not the message itself.
    let text = source("noIntent(format(intent('Pay now')), 'Formatted elsewhere')\n");
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    let facts = analysis.checked().expect("a checked unit");
    assert_eq!(facts.declarations().len(), 1);
    assert_eq!(facts.references().len(), 1);
    assert_eq!(facts.exclusions().len(), 1);
}

#[test]
fn exclusions_one_unit_makes_are_bounded_exactly() {
    let text = source("noIntent(a, 'One')\nnoIntent(b, 'Two')\n");
    let mut bounded = limits();
    bounded.exclusions = 2;
    assert!(try_analyze(Grammar::JsModule, &text, &profile(), &bounded).is_ok());
    bounded.exclusions = 1;
    assert_eq!(
        try_analyze(Grammar::JsModule, &text, &profile(), &bounded),
        Err(ProducerFailure::Limit(JsLimitKind::Exclusions))
    );
}
