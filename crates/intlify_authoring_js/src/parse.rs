// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reading one unit under its grammar, exactly once.
//!
//! A unit is accepted only when the parser reports nothing, the semantic
//! checks that carry the language's early errors report nothing, and a script
//! contains no module syntax. Anything else rejects the whole tree. A tree the
//! parser recovered is not read for facts, because what recovery kept is a
//! guess about a program the author did not write.
//!
//! The parser's messages and recovery state stay on this side of the
//! boundary. What crosses it is the earliest range the parser named that
//! falls on scalar boundaries inside the unit, checked here rather than
//! trusted.

use intlify_authoring::{ByteRange, Token};
use oxc_allocator::Allocator;
use oxc_ast::ast::{Program, Statement, TSModuleReference};
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::{ParseOptions, Parser};
use oxc_semantic::{Semantic, SemanticBuilder, Stats};
use oxc_span::GetSpan;

use crate::failure::ProducerFailure;
use crate::grammar::Grammar;
use crate::limits::{JsAuthoringLimits, JsLimitKind};

/// What reading one unit established.
#[derive(Debug)]
pub(crate) enum Reading<'a> {
    /// The unit is a valid program under its grammar.
    Accepted(Box<Parsed<'a>>),
    /// The host rejected the unit.
    ///
    /// The range is the earliest one the host named that could be checked, or
    /// `None` when it named none.
    Rejected(Option<ByteRange>),
}

/// An accepted unit's tree and what the semantic pass built over it.
///
/// Both live in the workspace's arena. Nothing borrowed from them may outlive
/// the analysis of this unit.
pub(crate) struct Parsed<'a> {
    pub(crate) program: &'a Program<'a>,
    pub(crate) semantic: Semantic<'a>,
    pub(crate) stats: Stats,
}

impl std::fmt::Debug for Parsed<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Parsed")
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

/// Parse one unit and check it.
pub(crate) fn read<'a, C>(
    arena: &'a Allocator,
    text: &'a str,
    grammar: Grammar,
    limits: &JsAuthoringLimits,
    unit: &Token,
    cancelled: &C,
) -> Result<Reading<'a>, ProducerFailure>
where
    C: Fn() -> bool + ?Sized,
{
    if cancelled() {
        return Err(ProducerFailure::Cancelled);
    }
    let parsed = Parser::new(arena, text, grammar.source_type())
        .with_options(ParseOptions {
            // An invalid pattern is an early error in the language; without
            // this the parser accepts a unit every engine refuses to load.
            parse_regular_expression: true,
            ..ParseOptions::default()
        })
        .parse();
    if cancelled() {
        return Err(ProducerFailure::Cancelled);
    }
    match parse_verdict(text, parsed.panicked, &parsed.diagnostics) {
        ParseVerdict::Clean => {}
        ParseVerdict::Rejected(range) => return Ok(Reading::Rejected(range)),
        ParseVerdict::Invariant => {
            return Err(ProducerFailure::ParserInvariant { unit: unit.clone() })
        }
    }

    let program = arena.alloc(parsed.program);
    // Counting first bounds the semantic pass by the caller's limit, and the
    // count is handed to the builder so the tree is not walked twice for it.
    let stats = Stats::count(program);
    if u64::from(stats.nodes) > limits.ast_nodes {
        return Err(ProducerFailure::Limit(JsLimitKind::AstNodes));
    }
    let semantic = SemanticBuilder::new()
        .with_check_syntax_error(true)
        .with_stats(stats)
        .build(program);
    if cancelled() {
        return Err(ProducerFailure::Cancelled);
    }

    let module_syntax = if grammar.is_script() {
        module_syntax(text, program)
    } else {
        None
    };
    if !semantic.diagnostics.is_empty() || module_syntax.is_some() {
        let reported = earliest(text, &semantic.diagnostics);
        return Ok(Reading::Rejected(match (reported, module_syntax) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (left, right) => left.or(right),
        }));
    }
    Ok(Reading::Accepted(Box::new(Parsed {
        program,
        semantic: semantic.semantic,
        stats,
    })))
}

/// What a finished parse says about the unit, before any tree is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseVerdict {
    /// The parser reported nothing, so the tree can be checked further.
    Clean,
    /// The parser rejected the unit, at the earliest range it named that
    /// could be checked.
    Rejected(Option<ByteRange>),
    /// The parser stopped without saying why.
    Invariant,
}

