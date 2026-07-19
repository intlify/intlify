// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! High performance `MessageFormat` 2 parser core.
//!
//! This crate provides the Rust core for parsing
//! [Unicode MessageFormat 2.0][message-format-wg] messages. It builds a
//! recovering, lossless concrete syntax tree (CST), materialises diagnostics,
//! optionally lowers semantic records, and can encode parse results into the
//! crate's Binary AST snapshot format.
//!
//! # Entry points
//!
//! Use [`parse_message`] for a one-shot parse of an in-memory message. Use
//! [`parse_source`] when the caller owns a [`SourceStore`], and
//! [`parse_source_session`] with [`ParseWorkspace`] when repeated parsing
//! should reuse allocations.
//!
//! # API layers
//!
//! The crate root re-exports the supported parser, CST, diagnostic, semantic,
//! source, and workspace types. The [`snapshot`] module is the public namespace
//! for Binary AST snapshot encoding, decoding, and zero-copy views.
//!
//! # Stability
//!
//! This crate is pre-1.0. Public APIs are intended to be useful and documented,
//! but minor releases may still refine names, shape, and snapshot details while
//! `MessageFormat` 2 integration work continues. Error code values are exposed so
//! language bindings can map failures without parsing display strings.
//!
//! # Example
//!
//! ```
//! use ox_mf2_parser::parse_message;
//!
//! let result = parse_message("Hello, {$name}!")?;
//! assert!(result.diagnostics.is_empty());
//! assert!(result.cst.node_count() > 0);
//! # Ok::<(), ox_mf2_parser::ParseError>(())
//! ```
//!
//! [message-format-wg]: https://github.com/unicode-org/message-format-wg

#![doc(html_root_url = "https://docs.rs/ox_mf2_parser/0.14.0-alpha.10")]

mod api;
mod diagnostic;
mod error;
mod parser;
mod scanner;
mod semantic;
pub mod snapshot;
mod source;
mod span;
mod syntax_kind;
mod tables;
mod view;
mod workspace;

pub use api::{
    parse_batch, parse_message, parse_source, parse_source_session, BatchExecution, BatchParseItem,
    BatchParseOptions, BatchParseResult, ParseInput, ParseOptions, ParseResult, ParseSessionResult,
};
pub use diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticIter, DiagnosticLabel, DiagnosticRef, DiagnosticSeverity,
    DiagnosticView,
};
pub use error::{
    ox_mf2_error_code_name, ox_mf2_error_domain, BatchParseError, BindingValidationErrorCode,
    InitializationErrorCode, OxMf2ErrorCode, OxMf2ErrorDomain, ParseError, ParseErrorCode,
    ParseResource, SourceTextErrorCode, OX_MF2_API_ERROR_MIN, OX_MF2_BINDING_VALIDATION_ERROR_MAX,
    OX_MF2_BINDING_VALIDATION_ERROR_MIN, OX_MF2_DECODE_ERROR_MAX, OX_MF2_DECODE_ERROR_MIN,
    OX_MF2_INITIALIZATION_ERROR_MAX, OX_MF2_INITIALIZATION_ERROR_MIN, OX_MF2_PARSE_ERROR_MAX,
    OX_MF2_PARSE_ERROR_MIN, OX_MF2_SNAPSHOT_WRITE_ERROR_MAX, OX_MF2_SNAPSHOT_WRITE_ERROR_MIN,
    OX_MF2_SOURCE_TEXT_ERROR_MAX, OX_MF2_SOURCE_TEXT_ERROR_MIN,
};
pub use semantic::{MessageMode, SemanticMessageKind, SemanticModel, SemanticView};
pub use snapshot::{
    decode_snapshot, decode_snapshot_owned, parse_batch_result_to_snapshot,
    parse_batch_to_snapshot, parse_message_to_snapshot, parse_result_to_snapshot,
    parse_session_to_snapshot, parse_source_to_snapshot, BatchSnapshotResult, DecodeError,
    DecodeErrorCode, RootId, SectionKind, SnapshotOptions, SnapshotResult, SnapshotSourceMetadata,
    SnapshotView, SnapshotViewOwned, SnapshotWriteError, SnapshotWriteErrorCode,
    SourceTextUnavailable,
};
pub use source::{SourceFile, SourceFileInput, SourceLocation, SourceStore, SourceStoreError};
pub use span::{EdgeId, NodeId, SourceId, Span, TokenId, TriviaId, NONE_U32};
pub use syntax_kind::SyntaxKind;
pub use tables::{CstCapacity, CstTables};
pub use view::{
    CstChild, CstChildren, CstNodeTokens, CstNodeView, CstTokenView, CstTriviaRange, CstTriviaView,
    CstView,
};
pub use workspace::{ParseCapacity, ParseWorkspace};
