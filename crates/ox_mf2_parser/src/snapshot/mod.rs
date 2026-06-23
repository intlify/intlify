// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Binary AST snapshot writer, decoder, and accessor.
//!
//! Implements the v0.1 versioned snapshot format defined in
//! `design/003-ox-mf2-phase-2-binary-ast-snapshot-design.md`. The
//! snapshot is the cross-process / cross-language / persistence
//! boundary; it is not the primary parser representation.
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
//! use exact version matching. Any intentional change to the wire
//! format MUST update
//! `design/003-ox-mf2-binary-ast-format-changelog.md` in the same
//! commit; the compatibility guard tests under
//! `crates/ox_mf2_parser/tests/snapshot_compat.rs` enforce that the
//! changelog still documents the live magic, major, and minor version.

pub mod decoder;
pub mod error;
pub mod format;
mod sections;
mod source_map;
mod string_table;
pub mod view;
pub mod writer;

pub use decoder::{decode_snapshot, decode_snapshot_owned};
pub use error::{DecodeError, DecodeErrorCode, SnapshotWriteError};
pub use format::{
    RootId, SectionKind, StringId, HEADER_SIZE, NONE_REF, ROOT_RECORD_SIZE, SECTION_ALIGNMENT,
    SECTION_RECORD_SIZE, SNAPSHOT_MAGIC, SNAPSHOT_MAJOR_VERSION, SNAPSHOT_MINOR_VERSION,
};
pub use view::{
    ChildIter, ChildView, DiagnosticLabelIter, DiagnosticLabelView, DiagnosticRecordView, NodeView,
    RootView, SectionIndex, SectionSlice, SectionView, SnapshotView, SnapshotViewOwned, SourceView,
    TokenView, TriviaIter, TriviaView,
};
pub use writer::{
    parse_batch_result_to_snapshot, parse_batch_to_snapshot, parse_message_to_snapshot,
    parse_result_to_snapshot, parse_session_to_snapshot, parse_source_to_snapshot,
    BatchSnapshotResult, SnapshotOptions, SnapshotResult, SnapshotSourceMetadata,
};
