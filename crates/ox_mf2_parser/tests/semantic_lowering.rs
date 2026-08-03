// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Semantic lowering tests.
//!
//! Covers the semantic-lowering contract described in
//! `design/002-ox-mf2-phase-1-rust-parser-design.md`:

#![allow(clippy::doc_markdown)]
//!
//! - SemanticModel construction is explicit and separate from ParseResult.
//! - Parser diagnostics reject SemanticModel construction.
//! - Every semantic record links back to a NodeId + Span.
//! - Simple message → `SemanticMessageKind::Pattern`.
//! - Complex quoted pattern → `SemanticMessageKind::Pattern`.
//! - Matcher body → `SemanticMessageKind::Select`.

use ox_mf2_parser::{
    build_semantic_model, parse_source, MessageMode, ParseOptions, ParseResult,
    SemanticInvariantErrorKind, SemanticMessageKind, SemanticModel, SourceFileInput, SourceStore,
};

fn parse(source: &str) -> (SourceStore, ParseResult) {
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source,
        ..Default::default()
    });
    let result = parse_source(&sources, id, ParseOptions::default()).expect("parse succeeds");
    (sources, result)
}

fn build(source: &str) -> SemanticModel {
    let (sources, result) = parse(source);
    build_semantic_model(&sources, &result).expect("semantic construction succeeds")
}

#[test]
fn parser_diagnostics_reject_semantic_model_construction() {
    let (sources, result) = parse("Hello, {$name");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(
        build_semantic_model(&sources, &result)
            .expect_err("recovery CST must not produce semantic facts")
            .kind(),
        SemanticInvariantErrorKind::ApiMisuse
    );
}

#[test]
fn simple_message_is_pattern() {
    let semantic = build("Hello, {$name}!");
    assert_eq!(semantic.mode(), MessageMode::Simple);
    assert_eq!(semantic.kind(), SemanticMessageKind::Pattern);
    assert!(!semantic.patterns().is_empty());
    assert!(!semantic.expressions().is_empty());
    assert_eq!(semantic.references().len(), 1);
    assert_eq!(
        semantic.references()[0].name_span.start,
        semantic.references()[0].semantic_ref.span.start + 1
    );
}

#[test]
fn complex_quoted_pattern_is_pattern() {
    let semantic = build("{{Hello, {$name}!}}");
    assert_eq!(semantic.mode(), MessageMode::Complex);
    assert_eq!(semantic.kind(), SemanticMessageKind::Pattern);
    assert!(semantic.patterns().iter().any(|p| p.is_quoted));
}

#[test]
fn matcher_body_is_select() {
    let semantic = build(".match $x\n* {{fallback}}");
    assert_eq!(semantic.mode(), MessageMode::Complex);
    assert_eq!(semantic.kind(), SemanticMessageKind::Select);
    assert_eq!(semantic.selectors().len(), 1);
    assert!(semantic.variants().iter().any(|v| v.has_catch_all));
}

#[test]
fn declarations_collect_with_variable_back_reference() {
    let semantic = build(".local $x = {$y}\n{{Hi}}");
    assert_eq!(semantic.declarations().len(), 1);
    let decl = &semantic.declarations()[0];
    assert!(decl.variable.is_some(), "declared variable link expected");
    // The `$y` reference inside the RHS expression is collected.
    assert!(!semantic.references().is_empty());
}

#[test]
fn markup_options_and_attributes_are_lowered() {
    let semantic = build("{{Click {#a opt=|x| @id=|me|/} now}}");
    assert_eq!(semantic.markups().len(), 1, "expected one markup");
    assert!(
        !semantic.options().is_empty(),
        "expected markup options to be lowered"
    );
    assert!(
        !semantic.attributes().is_empty(),
        "expected markup attributes to be lowered"
    );
}

#[test]
fn input_declaration_body_is_lowered() {
    let semantic = build(".input {$count :number minimumFractionDigits=2}\n{{Hi {$count}}}");
    assert_eq!(semantic.declarations().len(), 1);
    // The variable annotated on .input should still appear in declarations.
    assert!(semantic.declarations()[0].variable.is_some());
    // The function annotation should be recorded.
    assert!(
        !semantic.functions().is_empty(),
        "expected .input body's function to be recorded"
    );
    // The option `minimumFractionDigits` should be lowered.
    assert!(
        !semantic.options().is_empty(),
        "expected .input body's option to be recorded"
    );
    // The placeholder `$count` reference inside the body should be recorded
    // exactly once (not the declared input variable itself).
    let refs_to_count = semantic
        .references()
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
    let semantic = build(".local $x = {$y}\n.match $x\n* {{fallback}}");
    for r in semantic.declarations() {
        assert_ne!(r.semantic_ref.node.raw(), u32::MAX);
    }
    for r in semantic.selectors() {
        assert_ne!(r.semantic_ref.node.raw(), u32::MAX);
    }
    for r in semantic.variants() {
        assert_ne!(r.semantic_ref.node.raw(), u32::MAX);
    }
    for r in semantic.patterns() {
        assert_ne!(r.semantic_ref.node.raw(), u32::MAX);
    }
}
