// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Automatic UI: which `textContent` assignments are proven sinks.
//!
//! Every expected range is found in the source text by searching for the
//! bytes it covers, never read back from the analysis under test.

mod support;

use intlify_authoring::{intent_revision, OccurrenceRole, ReasonFamily, UnitOutcome};
use intlify_authoring_js::{detail, Grammar, ProducerFailure, UnitAnalysis};
use support::{
    analyze, analyze_dom, analyze_dom_as, at, diagnostics, dom_profile, expect, limits, occurrence,
    try_analyze, PRELUDE,
};

/// The declarations, as roles and ranges.
fn declared(analysis: &UnitAnalysis) -> Vec<(OccurrenceRole, (u64, u64))> {
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
fn used(analysis: &UnitAnalysis) -> Vec<Named> {
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
fn a_literal_assigned_to_each_admitted_origin_is_displayed_text() {
    let text = "const pay = document.querySelector('#pay')\n\
                pay.textContent = 'Pay now'\n\
                const total = document.createElement('span')\n\
                total.textContent = `Total`\n";
    let analysis = analyze_dom(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    assert_eq!(
        declared(&analysis),
        [
            (OccurrenceRole::UiLiteral, at(text, "'Pay now'")),
            (OccurrenceRole::UiLiteral, at(text, "`Total`")),
        ]
    );
    assert_eq!(
        used(&analysis),
        [
            (
                at(text, "pay.textContent = 'Pay now'"),
                vec![at(text, "'Pay now'")]
            ),
            (
                at(text, "total.textContent = `Total`"),
                vec![at(text, "`Total`")]
            ),
        ]
    );
    // The usage comes from the sink.
    let facts = &analysis.inspection_facts().declarations()[0];
    let usage = facts.projection().usage.as_ref().expect("a usage");
    assert_eq!(usage.value.as_str(), "text-content");
    assert_eq!(usage.profile.identity().as_str(), "intlify-web-dom-usage");
}

#[test]
fn displayed_text_keeps_its_braces_as_characters() {
    // In displayed text a brace is a character, not a placeholder, so the
    // same bytes an `intent()` would read as a variable stay literal here.
    let text = "const label = document.createElement('span')\nlabel.textContent = 'Hi {$name}'\n";
    let analysis = analyze_dom(text);
    assert_eq!(diagnostics(&analysis), []);
    let facts = &analysis.inspection_facts().declarations()[0];
    assert!(facts.projection().parameters.is_empty());
    assert_ne!(facts.mf2_source(), "Hi {$name}");
}

#[test]
fn an_origin_call_can_be_the_receiver_itself() {
    let text = "document.querySelector('#title').textContent = 'Checkout'\n";
    let analysis = analyze_dom(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(
        declared(&analysis),
        [(OccurrenceRole::UiLiteral, at(text, "'Checkout'"))]
    );
}

#[test]
fn only_the_standard_document_and_its_two_calls_are_origins() {
    // None of these has a known origin, so none is reported: each is only
    // counted as outside the profile.
    let text = "{\n  const document = { querySelector() { return {} } }\n  document.querySelector('#a').textContent = 'A'\n}\n\
                window.document.querySelector('#b').textContent = 'B'\n\
                document['querySelector']('#c').textContent = 'C'\n\
                document.getElementById('d').textContent = 'D'\n\
                const status = { textContent: 'internal-state' }\n\
                status.textContent = 'E'\n\
                export function show(element) {\n  element.textContent = 'F'\n  this.el.textContent = 'G'\n}\n";
    let analysis = analyze_dom(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis), []);
    assert_eq!(analysis.outside_profile(), 7);
}

#[test]
fn a_declared_document_is_not_the_global() {
    let text = "declare const document: { querySelector(s: string): { textContent: string } }\n\
                const pay = document.querySelector('#pay')\npay.textContent = 'Pay now'\n";
    let analysis = analyze_dom_as(Grammar::TsModule, text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis), []);
    assert_eq!(analysis.outside_profile(), 1);
}

#[test]
fn a_type_assertion_keeps_a_proof_but_makes_none() {
    let text = "const pay = document.querySelector('#pay') as HTMLButtonElement\n\
                (pay as HTMLElement).textContent = 'Pay now'\n\
                export function show(element: unknown) {\n  (element as HTMLElement).textContent = 'Shown'\n}\n";
    let analysis = analyze_dom_as(Grammar::TsModule, text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(
        declared(&analysis),
        [(OccurrenceRole::UiLiteral, at(text, "'Pay now'"))]
    );
    assert_eq!(analysis.outside_profile(), 1);
}

#[test]
fn a_proven_sink_assigned_anything_but_a_literal_is_reported() {
    let text = format!(
        "{PRELUDE}const pay = document.querySelector('#pay')\n\
         pay.textContent = label\n\
         pay.textContent = `Pay ${{amount}}`\n\
         pay.textContent += ' now'\n\
         pay.textContent = mf2`Pay now`\n"
    );
    let analysis = analyze_dom(&text);
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    assert_eq!(
        diagnostics(&analysis),
        [
            expect(
                unsupported,
                detail::sink_value_dynamic(),
                at(&text, "label")
            ),
            expect(
                unsupported,
                detail::sink_value_dynamic(),
                at(&text, "`Pay ${amount}`")
            ),
            expect(
                unsupported,
                detail::sink_compound_assignment(),
                at(&text, "pay.textContent += ' now'")
            ),
            expect(
                unsupported,
                detail::sink_descriptor(),
                at(&text, "mf2`Pay now`")
            ),
        ]
    );
    // The tag is still a declaration of its own; the literal after `+=` is
    // not one.
    assert_eq!(
        declared(&analysis),
        [(OccurrenceRole::Mf2Declaration, at(&text, "mf2`Pay now`"))]
    );
}

#[test]
fn a_chained_assignment_shows_the_inner_literal_and_reports_the_outer_value() {
    let text = "const a = document.createElement('a')\nconst b = document.createElement('b')\n\
                a.textContent = b.textContent = 'Both'\n";
    let analysis = analyze_dom(text);
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringFormUnsupported,
            detail::sink_value_dynamic(),
            at(text, "b.textContent = 'Both'")
        )]
    );
    assert_eq!(
        declared(&analysis),
        [(OccurrenceRole::UiLiteral, at(text, "'Both'"))]
    );
}

