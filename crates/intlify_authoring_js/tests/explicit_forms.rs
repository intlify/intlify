// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Explicit forms: `intent()` sources, `mf2` declarations, and their uses.
//!
//! Every expected range is found in the source text by searching for the
//! bytes it covers, never read back from the analysis under test.

mod support;

use intlify_authoring::{
    intent_revision, Location, OccurrenceRole, ReasonFamily, Stage, UnitOutcome,
};
use intlify_authoring_js::{detail, Grammar, UnitAnalysis};
use support::{analyze, analyze_as, at, diagnostics, expect, nth, occurrence, PRELUDE};

fn source(body: &str) -> String {
    format!("{PRELUDE}{body}")
}

/// The declarations, as roles and ranges.
fn declarations(analysis: &UnitAnalysis) -> Vec<(OccurrenceRole, (u64, u64))> {
    analysis
        .inspection_facts()
        .declarations()
        .iter()
        .map(|facts| occurrence(facts.occurrence()))
        .collect()
}

/// A reference's range and the ranges of the declarations it names.
type Named = ((u64, u64), Vec<(u64, u64)>);

/// The references, as their range and the ranges they name.
fn references(analysis: &UnitAnalysis) -> Vec<Named> {
    analysis
        .inspection_facts()
        .references()
        .iter()
        .map(|reference| {
            (
                occurrence(reference.occurrence()).1,
                reference
                    .declarations()
                    .iter()
                    .map(|target| occurrence(target).1)
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn a_literal_source_is_a_declaration_used_where_it_is_written() {
    let text = source("const pay = intent('Pay now')\nconst tpl = intent(`Pay later`)\n");
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    assert_eq!(
        declarations(&analysis),
        [
            (OccurrenceRole::IntentLiteral, at(&text, "'Pay now'")),
            (OccurrenceRole::IntentLiteral, at(&text, "`Pay later`")),
        ]
    );
    assert_eq!(
        references(&analysis),
        [
            (at(&text, "intent('Pay now')"), vec![at(&text, "'Pay now'")]),
            (
                at(&text, "intent(`Pay later`)"),
                vec![at(&text, "`Pay later`")]
            ),
        ]
    );

    // The message is the literal's cooked text, read as MF2, and its map
    // leads from the analyzed MF2 back to the bytes between the quotes.
    let facts = &analysis.checked().expect("a checked unit").declarations()[0];
    assert_eq!(facts.mf2_source(), "Pay now");
    let (start, end) = at(&text, "Pay now");
    let map = facts.extraction_map();
    assert_eq!(map.len(), 1);
    assert_eq!(
        (map[0].source().start(), map[0].source().end()),
        (start, end)
    );
}

#[test]
fn a_nested_tag_is_one_declaration_used_at_the_call() {
    let text = source("render(intent(mf2`Hello {$name}!`, { name }))\n");
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    let tag = at(&text, "mf2`Hello {$name}!`");
    assert_eq!(
        declarations(&analysis),
        [(OccurrenceRole::Mf2Declaration, tag)]
    );
    assert_eq!(
        references(&analysis),
        [(
            at(&text, "intent(mf2`Hello {$name}!`, { name })"),
            vec![tag]
        )]
    );
    let parameters = analysis.inspection_facts().references()[0].parameters();
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].name(), "name");
    // A shorthand property's value is the identifier itself.
    let (braces, _) = at(&text, "{ name }");
    assert_eq!(
        occurrence(parameters[0].expression()),
        (
            OccurrenceRole::ParameterExpression,
            (braces + 2, braces + 6)
        )
    );
}

#[test]
fn every_use_of_a_shared_declaration_names_the_same_occurrence() {
    let text = source(
        "const greeting = mf2`Hello {$name}!`\n\
         export function render(name) {\n\
         \x20 first(intent(greeting, { name }))\n\
         \x20 second(intent(greeting, { name }))\n\
         }\n",
    );
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    let tag = at(&text, "mf2`Hello {$name}!`");
    assert_eq!(
        declarations(&analysis),
        [(OccurrenceRole::Mf2Declaration, tag)]
    );
    assert_eq!(
        references(&analysis),
        [
            (nth(&text, "intent(greeting, { name })", 0), vec![tag]),
            (nth(&text, "intent(greeting, { name })", 1), vec![tag]),
        ]
    );
}

#[test]
fn equal_text_at_separate_declarations_is_separate_facts_with_one_meaning() {
    let text = source(
        "const a = intent('Pay now')\nconst b = mf2`Pay now`\nconst c = intent('Pay now')\n",
    );
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    let declared = analysis.checked().expect("a checked unit").declarations();
    assert_eq!(declared.len(), 3, "three declarations, not one");
    // An `intent()` literal and an `mf2` tag with the same cooked content
    // reach the same parser as the same MF2, so they mean the same thing.
    // Identity is a separate matter: each is its own occurrence.
    let revisions: Vec<_> = declared
        .iter()
        .map(|facts| intent_revision(facts.projection()).expect("a revision"))
        .collect();
    assert!(revisions.windows(2).all(|pair| pair[0] == pair[1]));
    assert_ne!(declared[0].occurrence(), declared[2].occurrence());
}

#[test]
fn a_declaration_nothing_uses_is_still_a_declaration() {
    let text = source("export const unused = mf2`Not shown yet`\n");
    let analysis = analyze(&text);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    assert_eq!(
        declarations(&analysis),
        [(
            OccurrenceRole::Mf2Declaration,
            at(&text, "mf2`Not shown yet`")
        )]
    );
    assert_eq!(references(&analysis), []);
}

#[test]
fn a_source_computed_at_run_time_is_dynamic() {
    let text = source(
        "intent(`Hello ${name}`)\n\
         intent(message())\n\
         intent(prefix + suffix)\n\
         intent(messages.pay)\n\
         intent(undeclared)\n\
         let changing = 'Pay now'\n\
         intent(changing)\n\
         const tagged = mf2`Hello ${name}`\n",
    );
    let analysis = analyze(&text);
    let dynamic = ReasonFamily::AuthoringSourceDynamic;
    assert_eq!(
        diagnostics(&analysis),
        [
            expect(
                dynamic,
                detail::template_substitution(),
                nth(&text, "`Hello ${name}`", 0)
            ),
            expect(dynamic, detail::message_dynamic(), at(&text, "message()")),
            expect(
                dynamic,
                detail::message_dynamic(),
                at(&text, "prefix + suffix")
            ),
            expect(
                dynamic,
                detail::message_dynamic(),
                at(&text, "messages.pay")
            ),
            expect(dynamic, detail::message_dynamic(), at(&text, "undeclared")),
            expect(
                dynamic,
                detail::message_dynamic(),
                nth(&text, "changing", 1)
            ),
            expect(
                dynamic,
                detail::template_substitution(),
                at(&text, "mf2`Hello ${name}`")
            ),
        ]
    );
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
    assert!(analysis.checked().is_none());
    assert_eq!(declarations(&analysis), []);
}

#[test]
fn a_conditional_selection_is_reported_and_neither_alternative_is_chosen() {
    let text = source(
        "const loading = mf2`Loading`\nconst done = mf2`Done`\nintent(pending ? loading : done)\n",
    );
    let analysis = analyze(&text);
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringFormUnsupported,
            detail::conditional_selection(),
            at(&text, "pending ? loading : done")
        )]
    );
    // Both declarations stand, and neither is used: the condition is not
    // evaluated to pick one.
    assert_eq!(
        declarations(&analysis),
        [
            (OccurrenceRole::Mf2Declaration, at(&text, "mf2`Loading`")),
            (OccurrenceRole::Mf2Declaration, at(&text, "mf2`Done`")),
        ]
    );
    assert_eq!(references(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);

    let logical = source("const a = mf2`A`\nintent(fallback || a)\n");
    let analysis = analyze(&logical);
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringFormUnsupported,
            detail::conditional_selection(),
            at(&logical, "fallback || a")
        )]
    );
}

