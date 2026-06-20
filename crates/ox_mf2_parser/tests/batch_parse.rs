//! Batch-parse contract tests.
//!
//! Covers the Milestone 9 acceptance criteria:

#![allow(clippy::field_reassign_with_default, dead_code)]
//!
//! - input order is preserved
//! - `SourceId` <-> `ParseInput` metadata stays consistent
//! - `ParseResult` and `CstTables` are `Send` (parallel-ready)
//! - parser workspace is worker-local (the sequential implementation
//!   already satisfies this because the loop holds a single workspace).

use ox_mf2_parser::{
    parse_batch, BatchExecution, BatchParseOptions, CstTables, ParseInput, ParseResult,
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
fn batch_default_runs_sequentially_and_is_not_degraded() {
    let inputs = vec![ParseInput {
        source: "Hi",
        ..Default::default()
    }];
    let result = parse_batch(&inputs, BatchParseOptions::default());
    assert_eq!(result.execution, BatchExecution::Sequential);
    assert!(!result.degraded);
}

#[test]
fn batch_parallel_request_falls_back_to_sequential_and_is_marked_degraded() {
    let inputs = vec![
        ParseInput {
            source: "Hello",
            ..Default::default()
        },
        ParseInput {
            source: "World",
            ..Default::default()
        },
    ];
    let mut options = BatchParseOptions::default();
    options.execution = BatchExecution::Parallel;
    options.max_threads = Some(4);
    options.preserve_order = false;
    let result = parse_batch(&inputs, options);
    assert_eq!(
        result.execution,
        BatchExecution::Sequential,
        "Phase 1 only implements sequential execution"
    );
    assert!(
        result.degraded,
        "Parallel request must be reported as degraded until Phase 2 lands"
    );
    // Order must still be preserved even when the caller asked for parallel.
    assert_eq!(result.items[0].result.source, result.items[0].source);
    assert_eq!(result.items[1].result.source, result.items[1].source);
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