#[test]
fn explicit_authoring_at_a_known_sink_is_read_once_as_explicit() {
    let text = format!(
        "{PRELUDE}const pay = document.querySelector('#pay')\n\
         pay.textContent = intent('Pay now')\n\
         pay.textContent = noIntent('Intlify', 'Product name')\n"
    );
    let analysis = analyze_dom(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(
        declared(&analysis),
        [(OccurrenceRole::IntentLiteral, at(&text, "'Pay now'"))]
    );
    assert_eq!(analysis.inspection_facts().exclusions().len(), 1);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
}

#[test]
fn an_origin_without_one_static_string_proves_nothing() {
    let text = "const byId = document.querySelector(selector)\nbyId.textContent = 'A'\n\
                const none = document.createElement()\nnone.textContent = 'B'\n\
                document.querySelector('#c', extra).textContent = 'C'\n";
    let analysis = analyze_dom(text);
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    let argument = detail::origin_argument_unsupported();
    assert_eq!(
        diagnostics(&analysis),
        [
            expect(unsupported, argument, at(text, "byId.textContent = 'A'")),
            expect(unsupported, argument, at(text, "none.textContent = 'B'")),
            expect(
                unsupported,
                argument,
                at(
                    text,
                    "document.querySelector('#c', extra).textContent = 'C'"
                )
            ),
        ]
    );
    assert_eq!(declared(&analysis), []);
}

#[test]
fn the_design_examples_read_as_their_comments_say() {
    // 016, Bounded automatic DOM recognition: the first example.
    let text = "const button = document.querySelector('#pay')\n\
                const target = button\n\
                if (target) {\n  target.textContent = 'Pay now'\n}\n\
                const label = document.createElement('span')\n\
                label.textContent = 'Total'\n\
                const status = { textContent: 'internal-state' }\n\
                const errorDescription = 'Service unavailable'\n";
    let analysis = analyze_dom(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(
        declared(&analysis),
        [
            (OccurrenceRole::UiLiteral, at(text, "'Pay now'")),
            (OccurrenceRole::UiLiteral, at(text, "'Total'")),
        ]
    );

    // The second: an unanalyzed call invalidates every alias, and the call
    // itself is not the error.
    let text = "const button = document.createElement('button')\n\
                const target = button\n\
                customize(button)\n\
                target.textContent = 'Pay now'\n";
    assert_eq!(
        diagnostics(&analyze_dom(text)),
        [expect(
            ReasonFamily::AuthoringFormUnsupported,
            detail::receiver_evidence_invalidated(),
            at(text, "target.textContent = 'Pay now'")
        )]
    );

    // The third: ordinary updates keep the evidence.
    let text = "const button = document.createElement('button')\n\
                button.textContent = 'Loading'\n\
                button.textContent = 'Ready'\n";
    let analysis = analyze_dom(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis).len(), 2);

    // The fourth: one reaching path invalidates the evidence.
    let text = "const button = document.createElement('button')\n\
                if (needsCustomization) {\n  customize(button)\n}\n\
                button.textContent = 'Pay now'\n";
    assert_eq!(
        diagnostics(&analyze_dom(text)),
        [expect(
            ReasonFamily::AuthoringFormUnsupported,
            detail::receiver_evidence_invalidated(),
            at(text, "button.textContent = 'Pay now'")
        )]
    );
}

#[test]
fn an_explicit_revision_does_not_depend_on_the_receiver_around_it() {
    let at_sink = format!(
        "{PRELUDE}const pay = document.querySelector('#pay')\npay.textContent = intent('Pay now')\n"
    );
    let invalidated = format!(
        "{PRELUDE}const pay = document.querySelector('#pay')\ncustomize(pay)\npay.textContent = intent('Pay now')\n"
    );
    let plain = format!("{PRELUDE}render(intent('Pay now'))\n");
    let revision = |text: &str| {
        let analysis = analyze_dom(text);
        assert_eq!(diagnostics(&analysis), [], "{text}");
        intent_revision(analysis.inspection_facts().declarations()[0].projection())
            .expect("a revision")
    };
    assert_eq!(revision(&at_sink), revision(&plain));
    assert_eq!(revision(&invalidated), revision(&plain));
    // The same text recognized automatically carries the sink's usage, so it
    // is a different revision.
    let automatic = "const pay = document.querySelector('#pay')\npay.textContent = 'Pay now'\n";
    assert_ne!(revision(automatic), revision(&plain));
}

#[test]
fn a_profile_admitting_no_dom_global_recognizes_and_counts_nothing() {
    let text = "const pay = document.querySelector('#pay')\npay.textContent = 'Pay now'\n";
    let analysis = analyze(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis), []);
    assert_eq!(analysis.outside_profile(), 0);
}