#[test]
fn only_a_const_bound_to_the_tag_itself_names_a_declaration() {
    let text = source(
        "import { shared } from './messages'\n\
         const greeting = mf2`Hi`\n\
         const alias = greeting\n\
         intent(alias)\n\
         intent(shared)\n",
    );
    let analysis = analyze(&text);
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    assert_eq!(
        diagnostics(&analysis),
        [
            expect(
                unsupported,
                detail::declaration_alias(),
                nth(&text, "alias", 1)
            ),
            expect(
                unsupported,
                detail::module_reference(),
                nth(&text, "shared", 1)
            ),
        ]
    );
    assert_eq!(
        declarations(&analysis),
        [(OccurrenceRole::Mf2Declaration, at(&text, "mf2`Hi`"))]
    );
    assert_eq!(references(&analysis), []);
}

#[test]
fn a_declaration_is_found_wherever_it_is_written_in_the_unit() {
    // Used inside a function above the module-level declaration it names.
    let text =
        source("export function render() {\n  return intent(later)\n}\nconst later = mf2`Later`\n");
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(
        references(&analysis),
        [(at(&text, "intent(later)"), vec![at(&text, "mf2`Later`")])]
    );
}

#[test]
fn intent_takes_a_source_and_at_most_a_parameter_object() {
    let text = source("intent()\nintent('Pay now', {}, { description: 'x' })\nintent(...args)\n");
    let analysis = analyze(&text);
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    assert_eq!(
        diagnostics(&analysis),
        [
            expect(
                unsupported,
                detail::intent_arguments(),
                at(&text, "intent()")
            ),
            expect(
                unsupported,
                detail::intent_arguments(),
                at(&text, "{ description: 'x' }")
            ),
            expect(
                unsupported,
                detail::intent_arguments(),
                at(&text, "...args")
            ),
        ]
    );
    assert_eq!(declarations(&analysis), []);
}

