// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Parameters: what a use site supplies, and how it is compared.
//!
//! Every expected range is found in the source text by searching for the
//! bytes it covers, never read back from the analysis under test.

mod support;

use intlify_authoring::{detail as shared, OccurrenceRole, ReasonFamily, UnitOutcome};
use intlify_authoring_js::{detail, Grammar, JsLimitKind, UnitAnalysis};
use support::{
    analyze, at, diagnostics, expect, limits, nth, occurrence, profile, try_analyze, PRELUDE,
};

fn source(body: &str) -> String {
    format!("{PRELUDE}{body}")
}

/// The parameters of each established reference, as names and ranges.
fn supplied(analysis: &UnitAnalysis) -> Vec<Vec<(String, (u64, u64))>> {
    analysis
        .inspection_facts()
        .references()
        .iter()
        .map(|reference| {
            reference
                .parameters()
                .iter()
                .map(|binding| {
                    let (role, range) = occurrence(binding.expression());
                    assert_eq!(role, OccurrenceRole::ParameterExpression);
                    (binding.name().to_owned(), range)
                })
                .collect()
        })
        .collect()
}

#[test]
fn a_use_site_supplies_its_parameters_by_position_in_source_order() {
    // The values call and read at run time; here they are only positions.
    let text =
        source("render(intent('{$total} for {$name}', { total: sum(items), name: user.name }))\n");
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    assert_eq!(
        supplied(&analysis),
        [vec![
            ("total".to_owned(), at(&text, "sum(items)")),
            ("name".to_owned(), at(&text, "user.name")),
        ]]
    );
}

#[test]
fn a_string_key_is_its_decoded_text() {
    let text = source("intent('{$name}', { 'na\\x6de': value })\n");
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(
        supplied(&analysis),
        [vec![("name".to_owned(), at(&text, "value"))]]
    );
}

#[test]
fn a_shorthand_proto_is_an_ordinary_property() {
    // Only `__proto__: value` sets a prototype; the shorthand is a property.
    let text = source("intent('{$__proto__}', { __proto__ })\n");
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
}

#[test]
fn a_mismatch_at_a_declarations_own_use_site_blocks_the_declaration() {
    let text = source(
        "intent('Hello {$name}!', {})\n\
         intent('Pay now', { name })\n\
         intent('Bye {$name}!', { name, name: other })\n",
    );
    let analysis = analyze(&text);
    let mismatch = ReasonFamily::AuthoringParameterMismatch;
    // The shared comparison reports at the declaration its use site belongs
    // to, and names the parameter.
    assert_eq!(
        diagnostics(&analysis),
        [
            expect(
                mismatch,
                shared::parameter_missing(),
                at(&text, "'Hello {$name}!'")
            ),
            expect(mismatch, shared::parameter_extra(), at(&text, "'Pay now'")),
            expect(
                mismatch,
                shared::parameter_duplicate(),
                at(&text, "'Bye {$name}!'")
            ),
        ]
    );
    let names: Vec<Option<&str>> = analysis
        .diagnostics()
        .iter()
        .map(|record| record.parameter())
        .collect();
    assert_eq!(names, [Some("name"), Some("name"), Some("name")]);
    assert!(analysis.inspection_facts().declarations().is_empty());
    assert!(analysis.inspection_facts().references().is_empty());
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
}

#[test]
fn a_mismatch_at_a_shared_declarations_use_blocks_only_that_use() {
    let text = source(
        "const greeting = mf2`Hello {$name}!`\n\
         intent(greeting)\n\
         intent(greeting, { name })\n",
    );
    let analysis = analyze(&text);
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringParameterMismatch,
            shared::parameter_missing(),
            at(&text, "intent(greeting)")
        )]
    );
    // The declaration stands; the use that supplied its name is a fact and
    // the one that did not is not.
    let facts = analysis.inspection_facts();
    assert_eq!(facts.declarations().len(), 1);
    assert_eq!(facts.references().len(), 1);
    assert_eq!(
        occurrence(facts.references()[0].occurrence()).1,
        at(&text, "intent(greeting, { name })")
    );
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
}

#[test]
fn a_parameter_object_whose_names_could_change_at_run_time_is_refused() {
    let text = source(
        "intent('Pay now', { ...rest })\n\
         intent('Pay now', { [key]: value })\n\
         intent('Pay now', { 1: value })\n\
         intent('Pay now', { get name() { return 1 } })\n\
         intent('Pay now', { name() {} })\n\
         intent('Pay now', { __proto__: base })\n\
         intent('Pay now', { '__proto__': base })\n\
         intent('Pay now', params)\n",
    );
    let analysis = analyze(&text);
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    assert_eq!(
        diagnostics(&analysis),
        [
            expect(
                unsupported,
                detail::parameter_spread(),
                at(&text, "...rest")
            ),
            expect(
                unsupported,
                detail::parameter_key(),
                at(&text, "[key]: value")
            ),
            expect(unsupported, detail::parameter_key(), at(&text, "1: value")),
            expect(
                unsupported,
                detail::parameter_accessor(),
                at(&text, "get name() { return 1 }")
            ),
            expect(
                unsupported,
                detail::parameter_accessor(),
                at(&text, "name() {}")
            ),
            expect(
                unsupported,
                detail::parameter_prototype(),
                nth(&text, "__proto__: base", 0)
            ),
            expect(
                unsupported,
                detail::parameter_prototype(),
                at(&text, "'__proto__': base")
            ),
            expect(
                unsupported,
                detail::parameters_opaque(),
                at(&text, "params")
            ),
        ]
    );
    // Each message is still a declaration; what cannot be established is its
    // use site.
    assert_eq!(analysis.inspection_facts().declarations().len(), 8);
    assert!(analysis.inspection_facts().references().is_empty());
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
}

#[test]
fn parameters_one_use_site_supplies_are_bounded_exactly() {
    let text = source("intent('{$a} {$b}', { a, b })\n");
    let mut bounded = limits();
    bounded.parameter_bindings = 2;
    let analysis = try_analyze(Grammar::JsModule, &text, &profile(), &bounded).unwrap();
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    assert_eq!(analysis.inspection_facts().references().len(), 1);

    // One name over is a bound on this use site, not on the invocation: the
    // object is reported, the message is still a declaration, and the use
    // site is not established.
    bounded.parameter_bindings = 1;
    let analysis = try_analyze(Grammar::JsModule, &text, &profile(), &bounded).unwrap();
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringResourceLimit,
            detail::limit(JsLimitKind::ParameterBindings),
            at(&text, "{ a, b }")
        )]
    );
    assert_eq!(analysis.inspection_facts().declarations().len(), 1);
    assert!(analysis.inspection_facts().references().is_empty());
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
}
