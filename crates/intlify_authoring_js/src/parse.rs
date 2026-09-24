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
use oxc_parser::{ParseOptions, Parser};
use oxc_semantic::{SemanticBuilder, Stats};
use oxc_span::GetSpan;

use crate::failure::ProducerFailure;
use crate::grammar::Grammar;
use crate::limits::{JsAuthoringLimits, JsLimitKind};

/// What reading one unit established.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Reading {
    /// The unit is a valid program under its grammar.
    Accepted(Stats),
    /// The host rejected the unit.
    ///
    /// The range is the earliest one the host named that could be checked, or
    /// `None` when it named none.
    Rejected(Option<ByteRange>),
}

/// Parse one unit and check it, releasing the tree before returning.
pub(crate) fn read<C>(
    arena: &Allocator,
    text: &str,
    grammar: Grammar,
    limits: &JsAuthoringLimits,
    unit: &Token,
    cancelled: &C,
) -> Result<Reading, ProducerFailure>
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
    if parsed.panicked && parsed.diagnostics.is_empty() {
        return Err(ProducerFailure::ParserInvariant { unit: unit.clone() });
    }
    if !parsed.diagnostics.is_empty() {
        return Ok(Reading::Rejected(earliest(text, &parsed.diagnostics)));
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
    Ok(Reading::Accepted(stats))
}

/// Return the earliest range any diagnostic names that can be checked.
fn earliest(text: &str, diagnostics: &[oxc_diagnostics::OxcDiagnostic]) -> Option<ByteRange> {
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
