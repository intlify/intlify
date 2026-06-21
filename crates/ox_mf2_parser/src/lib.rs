// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! ox-mf2 Phase 1 MF2 parser core.
//!
//! This crate provides the Rust core for parsing
//! [Unicode MessageFormat 2.0][message-format-wg] messages. The Phase 1 scope
//! is intentionally narrow: build a fast, recovering, lossless CST and an
//! optional `SemanticModel` that later phases (Binary AST snapshot, formatter,
//! linter, compiler, language bindings) can consume.
//!
//! The Phase 1 public surface is described in
//! `design/002-ox-mf2-phase-1-rust-parser-design.md` and the implementation
//! plan in `.plans/002-ox-mf2-phase-1-rust-parser-implementation.md`.
//!
//! [message-format-wg]: https://github.com/unicode-org/message-format-wg

#![doc(html_root_url = "https://docs.rs/ox_mf2_parser/0.0.0")]

pub mod api;
pub mod diagnostic;
pub mod parser;
pub mod scanner;
pub mod semantic;
pub mod source;
pub mod span;
pub mod syntax_kind;
pub mod tables;
pub mod view;
pub mod workspace;

pub use api::{
    parse_batch, parse_message, parse_source, parse_source_session, BatchExecution,
    BatchParseItem, BatchParseOptions, BatchParseResult, ParseInput, ParseOptions, ParseResult,
    ParseSessionResult,
};
pub use diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticIter, DiagnosticLabel, DiagnosticRef,
    DiagnosticSeverity, DiagnosticView,
};
pub use scanner::ScannerState;
pub use semantic::{MessageMode, SemanticMessageKind, SemanticModel, SemanticView};
pub use source::{SourceFile, SourceFileInput, SourceLocation, SourceStore};
pub use span::{EdgeId, NodeId, SourceId, Span, TokenId, TriviaId, NONE_U32};
pub use syntax_kind::SyntaxKind;
pub use tables::{CstCapacity, CstTables};
pub use view::{
    CstChild, CstChildren, CstNodeTokens, CstNodeView, CstTokenView, CstTriviaRange,
    CstTriviaView, CstView,
};
pub use workspace::{ParseCapacity, ParseWorkspace};
