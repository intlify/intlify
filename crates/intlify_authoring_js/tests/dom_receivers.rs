// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! DOM receivers: which bindings are followed, and what invalidates them.
//!
//! Every expected range is found in the source text by searching for the
//! bytes it covers, never read back from the analysis under test.

mod support;

use intlify_authoring::{OccurrenceRole, ReasonFamily};
use intlify_authoring_js::{detail, Grammar, JsLimitKind, UnitAnalysis};
use support::{
    analyze_dom, analyze_dom_as, at, diagnostics, dom_profile, expect, limits, occurrence,
    try_analyze, Reported, PRELUDE,
};

fn declared(analysis: &UnitAnalysis) -> Vec<(OccurrenceRole, (u64, u64))> {
    analysis
        .inspection_facts()
        .declarations()
        .iter()
        .map(|facts| occurrence(facts.occurrence()))
        .collect()
}

fn invalidated(text: &str, sink: &str) -> Reported {
    expect(
        ReasonFamily::AuthoringFormUnsupported,
        detail::receiver_evidence_invalidated(),
        at(text, sink),
    )
}

/// Declare `pay`, run `body`, then assign a literal to it.
fn after(body: &str) -> String {
    format!(
        "{PRELUDE}export function render(customize, other) {{\n  const pay = document.querySelector('#pay')\n{body}\n  pay.textContent = 'Pay now'\n}}\n"
    )
}

const SINK: &str = "pay.textContent = 'Pay now'";

#[test]
fn a_const_chain_is_followed_in_one_function_and_at_module_level() {
    let text = "const top = document.querySelector('#top')\nconst alias = top\nconst again = alias\n\
                again.textContent = 'Top'\n\
                export function render() {\n  const inner = document.createElement('p')\n  const same = inner\n  same.textContent = 'Inner'\n}\n";
    let analysis = analyze_dom(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(
        declared(&analysis),
        [
            (OccurrenceRole::UiLiteral, at(text, "'Top'")),
            (OccurrenceRole::UiLiteral, at(text, "'Inner'")),
        ]
    );
}

#[test]
fn an_alias_chain_is_bounded_exactly() {
    let text =
        "const a = document.querySelector('#a')\nconst b = a\nconst c = b\nc.textContent = 'C'\n";
    let mut bounded = limits();
    bounded.alias_chain = 2;
    let analysis = try_analyze(Grammar::JsModule, text, &dom_profile(), &bounded).unwrap();
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis).len(), 1);
    bounded.alias_chain = 1;
    let analysis = try_analyze(Grammar::JsModule, text, &dom_profile(), &bounded).unwrap();
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringResourceLimit,
            detail::limit(JsLimitKind::AliasChain),
            at(text, "c.textContent = 'C'")
        )]
    );
}

#[test]
fn a_binding_the_tracer_does_not_follow_is_reported_at_each_literal() {
    let text = "export function render() {\n\
                \x20 let changing = document.querySelector('#a')\n  changing.textContent = 'A'\n\
                \x20 var old = document.createElement('b')\n  old.textContent = 'B'\n\
                \x20 const kept = document.querySelector('#c')\n  let copy = kept\n  copy.textContent = 'C'\n\
                }\n";
    let analysis = analyze_dom(text);
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    let binding = detail::receiver_binding_unsupported();
    assert_eq!(
        diagnostics(&analysis),
        [
            expect(unsupported, binding, at(text, "changing.textContent = 'A'")),
            expect(unsupported, binding, at(text, "old.textContent = 'B'")),
            expect(unsupported, binding, at(text, "copy.textContent = 'C'")),
        ]
    );
    assert_eq!(declared(&analysis), []);
}

#[test]
fn a_scripts_top_level_const_is_shared_and_not_followed() {
    let text = "const pay = document.querySelector('#pay')\npay.textContent = 'Pay now'\n\
                function render() {\n  const inner = document.querySelector('#inner')\n  inner.textContent = 'Inner'\n}\n";
    let analysis = analyze_dom_as(Grammar::JsScript, text);
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringFormUnsupported,
            detail::receiver_binding_unsupported(),
            at(text, SINK)
        )]
    );
    assert_eq!(
        declared(&analysis),
        [(OccurrenceRole::UiLiteral, at(text, "'Inner'"))]
    );
}

#[test]
fn a_receiver_used_in_another_function_is_reported_there() {
    let text = "export function render() {\n\
                \x20 const pay = document.querySelector('#pay')\n\
                \x20 function later() { pay.textContent = 'Later' }\n\
                \x20 return later\n}\n";
    let analysis = analyze_dom(text);
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringFormUnsupported,
            detail::receiver_captured(),
            at(text, "pay.textContent = 'Later'")
        )]
    );
}

#[test]
fn a_receiver_reached_only_through_data_is_outside_the_profile() {
    let text = "const [first] = [document.querySelector('#a')]\nfirst.textContent = 'A'\n\
                const holder = { el: document.querySelector('#b') }\nholder.el.textContent = 'B'\n\
                export function show(element) { element.textContent = 'C' }\n";
    let analysis = analyze_dom(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis), []);
    assert_eq!(analysis.outside_profile(), 3);
}

#[test]
fn passing_a_receiver_anywhere_invalidates_every_alias_of_it() {
    for (effect, why) in [
        ("  customize(pay)", "an argument"),
        ("  new Wrapper(pay)", "a constructor argument"),
        ("  customize(...[pay])", "a spread argument"),
        ("  customize`${pay}`", "a tagged template"),
        (
            "  const alias = pay\n  customize(alias)",
            "an alias passed on",
        ),
    ] {
        let text = after(effect);
        assert_eq!(
            diagnostics(&analyze_dom(&text)),
            [invalidated(&text, SINK)],
            "{why}"
        );
    }
}

