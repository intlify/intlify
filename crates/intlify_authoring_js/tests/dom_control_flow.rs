// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Receiver control flow: evidence has to hold on every path to a sink.
//!
//! Each case places one effect, `customize(pay)`, and one sink,
//! `pay.textContent = 'Pay now'`. The expectation is derived from the
//! language: the sink is proven exactly when no path that can run reaches
//! it after the effect ran.

mod support;

use std::cell::Cell;
use std::fmt::Write;

use intlify_authoring::{OccurrenceRole, ReasonFamily, UnitOutcome};
use intlify_authoring_js::{
    analyze_unit, detail, Grammar, JsAnalysisWorkspace, JsLimitKind, ProducerFailure,
};
use support::{
    admit, analyze_dom, analyze_dom_as, at, context, diagnostics, dom_profile, expect, limits,
    occurrence, try_analyze,
};

const SINK: &str = "pay.textContent = 'Pay now'";

/// Wrap `body` in a function that makes `pay` first.
fn inside(body: &str) -> String {
    format!(
        "export function render(c, k, xs, o) {{\n  const pay = document.querySelector('#pay')\n  {body}\n}}\n"
    )
}

/// Whether the sink in `body` is proven.
fn proven(body: &str) -> bool {
    let text = inside(body);
    let analysis = analyze_dom(&text);
    let sink = at(&text, SINK);
    let invalidated = expect(
        ReasonFamily::AuthoringFormUnsupported,
        detail::receiver_evidence_invalidated(),
        sink,
    );
    let reported = diagnostics(&analysis);
    let declared = analysis
        .inspection_facts()
        .declarations()
        .iter()
        .any(|facts| {
            occurrence(facts.occurrence()) == (OccurrenceRole::UiLiteral, at(&text, "'Pay now'"))
        });
    match (reported.as_slice(), declared) {
        ([], true) => true,
        ([only], false) if *only == invalidated => false,
        other => panic!("{body:?} is either proven or invalidated: {other:?}"),
    }
}

#[test]
fn branches_join_on_every_path() {
    for (body, expected, why) in [
        (
            "if (c) { pay.hidden = true } else { log(c) }\n  pay.textContent = 'Pay now'",
            true,
            "neither branch invalidates",
        ),
        (
            "if (c) { customize(pay) } else { log(c) }\n  pay.textContent = 'Pay now'",
            false,
            "one branch invalidates",
        ),
        (
            "if (c) customize(pay)\n  pay.textContent = 'Pay now'",
            false,
            "the branch without an else",
        ),
        (
            "if (c) { customize(pay) } else { pay.textContent = 'Pay now' }",
            true,
            "the sink is on the other branch",
        ),
        (
            "c ? customize(pay) : (pay.textContent = 'Pay now')",
            true,
            "the sink is on the other arm",
        ),
        (
            "c && customize(pay)\n  pay.textContent = 'Pay now'",
            false,
            "the right operand may run",
        ),
        (
            "o ??= customize(pay)\n  pay.textContent = 'Pay now'",
            false,
            "a logical assignment may run",
        ),
        (
            "customize(pay) || (pay.textContent = 'Pay now')",
            false,
            "the left operand runs first",
        ),
        // The leading semicolon keeps the line from continuing the call above.
        (
            ";(pay.textContent = 'Pay now') && customize(pay)",
            true,
            "the sink runs before the right operand",
        ),
    ] {
        assert_eq!(proven(body), expected, "{why}: {body}");
    }
}

#[test]
fn a_switch_falls_through_and_a_break_stops_it() {
    for (body, expected, why) in [
        (
            "switch (k) { case 1: customize(pay)\n  case 2: pay.textContent = 'Pay now' }",
            false,
            "falling through",
        ),
        (
            "switch (k) { case 1: customize(pay); break\n  case 2: pay.textContent = 'Pay now' }",
            true,
            "a break ends the case",
        ),
        (
            "switch (k) { case 1: customize(pay) }\n  pay.textContent = 'Pay now'",
            false,
            "a matching case runs before",
        ),
        (
            "switch (k) { case customize(pay): break }\n  pay.textContent = 'Pay now'",
            false,
            "a case test runs too",
        ),
    ] {
        assert_eq!(proven(body), expected, "{why}: {body}");
    }
}

