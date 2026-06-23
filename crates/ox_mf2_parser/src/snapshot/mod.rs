// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Binary AST snapshot writer, decoder, and accessor.
//!
//! This module implements the v0.1 versioned snapshot format defined
//! in `design/003-ox-mf2-phase-2-binary-ast-snapshot-design.md`. The
//! snapshot is the cross-process / cross-language / persistence
//! boundary; it is not the primary parser representation.

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
    parse_batch_result_to_snapshot, parse_batch_to_snapshot, parse_result_to_snapshot,
    parse_session_to_snapshot, parse_source_to_snapshot, BatchSnapshotResult, SnapshotOptions,
    SnapshotResult,
};
