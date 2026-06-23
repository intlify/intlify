// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! End-to-end Binary AST snapshot round-trip tests.
//!
//! Exercises the public Phase 2 snapshot API: parse → snapshot → decode
//! → traverse. The lower-level wire format is locked down by
//! `crates/ox_mf2_parser/src/snapshot/*.rs` unit tests; these tests
//! cover the API contract and that decoded views match parser output.

use ox_mf2_parser::snapshot::{
    decode_snapshot, decode_snapshot_owned, view::ChildView, SectionKind, SnapshotOptions,
    SourceTextUnavailable, SNAPSHOT_MAGIC,
};
use ox_mf2_parser::{
    parse_batch_to_snapshot, parse_result_to_snapshot, parse_source, parse_source_to_snapshot,
    BatchParseOptions, ParseInput, ParseOptions, SourceFileInput, SourceStore, Span,
};

#[test]
fn simple_message_round_trips_through_snapshot() {
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: "Hello",
        ..Default::default()
    });
    let result = parse_source(&sources, id, ParseOptions::default());
    let snap = parse_result_to_snapshot(&sources, &result, SnapshotOptions::default())
        .expect("snapshot encode succeeds");
    assert!(!snap.bytes.is_empty(), "snapshot bytes are non-empty");
    assert_eq!(&snap.bytes[..8], &SNAPSHOT_MAGIC);

    let view = decode_snapshot(&snap.bytes).expect("decode succeeds");
    assert_eq!(view.root_count(), 1);
    let root = view.root(snap.root).expect("root view");
    let root_node = view.node(root.root_node()).expect("root node view");
    // Same kind that the parser produced for the root.
    let parser_root = result
        .cst
        .root_id()
        .and_then(|id| result.cst.node_count().checked_sub(1).map(|_| id))
        .expect("parser root exists");
    let _ = parser_root;
    assert_eq!(root_node.kind(), ox_mf2_parser::SyntaxKind::Root);
}

#[test]
fn snapshot_preserves_token_text_via_source_id_plus_span() {
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: "Hello",
        ..Default::default()
    });
    let result = parse_source(&sources, id, ParseOptions::default());
    let snap = parse_result_to_snapshot(&sources, &result, SnapshotOptions::default()).unwrap();
    let view = decode_snapshot(&snap.bytes).unwrap();

    // For a single-input snapshot the writer remaps Phase 1 `id` to
    // snapshot-local source 0 — assert that explicitly so a future
    // source-remapping regression cannot quietly pass this test on
    // span-based recovery alone.
    let root = view.root(snap.root).unwrap();
    assert_eq!(root.source_id().raw(), 0);
    let root_node = view.node(root.root_node()).unwrap();
    let token = find_first_token(root_node).expect("at least one token");
    assert_eq!(
        token.source_id().raw(),
        root.source_id().raw(),
        "token source identity must match the root's source identity"
    );
    let text = sources.slice_in(id, token.span());
    assert!(!text.is_empty(), "token covers non-empty source text");
}

fn find_first_token(
    node: ox_mf2_parser::snapshot::view::NodeView<'_>,
) -> Option<ox_mf2_parser::snapshot::view::TokenView<'_>> {
    for child in node.children() {
        match child {
            ChildView::Token(token) => return Some(token),
            ChildView::Node(child_node) => {
                if let Some(token) = find_first_token(child_node) {
                    return Some(token);
                }
            }
        }
    }
    None
}

