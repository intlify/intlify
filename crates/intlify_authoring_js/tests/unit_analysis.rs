// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Analysis: reading each admitted unit once, under its own grammar.

mod support;

use std::cell::Cell;

use intlify_authoring::{DiagnosticOrigin, Location, ReasonFamily, Severity, Stage, UnitOutcome};
use intlify_authoring_js::{
    analyze_unit, detail, Grammar, JsAnalysisWorkspace, JsLimitKind, ProducerFailure, UnitAnalysis,
};
use support::{admit, limits, never};

fn analyze(unit: &intlify_authoring_js::AdmittedUnit<'_>) -> UnitAnalysis {
    analyze_unit(unit, &limits(), &mut JsAnalysisWorkspace::new(), &never).expect("analysis runs")
}

/// Assert that `analysis` failed with exactly one record of `detail`.
fn assert_failed(analysis: &UnitAnalysis, detail: intlify_authoring::Detail) {
    assert_eq!(analysis.outcome(), UnitOutcome::Failed);
    let [diagnostic] = analysis.diagnostics() else {
        panic!("one diagnostic: {:?}", analysis.diagnostics());
    };
    assert_eq!(diagnostic.stage(), Stage::HostDiscovery);
    assert_eq!(
        diagnostic.origin(),
        DiagnosticOrigin::Authoring(ReasonFamily::AuthoringInputInvalid)
    );
    assert_eq!(diagnostic.severity(), Severity::Error);
    assert_eq!(diagnostic.detail(), Some(detail));
    assert_eq!(diagnostic.location().source(), analysis.source());
}

#[test]
fn each_text_unit_is_parsed_exactly_once() {
    let binary: &[u8] = &[0x66, 0x28, 0xff, 0x29];
    let units = admit(&[
        ("a", Grammar::JsModule, b"f('a')\n"),
        ("b", Grammar::JsScript, b"var x = 1\n"),
        ("c", Grammar::TsModule, b"const y: number = 2\n"),
        ("d", Grammar::JsModule, b"const = ;\n"),
        ("e", Grammar::JsModule, binary),
    ]);
    let mut workspace = JsAnalysisWorkspace::new();
    let attempts: Vec<u64> = units
        .iter()
        .map(|unit| {
            analyze_unit(unit, &limits(), &mut workspace, &never)
                .unwrap()
                .work()
                .parse_attempts
        })
        .collect();
    // A rejected unit was still parsed once; one that is not text never was.
    assert_eq!(attempts, [1, 1, 1, 1, 0]);
    let text_units = units.iter().filter(|unit| unit.text().is_some()).count() as u64;
    assert_eq!(attempts.iter().sum::<u64>(), text_units);
}

#[test]
fn an_accepted_unit_is_checked_and_reports_what_was_read() {
    let units = admit(&[("a", Grammar::JsModule, b"const pay = f('Pay now')\n")]);
    let analysis = analyze(&units[0]);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    assert!(analysis.diagnostics().is_empty());
    let work = analysis.work();
    assert_eq!(work.source_bytes, 25);
    assert_eq!(work.parse_attempts, 1);
    assert!(work.ast_nodes > 0 && work.symbols == 1);
}

#[test]
fn a_unit_that_is_not_text_fails_without_being_parsed() {
    let binary: &[u8] = &[0x66, 0x28, 0xff, 0x29];
    let units = admit(&[("a", Grammar::JsModule, binary)]);
    let analysis = analyze(&units[0]);
    assert_failed(&analysis, detail::unit_not_text());
    assert!(
        matches!(analysis.diagnostics()[0].location(), Location::Unit(_)),
        "no range can address bytes that are not text"
    );
    assert_eq!(analysis.work().parse_attempts, 0);
}

#[test]
fn a_unit_the_host_rejects_fails_at_a_range_inside_it() {
    for (grammar, source) in [
        // The parser itself rejects this.
        (Grammar::JsModule, "const = ;\n"),
        // The parser accepts this; the language's early errors do not.
        (Grammar::JsModule, "let a; let a\n"),
        // Strict code has no legacy octal escapes.
        (Grammar::JsModule, "f('\\101')\n"),
        // A pattern that does not parse is an early error too.
        (Grammar::JsModule, "const r = /(/\n"),
    ] {
        let units = admit(&[("a", grammar, source.as_bytes())]);
        let analysis = analyze(&units[0]);
        assert_failed(&analysis, detail::host_syntax_invalid());
        let Location::Region(region) = analysis.diagnostics()[0].location() else {
            panic!("{source:?}: a checked range is reported");
        };
        assert!(region.range().end() <= source.len() as u64);
        assert_eq!(
            analysis.work().ast_nodes,
            0,
            "a rejected tree is not counted"
        );
    }
}

#[test]
fn a_script_carrying_module_syntax_is_rejected_in_either_language() {
    for (grammar, source) in [
        (Grammar::JsScript, "import x from 'y'\n"),
        (Grammar::JsScript, "export const a = 1\n"),
        // The parser cannot yet tell a TypeScript script from a module, so it
        // accepts these; the snapshot says script, and a script has no imports.
        (Grammar::TsScript, "import x from 'y'\n"),
        (Grammar::TsScript, "import type { X } from 'y'\n"),
        (Grammar::TsScript, "export {}\n"),
        (Grammar::TsScript, "import x = require('y')\n"),
    ] {
        let units = admit(&[("a", grammar, source.as_bytes())]);
        assert_failed(&analyze(&units[0]), detail::host_syntax_invalid());
    }
    for (grammar, source) in [
        (Grammar::TsModule, "import x from 'y'\n"),
        // An alias of a namespace member is not module syntax.
        (
            Grammar::TsScript,
            "namespace N { export const y = 1 }\nimport x = N.y\n",
        ),
        // Nor is an ambient module declaration, whose body may export.
        (
            Grammar::TsScript,
            "declare module 'm' { export const y: number }\n",
        ),
    ] {
        let units = admit(&[("a", grammar, source.as_bytes())]);
        assert_eq!(
            analyze(&units[0]).outcome(),
            UnitOutcome::Checked,
            "{grammar:?} {source:?}"
        );
    }
}

