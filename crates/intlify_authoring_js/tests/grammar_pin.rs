// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The acceptance a grammar revision stands for.
//!
//! A grammar revision names what this Producer accepts, not which parser
//! version it links, so the parser version cannot be its evidence. This
//! corpus is. Whether each source is accepted was derived from the language,
//! where an early error the specification names is a rejection. For an
//! accepted source the corpus also records the shape of the tree, and for a
//! rejected one the range reported, as the parser gave them when the revision
//! was pinned.
//!
//! A parser upgrade that changes any of these fails here. The grammar
//! revision is then reviewed before the upgrade is accepted, because an
//! unchanged revision claims unchanged acceptance for every unit already
//! read under it.

mod support;

use intlify_authoring::{Location, UnitOutcome};
use intlify_authoring_js::{analyze_unit, Grammar, JsAnalysisWorkspace, GRAMMAR_REVISION};
use support::{admit, limits, never};

/// What the pinned revision does with one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pinned {
    /// Accepted, with the node, scope, symbol and reference counts of its tree.
    Accepted(u64, u64, u64, u64),
    /// Rejected, reporting this range, or no range at all.
    Rejected(Option<(u64, u64)>),
}

use Pinned::{Accepted, Rejected};

const CORPUS: &[(Grammar, &str, Pinned)] = &[
    // A module has imports, exports and top-level await.
    (
        Grammar::JsModule,
        "import { a } from 'b'\nexport const c = a\n",
        Accepted(11, 1, 2, 1),
    ),
    (Grammar::JsModule, "await load()\n", Accepted(5, 1, 0, 1)),
    (
        Grammar::JsModule,
        "const pattern = /(a)\\1/u\n",
        Accepted(5, 1, 1, 0),
    ),
    (
        Grammar::JsModule,
        "outer: for (;;) { break outer }\n",
        Accepted(7, 3, 0, 0),
    ),
    (
        Grammar::JsModule,
        "class A { #x = 1; static { this.y = 2 } }\n",
        Accepted(14, 3, 1, 0),
    ),
    // Decorators are not in ECMA-262 yet; the parser accepts them as a
    // proposal. Recorded so that a change in that choice is noticed.
    (
        Grammar::JsModule,
        "@decorate class A {}\n",
        Accepted(6, 2, 1, 1),
    ),
    // A byte order mark is whitespace to the language, and a hashbang is one
    // only at the very start of the text, so the two do not combine.
    (Grammar::JsModule, "\u{feff}f()\n", Accepted(4, 1, 0, 1)),
    (
        Grammar::JsModule,
        "#!/usr/bin/env node\nf()\n",
        Accepted(5, 1, 0, 1),
    ),
    (
        Grammar::JsModule,
        "\u{feff}#!/usr/bin/env node\nf()\n",
        Rejected(Some((4, 5))),
    ),
    // Module code is strict, and has no Annex B comment forms.
    (Grammar::JsModule, "<!-- comment\n", Rejected(Some((0, 4)))),
    (Grammar::JsModule, "f('\\101')\n", Rejected(Some((2, 8)))),
    (Grammar::JsModule, "with (o) {}\n", Rejected(Some((0, 4)))),
    (Grammar::JsModule, "delete x\n", Rejected(Some((7, 8)))),
    (Grammar::JsModule, "var await = 1\n", Rejected(Some((4, 9)))),
    (Grammar::JsModule, "return 1\n", Rejected(Some((0, 6)))),
    // Early errors the parser leaves to the semantic checks.
    (Grammar::JsModule, "let a; let a\n", Rejected(Some((4, 5)))),
    (
        Grammar::JsModule,
        "const r = /(/\n",
        Rejected(Some((11, 12))),
    ),
    (
        Grammar::JsModule,
        "const r = /a/gg\n",
        Rejected(Some((10, 15))),
    ),
    // Neither JSX nor TypeScript is JavaScript.
    (
        Grammar::JsModule,
        "const a = <div />\n",
        Rejected(Some((10, 11))),
    ),
    (
        Grammar::JsModule,
        "const a: number = 1\n",
        Rejected(Some((6, 7))),
    ),
    // A classic script is sloppy unless it says otherwise.
    (Grammar::JsScript, "var await = 1\n", Accepted(5, 1, 1, 0)),
    (
        Grammar::JsScript,
        "with (o) { f() }\n",
        Accepted(7, 3, 0, 2),
    ),
    (Grammar::JsScript, "f('\\101')\n", Accepted(5, 1, 0, 1)),
    (
        Grammar::JsScript,
        "<!-- comment\nf()\n",
        Accepted(4, 1, 0, 1),
    ),
    (
        Grammar::JsScript,
        "'use strict'; f('\\101')\n",
        Rejected(Some((16, 22))),
    ),
    // A script has no module syntax.
    (
        Grammar::JsScript,
        "import x from 'y'\n",
        Rejected(Some((0, 6))),
    ),
    (
        Grammar::JsScript,
        "export const a = 1\n",
        Rejected(Some((0, 6))),
    ),
    (
        Grammar::JsScript,
        "f(import.meta.url)\n",
        Rejected(Some((2, 13))),
    ),
    (Grammar::JsScript, "await f()\n", Rejected(Some((0, 5)))),
    (Grammar::JsScript, "return 1\n", Rejected(Some((0, 6)))),
    (
        Grammar::TsModule,
        "import type { A } from 'b'\nconst x: A = y as A\nenum E { A }\n",
        Accepted(21, 2, 4, 3),
    ),
    // Without JSX, an angle bracket before an expression is an assertion.
    (
        Grammar::TsModule,
        "const a = <number>b\n",
        Accepted(7, 1, 1, 1),
    ),
    (
        Grammar::TsModule,
        "abstract class A { abstract m(): void }\n",
        Accepted(10, 3, 1, 0),
    ),
    (Grammar::TsModule, "f('\\101')\n", Rejected(Some((2, 8)))),
    (
        Grammar::TsModule,
        "const a = <div />\n",
        Rejected(Some((15, 16))),
    ),
    (Grammar::TsModule, "let a; let a\n", Rejected(Some((4, 5)))),
    (
        Grammar::TsScript,
        "namespace N { export const x = 1 }\n",
        Accepted(9, 2, 2, 0),
    ),
    (
        Grammar::TsScript,
        "declare module 'm' { export const y: number }\n",
        Accepted(10, 2, 1, 0),
    ),
    (
        Grammar::TsScript,
        "namespace N { export const y = 1 }\nimport x = N.y\n",
        Accepted(14, 2, 3, 1),
    ),
    (Grammar::TsScript, "f('\\101')\n", Accepted(5, 1, 0, 1)),
    // The parser accepts these under TypeScript; this Producer does not,
    // because a unit declared a script is not a module.
    (
        Grammar::TsScript,
        "import x from 'y'\n",
        Rejected(Some((0, 17))),
    ),
    (Grammar::TsScript, "export {}\n", Rejected(Some((0, 9)))),
    (
        Grammar::TsScript,
        "import x = require('y')\n",
        Rejected(Some((0, 23))),
    ),
    (
        Grammar::TsScript,
        "f(import.meta.url)\n",
        Rejected(Some((2, 13))),
    ),
];