#[test]
fn include_diagnostics_false_drops_diagnostic_sections_and_bytes() {
    // `{$unclosed` is malformed: the parser emits at least one
    // diagnostic. With include_diagnostics = true the snapshot must
    // contain the diagnostics section; with include_diagnostics =
    // false the writer must skip emitting diagnostic bytes entirely,
    // which produces a strictly smaller snapshot.
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: "{$unclosed",
        ..Default::default()
    });
    let with_diag = parse_source_to_snapshot(
        &sources,
        id,
        ParseOptions::default(),
        SnapshotOptions::default(),
    )
    .unwrap();
    let without_diag = parse_source_to_snapshot(
        &sources,
        id,
        ParseOptions::default(),
        SnapshotOptions {
            include_diagnostics: false,
            ..SnapshotOptions::default()
        },
    )
    .unwrap();

    let with_view = decode_snapshot(&with_diag.bytes).unwrap();
    let without_view = decode_snapshot(&without_diag.bytes).unwrap();

    assert!(with_view.diagnostic_count() > 0);
    assert!(with_view.section(SectionKind::Diagnostics).is_some());

    assert_eq!(without_view.diagnostic_count(), 0);
    assert!(without_view.section(SectionKind::Diagnostics).is_none());
    assert!(without_view
        .section(SectionKind::DiagnosticLabels)
        .is_none());

    assert!(
        without_diag.bytes.len() < with_diag.bytes.len(),
        "include_diagnostics = false must not include diagnostic bytes \
         (with={} bytes, without={} bytes)",
        with_diag.bytes.len(),
        without_diag.bytes.len()
    );

    // Caller convenience: `diagnostics` is always populated so the
    // caller can still inspect them.
    assert!(!without_diag.diagnostics.is_empty());
    assert_eq!(without_diag.diagnostics.len(), with_diag.diagnostics.len());
}

#[test]
fn snapshot_omits_optional_sections_when_disabled() {
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: "Hello",
        ..Default::default()
    });
    let snap = parse_source_to_snapshot(
        &sources,
        id,
        ParseOptions {
            collect_trivia: false,
            ..ParseOptions::default()
        },
        SnapshotOptions {
            include_diagnostics: false,
            include_source_text: false,
            include_trivia: false,
        },
    )
    .unwrap();
    let view = decode_snapshot(&snap.bytes).unwrap();
    assert!(view.section(SectionKind::Trivia).is_none());
    assert!(view.section(SectionKind::Diagnostics).is_none());
    assert!(view.section(SectionKind::DiagnosticLabels).is_none());
    assert!(view.section(SectionKind::SourceTextData).is_none());
    // Core sections still present even with all optional sections off.
    assert!(view.section(SectionKind::Roots).is_some());
    assert!(view.section(SectionKind::Sources).is_some());
    assert!(view.section(SectionKind::Nodes).is_some());
    assert!(view.section(SectionKind::Edges).is_some());
    assert!(view.section(SectionKind::Tokens).is_some());
    assert!(view.section(SectionKind::StringOffsets).is_some());
    assert!(view.section(SectionKind::StringData).is_some());
}

#[test]
fn source_slice_distinguishes_not_included_from_out_of_bounds() {
    // `include_source_text = false` (the default) → NotIncluded.
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: "Hello",
        ..Default::default()
    });
    let snap = parse_source_to_snapshot(
        &sources,
        id,
        ParseOptions::default(),
        SnapshotOptions::default(),
    )
    .unwrap();
    let view = decode_snapshot(&snap.bytes).unwrap();
    let source = view
        .source(view.root(snap.root).unwrap().source_id())
        .unwrap();
    assert_eq!(
        source.source_slice(Span::new(0, 5)).unwrap_err(),
        SourceTextUnavailable::NotIncluded
    );

    // `include_source_text = true` → in-range span resolves, span
    // past the end / inverted span both surface `SpanOutOfBounds`.
    let snap = parse_source_to_snapshot(
        &sources,
        id,
        ParseOptions::default(),
        SnapshotOptions {
            include_source_text: true,
            ..SnapshotOptions::default()
        },
    )
    .unwrap();
    let view = decode_snapshot(&snap.bytes).unwrap();
    let source = view
        .source(view.root(snap.root).unwrap().source_id())
        .unwrap();
    assert_eq!(source.source_slice(Span::new(0, 5)).unwrap(), "Hello");
    assert_eq!(source.source_slice(Span::new(1, 4)).unwrap(), "ell");
    assert_eq!(source.source_slice(Span::new(0, 0)).unwrap(), "");
    assert_eq!(
        source.source_slice(Span::new(0, 99)).unwrap_err(),
        SourceTextUnavailable::SpanOutOfBounds
    );
    assert_eq!(
        source.source_slice(Span::new(4, 2)).unwrap_err(),
        SourceTextUnavailable::SpanOutOfBounds
    );
}

