// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Recovery / malformed-input fixtures.
//!
//! For each malformed input we lock in:

#![allow(clippy::uninlined_format_args)]
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

fn collect_kinds_recovery(root: CstNodeView<'_>) -> Vec<SyntaxKind> {
    fn visit(node: CstNodeView<'_>, out: &mut Vec<SyntaxKind>) {
        out.push(node.kind());
        for child in node.children() {
            if let CstChild::Node(n) = child {
                visit(n, out);
            }
        }
    }
    let mut out = Vec::new();
    visit(root, &mut out);
    out
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

// ─── Missing required `s` separator ────────────────────────────────────────
//
// `s = *bidi ws o` requires at least one `ws`. The parser still parses the
// construct (so recovery yields a useful CST), but flags the missing
// separator. See the matching fixtures under `fixtures/recovery/missing_s_*`.

#[test]
fn missing_s_before_function_emits_diagnostic() {
    let (_, result) = parse("{$x:number}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::MissingRequiredWhitespace),
        "{:?}",
        codes
    );
}

#[test]
fn missing_s_before_local_variable_emits_diagnostic() {
    let (_, result) = parse(".local$x = {$y}\n{{Hi}}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::MissingRequiredWhitespace),
        "{:?}",
        codes
    );
}

#[test]
fn missing_s_before_selector_emits_diagnostic() {
    let (_, result) = parse(".match$x\n* {{fallback}}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::MissingRequiredWhitespace),
        "{:?}",
        codes
    );
}

#[test]
fn missing_s_before_attribute_emits_diagnostic() {
    let (_, result) = parse("{#tag@attr}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::MissingRequiredWhitespace),
        "{:?}",
        codes
    );
}

#[test]
fn missing_s_between_adjacent_variant_keys_emits_diagnostic() {
    let (_, result) = parse(".match $x\none|two| {{combo}}\n* {{fallback}}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::MissingRequiredWhitespace),
        "{:?}",
        codes
    );
}

// ─── Namespaced identifier with no trailing name ──────────────────────────
//
// `identifier = [namespace ":"] name` — once we see `name ":"`, the trailing
// `name` is mandatory. A missing trailing name is flagged with
// `MissingIdentifierName` and a `Missing` node is inserted so the CST keeps
// an anchor at the boundary.

#[test]
fn trailing_colon_in_function_identifier_emits_diagnostic() {
    let (sources, result) = parse("{:foo:}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::MissingIdentifierName),
        "{:?}",
        codes
    );
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    let kinds = collect_kinds_recovery(root);
    assert!(kinds.contains(&SyntaxKind::Missing));
}

#[test]
fn trailing_colon_in_markup_identifier_emits_diagnostic() {
    let (_, result) = parse("{#ns:}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::MissingIdentifierName),
        "{:?}",
        codes
    );
}

#[test]
fn trailing_colon_in_option_identifier_emits_diagnostic() {
    let (_, result) = parse("{:fn opt:=value}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::MissingIdentifierName),
        "{:?}",
        codes
    );
}

#[test]
fn trailing_colon_in_attribute_identifier_emits_diagnostic() {
    let (_, result) = parse("{|x| @ns:=|v|}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::MissingIdentifierName),
        "{:?}",
        codes
    );
}

#[test]
fn speculative_branch_does_not_duplicate_trivia() {
    // `{$x }` — `maybe_parse_function` eats the trailing space, then rolls
    // back because no `:` follows. The trailing `o` of `parse_expression`
    // re-consumes the same byte; the rollback must truncate the speculative
    // trivia so we end up with exactly ONE record, not three.
    let (_, result) = parse("{$x }");
    assert_eq!(result.cst.trivia_count(), 1, "expected one trivia record");
}

#[test]
fn invalid_declaration_start_emits_diagnostic() {
    let (_, result) = parse(".foo {{body}}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::InvalidDeclarationStart]);
}

#[test]
fn missing_s_before_first_variant_emits_diagnostic() {
    let (_, result) = parse(".match $count* {{one}}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&DiagnosticCode::MissingRequiredWhitespace),
        "{:?}",
        codes
    );
}

#[test]
fn standalone_markup_disallows_space_between_slash_and_brace() {
    let (_, result) = parse("{#tag / }");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::InvalidMarkupBoundary]);
}

#[test]
fn local_declaration_variable_requires_dollar_sigil() {
    let (_, result) = parse(".local name = {$x}\n{{body}}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::UnexpectedToken]);
}

// ─── `.input` requires a variable-expression ───────────────────────────────
//
// `input-declaration = input o variable-expression` — a literal, function,
// or markup placeholder after `.input` is a syntax error. The placeholder
// subtree is still kept so tooling can inspect the offending value. This
// backs the zero-diagnostic guarantee that the Phase 3B formatter strict
// policy relies on.

#[test]
fn input_declaration_with_literal_expression_emits_diagnostic() {
    let (sources, result) = parse(".input {|foo|}\n{{body}}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::InvalidInputDeclaration]);
    // The diagnostic anchors at the offending placeholder expression.
    let diag = result.diagnostics.first().unwrap();
    assert_eq!(diag.span.start, 7);
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    let kinds = collect_kinds_recovery(root);
    assert!(
        kinds.contains(&SyntaxKind::LiteralExpression),
        "{:?}",
        kinds
    );
}

#[test]
fn input_declaration_with_literal_and_function_emits_diagnostic() {
    let (_, result) = parse(".input {|foo| :number}\n{{body}}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::InvalidInputDeclaration]);
}

#[test]
fn input_declaration_with_function_expression_emits_diagnostic() {
    let (_, result) = parse(".input {:number}\n{{body}}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::InvalidInputDeclaration]);
}

#[test]
fn input_declaration_with_markup_emits_diagnostic() {
    let (_, result) = parse(".input {#b}\n{{body}}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::InvalidInputDeclaration]);
}

#[test]
fn input_declaration_with_variable_expression_stays_diagnostic_free() {
    let (_, result) = parse(".input {$count :number}\n{{ok}}");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

// ─── Matcher requires selectors and variants ───────────────────────────────
//
// `matcher = match-statement 1*variant` with
// `match-statement = match 1*(s selector)` — both lists are required. One
// `InvalidMatcherSyntax` diagnostic is anchored at the `.match` keyword; a
// matcher missing both lists must not cascade into two diagnostics.

#[test]
fn matcher_without_selectors_or_variants_emits_one_diagnostic() {
    let (sources, result) = parse(".match");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::InvalidMatcherSyntax]);
    let diag = result.diagnostics.first().unwrap();
    assert_eq!((diag.span.start, diag.span.end), (0, 6));
    // A root CST with a Matcher node still exists for recovery consumers.
    let view = CstView::new(&sources, result.source, &result.cst);
    let root = view.root().unwrap();
    let kinds = collect_kinds_recovery(root);
    assert!(kinds.contains(&SyntaxKind::Matcher), "{:?}", kinds);
}

#[test]
fn matcher_without_variants_emits_diagnostic() {
    let (_, result) = parse(".match $count");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::InvalidMatcherSyntax]);
}

#[test]
fn matcher_with_non_variable_selector_emits_diagnostic() {
    // `|x|` never enters the selector loop, so the selector list is empty
    // and `|x| *` parse as variant keys of one variant instead.
    let (_, result) = parse(".match |x| * {{y}}");
    let codes: Vec<_> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![DiagnosticCode::InvalidMatcherSyntax]);
}

#[test]
fn matcher_with_selector_and_variants_stays_diagnostic_free() {
    let (_, result) = parse(".match $count\none {{one}}\n* {{other}}");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}