#[test]
fn a_loop_carries_what_an_earlier_iteration_did() {
    for (body, expected, why) in [
        ("while (c) { pay.textContent = 'Pay now'; customize(pay) }", false, "the next iteration"),
        ("while (c) { pay.textContent = 'Pay now'; break; customize(pay) }", true, "the effect never runs"),
        ("do { customize(pay) } while (c)\n  pay.textContent = 'Pay now'", false, "the body runs at least once"),
        ("for (let i = 0; i < 2; i++) { pay.textContent = 'Pay now'; if (i) customize(pay) }", false, "a later iteration"),
        ("for (const x of xs) { if (x) continue; customize(pay) }\n  pay.textContent = 'Pay now'", false, "some iteration reaches the effect"),
        ("for (const x in o) { pay.textContent = 'Pay now' }\n  customize(pay)", true, "the effect is after the loop"),
        ("while (c) { while (k) { pay.textContent = 'Pay now' } customize(pay) }", false, "an outer iteration reaches the inner loop again"),
        ("for (;;) { if (c) break; customize(pay) }\n  pay.textContent = 'Pay now'", false, "an iteration before the break"),
        ("for (;;) { if (c) { customize(pay); continue } break }\n  pay.textContent = 'Pay now'", false, "a continue loops back"),
    ] {
        assert_eq!(proven(body), expected, "{why}: {body}");
    }
}

#[test]
fn labels_send_a_state_where_the_jump_goes() {
    for (body, expected, why) in [
        ("block: { if (c) break block; customize(pay) }\n  pay.textContent = 'Pay now'", false, "the path past the break"),
        ("block: { break block; customize(pay) }\n  pay.textContent = 'Pay now'", true, "the effect is never reached"),
        ("outer: for (const x of xs) { for (const y of xs) { if (y) continue outer; customize(pay) } }\n  pay.textContent = 'Pay now'", false, "a labeled continue"),
    ] {
        assert_eq!(proven(body), expected, "{why}: {body}");
    }
}

#[test]
fn a_try_statement_passes_every_path_through_its_finally() {
    for (body, expected, why) in [
        ("try { customize(pay) } catch {}\n  pay.textContent = 'Pay now'", false, "the block ran"),
        ("try { c() } catch { customize(pay) }\n  pay.textContent = 'Pay now'", false, "the handler ran"),
        ("try { pay.textContent = 'Pay now' } catch { customize(pay) }", true, "the handler runs after the sink"),
        ("try { customize(pay); c() } catch { pay.textContent = 'Pay now' }", false, "a throw after the effect"),
        ("try { c() } finally { customize(pay) }\n  pay.textContent = 'Pay now'", false, "the finally ran"),
        ("try { pay.textContent = 'Pay now' } finally { customize(pay) }", true, "the finally runs after the sink"),
        ("for (;;) { try { break } finally { customize(pay) } }\n  pay.textContent = 'Pay now'", false, "a break runs the finally"),
        ("for (let i = 0; i < 2; i++) { try { continue } finally { customize(pay) } }\n  pay.textContent = 'Pay now'", false, "a continue runs the finally"),
        ("outer: { try { break outer } finally { customize(pay) } }\n  pay.textContent = 'Pay now'", false, "a labeled break runs the finally"),
        ("for (;;) { try { break } finally { log(c) } }\n  pay.textContent = 'Pay now'", true, "the finally does nothing to it"),
    ] {
        assert_eq!(proven(body), expected, "{why}: {body}");
    }
}

#[test]
fn a_path_that_leaves_the_function_does_not_reach_the_sink() {
    for (body, expected, why) in [
        (
            "if (c) { customize(pay); return }\n  pay.textContent = 'Pay now'",
            true,
            "the effect's path returns",
        ),
        (
            "if (c) { customize(pay); throw new Error('stop') }\n  pay.textContent = 'Pay now'",
            true,
            "the effect's path throws",
        ),
        // Code after an unconditional return is still read, with the state
        // it would have had.
        (
            "customize(pay)\n  return\n  pay.textContent = 'Pay now'",
            false,
            "dead code keeps its state",
        ),
    ] {
        assert_eq!(proven(body), expected, "{why}: {body}");
    }
}