#[test]
fn transparent_wrappers_keep_a_literal_a_literal() {
    let text = format!(
        "{PRELUDE}intent('Pay now' as string)\nintent(('Pay later'))\nintent(<string>'Save')\nintent('Close'!)\n"
    );
    let analysis = analyze_as(Grammar::TsModule, &text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(
        declarations(&analysis),
        [
            (OccurrenceRole::IntentLiteral, at(&text, "'Pay now'")),
            (OccurrenceRole::IntentLiteral, at(&text, "'Pay later'")),
            (OccurrenceRole::IntentLiteral, at(&text, "'Save'")),
            (OccurrenceRole::IntentLiteral, at(&text, "'Close'")),
        ]
    );
}

#[test]
fn an_escape_the_profile_refuses_is_reported_where_it_is_written() {
    let text = source("intent('Pay \\uD800 now')\n");
    let analysis = analyze(&text);
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringFormUnsupported,
            detail::surrogate_escape(),
            at(&text, "\\uD800")
        )]
    );
    assert_eq!(declarations(&analysis), []);
    assert_eq!(references(&analysis), []);
}

#[test]
fn the_mf2_parser_keeps_its_own_codes_through_the_host() {
    let text = source("intent('Hello {$name')\n");
    let analysis = analyze(&text);
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
    let [record] = analysis.diagnostics() else {
        panic!("one syntax record: {:?}", analysis.diagnostics());
    };
    assert_eq!(record.stage(), Stage::MessageAnalysis);
    assert!(
        !record.origin().code().starts_with("authoring-"),
        "a parser code is not relabelled: {}",
        record.origin().code()
    );
    // The record points at the declaration it concerns, and the work that
    // depends on a parsed message is skipped: no facts, no use.
    let Location::Occurrence(declaration) = record.location() else {
        panic!("a message record points at its declaration");
    };
    assert_eq!(
        occurrence(declaration),
        (OccurrenceRole::IntentLiteral, at(&text, "'Hello {$name'"))
    );
    assert_eq!(declarations(&analysis), []);
    assert_eq!(references(&analysis), []);

    let semantic = source("intent('.input {$a} .input {$a} {{text}}')\n");
    let analysis = analyze(&semantic);
    let codes: Vec<&str> = analysis
        .diagnostics()
        .iter()
        .map(|record| record.origin().code())
        .collect();
    assert_eq!(codes, ["duplicate-declaration"]);
}

#[test]
fn the_representative_module_is_read_for_its_explicit_forms() {
    // Design 028's representative application, under a profile that admits
    // no DOM global: the `'Save'` assignment is outside it, and `dom_sinks.rs`
    // reads the same module with `document` admitted. The annotation
    // describes the greeting alone.
    let text = "import { intent, mf2, noIntent } from 'fixture-authoring'\n\
                \n\
                /* @intlify { \"description\": \"Greeting addressed to the signed-in user\" } */\n\
                const greeting = mf2`Hello {$name}!`\n\
                \n\
                export function render(name) {\n\
                \x20 const save = document.querySelector('#save')\n\
                \x20 const heading = document.querySelector('#heading')\n\
                \x20 const first = document.querySelector('#first')\n\
                \x20 const second = document.querySelector('#second')\n\
                \x20 const brand = document.querySelector('#brand')\n\
                \n\
                \x20 save.textContent = 'Save'\n\
                \x20 heading.textContent = intent('Welcome')\n\
                \x20 first.textContent = intent(greeting, { name })\n\
                \x20 second.textContent = intent(greeting, { name })\n\
                \x20 brand.textContent = noIntent('Intlify', 'Product name')\n\
                }\n";
    let analysis = analyze(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    let tag = at(text, "mf2`Hello {$name}!`");
    assert_eq!(
        declarations(&analysis),
        [
            (OccurrenceRole::Mf2Declaration, tag),
            (OccurrenceRole::IntentLiteral, at(text, "'Welcome'")),
        ]
    );
    assert_eq!(
        references(&analysis),
        [
            (at(text, "intent('Welcome')"), vec![at(text, "'Welcome'")]),
            (nth(text, "intent(greeting, { name })", 0), vec![tag]),
            (nth(text, "intent(greeting, { name })", 1), vec![tag]),
        ]
    );
    let descriptions: Vec<Option<&str>> = analysis
        .inspection_facts()
        .declarations()
        .iter()
        .map(|facts| {
            facts
                .projection()
                .description
                .as_ref()
                .map(intlify_authoring::NonemptyText::as_str)
        })
        .collect();
    assert_eq!(
        descriptions,
        [Some("Greeting addressed to the signed-in user"), None]
    );
    let exclusions = analysis.inspection_facts().exclusions();
    assert_eq!(exclusions.len(), 1);
    assert_eq!(
        occurrence(exclusions[0].occurrence()).1,
        at(text, "noIntent('Intlify', 'Product name')")
    );
    assert_eq!(exclusions[0].reason(), "Product name");
}
