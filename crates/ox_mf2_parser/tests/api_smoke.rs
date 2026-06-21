//! End-to-end smoke tests for the public parse API.
//!
//! These do not assert on grammar — the grammar lands in Milestone 6 — they
//! just lock in the API shape, lifetime contracts, and batch ordering.

use ox_mf2_parser::{
    parse_batch, parse_message, parse_source, parse_source_session, BatchParseOptions,
    ParseCapacity, ParseInput, ParseOptions, ParseWorkspace, SourceFileInput, SourceStore,
};

#[test]
fn parse_message_returns_owned_result() {
    let result = parse_message("Hello");
    assert!(result.diagnostics.is_empty(), "unexpected diagnostics: {:?}", result.diagnostics);
    // Root, SimpleMessage, Pattern, Text — see design/002 §"Message Mode".
    assert!(result.cst.node_count() >= 4);
}

#[test]
fn parse_source_uses_caller_owned_source_store() {
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: "Hi",
        path: Some("greeting.mf2"),
        ..Default::default()
    });
    let result = parse_source(&sources, id, ParseOptions::default());
    assert_eq!(result.source, id);
}

#[test]
fn parse_source_session_reuses_workspace_capacity() {
    let mut sources = SourceStore::new();
    let id_a = sources.add(SourceFileInput {
        source: "one",
        ..Default::default()
    });
    let id_b = sources.add(SourceFileInput {
        source: "two",
        ..Default::default()
    });

    let mut workspace = ParseWorkspace::with_capacity(ParseCapacity::new(8, 8, 8, 8, 4));
    let cap_before = workspace.node_capacity();
    let _ = parse_source_session(&sources, id_a, &mut workspace, ParseOptions::default());
    let cap_after_a = workspace.node_capacity();
    let _ = parse_source_session(&sources, id_b, &mut workspace, ParseOptions::default());
    let cap_after_b = workspace.node_capacity();

    assert!(cap_after_a >= cap_before);
    assert!(cap_after_b >= cap_after_a);
}

#[test]
fn parse_batch_preserves_input_order() {
    let inputs = vec![
        ParseInput {
            source: "first",
            path: Some("first.mf2"),
            ..Default::default()
        },
        ParseInput {
            source: "second",
            path: Some("second.mf2"),
            ..Default::default()
        },
        ParseInput {
            source: "third",
            path: Some("third.mf2"),
            ..Default::default()
        },
    ];
    let result = parse_batch(&inputs, BatchParseOptions::default());
    assert_eq!(result.items.len(), 3);
    assert_eq!(result.items[0].source.raw(), 0);
    assert_eq!(result.items[1].source.raw(), 1);
    assert_eq!(result.items[2].source.raw(), 2);

    let f0 = result.sources.get(result.items[0].source).unwrap();
    let f2 = result.sources.get(result.items[2].source).unwrap();
    assert_eq!(f0.path.as_deref(), Some("first.mf2"));
    assert_eq!(f2.path.as_deref(), Some("third.mf2"));
}
