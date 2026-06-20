//! Recovery / malformed-input fixtures.
//!
//! For each malformed input we lock in:
//!
//! 1. The diagnostic count (to detect cascades).
//! 2. The first diagnostic code + span (so the "useful" diagnostic stays
//!    anchored at the right byte offset).
//! 3. That a root CST is still produced.
//! 4. That a CST traversal walks past the recovery point.

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
    let result = parse_source(&sources, id, ParseOptions::default());
    (sources, result)
}

fn descendant_count(root: CstNodeView<'_>) -> usize {
    fn visit(node: CstNodeView<'_>, count: &mut usize) {
        *count += 1;
        for child in node.children() {
            if let CstChild::Node(n) = child {
                visit(n, count);
            }
        }
    }
    let mut count = 0;
    visit(root, &mut count);
    count
}

#[test]
fn unclosed_placeholder_returns_root_and_one_diagnostic() {
    let (sources, result) = parse("Hello, {$name");
    assert_eq!(result.diagnostics.len(), 1, "{:?}", result.diagnostics);
    let diag = &result.diagnostics[0];
    assert_eq!(diag.code, DiagnosticCode::UnclosedExpression);
    // span anchors at the `{` and runs to EOF.
    assert_eq!(diag.span.start, 7);
    assert_eq!(diag.span.end, 13);

    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().expect("root exists");
    assert_eq!(root.kind(), SyntaxKind::Root);
    // Recovery should keep enough nodes that a traversal is non-trivial.
    assert!(descendant_count(root) >= 5);
}

#[test]
fn unclosed_quoted_literal_returns_root_and_one_diagnostic() {
    let (sources, result) = parse("{|incomplete}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::UnclosedQuotedLiteral),
        "{:?}",
        codes
    );
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().expect("root exists");
    assert_eq!(root.kind(), SyntaxKind::Root);
}

#[test]
fn unclosed_quoted_pattern_returns_root_and_diagnostic() {
    let (sources, result) = parse("{{unterminated");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::UnclosedQuotedPattern),
        "{:?}",
        codes
    );
    let view = CstView::new(&sources, result.source, &result.cst);
    assert_eq!(view.root().unwrap().kind(), SyntaxKind::Root);
}

#[test]
fn declarations_without_complex_body_emit_missing_body_diagnostic() {
    let (sources, result) = parse(".input {$x}\n");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::MissingComplexBody),
        "{:?}",
        codes
    );
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().expect("root exists");
    // Root → ComplexMessage → declarations + empty ComplexBody.
    assert_eq!(root.kind(), SyntaxKind::Root);
}

#[test]
fn invalid_escape_emits_one_diagnostic_and_keeps_progress() {
    let (sources, result) = parse("Hello \\X world");
    let invalid: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::InvalidEscape)
        .collect();
    assert_eq!(invalid.len(), 1);
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().expect("root exists");
    assert_eq!(root.kind(), SyntaxKind::Root);
}

#[test]
fn malformed_variant_emits_only_one_diagnostic() {
    // missing quoted-pattern body after the catch-all key.
    let (sources, result) = parse(".match $x\n*");
    let invalid: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::InvalidVariantBoundary)
        .collect();
    assert_eq!(invalid.len(), 1, "{:?}", result.diagnostics);
    let view = CstView::new(&sources, result.source, &result.cst);
    assert_eq!(view.root().unwrap().kind(), SyntaxKind::Root);
}