fn observe(grammar: Grammar, source: &str) -> Pinned {
    let units = admit(&[("corpus", grammar, source.as_bytes())]);
    let analysis = analyze_unit(
        &units[0],
        &limits(),
        &mut JsAnalysisWorkspace::new(),
        &never,
    )
    .expect("analysis runs");
    match analysis.outcome() {
        UnitOutcome::Checked => {
            let work = analysis.work();
            Accepted(work.ast_nodes, work.scopes, work.symbols, work.references)
        }
        UnitOutcome::Failed => Rejected(match analysis.diagnostics()[0].location() {
            Location::Region(region) => Some((region.range().start(), region.range().end())),
            _ => None,
        }),
        UnitOutcome::Blocked => panic!("{source:?}: reading a unit never blocks it"),
    }
}

#[test]
fn every_grammar_accepts_and_rejects_what_its_revision_was_pinned_with() {
    let mismatches: Vec<String> = CORPUS
        .iter()
        .filter_map(|&(grammar, source, pinned)| {
            let observed = observe(grammar, source);
            (observed != pinned).then(|| {
                format!("{grammar:?} {source:?}: pinned {pinned:?}, observed {observed:?}")
            })
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "the grammar pinned as revision {GRAMMAR_REVISION} changed; review the revision \
         before accepting a parser upgrade, because an unchanged revision claims unchanged \
         acceptance for every unit already read under it:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn every_grammar_is_pinned_by_both_an_acceptance_and_a_rejection() {
    for grammar in Grammar::ALL {
        let verdicts: Vec<Pinned> = CORPUS
            .iter()
            .filter(|(entry, _, _)| *entry == grammar)
            .map(|(_, _, pinned)| *pinned)
            .collect();
        assert!(verdicts.iter().any(|pinned| matches!(pinned, Accepted(..))));
        assert!(verdicts.iter().any(|pinned| matches!(pinned, Rejected(_))));
    }
}