#[test]
fn source_slice_rejects_span_splitting_multibyte_scalar() {
    // "あ" is 3 UTF-8 bytes (0xE3 0x81 0x84). A span that ends at
    // offset 1 splits the scalar — `source_slice` must surface
    // `SpanOutOfBounds`, not return invalid bytes.
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: "あ",
        ..Default::default()
    });
    let snap = parse_source_to_snapshot(
        &sources,
        id,
        ParseOptions::default(),
        SnapshotOptions {
            include_source_text: true,
            ..SnapshotOptions::default()
        },
    )
    .unwrap();
    let view = decode_snapshot(&snap.bytes).unwrap();
    let source = view
        .source(view.root(snap.root).unwrap().source_id())
        .unwrap();
    assert_eq!(source.source_slice(Span::new(0, 3)).unwrap(), "あ");
    assert_eq!(
        source.source_slice(Span::new(0, 1)).unwrap_err(),
        SourceTextUnavailable::SpanOutOfBounds
    );
}

#[test]
fn source_metadata_interns_before_diagnostic_messages() {
    // design/003 §"String Table" requires the first-seen string
    // order to be: 1) source metadata, 2) diagnostic messages.
    // Use a malformed input with non-default metadata so both
    // categories land in the string table, then assert metadata
    // StringIds come before the diagnostic message StringId.
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: "{$unclosed",
        path: Some("greeting.mf2"),
        locale: Some("en"),
        message_id: Some("hello"),
        ..Default::default()
    });
    let snap = parse_source_to_snapshot(
        &sources,
        id,
        ParseOptions::default(),
        SnapshotOptions::default(),
    )
    .unwrap();
    let view = decode_snapshot(&snap.bytes).unwrap();
    // The string table must hold "greeting.mf2", "en", "hello",
    // and the diagnostic catalog message — in that order.
    assert_eq!(
        view.string(ox_mf2_parser::snapshot::StringId::new(0)),
        Some("greeting.mf2")
    );
    assert_eq!(
        view.string(ox_mf2_parser::snapshot::StringId::new(1)),
        Some("en")
    );
    assert_eq!(
        view.string(ox_mf2_parser::snapshot::StringId::new(2)),
        Some("hello")
    );
    // The diagnostic message StringId must reference an entry
    // strictly after the source metadata block.
    let diag = view.diagnostic(0).expect("malformed input has diagnostics");
    let label_range = diag.label_range();
    assert_eq!(label_range, (0, 0));
    let message = diag.message().expect("diagnostic message interned");
    // Static catalog message for UnclosedExpression.
    assert!(message.starts_with("unclosed"));
    // Lookup via section count: the message StringId must be >= 3.
    let so = view.section(SectionKind::StringOffsets).unwrap();
    assert!(so.count >= 4, "expected at least 4 interned strings");
}

#[test]
fn empty_source_text_round_trips_when_include_source_text_is_true() {
    // Empty source text with `include_source_text = true` is the
    // boundary case where the writer must emit an (empty) source
    // text data section: the SourceRecord text refs are non-sentinel
    // so the decoder needs the section to resolve them.
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: "",
        ..Default::default()
    });
    let snap = parse_source_to_snapshot(
        &sources,
        id,
        ParseOptions::default(),
        SnapshotOptions {
            include_source_text: true,
            ..SnapshotOptions::default()
        },
    )
    .unwrap();
    let view = decode_snapshot(&snap.bytes).expect("decode succeeds");
    let source = view
        .source(view.root(snap.root).unwrap().source_id())
        .unwrap();
    assert_eq!(source.text(), Some(""));
    // Section exists with an empty payload.
    let section = view.section(SectionKind::SourceTextData).unwrap();
    assert_eq!(section.byte_len, 0);
}

