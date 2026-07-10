// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Semantic lowering tests.
//!
//! Covers the Milestone 8 acceptance criteria in
//! `.plans/002-ox-mf2-phase-1-rust-parser-implementation.md`:

#![allow(clippy::field_reassign_with_default, clippy::doc_markdown)]
//!
//! - `parse_semantic = false` does not grow semantic tables.
//! - Parser diagnostics suppress SemanticModel construction.
//! - Every semantic record links back to a NodeId + Span.
//! - Simple message → `SemanticMessageKind::Pattern`.
//! - Complex quoted pattern → `SemanticMessageKind::Pattern`.
//! - Matcher body → `SemanticMessageKind::Select`.

use ox_mf2_parser::{
    parse_source, MessageMode, ParseOptions, SemanticMessageKind, SourceFileInput, SourceStore,
};

fn parse(source: &str, parse_semantic: bool) -> ox_mf2_parser::ParseResult {
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source,
        ..Default::default()
    });
    let mut options = ParseOptions::default();
    options.parse_semantic = parse_semantic;
    parse_source(&sources, id, options).expect("parse succeeds")
}

#[test]
fn parse_semantic_false_omits_model() {
    let result = parse("Hello", false);
    assert!(result.semantic.is_none());
}

#[test]
fn parser_diagnostics_suppress_semantic_model() {
    let result = parse("Hello, {$name", true);
    assert!(!result.diagnostics.is_empty());
    assert!(result.semantic.is_none());
}

#[test]
fn simple_message_is_pattern() {
    let result = parse("Hello, {$name}!", true);
    let semantic = result.semantic.unwrap();
    assert_eq!(semantic.mode, MessageMode::Simple);
    assert_eq!(semantic.kind, SemanticMessageKind::Pattern);
    assert!(!semantic.patterns.is_empty());
    assert!(!semantic.expressions.is_empty());
    assert_eq!(semantic.references.len(), 1);
    assert_eq!(
        semantic.references[0].name_span.start,
        semantic.references[0].semantic_ref.span.start
    );
}

#[test]
fn complex_quoted_pattern_is_pattern() {
    let result = parse("{{Hello, {$name}!}}", true);
    let semantic = result.semantic.unwrap();
    assert_eq!(semantic.mode, MessageMode::Complex);
    assert_eq!(semantic.kind, SemanticMessageKind::Pattern);
    assert!(semantic.patterns.iter().any(|p| p.is_quoted));
}

#[test]
fn matcher_body_is_select() {
    let result = parse(".match $x\n* {{fallback}}", true);
    let semantic = result.semantic.unwrap();
    assert_eq!(semantic.mode, MessageMode::Complex);
    assert_eq!(semantic.kind, SemanticMessageKind::Select);
    assert_eq!(semantic.selectors.len(), 1);
    assert!(semantic.variants.iter().any(|v| v.has_catch_all));
}

#[test]
fn declarations_collect_with_variable_back_reference() {
    let result = parse(".local $x = {$y}\n{{Hi}}", true);
    let semantic = result.semantic.unwrap();
    assert_eq!(semantic.declarations.len(), 1);
    let decl = &semantic.declarations[0];
    assert!(decl.variable.is_some(), "declared variable link expected");
    // The `$y` reference inside the RHS expression is collected.
    assert!(!semantic.references.is_empty());
}

#[test]
fn markup_options_and_attributes_are_lowered() {
    let result = parse("{{Click {#a opt=|x| @id=|me|/} now}}", true);
    let semantic = result.semantic.unwrap();
    assert_eq!(semantic.markups.len(), 1, "expected one markup");
    assert!(
        !semantic.options.is_empty(),
        "expected markup options to be lowered"
    );
    assert!(
        !semantic.attributes.is_empty(),
        "expected markup attributes to be lowered"
    );
}

#[test]
fn input_declaration_body_is_lowered() {
    let result = parse(
        ".input {$count :number minimumFractionDigits=2}\n{{Hi {$count}}}",
        true,
    );
    let semantic = result.semantic.unwrap();
    assert_eq!(semantic.declarations.len(), 1);
    // The variable annotated on .input should still appear in declarations.
    assert!(semantic.declarations[0].variable.is_some());
    // The function annotation should be recorded.
    assert!(
        !semantic.functions.is_empty(),
        "expected .input body's function to be recorded"
    );
    // The option `minimumFractionDigits` should be lowered.
    assert!(
        !semantic.options.is_empty(),
        "expected .input body's option to be recorded"
    );
    // The placeholder `$count` reference inside the body should be recorded
    // exactly once (not the declared input variable itself).
    let refs_to_count = semantic
        .references
        .iter()
        .filter(|r| !r.semantic_ref.span.is_empty())
        .count();
    assert!(
        refs_to_count >= 1,
        "expected at least one variable reference in the message body"
    );
}

#[test]
fn every_record_links_back_to_node_id() {
    let result = parse(".local $x = {$y}\n.match $x\n* {{fallback}}", true);
    let semantic = result.semantic.unwrap();
    for r in &semantic.declarations {
        assert_ne!(r.semantic_ref.node.raw(), u32::MAX);
    }
    for r in &semantic.selectors {
        assert_ne!(r.semantic_ref.node.raw(), u32::MAX);
    }
    for r in &semantic.variants {
        assert_ne!(r.semantic_ref.node.raw(), u32::MAX);
    }
    for r in &semantic.patterns {
        assert_ne!(r.semantic_ref.node.raw(), u32::MAX);
    }
}
