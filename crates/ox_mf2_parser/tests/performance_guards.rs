//! Performance regression guards for Phase 1.
//!
//! These tests do not measure wall-clock time — they sit alongside the
//! benchmark harness (`benches/hyperfine.sh`) and instead guard the
//! invariants that the benchmark numbers depend on:
//!
//! 1. Record layout sizes stay within the design's budget (so cache
//!    locality assumptions hold).
//! 2. Diagnostic record buffers do not grow on a valid input's success
//!    path (no allocated message strings, no orphan records).
//! 3. ParseWorkspace reuse keeps allocator pressure flat across repeated
//!    parses of the same source (capacity is preserved, not regrown).
//! 4. CstView traversal of a known input does not silently skip nodes.
//! 5. Recovery fixtures still produce exactly one useful diagnostic
//!    rather than a cascade.

use core::mem::size_of;

use ox_mf2_parser::{
    parse_message, parse_source, parse_source_session, CstChild, CstNodeView, CstView,
    DiagnosticCode, ParseCapacity, ParseOptions, ParseWorkspace, SourceFileInput, SourceStore,
    SyntaxKind,
};

// ── 1. record layout sizes ───────────────────────────────────────────────

#[test]
fn syntax_kind_stays_a_u16() {
    assert_eq!(size_of::<SyntaxKind>(), 2);
}

// CstNodeRecord / CstEdgeRecord / TokenRecord / TriviaRecord size budgets
// live in `tables::tests::record_sizes_stay_within_budget`. We re-assert here
// what we *expose publicly* so downstream consumers also see the guard.

#[test]
fn span_stays_eight_bytes() {
    assert_eq!(size_of::<ox_mf2_parser::Span>(), 8);
}

// ── 2. valid input success path has no diagnostics ───────────────────────

#[test]
fn valid_input_does_not_emit_diagnostics() {
    for case in [
        "",
        "Hello",
        "Hello, {$name}!",
        "{:datetime}",
        "{{Hello, world!}}",
        ".input {$x}\n{{Hi {$x}}}",
        ".match $x\n* {{fallback}}",
    ] {
        let result = parse_message(case);
        assert!(
            result.diagnostics.is_empty(),
            "case `{case}` produced diagnostics: {:?}",
            result.diagnostics
        );
    }
}

// ── 3. workspace reuse keeps capacity flat ───────────────────────────────

#[test]
fn workspace_reuse_does_not_regrow_capacity() {
    let input = "Hello, {$name}!";
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: input,
        ..Default::default()
    });

    let mut workspace = ParseWorkspace::with_capacity(ParseCapacity::new(32, 32, 32, 32, 0));
    workspace.reserve_for_source_len(input.len() * 4);

    // Warm up so capacity reflects steady-state.
    for _ in 0..4 {
        let _ = parse_source_session(&sources, id, &mut workspace, ParseOptions::default());
    }
    let cap_after_warmup = workspace.node_capacity();

    // Many more iterations of the same source must not regrow capacity.
    for _ in 0..1024 {
        let _ = parse_source_session(&sources, id, &mut workspace, ParseOptions::default());
    }
    assert_eq!(workspace.node_capacity(), cap_after_warmup);
}

// ── 4. CstView traversal completeness ────────────────────────────────────

#[test]
fn cst_view_traversal_visits_every_node_and_token() {
    let result = parse_message("Hello, {$name}!");
    let sources = SourceStore::new();
    let view = CstView::new(&sources, result.source, &result.cst);
    let mut visited_nodes = 0usize;
    let mut visited_tokens = 0usize;
    if let Some(root) = view.root() {
        walk(root, &mut visited_nodes, &mut visited_tokens);
    }
    assert_eq!(visited_nodes, result.cst.node_count());
    assert_eq!(visited_tokens, result.cst.token_count());
}

fn walk(node: CstNodeView<'_>, nodes: &mut usize, tokens: &mut usize) {
    *nodes += 1;
    for child in node.children() {
        match child {
            CstChild::Node(n) => walk(n, nodes, tokens),
            CstChild::Token(_) => *tokens += 1,
        }
    }
}

// ── 5. recovery still produces exactly one useful diagnostic ─────────────

#[test]
fn recovery_does_not_cascade() {
    // Each case: (input, expected first diagnostic, upper bound on
    // diagnostic count). The cap is here so a regression that goes from
    // 1 → 5 still fails, while genuinely two-root-cause inputs stay
    // accepted.
    for (input, expected_code, max_diagnostics) in [
        ("Hello, {$name", DiagnosticCode::UnclosedExpression, 1),
        ("{{unterminated", DiagnosticCode::UnclosedQuotedPattern, 1),
        // `{|broken}` has both an unterminated quoted literal AND no
        // closing `}` for the placeholder. Two diagnostics is correct;
        // anything beyond two is a cascade.
        ("{|broken}", DiagnosticCode::UnclosedQuotedLiteral, 2),
    ] {
        let mut sources = SourceStore::new();
        let id = sources.add(SourceFileInput {
            source: input,
            ..Default::default()
        });
        let result = parse_source(&sources, id, ParseOptions::default());
        assert!(
            result.diagnostics.len() <= max_diagnostics,
            "case `{input}` cascaded: {:?}",
            result.diagnostics
        );
        assert!(
            result.diagnostics.iter().any(|d| d.code == expected_code),
            "case `{input}` did not emit {expected_code:?}: {:?}",
            result.diagnostics
        );
    }
}