#[test]
fn the_grammar_comes_from_the_snapshot_never_from_the_unit_name() {
    let typescript = b"const label: string = 'Pay now'\n";
    // A unit named like a TypeScript file but declared JavaScript is read as
    // JavaScript, and a unit named like JavaScript but declared TypeScript is
    // read as TypeScript.
    let units = admit(&[
        ("view.ts", Grammar::JsModule, typescript),
        ("view.js", Grammar::TsModule, typescript),
    ]);
    assert_eq!(units[0].snapshot().unit().as_str(), "view.js");
    assert_eq!(analyze(&units[0]).outcome(), UnitOutcome::Checked);
    assert_eq!(units[1].snapshot().unit().as_str(), "view.ts");
    assert_failed(&analyze(&units[1]), detail::host_syntax_invalid());
}

#[test]
fn a_byte_order_mark_and_a_hashbang_are_read_as_part_of_the_unit() {
    for source in [
        "\u{feff}f('Pay now')\n",
        "#!/usr/bin/env node\nf('Pay now')\n",
    ] {
        let units = admit(&[("a", Grammar::JsModule, source.as_bytes())]);
        let analysis = analyze(&units[0]);
        assert_eq!(analysis.outcome(), UnitOutcome::Checked, "{source:?}");
        assert_eq!(analysis.work().source_bytes, source.len() as u64);
    }
    // The unit's text is its bytes, byte order mark included, and to the
    // language that mark is whitespace. A hashbang after it is no longer at
    // the start of the text, so it is not a hashbang. A host that strips the
    // mark while decoding would run this file; this Producer reads the bytes
    // it was given, and the range it reports stays in those bytes.
    let source = "\u{feff}#!/usr/bin/env node\nf('Pay now')\n";
    let units = admit(&[("a", Grammar::JsModule, source.as_bytes())]);
    assert_failed(&analyze(&units[0]), detail::host_syntax_invalid());
}

#[test]
fn syntax_tree_nodes_are_bounded_exactly() {
    let units = admit(&[("a", Grammar::JsModule, b"f('a', g('b'))\n")]);
    let nodes = analyze(&units[0]).work().ast_nodes;
    let mut bounded = limits();
    bounded.ast_nodes = nodes;
    assert_eq!(
        analyze_unit(&units[0], &bounded, &mut JsAnalysisWorkspace::new(), &never)
            .unwrap()
            .outcome(),
        UnitOutcome::Checked
    );
    bounded.ast_nodes = nodes - 1;
    assert_eq!(
        analyze_unit(&units[0], &bounded, &mut JsAnalysisWorkspace::new(), &never),
        Err(ProducerFailure::Limit(JsLimitKind::AstNodes))
    );
}

#[test]
fn cancellation_at_any_probe_is_an_operational_failure() {
    let units = admit(&[("a", Grammar::JsModule, b"f('a')\n")]);
    // Asked once before parsing, once after, and once after the semantic
    // checks. Stopping at each of them yields no result.
    for stop_at in 1..=3 {
        let calls = Cell::new(0);
        let probe = || {
            calls.set(calls.get() + 1);
            calls.get() >= stop_at
        };
        assert_eq!(
            analyze_unit(
                &units[0],
                &limits(),
                &mut JsAnalysisWorkspace::new(),
                &probe
            ),
            Err(ProducerFailure::Cancelled),
            "stopping at probe {stop_at}"
        );
    }
}

#[test]
fn a_reused_workspace_gives_what_a_fresh_one_gives() {
    let binary: &[u8] = &[0x66, 0x28, 0xff, 0x29];
    let units = admit(&[
        ("a", Grammar::JsModule, b"const pay = f('Pay now')\n"),
        ("b", Grammar::JsModule, b"const = ;\n"),
        ("c", Grammar::JsModule, binary),
        ("d", Grammar::TsModule, b"enum E { A }\nconst e: E = E.A\n"),
    ]);
    let fresh: Vec<UnitAnalysis> = units.iter().map(analyze).collect();

    let mut workspace = JsAnalysisWorkspace::new();
    let first = analyze_unit(&units[0], &limits(), &mut workspace, &never).unwrap();
    let retained = workspace.arena_capacity();
    assert!(retained > 0, "the arena was used");
    // A unit cancelled after its tree was built leaves nothing behind for the
    // next one.
    let calls = Cell::new(0);
    let after_parsing = || {
        calls.set(calls.get() + 1);
        calls.get() >= 2
    };
    assert_eq!(
        analyze_unit(&units[3], &limits(), &mut workspace, &after_parsing),
        Err(ProducerFailure::Cancelled)
    );
    let mut reused = Vec::new();
    for unit in units.iter().rev().chain(&units) {
        reused.push(analyze_unit(unit, &limits(), &mut workspace, &never).unwrap());
    }
    assert!(workspace.arena_capacity() >= retained, "capacity is kept");

    // Results from before the workspace moved on are still whole: nothing in
    // them pointed into the arena.
    assert_eq!(first, fresh[0]);
    let expected: Vec<&UnitAnalysis> = fresh.iter().rev().chain(&fresh).collect();
    assert_eq!(reused.iter().collect::<Vec<_>>(), expected);
}