#[test]
fn snapshot_with_source_text_resolves_text_through_view() {
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: "Hi",
        path: Some("greeting.mf2"),
        locale: Some("en"),
        ..Default::default()
    });
    let snap = parse_source_to_snapshot(
        &sources,
        id,
        ParseOptions::default(),
        SnapshotOptions {
            include_source_text: true,
            ..SnapshotOptions::default()
        },
    )
    .unwrap();
    let view = decode_snapshot(&snap.bytes).unwrap();
    let source = view
        .source(view.root(snap.root).unwrap().source_id())
        .unwrap();
    assert_eq!(source.path(), Some("greeting.mf2"));
    assert_eq!(source.locale(), Some("en"));
    assert_eq!(source.text(), Some("Hi"));
}

#[test]
fn parse_message_to_snapshot_carries_metadata() {
    // The standalone convenience encodes through its own one-entry
    // SourceStore, so SnapshotResult.root is always 0 and any
    // caller-supplied metadata round-trips into the source record.
    // `SnapshotSourceMetadata` omits the `source` field by
    // construction so the parsed text and the snapshot's text can
    // never diverge.
    let snap = ox_mf2_parser::parse_message_to_snapshot(
        "Hi",
        Some(ox_mf2_parser::SnapshotSourceMetadata {
            path: Some("greeting.mf2"),
            locale: Some("en"),
            message_id: Some("hello"),
            base_offset: Some(7),
        }),
        ParseOptions::default(),
        SnapshotOptions {
            include_source_text: true,
            ..SnapshotOptions::default()
        },
    )
    .unwrap();
    let view = decode_snapshot(&snap.bytes).unwrap();
    let source = view
        .source(view.root(snap.root).unwrap().source_id())
        .unwrap();
    assert_eq!(source.path(), Some("greeting.mf2"));
    assert_eq!(source.locale(), Some("en"));
    assert_eq!(source.message_id(), Some("hello"));
    assert_eq!(source.base_offset(), 7);
    assert_eq!(source.text(), Some("Hi"));
}

#[test]
fn decode_snapshot_owned_shares_buffer() {
    let snap = ox_mf2_parser::parse_message_to_snapshot(
        "Hello",
        None,
        ParseOptions::default(),
        SnapshotOptions::default(),
    )
    .unwrap();
    let owned: std::sync::Arc<[u8]> = snap.bytes.into();
    let view = decode_snapshot_owned(owned.clone()).unwrap();
    // Same bytes are visible through both .as_bytes() and the cloned arc.
    assert_eq!(view.as_bytes(), &*owned);
}

#[test]
fn batch_snapshot_carries_one_root_per_input() {
    let inputs = [
        ParseInput {
            source: "Hello",
            ..Default::default()
        },
        ParseInput {
            source: "World",
            ..Default::default()
        },
    ];
    let snap = parse_batch_to_snapshot(
        &inputs,
        BatchParseOptions::default(),
        SnapshotOptions::default(),
    )
    .unwrap();
    assert_eq!(snap.roots.len(), 2);
    let view = decode_snapshot(&snap.bytes).unwrap();
    assert_eq!(view.root_count(), 2);
    for (i, root_id) in snap.roots.iter().enumerate() {
        let root = view.root(*root_id).unwrap();
        let source = view.source(root.source_id()).unwrap();
        // Each input gets its own SourceRecord in v0.1 (no dedup).
        assert_eq!(source.id().raw(), i as u32);
    }
}
