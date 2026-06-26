// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Binary AST snapshot writer, decoder, and accessor.
//!
//! The snapshot is the cross-process, cross-language, and persistence boundary
//! for parser output. It is not the primary in-process parser representation;
//! use [`crate::ParseResult`] and [`crate::CstView`] when a Rust caller only
//! needs to inspect a fresh parse.
//!
//! # Quick start
//!
//! Use [`parse_message_to_snapshot`] for a one-shot, standalone
//! encode that does not require building a [`crate::SourceStore`].
//! Use [`parse_source_to_snapshot`] / [`parse_result_to_snapshot`] /
//! [`parse_session_to_snapshot`] when the caller already owns a
//! `SourceStore` (e.g. a batch parse pipeline). Mixing
//! [`crate::parse_message`]'s `ParseResult` with an unrelated
//! `SourceStore` is **not safe** — see
//! [`parse_result_to_snapshot`]'s documentation for the contract.
//!
//! ```
//! use ox_mf2_parser::{snapshot::{
//!     decode_snapshot, parse_message_to_snapshot, SnapshotOptions,
//! }, ParseOptions};
//!
//! let snap = parse_message_to_snapshot(
//!     "Hello",
//!     None,
//!     ParseOptions::default(),
//!     SnapshotOptions::default(),
//! ).unwrap();
//! let view = decode_snapshot(&snap.bytes).unwrap();
//! assert_eq!(view.root_count(), 1);
//! ```
//!
//! # Format stability
//!
//! While `major_version = 0`, the wire format is draft and decoders
//! use exact version matching.

mod decoder;
mod error;
mod format;
mod sections;
mod string_table;
mod view;
mod writer;

pub use decoder::{decode_snapshot, decode_snapshot_owned};
pub use error::{DecodeError, DecodeErrorCode, SnapshotWriteError, SnapshotWriteErrorCode};
pub use format::{
    RootId, SectionKind, StringId, DIAGNOSTIC_LABEL_RECORD_SIZE, DIAGNOSTIC_RECORD_SIZE,
    EDGE_KIND_NODE, EDGE_KIND_TOKEN, EDGE_RECORD_SIZE, EXTENDED_DATA_HEADER_SIZE, HEADER_SIZE,
    NODE_RECORD_SIZE, NONE_REF, ROOT_RECORD_SIZE, SECTION_ALIGNMENT, SECTION_FLAG_REQUIRED,
    SECTION_RECORD_SIZE, SNAPSHOT_FEATURE_FLAGS, SNAPSHOT_MAGIC, SNAPSHOT_MAJOR_VERSION,
    SNAPSHOT_MINOR_VERSION, SOURCE_RECORD_SIZE, STRING_OFFSET_RECORD_SIZE, TOKEN_RECORD_SIZE,
    TRIVIA_RECORD_SIZE,
};
pub use view::{
    ChildIter, ChildView, DiagnosticIter, DiagnosticLabelIter, DiagnosticLabelView,
    DiagnosticRecordView, NodeView, RootView, SectionIndex, SectionSlice, SectionView,
    SnapshotView, SnapshotViewOwned, SourceTextUnavailable, SourceView, TokenView, TriviaIter,
    TriviaView,
};
pub use writer::{
    parse_batch_result_to_snapshot, parse_batch_to_snapshot, parse_message_to_snapshot,
    parse_result_to_snapshot, parse_session_to_snapshot, parse_source_to_snapshot,
    BatchSnapshotResult, SnapshotOptions, SnapshotResult, SnapshotSourceMetadata,
};