#[test]
fn a_context_has_to_register_the_usage_profile_this_producer_assigns_from() {
    use intlify_authoring::test_context::TestContext;
    use intlify_authoring::{SurfaceVocabulary, VersionedIdentity};
    use intlify_authoring_js::{analyze_unit, JsAnalysisWorkspace, JsAuthoringProfile};

    let text = "const pay = document.querySelector('#pay')\npay.textContent = 'Pay now'\n";
    let units = support::admit(&[("checkout", Grammar::JsModule, text.as_bytes())]);
    let without = TestContext::builder(
        support::owner(),
        SurfaceVocabulary::new(["checkout"]).unwrap(),
    )
    .authoring_profile(JsAuthoringProfile::new().identity().clone())
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .unwrap();
    let analyze_with = |context: &TestContext, profile: &JsAuthoringProfile| {
        analyze_unit(
            context,
            profile,
            &units[0],
            &limits(),
            &mut JsAnalysisWorkspace::new(),
            &support::never,
        )
    };
    assert_eq!(
        analyze_with(&without, &dom_profile()),
        Err(ProducerFailure::UsageProfileMismatch)
    );
    // Without a DOM global nothing carries a usage, so none is needed.
    assert!(analyze_with(&without, &support::profile()).is_ok());
    let another = TestContext::builder(
        support::owner(),
        SurfaceVocabulary::new(["checkout"]).unwrap(),
    )
    .authoring_profile(JsAuthoringProfile::new().identity().clone())
    .usage_profile(VersionedIdentity::literal("intlify-other-usage", "0"))
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .unwrap();
    assert_eq!(
        analyze_with(&another, &support::profile()),
        Err(ProducerFailure::UsageProfileMismatch)
    );
}

#[test]
fn the_representative_module_is_read_with_its_ui_text() {
    // Design 028's representative application, now with the DOM global
    // admitted: the plain `'Save'` becomes displayed text, and the sinks
    // carrying explicit forms stay explicit.
    let text = "import { intent, mf2, noIntent } from 'fixture-authoring'\n\
                \n\
                const greeting = mf2`Hello {$name}!`\n\
                \n\
                export function render(name) {\n\
                \x20 const save = document.querySelector('#save')\n\
                \x20 const heading = document.querySelector('#heading')\n\
                \x20 const first = document.querySelector('#first')\n\
                \x20 const brand = document.querySelector('#brand')\n\
                \n\
                \x20 save.textContent = 'Save'\n\
                \x20 heading.textContent = intent('Welcome')\n\
                \x20 first.textContent = intent(greeting, { name })\n\
                \x20 brand.textContent = noIntent('Intlify', 'Product name')\n\
                }\n";
    let analysis = try_analyze(Grammar::JsModule, text, &dom_profile(), &limits()).unwrap();
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    assert_eq!(
        declared(&analysis),
        [
            (
                OccurrenceRole::Mf2Declaration,
                at(text, "mf2`Hello {$name}!`")
            ),
            (OccurrenceRole::UiLiteral, at(text, "'Save'")),
            (OccurrenceRole::IntentLiteral, at(text, "'Welcome'")),
        ]
    );
    assert_eq!(used(&analysis).len(), 3);
    assert_eq!(
        used(&analysis)[0],
        (
            at(text, "save.textContent = 'Save'"),
            vec![at(text, "'Save'")]
        )
    );
}

#[test]
fn only_a_direct_call_on_the_global_is_an_origin() {
    let text = "const optional = document.querySelector?.('#a')\noptional.textContent = 'A'\n\
                const wrapped = (document.querySelector)('#b')\nwrapped.textContent = 'B'\n\
                const chained = document?.createElement('c')\nchained.textContent = 'C'\n";
    let analysis = analyze_dom(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis), []);
    assert_eq!(analysis.outside_profile(), 3);
}
