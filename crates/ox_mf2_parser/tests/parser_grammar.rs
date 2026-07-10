// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Coarse-grained grammar tests for the Phase 1 parser.
//!
//! Each test exercises a vertical slice from `design/002` and the WG spec
//! grammar in `refers/message-format-wg/spec/message.abnf`. Tests inspect
//! root kind, child counts, and a flat traversal of `SyntaxKind` so a
//! regression in the parser shape shows up here before fixture snapshots
//! land in Milestone 10.

use ox_mf2_parser::{
    parse_source, CstChild, CstNodeView, CstView, DiagnosticCode, ParseOptions, SourceFileInput,
    SourceStore, SyntaxKind,
};

fn parse(source: &str) -> (SourceStore, ox_mf2_parser::ParseResult) {
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source,
        ..Default::default()
    });
    let result = parse_source(&sources, id, ParseOptions::default()).expect("parse succeeds");
    (sources, result)
}

fn collect_kinds(root: CstNodeView<'_>) -> Vec<SyntaxKind> {
    fn visit(node: CstNodeView<'_>, out: &mut Vec<SyntaxKind>) {
        out.push(node.kind());
        for child in node.children() {
            match child {
                CstChild::Node(n) => visit(n, out),
                CstChild::Token(t) => out.push(t.kind()),
            }
        }
    }
    let mut out = Vec::new();
    visit(root, &mut out);
    out
}

#[test]
fn empty_simple_message_is_valid() {
    let (sources, result) = parse("");
    assert!(result.diagnostics.is_empty());
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    assert_eq!(root.kind(), SyntaxKind::Root);
    let kinds = collect_kinds(root);
    assert_eq!(
        &kinds[..3],
        &[
            SyntaxKind::Root,
            SyntaxKind::SimpleMessage,
            SyntaxKind::Pattern
        ]
    );
}

#[test]
fn simple_message_with_text_and_variable_placeholder() {
    let (sources, result) = parse("Hello, {$name}!");
    assert!(
        result.diagnostics.is_empty(),
        "diagnostics: {:?}",
        result.diagnostics
    );
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    let kinds = collect_kinds(root);
    assert!(kinds.contains(&SyntaxKind::Placeholder));
    assert!(kinds.contains(&SyntaxKind::VariableExpression));
    assert!(kinds.contains(&SyntaxKind::Variable));
    assert!(kinds.contains(&SyntaxKind::DollarToken));
    assert!(kinds.contains(&SyntaxKind::LeftBraceToken));
    assert!(kinds.contains(&SyntaxKind::RightBraceToken));
    assert!(kinds.contains(&SyntaxKind::TextToken));
    assert!(kinds.contains(&SyntaxKind::NameToken));
}

#[test]
fn unclosed_placeholder_emits_diagnostic_and_keeps_root() {
    let (sources, result) = parse("Hello, {$name");
    assert!(!result.diagnostics.is_empty());
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&DiagnosticCode::UnclosedExpression));
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    assert_eq!(root.kind(), SyntaxKind::Root);
}

#[test]
fn function_expression_parses() {
    let (sources, result) = parse("{:datetime}");
    assert!(
        result.diagnostics.is_empty(),
        "diagnostics: {:?}",
        result.diagnostics
    );
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    let kinds = collect_kinds(root);
    assert!(kinds.contains(&SyntaxKind::FunctionExpression));
    assert!(kinds.contains(&SyntaxKind::Function));
    assert!(kinds.contains(&SyntaxKind::ColonToken));
}

#[test]
fn quoted_literal_expression_parses() {
    let (sources, result) = parse("{|hello world|}");
    assert!(
        result.diagnostics.is_empty(),
        "diagnostics: {:?}",
        result.diagnostics
    );
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    let kinds = collect_kinds(root);
    assert!(kinds.contains(&SyntaxKind::LiteralExpression));
    assert!(kinds.contains(&SyntaxKind::QuotedLiteral));
    assert!(kinds.contains(&SyntaxKind::PipeToken));
    assert!(kinds.contains(&SyntaxKind::QuotedTextToken));
}

#[test]
fn quoted_pattern_without_declarations_is_complex() {
    let (sources, result) = parse("{{Hello}}");
    assert!(
        result.diagnostics.is_empty(),
        "diagnostics: {:?}",
        result.diagnostics
    );
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    let kinds = collect_kinds(root);
    assert!(kinds.contains(&SyntaxKind::ComplexMessage));
    assert!(kinds.contains(&SyntaxKind::QuotedPattern));
    assert!(kinds.contains(&SyntaxKind::LeftDoubleBraceToken));
    assert!(kinds.contains(&SyntaxKind::RightDoubleBraceToken));
}

#[test]
fn input_declaration_followed_by_quoted_pattern() {
    let (sources, result) = parse(".input {$x}\n{{Hello {$x}}}");
    assert!(
        result.diagnostics.is_empty(),
        "diagnostics: {:?}",
        result.diagnostics
    );
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    let kinds = collect_kinds(root);
    assert!(kinds.contains(&SyntaxKind::ComplexMessage));
    assert!(kinds.contains(&SyntaxKind::InputDeclaration));
    assert!(kinds.contains(&SyntaxKind::QuotedPattern));
}

#[test]
fn matcher_accepts_numeric_exact_keys() {
    let (sources, result) = parse(".match $count\n0 {{none}}\n1 {{one}}\n* {{other}}");
    assert!(
        result.diagnostics.is_empty(),
        "diagnostics: {:?}",
        result.diagnostics
    );
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    let kinds = collect_kinds(root);
    assert!(kinds.contains(&SyntaxKind::Matcher));
    // Three variants (0, 1, *).
    let variant_count = kinds.iter().filter(|k| **k == SyntaxKind::Variant).count();
    assert_eq!(variant_count, 3);
    let variant_key_count = kinds
        .iter()
        .filter(|k| **k == SyntaxKind::VariantKey)
        .count();
    assert_eq!(variant_key_count, 2, "expected two numeric VariantKeys");
}

#[test]
fn matcher_accepts_fractional_exact_key() {
    let (_, result) = parse(".match $price\n1.5 {{half}}\n* {{other}}");
    assert!(
        result.diagnostics.is_empty(),
        "diagnostics: {:?}",
        result.diagnostics
    );
}

#[test]
fn local_declaration_followed_by_matcher() {
    let (sources, result) = parse(".local $x = {$y}\n.match $x\n* {{fallback}}");
    assert!(
        result.diagnostics.is_empty(),
        "diagnostics: {:?}",
        result.diagnostics
    );
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    let kinds = collect_kinds(root);
    assert!(kinds.contains(&SyntaxKind::LocalDeclaration));
    assert!(kinds.contains(&SyntaxKind::Matcher));
    assert!(kinds.contains(&SyntaxKind::Variant));
    assert!(kinds.contains(&SyntaxKind::CatchAllKey));
    assert!(kinds.contains(&SyntaxKind::Selector));
}