/// Decide what a finished parse says about the unit.
///
/// A parser that gives up has to name what it rejected. One that stops
/// without a diagnostic has failed itself, so the unit is not reported as the
/// author's mistake.
fn parse_verdict(text: &str, panicked: bool, diagnostics: &[OxcDiagnostic]) -> ParseVerdict {
    if !diagnostics.is_empty() {
        ParseVerdict::Rejected(earliest(text, diagnostics))
    } else if panicked {
        ParseVerdict::Invariant
    } else {
        ParseVerdict::Clean
    }
}

/// Return the earliest range any diagnostic names that can be checked.
fn earliest(text: &str, diagnostics: &[OxcDiagnostic]) -> Option<ByteRange> {
    diagnostics
        .iter()
        .flat_map(|diagnostic| diagnostic.labels.iter())
        .filter_map(|label| {
            let start = usize::try_from(label.offset()).ok()?;
            let end = start.checked_add(usize::try_from(label.len()).ok()?)?;
            checked(text, start, end)
        })
        .min()
}

/// Return the range when it lies inside the text on scalar boundaries.
fn checked(text: &str, start: usize, end: usize) -> Option<ByteRange> {
    if start > end || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return None;
    }
    ByteRange::new(start as u64, end as u64).ok()
}

/// Return the first top-level statement only a module may contain.
///
/// The parser rejects these in a JavaScript script but not in a TypeScript
/// one, because it cannot yet tell which a TypeScript file is. A unit whose
/// snapshot says script is a script, so the rule is applied here to both
/// grammars, and a TypeScript script carrying an import is read as the grammar
/// mismatch it is instead of as a module.
fn module_syntax(text: &str, program: &Program<'_>) -> Option<ByteRange> {
    program.body.iter().find_map(|statement| {
        let module = statement.is_module_declaration()
            || matches!(
                statement,
                Statement::TSImportEqualsDeclaration(declaration)
                    if matches!(
                        declaration.module_reference,
                        TSModuleReference::ExternalModuleReference(_)
                    )
            );
        if !module {
            return None;
        }
        let span = statement.span();
        checked(text, span.start as usize, span.end as usize)
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use oxc_span::Span;

    use super::*;
    use crate::limits::tests::generous;

    fn range(start: u64, end: u64) -> ByteRange {
        ByteRange::new(start, end).unwrap()
    }

    fn unit() -> Token {
        Token::new("checkout").unwrap()
    }

    /// What `read` established, without the tree it borrowed.
    #[derive(Debug)]
    enum Summary {
        Accepted(Stats),
        Rejected(Option<ByteRange>),
    }

    fn read_with<C>(
        grammar: Grammar,
        text: &str,
        limits: &JsAuthoringLimits,
        cancelled: &C,
    ) -> Result<Summary, ProducerFailure>
    where
        C: Fn() -> bool + ?Sized,
    {
        let allocator = Allocator::default();
        read(&allocator, text, grammar, limits, &unit(), cancelled).map(|reading| match reading {
            Reading::Accepted(parsed) => Summary::Accepted(parsed.stats),
            Reading::Rejected(range) => Summary::Rejected(range),
        })
    }

    fn read_as(grammar: Grammar, text: &str) -> Result<Summary, ProducerFailure> {
        read_with(grammar, text, &generous(), &|| false)
    }

    fn accepted(grammar: Grammar, text: &str) -> Stats {
        match read_as(grammar, text) {
            Ok(Summary::Accepted(stats)) => stats,
            other => panic!("{grammar:?} {text:?} is accepted, not {other:?}"),
        }
    }

    fn rejected(grammar: Grammar, text: &str) -> Option<ByteRange> {
        match read_as(grammar, text) {
            Ok(Summary::Rejected(range)) => range,
            other => panic!("{grammar:?} {text:?} is rejected, not {other:?}"),
        }
    }

    /// One diagnostic naming each of `spans`.
    fn diagnostic(spans: &[(u32, u32)]) -> OxcDiagnostic {
        OxcDiagnostic::error("rejected")
            .with_labels(spans.iter().map(|&(start, end)| Span::new(start, end)))
    }

    /// The node count of `text`, taken the way `read` takes it.
    fn nodes(grammar: Grammar, text: &str) -> u64 {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, text, grammar.source_type()).parse();
        assert!(parsed.diagnostics.is_empty());
        u64::from(Stats::count(&parsed.program).nodes)
    }

    #[test]
    fn a_checked_range_lies_inside_the_text_on_scalar_boundaries() {
        // `a` is byte 0, `あ` bytes 1 to 4, `b` byte 4.
        let text = "aあb";
        assert_eq!(checked(text, 1, 4), Some(range(1, 4)));
        assert_eq!(checked(text, 0, 5), Some(range(0, 5)));
        assert_eq!(
            checked(text, 5, 5),
            Some(range(5, 5)),
            "the end of the text is a position"
        );
        assert_eq!(checked(text, 2, 4), None, "a start inside a scalar");
        assert_eq!(checked(text, 1, 3), None, "an end inside a scalar");
        assert_eq!(checked(text, 4, 6), None, "past the end of the text");
        assert_eq!(checked(text, 4, 1), None, "reversed");
    }

    #[test]
    fn the_earliest_checkable_label_of_any_diagnostic_is_reported() {
        let text = "aあb";
        assert_eq!(
            earliest(text, &[diagnostic(&[(4, 5)]), diagnostic(&[(0, 1)])]),
            Some(range(0, 1)),
            "across diagnostics, not only within the first"
        );
        assert_eq!(
            earliest(text, &[diagnostic(&[(1, 4), (1, 1)])]),
            Some(range(1, 1)),
            "ranges starting together are ordered by their end"
        );
        assert_eq!(
            earliest(text, &[diagnostic(&[(2, 3), (4, 5)])]),
            Some(range(4, 5)),
            "a label inside a scalar is skipped, not rounded"
        );
        assert_eq!(earliest(text, &[diagnostic(&[(2, 3), (5, 9)])]), None);
        assert_eq!(
            earliest(text, &[OxcDiagnostic::error("rejected")]),
            None,
            "a diagnostic may name no range at all"
        );
    }

    #[test]
    fn a_parse_that_stops_without_a_diagnostic_is_the_parsers_own_failure() {
        let text = "f()";
        assert_eq!(parse_verdict(text, false, &[]), ParseVerdict::Clean);
        assert_eq!(parse_verdict(text, true, &[]), ParseVerdict::Invariant);
        // A parser that gives up and says why has rejected the unit.
        assert_eq!(
            parse_verdict(text, true, &[diagnostic(&[(0, 1)])]),
            ParseVerdict::Rejected(Some(range(0, 1)))
        );
        // A recovered tree is rejected just the same as an abandoned one.
        assert_eq!(
            parse_verdict(text, false, &[diagnostic(&[(1, 2)])]),
            ParseVerdict::Rejected(Some(range(1, 2)))
        );
        assert_eq!(
            parse_verdict(text, false, &[OxcDiagnostic::error("rejected")]),
            ParseVerdict::Rejected(None)
        );
    }

    #[test]
    fn an_accepted_unit_reports_the_counts_its_tree_was_checked_with() {
        // The program, the declaration, its declarator, the binding and the
        // number: five nodes, one scope, one symbol, no reference.
        let stats = accepted(Grammar::JsModule, "const a = 1\n");
        assert_eq!(
            (stats.nodes, stats.scopes, stats.symbols, stats.references),
            (5, 1, 1, 0)
        );
    }

    #[test]
    fn each_stage_that_rejects_a_unit_names_where() {
        // The parser rejects a comment form a module does not have.
        assert_eq!(
            rejected(Grammar::JsModule, "<!-- comment\n"),
            Some(range(0, 4))
        );
        // The parser accepts this; the semantic checks report the second
        // declaration of the same name.
        assert_eq!(
            rejected(Grammar::JsModule, "let a; let a\n"),
            Some(range(4, 5))
        );
    }

    #[test]
    fn regular_expression_patterns_are_checked() {
        assert_eq!(
            rejected(Grammar::JsModule, "const r = /(/\n"),
            Some(range(11, 12))
        );
        accepted(Grammar::JsModule, "const r = /(a)/u\n");
    }

    #[test]
    fn the_goal_comes_from_the_grammar() {
        // Top-level await exists only under the module goal, whatever the
        // parser would guess from a TypeScript source.
        accepted(Grammar::TsModule, "await f()\n");
        assert_eq!(
            rejected(Grammar::TsScript, "await f()\n"),
            Some(range(0, 5))
        );
    }

    #[test]
    fn a_script_is_checked_for_module_syntax_and_a_module_is_not() {
        let import = "import x from 'y'\n";
        accepted(Grammar::TsModule, import);
        // The parser leaves TypeScript alone, so the statement itself is the
        // range reported.
        assert_eq!(rejected(Grammar::TsScript, import), Some(range(0, 17)));
        // For JavaScript the semantic checks also report it, at the keyword,
        // and the earlier of the two ranges is the one kept.
        assert_eq!(rejected(Grammar::JsScript, import), Some(range(0, 6)));
    }

    /// The module syntax `text` carries, read as a TypeScript script.
    fn module_syntax_of(text: &str) -> Option<ByteRange> {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, text, Grammar::TsScript.source_type()).parse();
        assert!(parsed.diagnostics.is_empty(), "{text:?} parses");
        module_syntax(text, &parsed.program)
    }

    #[test]
    fn module_syntax_is_any_top_level_import_or_export() {
        for (text, start, end) in [
            ("import x from 'y'\nconst a = 1\n", 0, 17),
            ("import type { X } from 'y'\n", 0, 26),
            ("const a = 1\nexport {}\n", 12, 21),
            ("export default 1\n", 0, 16),
            ("export * from 'y'\n", 0, 17),
            ("export = x\n", 0, 10),
            ("import x = require('y')\n", 0, 23),
            // The first one in source order is reported.
            ("const a = 1\nimport x from 'y'\nexport {}\n", 12, 29),
        ] {
            assert_eq!(module_syntax_of(text), Some(range(start, end)), "{text:?}");
        }
    }

    #[test]
    fn syntax_that_only_resembles_module_syntax_is_not_it() {
        for text in [
            // An alias of a namespace member is not an import of a module.
            "namespace N { export const y = 1 }\nimport x = N.y\n",
            // An ambient module's body may export; the file is still a script.
            "declare module 'm' { export const y: number }\n",
            // A dynamic import is an expression a script may contain.
            "function f() { return import('y') }\n",
        ] {
            assert_eq!(module_syntax_of(text), None, "{text:?}");
        }
    }

    #[test]
    fn the_node_limit_applies_before_the_semantic_checks() {
        let text = "let a; let a\n";
        let count = nodes(Grammar::JsModule, text);
        let mut limits = generous();
        limits.ast_nodes = count;
        assert!(matches!(
            read_with(Grammar::JsModule, text, &limits, &|| false),
            Ok(Summary::Rejected(Some(found))) if found == range(4, 5)
        ));
        // One node over the bound stops the unit before the semantic checks
        // could reject it, so the limit is what is reported.
        limits.ast_nodes = count - 1;
        assert_eq!(
            read_with(Grammar::JsModule, text, &limits, &|| false).err(),
            Some(ProducerFailure::Limit(JsLimitKind::AstNodes))
        );
    }

    /// How many times the probe was asked while reading `text`.
    fn probes(text: &str) -> u32 {
        let calls = Cell::new(0);
        let probe = || {
            calls.set(calls.get() + 1);
            false
        };
        read_with(Grammar::JsModule, text, &generous(), &probe).expect("the reading runs");
        calls.get()
    }

    #[test]
    fn the_probe_is_asked_around_parsing_and_after_the_semantic_checks() {
        assert_eq!(probes("const a = 1\n"), 3);
        assert_eq!(
            probes("let a; let a\n"),
            3,
            "rejected by the semantic checks"
        );
        assert_eq!(
            probes("const = ;\n"),
            2,
            "rejected by the parser, before any semantic check"
        );

        for stop_at in 1..=3 {
            let calls = Cell::new(0);
            let probe = || {
                calls.set(calls.get() + 1);
                calls.get() >= stop_at
            };
            assert_eq!(
                read_with(Grammar::JsModule, "const a = 1\n", &generous(), &probe).err(),
                Some(ProducerFailure::Cancelled),
                "stopping at probe {stop_at}"
            );
            assert_eq!(calls.get(), stop_at, "nothing runs after the stop");
        }
    }
}