#[test]
fn the_call_itself_is_not_an_error_and_intent_stays_valid() {
    // Replacing the literal with `intent()` authors it explicitly, which
    // needs no receiver evidence and restores none for a literal after it.
    let text = format!(
        "{PRELUDE}const pay = document.querySelector('#pay')\n\
         customize(pay)\n\
         pay.textContent = intent('Pay now')\n\
         pay.textContent = 'Pay later'\n"
    );
    let analysis = analyze_dom(&text);
    assert_eq!(
        diagnostics(&analysis),
        [invalidated(&text, "pay.textContent = 'Pay later'")]
    );
    assert_eq!(
        declared(&analysis),
        [(OccurrenceRole::IntentLiteral, at(&text, "'Pay now'"))]
    );
}

#[test]
fn a_method_call_or_a_harmless_read_keeps_the_evidence() {
    let text = after(
        "  pay.addEventListener('click', handle)\n\
         \x20 if (pay && typeof pay === 'object' && !pay.hidden && pay instanceof HTMLElement) { log(pay.id) }\n\
         \x20 pay.dataset.state = 'ready'\n\
         \x20 pay.textContent = 'Loading'",
    );
    let analysis = analyze_dom(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis).len(), 2);
}

#[test]
fn every_other_effect_on_a_receiver_invalidates_it() {
    for (effect, why) in [
        (
            "  delete pay.textContent",
            "redefining the display property",
        ),
        ("  pay.__proto__ = other", "changing the prototype"),
        ("  pay[other] = 1", "a computed write"),
        ("  const box = { pay }", "storing in an object"),
        ("  const list = [pay]", "storing in an array"),
        ("  let copy = pay", "assigning to another variable"),
        ("  other.el = pay", "assigning to a property"),
        ("  const found = other || pay", "a value chosen at run time"),
        (
            "  const later = () => pay.id",
            "a closure made before the sink",
        ),
        (
            "  const make = function () { return pay }",
            "a function expression",
        ),
        ("  class Holder { el = pay }", "a class"),
    ] {
        let text = after(effect);
        assert_eq!(
            diagnostics(&analyze_dom(&text)),
            [invalidated(&text, SINK)],
            "{why}"
        );
    }
}

#[test]
fn a_hoisted_declaration_captures_from_where_its_function_starts() {
    // `helper` could run before the sink, so the capture counts from the
    // start of `render`, not from where the declaration is written.
    let text = format!(
        "{PRELUDE}export function render() {{\n  const pay = document.querySelector('#pay')\n  pay.textContent = 'Before'\n  function helper() {{ return pay }}\n}}\n"
    );
    assert_eq!(
        diagnostics(&analyze_dom(&text)),
        [invalidated(&text, "pay.textContent = 'Before'")]
    );
}

#[test]
fn a_capture_counts_from_where_the_closure_is_made() {
    let text = "export function render() {\n\
                \x20 const pay = document.querySelector('#pay')\n\
                \x20 pay.textContent = 'Before'\n\
                \x20 const later = () => pay.id\n\
                \x20 pay.textContent = 'After'\n\
                \x20 return later\n}\n";
    let analysis = analyze_dom(text);
    assert_eq!(
        diagnostics(&analysis),
        [invalidated(text, "pay.textContent = 'After'")]
    );
    assert_eq!(
        declared(&analysis),
        [(OccurrenceRole::UiLiteral, at(text, "'Before'"))]
    );
}

#[test]
fn yielding_or_exporting_a_receiver_exposes_it() {
    let text = "export function* render() {\n  const pay = document.querySelector('#pay')\n  yield pay\n  pay.textContent = 'Pay now'\n}\n";
    assert_eq!(diagnostics(&analyze_dom(text)), [invalidated(text, SINK)]);

    // A path that returns the receiver ends there, so the sink after it is
    // reached only on the path that kept the evidence.
    let text = format!(
        "{PRELUDE}export function render() {{\n  const pay = document.querySelector('#pay')\n  if (flag) return pay\n  pay.textContent = 'Pay now'\n}}\n"
    );
    let analysis = analyze_dom(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis).len(), 1);

    let text = "export const pay = document.querySelector('#pay')\npay.textContent = 'Pay now'\n";
    assert_eq!(diagnostics(&analyze_dom(text)), [invalidated(text, SINK)]);
    let text =
        "const pay = document.querySelector('#pay')\nexport { pay }\npay.textContent = 'Pay now'\n";
    assert_eq!(diagnostics(&analyze_dom(text)), [invalidated(text, SINK)]);
}

#[test]
fn an_alias_made_after_invalidation_does_not_restore_evidence() {
    let text = after("  customize(pay)\n  const fresh = pay\n  fresh.textContent = 'Fresh'");
    assert_eq!(
        diagnostics(&analyze_dom(&text)),
        [
            invalidated(&text, "fresh.textContent = 'Fresh'"),
            invalidated(&text, SINK),
        ]
    );
}

#[test]
fn a_sink_in_a_scope_no_walk_reads_is_not_proven() {
    // A class static block runs where the class is made, but it is not a
    // function this walk follows, so nothing in it is proven.
    let text = "class Panel {\n  static {\n    const label = document.createElement('p')\n    label.textContent = 'Static'\n  }\n}\n";
    assert_eq!(
        diagnostics(&analyze_dom(text)),
        [invalidated(text, "label.textContent = 'Static'")]
    );
}
