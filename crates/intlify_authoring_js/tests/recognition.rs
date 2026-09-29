// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Recognition as a whole: bounds, cancellation, and workspace reuse.

mod support;

use std::cell::Cell;
use std::fmt::Write;

use intlify_authoring::{AuthoringFailure, LimitKind, ReasonFamily, UnitOutcome};
use intlify_authoring_js::{
    analyze_unit, detail, Grammar, JsAnalysisWorkspace, JsLimitKind, ProducerFailure, UnitAnalysis,
};
use support::{
    admit, at, context, diagnostics, expect, limits, never, occurrence, profile, try_analyze,
    PRELUDE,
};

fn source(body: &str) -> String {
    format!("{PRELUDE}{body}")
}

#[test]
fn declarations_one_unit_holds_are_the_shared_bound_exactly() {
    let text = source("intent('One')\nintent('Two')\n");
    let mut bounded = limits();
    bounded.authoring.declarations = 2;
    assert!(try_analyze(Grammar::JsModule, &text, &profile(), &bounded).is_ok());
    bounded.authoring.declarations = 1;
    assert_eq!(
        try_analyze(Grammar::JsModule, &text, &profile(), &bounded),
        Err(ProducerFailure::Authoring(AuthoringFailure::Limit(
            LimitKind::Declarations
        )))
    );
}

#[test]
fn references_one_unit_makes_are_bounded_exactly() {
    let text = source("const shared = mf2`Shared`\nintent(shared)\nintent(shared)\n");
    let mut bounded = limits();
    bounded.references = 2;
    assert!(try_analyze(Grammar::JsModule, &text, &profile(), &bounded).is_ok());
    bounded.references = 1;
    assert_eq!(
        try_analyze(Grammar::JsModule, &text, &profile(), &bounded),
        Err(ProducerFailure::Limit(JsLimitKind::References))
    );
}

#[test]
fn a_bound_on_one_literal_blocks_only_its_declaration() {
    // `a`, the escape and `b` are three runs, one more than allowed.
    let text = source("intent('a\\nb')\nintent('Pay')\n");
    let mut bounded = limits();
    bounded.input_segments = 2;
    let analysis = try_analyze(Grammar::JsModule, &text, &profile(), &bounded).unwrap();
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringResourceLimit,
            detail::limit(JsLimitKind::InputSegments),
            at(&text, "'a\\nb'")
        )]
    );
    let facts = analysis.inspection_facts();
    assert_eq!(facts.declarations().len(), 1);
    assert_eq!(
        occurrence(facts.declarations()[0].occurrence()).1,
        at(&text, "'Pay'")
    );
    assert_eq!(facts.references().len(), 1);
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
}

#[test]
fn a_shared_bound_on_one_message_blocks_only_its_declaration() {
    let text = source("intent('Pay')\nintent('Pay now')\n");
    let mut bounded = limits();
    bounded.authoring.message_text_bytes = 3;
    let analysis = try_analyze(Grammar::JsModule, &text, &profile(), &bounded).unwrap();
    // The shared crate names its own bound in the record's limit.
    let [record] = analysis.diagnostics() else {
        panic!("one record: {:?}", analysis.diagnostics());
    };
    assert_eq!(
        record.origin().code(),
        ReasonFamily::AuthoringResourceLimit.as_str()
    );
    assert_eq!(record.limit(), Some(LimitKind::MessageTextBytes));
    assert_eq!(
        record.occurrence().map(|at| occurrence(at).1),
        Some(at(&text, "'Pay now'"))
    );
    let facts = analysis.inspection_facts();
    assert_eq!(facts.declarations().len(), 1);
    assert_eq!(facts.references().len(), 1);
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
}

#[test]
fn diagnostics_are_the_shared_bound_for_host_and_shared_records_together() {
    // Two host records and one from the shared crate.
    let text = source("intent(first())\nintent(second())\nintent('Hello {$name}!', {})\n");
    let mut bounded = limits();
    bounded.authoring.diagnostics = 3;
    let analysis = try_analyze(Grammar::JsModule, &text, &profile(), &bounded).unwrap();
    assert_eq!(analysis.diagnostics().len(), 3);
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
    // Reaching the bound stops the analysis: a shortened list could make a
    // blocked unit look checked.
    bounded.authoring.diagnostics = 2;
    assert_eq!(
        try_analyze(Grammar::JsModule, &text, &profile(), &bounded),
        Err(ProducerFailure::Authoring(AuthoringFailure::Limit(
            LimitKind::Diagnostics
        )))
    );
}