#[test]
fn a_dynamically_scoped_function_admits_no_origin() {
    let text = "function withScope(o) {\n  with (o) {\n    const pay = document.querySelector('#pay')\n    pay.textContent = 'Pay now'\n  }\n}\n\
                function evaluated(code) {\n  eval(code)\n  document.querySelector('#b').textContent = 'B'\n}\n\
                function plain() {\n  const c = document.querySelector('#c')\n  c.textContent = 'C'\n}\n";
    let analysis = analyze_dom_as(Grammar::JsScript, text);
    let dynamic = detail::origin_scope_dynamic();
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    assert_eq!(
        diagnostics(&analysis),
        [
            expect(unsupported, dynamic, at(text, SINK)),
            expect(
                unsupported,
                dynamic,
                at(text, "document.querySelector('#b').textContent = 'B'")
            ),
        ]
    );
    assert_eq!(analysis.inspection_facts().declarations().len(), 1);

    // A module is strict, so `eval` cannot declare anything around it.
    let module =
        "const pay = document.querySelector('#pay')\neval(code)\npay.textContent = 'Pay now'\n";
    let analysis = analyze_dom(module);
    assert_eq!(diagnostics(&analysis), []);
}

#[test]
fn origins_one_function_tracks_are_bounded_exactly() {
    let text = inside("const other = document.createElement('p')\n  pay.textContent = 'Pay now'");
    let mut bounded = limits();
    bounded.tracked_origins = 2;
    let analysis = try_analyze(Grammar::JsModule, &text, &dom_profile(), &bounded).unwrap();
    assert_eq!(diagnostics(&analysis), []);
    bounded.tracked_origins = 1;
    let analysis = try_analyze(Grammar::JsModule, &text, &dom_profile(), &bounded).unwrap();
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringResourceLimit,
            detail::limit(JsLimitKind::TrackedOrigins),
            at(&text, SINK)
        )]
    );
}

#[test]
fn proof_steps_are_bounded_exactly_and_an_unfinished_proof_proves_nothing() {
    let text = inside("while (c) { log(pay.id) }\n  pay.textContent = 'Pay now'");
    let within = |steps| {
        let mut bounded = limits();
        bounded.proof_steps = steps;
        try_analyze(Grammar::JsModule, &text, &dom_profile(), &bounded).unwrap()
    };
    let steps = (0..=4096)
        .find(|&steps| within(steps).outcome() == UnitOutcome::Checked)
        .expect("some bound finishes the proof");
    assert!(steps > 0);
    assert_eq!(diagnostics(&within(steps)), []);
    assert_eq!(
        diagnostics(&within(steps - 1)),
        [expect(
            ReasonFamily::AuthoringResourceLimit,
            detail::limit(JsLimitKind::ProofSteps),
            at(&text, SINK)
        )]
    );
}

#[test]
fn stopping_at_any_probe_during_a_proof_yields_no_result() {
    let mut body = String::new();
    for index in 0..400 {
        writeln!(body, "  pay.textContent = 'Message {index}'").expect("writing to a string");
    }
    let text = inside(&body);
    let units = admit(&[("checkout", Grammar::JsModule, text.as_bytes())]);
    let stopping = |stop_at: Option<u32>| {
        let asked = Cell::new(0);
        let probe = || {
            asked.set(asked.get() + 1);
            stop_at.is_some_and(|stop| asked.get() >= stop)
        };
        let mut roomy = limits();
        roomy.authoring.declarations = 1024;
        roomy.references = 1024;
        let result = analyze_unit(
            &context(),
            &dom_profile(),
            &units[0],
            &roomy,
            &mut JsAnalysisWorkspace::new(),
            &probe,
        );
        (result, asked.get())
    };
    let (finished, asked) = stopping(None);
    assert_eq!(
        finished.expect("the analysis runs").outcome(),
        UnitOutcome::Checked
    );
    for stop_at in 1..=asked {
        let (result, asked_until_stop) = stopping(Some(stop_at));
        assert_eq!(
            result,
            Err(ProducerFailure::Cancelled),
            "stopping at {stop_at}"
        );
        assert_eq!(asked_until_stop, stop_at, "nothing asks again after a stop");
    }
}
