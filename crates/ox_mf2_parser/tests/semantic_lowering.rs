//! Semantic lowering tests.
//!
//! Covers the Milestone 8 acceptance criteria in
//! `.plans/002-ox-mf2-phase-1-rust-parser-implementation.md`:
//!
//! - `parse_semantic = false` does not grow semantic tables.
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
    parse_source(&sources, id, options)
}

#[test]
fn parse_semantic_false_omits_model() {
    let result = parse("Hello", false);
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
    assert_eq!(semantic.references[0].name_span.start, semantic.references[0].semantic_ref.span.start);
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
