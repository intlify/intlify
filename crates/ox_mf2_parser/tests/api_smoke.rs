// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! End-to-end smoke tests for the public parse API.
//!
//! These do not assert on grammar — the grammar lands in Milestone 6 — they
//! just lock in the API shape, lifetime contracts, and batch ordering.

use ox_mf2_parser::{
    ox_mf2_error_code_name, parse_batch, parse_message, parse_source, parse_source_session,
    BatchParseOptions, DecodeError, DecodeErrorCode, OxMf2ErrorCode, ParseCapacity, ParseError,
    ParseInput, ParseOptions, ParseWorkspace, SnapshotWriteError, SourceFileInput, SourceId,
    SourceStore, SourceTextUnavailable,
};

#[test]
fn error_code_exports_are_available_from_crate_root() {
    let decode: OxMf2ErrorCode =
        DecodeError::new(DecodeErrorCode::InvalidMagic).as_ox_mf2_error_code();
    assert_eq!(decode, 1001);
    assert_eq!(ox_mf2_error_code_name(decode), "DecodeInvalidMagic");

    let write: OxMf2ErrorCode = SnapshotWriteError::MissingRoot.as_ox_mf2_error_code();
    assert_eq!(write, 2011);
    assert_eq!(ox_mf2_error_code_name(write), "SnapshotWriteMissingRoot");

    let source: OxMf2ErrorCode = SourceTextUnavailable::NotIncluded.as_ox_mf2_error_code();
    assert_eq!(source, 3000);
    assert_eq!(ox_mf2_error_code_name(source), "SourceTextNotIncluded");
}

#[test]
fn parse_message_returns_owned_result() {
    let result = parse_message("Hello").expect("parse succeeds");
    assert!(
        result.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        result.diagnostics
    );
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
    let result = parse_source(&sources, id, ParseOptions::default()).expect("parse succeeds");
    assert_eq!(result.source, id);
}

#[test]
fn parse_source_rejects_unknown_source_id() {
    let sources = SourceStore::new();
    let error = parse_source(&sources, SourceId::new(7), ParseOptions::default())
        .expect_err("unknown source id must fail");
    assert_eq!(
        error,
        ParseError::InvalidSourceId {
            source_id: SourceId::new(7)
        }
    );
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
    let _ = parse_source_session(&sources, id_a, &mut workspace, ParseOptions::default())
        .expect("parse succeeds");
    let cap_after_a = workspace.node_capacity();
    let _ = parse_source_session(&sources, id_b, &mut workspace, ParseOptions::default())
        .expect("parse succeeds");
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
    let result = parse_batch(&inputs, BatchParseOptions::default()).expect("batch parse succeeds");
    assert_eq!(result.items.len(), 3);
    assert_eq!(result.items[0].source.raw(), 0);
    assert_eq!(result.items[1].source.raw(), 1);
    assert_eq!(result.items[2].source.raw(), 2);

    let f0 = result.sources.get(result.items[0].source).unwrap();
    let f2 = result.sources.get(result.items[2].source).unwrap();
    assert_eq!(f0.path.as_deref(), Some("first.mf2"));
    assert_eq!(f2.path.as_deref(), Some("third.mf2"));
}