/// Analyze `text` with a probe that stops at call `stop_at`, returning the
/// result and how many times the probe was asked.
fn stopping(text: &str, stop_at: Option<u32>) -> (Result<UnitAnalysis, ProducerFailure>, u32) {
    let units = admit(&[("checkout", Grammar::JsModule, text.as_bytes())]);
    let calls = Cell::new(0);
    let probe = || {
        calls.set(calls.get() + 1);
        stop_at.is_some_and(|stop| calls.get() >= stop)
    };
    let mut roomy = limits();
    roomy.authoring.declarations = 1024;
    roomy.references = 1024;
    let result = analyze_unit(
        &context(),
        &profile(),
        &units[0],
        &roomy,
        &mut JsAnalysisWorkspace::new(),
        &probe,
    );
    (result, calls.get())
}

#[test]
fn stopping_at_any_probe_yields_no_result() {
    // Enough calls that the walk asks the probe itself, and enough
    // declarations that the shared crate asks it between them. The walk's
    // interval counts the nodes it enters, three per call here, so 400 calls
    // pass it once; `explicit.rs` pins where the walk asks.
    let mut body = String::new();
    for index in 0..400 {
        writeln!(body, "intent('Message {index}')").expect("writing to a string");
    }
    let text = source(&body);
    let (finished, asked) = stopping(&text, None);
    let finished = finished.expect("an unstopped analysis runs");
    assert_eq!(finished.outcome(), UnitOutcome::Checked);
    assert!(
        asked > 3,
        "the walk and the shared crate ask as well: {asked}"
    );
    for stop_at in 1..=asked {
        let (result, asked_until_stop) = stopping(&text, Some(stop_at));
        assert_eq!(
            result,
            Err(ProducerFailure::Cancelled),
            "stopping at {stop_at}"
        );
        assert_eq!(asked_until_stop, stop_at, "nothing asks again after a stop");
    }
}

#[test]
fn a_reused_workspace_gives_what_a_fresh_one_gives_for_recognized_units() {
    let texts = [
        source("const greeting = mf2`Hello {$name}!`\nintent(greeting, { name })\nnoIntent(brand, 'Brand')\n"),
        source("intent(computed())\nintent('Hello {$name}!', {})\n"),
        source("intent('Pay now')\n"),
    ];
    let units: Vec<_> = texts
        .iter()
        .enumerate()
        .map(|(index, text)| (format!("unit-{index}"), text.as_bytes()))
        .collect();
    let named: Vec<(&str, Grammar, &[u8])> = units
        .iter()
        .map(|(name, bytes)| (name.as_str(), Grammar::JsModule, *bytes))
        .collect();
    let admitted = admit(&named);
    let analyze = |unit, workspace: &mut JsAnalysisWorkspace| {
        analyze_unit(&context(), &profile(), unit, &limits(), workspace, &never).unwrap()
    };
    let fresh: Vec<UnitAnalysis> = admitted
        .iter()
        .map(|unit| analyze(unit, &mut JsAnalysisWorkspace::new()))
        .collect();

    let mut workspace = JsAnalysisWorkspace::new();
    // A unit cancelled after its tree was built and its forms were read.
    let calls = Cell::new(0);
    let late = || {
        calls.set(calls.get() + 1);
        calls.get() >= 4
    };
    assert_eq!(
        analyze_unit(
            &context(),
            &profile(),
            &admitted[0],
            &limits(),
            &mut workspace,
            &late
        ),
        Err(ProducerFailure::Cancelled)
    );
    let reused: Vec<UnitAnalysis> = admitted
        .iter()
        .rev()
        .chain(&admitted)
        .map(|unit| analyze(unit, &mut workspace))
        .collect();
    let expected: Vec<&UnitAnalysis> = fresh.iter().rev().chain(&fresh).collect();
    assert_eq!(reused.iter().collect::<Vec<_>>(), expected);
    assert_eq!(fresh[0].outcome(), UnitOutcome::Checked);
    assert_eq!(fresh[1].outcome(), UnitOutcome::Blocked);
}
