//! Batch-parse contract tests.
//!
//! Covers the Milestone 9 acceptance criteria:
//!
//! - input order is preserved
//! - `SourceId` <-> `ParseInput` metadata stays consistent
//! - `ParseResult` and `CstTables` are `Send` (parallel-ready)
//! - parser workspace is worker-local (the sequential implementation
//!   already satisfies this because the loop holds a single workspace).

use ox_mf2_parser::{
    parse_batch, BatchParseOptions, CstTables, ParseInput, ParseResult,
};

/// Compile-time `Send` assertion. If parser state ever picks up a `!Send`
/// field, this fails to compile.
const _: () = {
    fn assert_send<T: Send>() {}
    fn _check() {
        assert_send::<CstTables>();
        assert_send::<ParseResult>();
    }
};

#[test]
fn batch_preserves_input_order_and_metadata() {
    let inputs = vec![
        ParseInput {
            source: "first",
            path: Some("a.mf2"),
            locale: Some("en"),
            message_id: Some("a"),
            ..Default::default()
        },
        ParseInput {
            source: "{{second}}",
            path: Some("b.mf2"),
            locale: Some("ja"),
            message_id: Some("b"),
            ..Default::default()
        },
        ParseInput {
            source: ".match $x\n* {{c}}",
            path: Some("c.mf2"),
            locale: Some("fr"),
            message_id: Some("c"),
            ..Default::default()
        },
    ];

    let result = parse_batch(&inputs, BatchParseOptions::default());
    assert_eq!(result.items.len(), 3);
    for (i, item) in result.items.iter().enumerate() {
        let file = result
            .sources
            .get(item.source)
            .expect("source registered");
        match i {
            0 => {
                assert_eq!(file.path.as_deref(), Some("a.mf2"));
                assert_eq!(file.locale.as_deref(), Some("en"));
                assert_eq!(file.message_id.as_deref(), Some("a"));
                assert_eq!(file.text, "first");
            }
            1 => {
                assert_eq!(file.path.as_deref(), Some("b.mf2"));
                assert_eq!(file.locale.as_deref(), Some("ja"));
                assert_eq!(file.text, "{{second}}");
            }
            2 => {
                assert_eq!(file.path.as_deref(), Some("c.mf2"));
                assert_eq!(file.locale.as_deref(), Some("fr"));
                assert!(file.text.starts_with(".match"));
            }
            _ => unreachable!(),
        }
        // Each item.source is what the parser was told to parse.
        assert_eq!(item.result.source, item.source);
    }
}

#[test]
fn batch_each_result_has_independent_cst_state() {
    // Two messages with very different shapes; ensure neither result leaks
    // nodes from the other (would only happen if the workspace wasn't
    // cleared between sessions).
    let inputs = vec![
        ParseInput {
            source: "Hello, {$name}!",
            ..Default::default()
        },
        ParseInput {
            source: "{{x}}",
            ..Default::default()
        },
    ];
    let result = parse_batch(&inputs, BatchParseOptions::default());
    let count_a = result.items[0].result.cst.node_count();
    let count_b = result.items[1].result.cst.node_count();
    assert!(count_a > 0);
    assert!(count_b > 0);
    assert_ne!(count_a, count_b);
}

#[test]
fn batch_handles_malformed_input_without_aborting_subsequent_items() {
    let inputs = vec![
        ParseInput {
            source: "Hello, {$name",
            ..Default::default()
        },
        ParseInput {
            source: "Bonjour, {$name}!",
            ..Default::default()
        },
    ];
    let result = parse_batch(&inputs, BatchParseOptions::default());
    assert!(!result.items[0].result.diagnostics.is_empty());
    assert!(result.items[1].result.diagnostics.is_empty());
}
