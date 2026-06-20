//! Recursive-descent MF2 parser.
//!
//! The parser drives an internal [`crate::scanner::Cursor`] over the source
//! bytes, commits accepted productions through [`crate::tables::CstBuilder`],
//! and emits diagnostics through [`crate::diagnostic::DiagnosticSink`]. It
//! does not build an intermediate typed AST; nodes are appended directly to
//! the flat tables in the workspace.
//!
//! Grammar productions follow `refers/message-format-wg/spec/message.abnf`
//! and are split into helpers prefixed with `parse_` so the spec → code map
//! stays direct. Each helper expects the cursor positioned at its first
//! significant byte and leaves the cursor positioned at the next byte.
//!
//! Recovery (Milestone 7) snapshots the cursor + builder lengths and rolls
//! back on failure; the resulting CST always has a root and surviving
//! malformed input is held in `Error` / `Missing` nodes.

#![allow(clippy::while_let_loop)] // explicit loop {} + Some(_) else-break is
                                   // clearer than nested while-let across the
                                   // many byte/codepoint dual paths in the
                                   // hot parsing routines.

use crate::api::ParseOptions;
use crate::diagnostic::{DiagnosticCode, DiagnosticLabelRecord, DiagnosticSink};
use crate::scanner::{
    detect_keyword, is_bidi_char, is_ws_byte, is_ws_char, scan_name, scan_quoted_text_run,
    scan_text_run, scan_unquoted_literal, ScannerState, TriviaMode,
};
use crate::scanner::Cursor;
use crate::semantic::MessageMode;
use crate::source::SourceStore;
use crate::span::{NodeId, SourceId, Span, TokenId};
use crate::syntax_kind::SyntaxKind;
use crate::tables::{BuilderLengths, CstBuilder};
use crate::workspace::ParseWorkspace;

/// Speculative parser state used by recovery points.
///
/// A checkpoint captures four things atomically: the scanner offset, the
/// builder lengths (so any nodes / edges / tokens / trivia pushed during a
/// failed attempt can be truncated), the diagnostic length (so cascades
/// from a discarded branch do not surface to the caller), and the
/// `pending_trivia_start` (so the next token's leading-trivia anchor is
/// restored to its pre-speculation value).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Checkpoint {
    pub builder: BuilderLengths,
    pub diagnostic_len: u32,
    pub scanner_state: ScannerState,
    pub pending_trivia_start: u32,
}

/// Result of [`Parser::eat_trivia`]: how many `ws` and `bidi` runs were
/// consumed. Lets callers enforce `s = *bidi ws o` by checking `ws_runs >= 1`
/// without re-scanning the source.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct TriviaConsumed {
    pub ws_runs: u32,
    pub bidi_runs: u32,
}

impl TriviaConsumed {
    /// True if at least one `ws` was seen, satisfying the `s` requirement.
    #[inline]
    pub fn satisfies_required_s(self) -> bool {
        self.ws_runs > 0
    }
}

/// Parser entry point invoked by every owned and borrowed-session API.
pub(crate) fn run_parse(
    sources: &SourceStore,
    source_id: SourceId,
    workspace: &mut ParseWorkspace,
    options: ParseOptions,
) {
    let Some(file) = sources.get(source_id) else {
        return;
    };

    let cursor = Cursor::new(&file.text);

    // Temporary label-record buffer. Phase 1 never emits labels, but the
    // sink keeps the API ready for Milestone 8 semantic diagnostics.
    let mut labels: Vec<DiagnosticLabelRecord> = Vec::new();

    let mut builder = CstBuilder::new();
    // Move pre-allocated tables AND staging stacks out of the workspace into
    // the builder so the parser can grow them without aliasing trouble; we
    // swap them back when parsing finishes. Keeping the staging stacks in
    // the workspace lets repeated parses reuse their capacity.
    core::mem::swap(&mut builder.tables, &mut workspace.parser.tables);
    core::mem::swap(&mut builder.pending_edges, &mut workspace.parser.pending_edges);
    core::mem::swap(&mut builder.frame_starts, &mut workspace.parser.frame_starts);

    {
        let sink = DiagnosticSink::new(&mut workspace.parser.diagnostics, &mut labels);
        let mut parser = Parser {
            cursor,
            source: source_id,
            builder: &mut builder,
            diagnostics: sink,
            options,
            pending_trivia_start: 0,
        };
        parser.parse_root();
    }

    // Restore everything to the workspace.
    core::mem::swap(&mut builder.tables, &mut workspace.parser.tables);
    core::mem::swap(&mut builder.pending_edges, &mut workspace.parser.pending_edges);
    core::mem::swap(&mut builder.frame_starts, &mut workspace.parser.frame_starts);
}

struct Parser<'src, 'ws> {
    cursor: Cursor<'src>,
    source: SourceId,
    builder: &'ws mut CstBuilder,
    diagnostics: DiagnosticSink<'ws>,
    options: ParseOptions,
    /// Index into the trivia table where the next token's leading trivia
    /// begins. Updated each time we scan a `ws` / `bidi` run.
    pending_trivia_start: u32,
}

impl Parser<'_, '_> {
    /// Snapshot the cursor, builder lengths, diagnostic length, and the
    /// pending-trivia anchor so a speculative parse can be rolled back
    /// without leaking partial nodes, cascading diagnostics, or duplicated
    /// trivia records.
    pub(crate) fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            builder: self.builder.lengths(),
            diagnostic_len: self.diagnostics.records.len() as u32,
            scanner_state: self.cursor.state(),
            pending_trivia_start: self.pending_trivia_start,
        }
    }

    /// Restore state captured by [`Self::checkpoint`]. Truncates any trivia
    /// committed during the speculative branch so the next attempt re-scans
    /// from the same byte without duplicating records.
    pub(crate) fn rollback(&mut self, cp: Checkpoint) {
        self.cursor.restore(cp.scanner_state);
        self.builder.rollback_to(cp.builder);
        self.diagnostics
            .records
            .truncate(cp.diagnostic_len as usize);
        self.pending_trivia_start = cp.pending_trivia_start;
    }
}

impl Parser<'_, '_> {
    // ───────────────────────── entry point ─────────────────────────────

    fn parse_root(&mut self) {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Root, start);

        // Mode detection happens by peeking past leading trivia. The parser
        // does not commit trivia to the CST until it commits its first
        // token; for the simple-message branch leading whitespace becomes
        // part of `text`, so we restore the cursor before dispatching.
        let mode = self.detect_message_mode();
        let message = match mode {
            MessageMode::Simple => self.parse_simple_message(),
            MessageMode::Complex => self.parse_complex_message(),
        };
        self.builder.push_node_edge(message);

        // Any trailing bytes that the parser could not consume become an
        // Error node so source fidelity stays intact.
        if !self.cursor.is_eof() {
            let err_start = self.cursor.offset();
            let pending_err = self.builder.start_node(SyntaxKind::Error, err_start);
            // Slurp the rest as raw text-run for recovery.
            let _ = scan_text_run(&mut self.cursor);
            // If the text-run didn't move (delimiter), advance one byte so
            // we don't loop indefinitely.
            if self.cursor.offset() == err_start && !self.cursor.is_eof() {
                let _ = self.cursor.bump_byte();
            }
            self.diagnostics.push(
                self.source,
                Span::new(err_start, self.cursor.offset()),
                DiagnosticCode::UnexpectedToken,
            );
            let err_node = self.builder.finish_node(pending_err, self.cursor.offset());
            self.builder.push_node_edge(err_node);
        }

        let end = self.cursor.offset();
        let _root = self.builder.finish_node(pending, end);
    }

    // ───────────────────────── mode detection ──────────────────────────

    /// Decide which mode the message is in by peeking past optional `o`.
    /// The cursor is restored before returning so each branch can decide how
    /// to attribute the leading whitespace (text vs trivia).
    fn detect_message_mode(&self) -> MessageMode {
        let mut peek = self.cursor;
        loop {
            let Some(b) = peek.peek_byte() else { break };
            if b < 0x80 {
                if is_ws_byte(b) {
                    peek.set_offset(peek.offset() + 1);
                    continue;
                }
                break;
            }
            let Some((c, len)) = peek.peek_char() else { break };
            if is_ws_char(c) || is_bidi_char(c) {
                peek.set_offset(peek.offset() + len);
                continue;
            }
            break;
        }

        if peek.is_eof() {
            return MessageMode::Simple;
        }

        if peek.try_eat_at_offset(b"{{") {
            return MessageMode::Complex;
        }
        if detect_keyword(&peek).is_some() {
            return MessageMode::Complex;
        }
        MessageMode::Simple
    }

    // ───────────────────────── simple message ──────────────────────────

    fn parse_simple_message(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::SimpleMessage, start);
        let pattern = self.parse_pattern(PatternMode::Simple);
        self.builder.push_node_edge(pattern);
        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    // ───────────────────────── pattern ─────────────────────────────────

    fn parse_pattern(&mut self, mode: PatternMode) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Pattern, start);

        loop {
            match self.cursor.peek_byte() {
                None => break,
                Some(b'{') => {
                    if mode.is_quoted() && self.cursor.peek_byte_at(1) == Some(b'}') {
                        // `{}` cannot appear in a quoted pattern; let the
                        // outer body handle the boundary.
                    }
                    let placeholder = self.parse_placeholder();
                    self.builder.push_node_edge(placeholder);
                }
                Some(b'}') if mode.is_quoted() => break,
                Some(_) => {
                    let text_node = self.parse_text();
                    if text_node.is_none() {
                        break;
                    }
                    let id = text_node.unwrap();
                    self.builder.push_node_edge(id);
                }
            }
        }

        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    // ───────────────────────── text ────────────────────────────────────

    fn parse_text(&mut self) -> Option<NodeId> {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Text, start);

        let mut produced = false;
        loop {
            // text-char run
            let span = scan_text_run(&mut self.cursor);
            if !span.is_empty() {
                let id = self.builder.push_token(
                    SyntaxKind::TextToken,
                    self.source,
                    span,
                    self.pending_trivia_start,
                    0,
                    0,
                );
                self.builder.push_token_edge(id);
                produced = true;
            }

            // escaped-char
            if self.try_consume_escape() {
                produced = true;
                continue;
            }
            break;
        }

        let end = self.cursor.offset();
        if !produced {
            // Rolling back a pushed-but-empty node would invalidate edge
            // indexes, so we let an empty Text node survive — it stays valid
            // and downstream consumers can ignore zero-length nodes.
            let _ = self.builder.finish_node(pending, end);
            return None;
        }
        Some(self.builder.finish_node(pending, end))
    }

    /// Consume an `escaped-char` (`\\`, `\{`, `\|`, `\}`). Returns true if
    /// one was consumed. Emits an `InvalidEscape` diagnostic for any other
    /// `\X`, but still consumes the two bytes so the parser makes progress.
    fn try_consume_escape(&mut self) -> bool {
        if self.cursor.peek_byte() != Some(b'\\') {
            return false;
        }
        let start = self.cursor.offset();
        let _ = self.cursor.bump_byte(); // '\\'
        match self.cursor.peek_byte() {
            Some(b'\\' | b'{' | b'|' | b'}') => {
                let _ = self.cursor.bump_byte();
                let span = Span::new(start, self.cursor.offset());
                let id = self.builder.push_token(
                    SyntaxKind::EscapeToken,
                    self.source,
                    span,
                    self.pending_trivia_start,
                    0,
                    0,
                );
                self.builder.push_token_edge(id);
            }
            _ => {
                self.diagnostics.push(
                    self.source,
                    Span::new(start, self.cursor.offset()),
                    DiagnosticCode::InvalidEscape,
                );
            }
        }
        true
    }

    // ───────────────────────── placeholder ─────────────────────────────

    fn parse_placeholder(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Placeholder, start);

        // We do not yet recognise markup; the parser routes everything
        // through `parse_expression`. Markup support lands later in this
        // milestone after the core expression grammar is in.
        let inner = self.parse_expression();
        self.builder.push_node_edge(inner);
        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    // ───────────────────────── expression ──────────────────────────────

    fn parse_expression(&mut self) -> NodeId {
        let start = self.cursor.offset();
        // Consume opening `{`.
        let lbrace_span_start = start;
        let lbrace = if self.cursor.peek_byte() == Some(b'{') {
            let _ = self.cursor.bump_byte();
            Some(self.commit_token(
                SyntaxKind::LeftBraceToken,
                Span::new(lbrace_span_start, self.cursor.offset()),
            ))
        } else {
            // Missing `{`. The placeholder caller already saw one, so this
            // path is mostly defensive against future markup branching.
            None
        };

        // Optional `o`.
        self.eat_trivia(TriviaMode::Optional);

        // Decide expression flavour based on first significant byte. EOF
        // inside the expression falls through to the literal path so the
        // closing-`}` check below can emit `UnclosedExpression`.
        let kind = match self.cursor.peek_byte() {
            Some(b'$') => SyntaxKind::VariableExpression,
            Some(b':') => SyntaxKind::FunctionExpression,
            Some(b'#' | b'/') => SyntaxKind::Markup,
            Some(_) | None => SyntaxKind::LiteralExpression,
        };

        let pending_expr = self.builder.start_node(kind, start);

        if let Some(tok) = lbrace {
            self.builder.push_token_edge(tok);
        }

        match kind {
            SyntaxKind::VariableExpression => self.parse_variable_expression_body(),
            SyntaxKind::FunctionExpression => self.parse_function_expression_body(),
            SyntaxKind::LiteralExpression => self.parse_literal_expression_body(),
            SyntaxKind::Markup => self.parse_markup_body(),
            _ => unreachable!(),
        }

        // Expect closing `}`.
        self.eat_trivia(TriviaMode::Optional);
        let rbrace_start = self.cursor.offset();
        if self.cursor.peek_byte() == Some(b'}') {
            let _ = self.cursor.bump_byte();
            let tok = self.commit_token(
                SyntaxKind::RightBraceToken,
                Span::new(rbrace_start, self.cursor.offset()),
            );
            self.builder.push_token_edge(tok);
        } else {
            // Unclosed expression. Emit a diagnostic anchored at the opener.
            self.diagnostics.push(
                self.source,
                Span::new(start, self.cursor.offset()),
                DiagnosticCode::UnclosedExpression,
            );
            // Insert a Missing node child so traversal continues.
            let missing_pending = self.builder.start_node(SyntaxKind::Missing, rbrace_start);
            let id = self
                .builder
                .finish_node(missing_pending, self.cursor.offset());
            self.builder.push_node_edge(id);
        }

        let end = self.cursor.offset();
        self.builder.finish_node(pending_expr, end)
    }

    fn parse_variable_expression_body(&mut self) {
        let var = self.parse_variable();
        self.builder.push_node_edge(var);
        // Optional [s function]
        self.maybe_parse_function();
        // *(s attribute)
        self.parse_attributes_zero_or_more();
    }

    fn parse_function_expression_body(&mut self) {
        let func = self.parse_function();
        self.builder.push_node_edge(func);
        self.parse_attributes_zero_or_more();
    }

    fn parse_literal_expression_body(&mut self) {
        let lit = self.parse_literal();
        self.builder.push_node_edge(lit);
        self.maybe_parse_function();
        self.parse_attributes_zero_or_more();
    }

    /// `markup = "{" o "#" identifier *(s option) *(s attribute) o ["/"] "}"`
    /// or `"{" o "/" identifier *(s option) *(s attribute) o "}"`.
    /// The opening `{` and the closing `}` are handled by `parse_expression`;
    /// this body covers the sigil, identifier, options, attributes, and the
    /// optional standalone-marker `/`.
    fn parse_markup_body(&mut self) {
        // Sigil: `#` for open / standalone, `/` for close.
        let is_open = match self.cursor.peek_byte() {
            Some(b'#') => {
                let sigil_start = self.cursor.offset();
                let _ = self.cursor.bump_byte();
                let tok = self.commit_token(
                    SyntaxKind::HashToken,
                    Span::new(sigil_start, self.cursor.offset()),
                );
                self.builder.push_token_edge(tok);
                true
            }
            Some(b'/') => {
                let sigil_start = self.cursor.offset();
                let _ = self.cursor.bump_byte();
                let tok = self.commit_token(
                    SyntaxKind::SlashToken,
                    Span::new(sigil_start, self.cursor.offset()),
                );
                self.builder.push_token_edge(tok);
                false
            }
            _ => true,
        };

        let ident = self.parse_identifier();
        self.builder.push_node_edge(ident);

        // *(s option) — each option requires `s` before it.
        loop {
            let cp = self.checkpoint();
            let consumed = self.eat_trivia(TriviaMode::Required);
            if self.peek_option_start() {
                if !consumed.satisfies_required_s() {
                    let at = self.cursor.offset();
                    self.diagnostics.push(
                        self.source,
                        Span::new(at, at),
                        DiagnosticCode::MissingRequiredWhitespace,
                    );
                }
                let opt = self.parse_option();
                self.builder.push_node_edge(opt);
            } else {
                self.rollback(cp);
                break;
            }
        }

        // *(s attribute)
        self.parse_attributes_zero_or_more();

        // [o "/"] — only valid on the open form (turns it into standalone).
        if is_open {
            let cp = self.checkpoint();
            self.eat_trivia(TriviaMode::Optional);
            if self.cursor.peek_byte() == Some(b'/') {
                let slash_start = self.cursor.offset();
                let _ = self.cursor.bump_byte();
                let tok = self.commit_token(
                    SyntaxKind::SlashToken,
                    Span::new(slash_start, self.cursor.offset()),
                );
                self.builder.push_token_edge(tok);
            } else {
                self.rollback(cp);
            }
        }
    }

    // ───────────────────────── variable / function / option / attribute ─

    fn parse_variable(&mut self) -> NodeId {
        let var_start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Variable, var_start);

        if self.cursor.peek_byte() == Some(b'$') {
            let dollar_start = self.cursor.offset();
            let _ = self.cursor.bump_byte();
            let tok = self.commit_token(
                SyntaxKind::DollarToken,
                Span::new(dollar_start, self.cursor.offset()),
            );
            self.builder.push_token_edge(tok);
        }
        let name = self.parse_name();
        self.builder.push_node_edge(name);

        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn maybe_parse_function(&mut self) {
        // `[s function]` — the leading `s` requires at least one `ws`. When a
        // `:` follows without it (e.g. `{$x:number}`), emit a diagnostic but
        // still parse the function to keep recovery progressing.
        let cp = self.checkpoint();
        let consumed = self.eat_trivia(TriviaMode::Required);
        if self.cursor.peek_byte() == Some(b':') {
            if !consumed.satisfies_required_s() {
                let at = self.cursor.offset();
                self.diagnostics.push(
                    self.source,
                    Span::new(at, at),
                    DiagnosticCode::MissingRequiredWhitespace,
                );
            }
            let func = self.parse_function();
            self.builder.push_node_edge(func);
        } else {
            self.rollback(cp);
        }
    }

    fn parse_function(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Function, start);
        if self.cursor.peek_byte() == Some(b':') {
            let colon_start = self.cursor.offset();
            let _ = self.cursor.bump_byte();
            let tok = self.commit_token(
                SyntaxKind::ColonToken,
                Span::new(colon_start, self.cursor.offset()),
            );
            self.builder.push_token_edge(tok);
        }
        let ident = self.parse_identifier();
        self.builder.push_node_edge(ident);
        // *(s option) — each option requires `s` before it.
        loop {
            let cp = self.checkpoint();
            let consumed = self.eat_trivia(TriviaMode::Required);
            if self.peek_option_start() {
                if !consumed.satisfies_required_s() {
                    let at = self.cursor.offset();
                    self.diagnostics.push(
                        self.source,
                        Span::new(at, at),
                        DiagnosticCode::MissingRequiredWhitespace,
                    );
                }
                let opt = self.parse_option();
                self.builder.push_node_edge(opt);
            } else {
                self.rollback(cp);
                break;
            }
        }
        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn peek_option_start(&self) -> bool {
        // option starts with `identifier`, which starts with name-start
        // (or namespace name-start). The ASCII fast path covers A-Z, a-z,
        // +, _, plus optional bidi.
        let Some(b) = self.cursor.peek_byte() else {
            return false;
        };
        if b < 0x80 {
            matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'+' | b'_')
        } else {
            // Could be a bidi mark introducing the identifier — accept and
            // let the scanner decide.
            true
        }
    }

    fn parse_option(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Option, start);
        let ident = self.parse_identifier();
        self.builder.push_node_edge(ident);
        self.eat_trivia(TriviaMode::Optional);
        if self.cursor.peek_byte() == Some(b'=') {
            let eq_start = self.cursor.offset();
            let _ = self.cursor.bump_byte();
            let tok = self.commit_token(
                SyntaxKind::EqualsToken,
                Span::new(eq_start, self.cursor.offset()),
            );
            self.builder.push_token_edge(tok);
            self.eat_trivia(TriviaMode::Optional);
            if self.cursor.peek_byte() == Some(b'$') {
                let v = self.parse_variable();
                self.builder.push_node_edge(v);
            } else {
                let l = self.parse_literal();
                self.builder.push_node_edge(l);
            }
        } else {
            self.diagnostics.push(
                self.source,
                Span::new(start, self.cursor.offset()),
                DiagnosticCode::UnexpectedToken,
            );
        }
        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn parse_attributes_zero_or_more(&mut self) {
        // `*(s attribute)` — each attribute requires `s` before it.
        loop {
            let cp = self.checkpoint();
            let consumed = self.eat_trivia(TriviaMode::Required);
            if self.cursor.peek_byte() == Some(b'@') {
                if !consumed.satisfies_required_s() {
                    let at = self.cursor.offset();
                    self.diagnostics.push(
                        self.source,
                        Span::new(at, at),
                        DiagnosticCode::MissingRequiredWhitespace,
                    );
                }
                let attr = self.parse_attribute();
                self.builder.push_node_edge(attr);
            } else {
                self.rollback(cp);
                break;
            }
        }
    }

    fn parse_attribute(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Attribute, start);

        if self.cursor.peek_byte() == Some(b'@') {
            let at_start = self.cursor.offset();
            let _ = self.cursor.bump_byte();
            let tok = self.commit_token(
                SyntaxKind::AtToken,
                Span::new(at_start, self.cursor.offset()),
            );
            self.builder.push_token_edge(tok);
        }
        let ident = self.parse_identifier();
        self.builder.push_node_edge(ident);

        // Optional `[o "=" o literal]` — speculate to see if there's an `=`.
        let cp = self.checkpoint();
        self.eat_trivia(TriviaMode::Optional);
        if self.cursor.peek_byte() == Some(b'=') {
            let eq_start = self.cursor.offset();
            let _ = self.cursor.bump_byte();
            let tok = self.commit_token(
                SyntaxKind::EqualsToken,
                Span::new(eq_start, self.cursor.offset()),
            );
            self.builder.push_token_edge(tok);
            self.eat_trivia(TriviaMode::Optional);
            let lit = self.parse_literal();
            self.builder.push_node_edge(lit);
        } else {
            self.rollback(cp);
        }

        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    // ───────────────────────── literal / name / identifier ─────────────

    fn parse_literal(&mut self) -> NodeId {
        if self.cursor.peek_byte() == Some(b'|') {
            self.parse_quoted_literal()
        } else {
            self.parse_unquoted_literal()
        }
    }

    fn parse_unquoted_literal(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::UnquotedLiteral, start);
        // unquoted-literal = 1*name-char (NOT name = [bidi] name-start *name-char [bidi]).
        // Allow DIGIT-leading values like `2` in `option=2`.
        if let Some(span) = scan_unquoted_literal(&mut self.cursor) {
            let tok = self.commit_token(SyntaxKind::NameToken, span);
            self.builder.push_token_edge(tok);
        } else {
            self.diagnostics.push(
                self.source,
                Span::new(start, self.cursor.offset()),
                DiagnosticCode::UnexpectedToken,
            );
        }
        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn parse_quoted_literal(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::QuotedLiteral, start);

        let pipe_start = self.cursor.offset();
        let _ = self.cursor.bump_byte();
        let tok = self.commit_token(
            SyntaxKind::PipeToken,
            Span::new(pipe_start, self.cursor.offset()),
        );
        self.builder.push_token_edge(tok);

        loop {
            let span = scan_quoted_text_run(&mut self.cursor);
            if !span.is_empty() {
                let id = self.builder.push_token(
                    SyntaxKind::QuotedTextToken,
                    self.source,
                    span,
                    self.pending_trivia_start,
                    0,
                    0,
                );
                self.builder.push_token_edge(id);
            }
            if self.cursor.peek_byte() == Some(b'\\') {
                let _ = self.try_consume_escape();
                continue;
            }
            break;
        }

        if self.cursor.peek_byte() == Some(b'|') {
            let pipe_start = self.cursor.offset();
            let _ = self.cursor.bump_byte();
            let tok = self.commit_token(
                SyntaxKind::PipeToken,
                Span::new(pipe_start, self.cursor.offset()),
            );
            self.builder.push_token_edge(tok);
        } else {
            self.diagnostics.push(
                self.source,
                Span::new(start, self.cursor.offset()),
                DiagnosticCode::UnclosedQuotedLiteral,
            );
        }

        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn parse_name(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Name, start);
        if let Some(span) = scan_name(&mut self.cursor) {
            let tok = self.commit_token(SyntaxKind::NameToken, span);
            self.builder.push_token_edge(tok);
        } else {
            self.diagnostics.push(
                self.source,
                Span::new(start, self.cursor.offset()),
                DiagnosticCode::UnexpectedToken,
            );
            let m = self.builder.start_node(SyntaxKind::Missing, start);
            let id = self.builder.finish_node(m, self.cursor.offset());
            self.builder.push_node_edge(id);
        }
        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn parse_identifier(&mut self) -> NodeId {
        // `identifier = [namespace ":"] name` — when the namespace separator
        // `:` is present, the trailing `name` is mandatory; emit a diagnostic
        // and insert a Missing anchor when it is absent so downstream
        // consumers see the missing-name boundary explicitly.
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Identifier, start);
        if let Some(span) = scan_name(&mut self.cursor) {
            let tok = self.commit_token(SyntaxKind::NameToken, span);
            self.builder.push_token_edge(tok);
            if self.cursor.peek_byte() == Some(b':') {
                let colon_start = self.cursor.offset();
                let _ = self.cursor.bump_byte();
                let tok = self.commit_token(
                    SyntaxKind::ColonToken,
                    Span::new(colon_start, self.cursor.offset()),
                );
                self.builder.push_token_edge(tok);
                if let Some(span) = scan_name(&mut self.cursor) {
                    let tok = self.commit_token(SyntaxKind::NameToken, span);
                    self.builder.push_token_edge(tok);
                } else {
                    let missing_at = self.cursor.offset();
                    self.diagnostics.push(
                        self.source,
                        Span::new(colon_start, missing_at),
                        DiagnosticCode::MissingIdentifierName,
                    );
                    let m = self.builder.start_node(SyntaxKind::Missing, missing_at);
                    let id = self.builder.finish_node(m, missing_at);
                    self.builder.push_node_edge(id);
                }
            }
        } else {
            self.diagnostics.push(
                self.source,
                Span::new(start, self.cursor.offset()),
                DiagnosticCode::UnexpectedToken,
            );
        }
        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    // ───────────────────────── complex message (stub) ──────────────────

    fn parse_complex_message(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::ComplexMessage, start);

        // Leading `o`
        self.eat_trivia(TriviaMode::Optional);

        // Declarations *(declaration o). `.match` is part of complex-body,
        // not a declaration — break on it so parse_complex_body can route it.
        loop {
            match detect_keyword(&self.cursor) {
                Some(crate::scanner::KeywordHit::Input | crate::scanner::KeywordHit::Local) => {
                    let kw = detect_keyword(&self.cursor).unwrap();
                    let decl = self.parse_declaration(kw);
                    self.builder.push_node_edge(decl);
                    self.eat_trivia(TriviaMode::Optional);
                }
                _ => break,
            }
        }

        // Complex body
        let body = self.parse_complex_body();
        self.builder.push_node_edge(body);

        // Trailing `o`
        self.eat_trivia(TriviaMode::Optional);

        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn parse_declaration(&mut self, kw: crate::scanner::KeywordHit) -> NodeId {
        use crate::scanner::KeywordHit;
        let start = self.cursor.offset();
        let kind = match kw {
            KeywordHit::Input => SyntaxKind::InputDeclaration,
            KeywordHit::Local => SyntaxKind::LocalDeclaration,
            KeywordHit::Match => return self.parse_match_body_recovered(start),
        };
        let pending = self.builder.start_node(kind, start);

        // Consume the keyword.
        let kw_start = self.cursor.offset();
        let kw_kind = match kw {
            KeywordHit::Input => SyntaxKind::InputKeyword,
            KeywordHit::Local => SyntaxKind::LocalKeyword,
            KeywordHit::Match => unreachable!(),
        };
        self.cursor.set_offset(kw_start + kw.length());
        let tok = self.commit_token(kw_kind, Span::new(kw_start, self.cursor.offset()));
        self.builder.push_token_edge(tok);

        match kw {
            KeywordHit::Input => {
                self.eat_trivia(TriviaMode::Optional);
                // expect variable-expression — `{ ... }` whose content is a variable
                if self.cursor.peek_byte() == Some(b'{') {
                    let expr = self.parse_placeholder();
                    self.builder.push_node_edge(expr);
                } else {
                    self.diagnostics.push(
                        self.source,
                        Span::new(start, self.cursor.offset()),
                        DiagnosticCode::UnexpectedToken,
                    );
                }
            }
            KeywordHit::Local => {
                // `.local s variable` — `s` is required.
                let s_at = self.cursor.offset();
                let consumed = self.eat_trivia(TriviaMode::Required);
                if !consumed.satisfies_required_s() {
                    self.diagnostics.push(
                        self.source,
                        Span::new(s_at, s_at),
                        DiagnosticCode::MissingRequiredWhitespace,
                    );
                }
                let var = self.parse_variable();
                self.builder.push_node_edge(var);
                self.eat_trivia(TriviaMode::Optional);
                if self.cursor.peek_byte() == Some(b'=') {
                    let eq_start = self.cursor.offset();
                    let _ = self.cursor.bump_byte();
                    let tok = self.commit_token(
                        SyntaxKind::EqualsToken,
                        Span::new(eq_start, self.cursor.offset()),
                    );
                    self.builder.push_token_edge(tok);
                } else {
                    self.diagnostics.push(
                        self.source,
                        Span::new(start, self.cursor.offset()),
                        DiagnosticCode::UnexpectedToken,
                    );
                }
                self.eat_trivia(TriviaMode::Optional);
                if self.cursor.peek_byte() == Some(b'{') {
                    let expr = self.parse_placeholder();
                    self.builder.push_node_edge(expr);
                }
            }
            KeywordHit::Match => unreachable!(),
        }

        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn parse_match_body_recovered(&mut self, start: u32) -> NodeId {
        let pending = self.builder.start_node(SyntaxKind::Matcher, start);
        let kw_start = self.cursor.offset();
        self.cursor.set_offset(kw_start + 6); // ".match" length
        let tok = self.commit_token(
            SyntaxKind::MatchKeyword,
            Span::new(kw_start, self.cursor.offset()),
        );
        self.builder.push_token_edge(tok);

        // 1*(s selector) — each selector requires `s` before it.
        loop {
            let cp = self.checkpoint();
            let consumed = self.eat_trivia(TriviaMode::Required);
            if self.cursor.peek_byte() == Some(b'$') {
                if !consumed.satisfies_required_s() {
                    let at = self.cursor.offset();
                    self.diagnostics.push(
                        self.source,
                        Span::new(at, at),
                        DiagnosticCode::MissingRequiredWhitespace,
                    );
                }
                let sel_start = self.cursor.offset();
                let sel_pending = self.builder.start_node(SyntaxKind::Selector, sel_start);
                let var = self.parse_variable();
                self.builder.push_node_edge(var);
                let sel_end = self.cursor.offset();
                let sel_id = self.builder.finish_node(sel_pending, sel_end);
                self.builder.push_node_edge(sel_id);
            } else {
                self.rollback(cp);
                break;
            }
        }

        // variant *(o variant) — `o` between variants is optional.
        loop {
            let cp = self.checkpoint();
            self.eat_trivia(TriviaMode::Optional);
            if self.peek_variant_start() {
                let variant = self.parse_variant();
                self.builder.push_node_edge(variant);
            } else {
                self.rollback(cp);
                break;
            }
        }

        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn peek_variant_start(&self) -> bool {
        // variant-key = `*` (catch-all) | quoted-literal (`|...|`) |
        // unquoted-literal (`1*name-char`). `name-char` includes DIGIT,
        // `-`, and `.`, so the lookahead must accept those too — otherwise
        // exact numeric keys like `1 {{one}}` would be rejected.
        let Some(b) = self.cursor.peek_byte() else {
            return false;
        };
        if matches!(b, b'*' | b'|') {
            return true;
        }
        if b < 0x80 {
            // ASCII name-char = ALPHA / DIGIT / `+` / `_` / `-` / `.`.
            return matches!(
                b,
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'_' | b'-' | b'.'
            );
        }
        true
    }

    fn parse_variant(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Variant, start);

        // key *(s key) — adjacent keys require `s`.
        loop {
            let key = self.parse_variant_key();
            self.builder.push_node_edge(key);

            let cp = self.checkpoint();
            let consumed = self.eat_trivia(TriviaMode::Required);
            if !self.peek_variant_key_start() {
                self.rollback(cp);
                break;
            }
            if !consumed.satisfies_required_s() {
                let at = self.cursor.offset();
                self.diagnostics.push(
                    self.source,
                    Span::new(at, at),
                    DiagnosticCode::MissingRequiredWhitespace,
                );
            }
        }

        // o quoted-pattern
        self.eat_trivia(TriviaMode::Optional);
        if self.cursor.peek_byte() == Some(b'{') && self.cursor.peek_byte_at(1) == Some(b'{') {
            let qp = self.parse_quoted_pattern();
            self.builder.push_node_edge(qp);
        } else {
            self.diagnostics.push(
                self.source,
                Span::new(start, self.cursor.offset()),
                DiagnosticCode::InvalidVariantBoundary,
            );
        }

        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn peek_variant_key_start(&self) -> bool {
        // Same coverage as `peek_variant_start` — adjacent variant keys may
        // begin with any `name-char`, not just `name-start`.
        let Some(b) = self.cursor.peek_byte() else {
            return false;
        };
        matches!(
            b,
            b'*' | b'|'
                | b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'_' | b'-' | b'.'
        ) || b >= 0x80
    }

    fn parse_variant_key(&mut self) -> NodeId {
        let start = self.cursor.offset();
        if self.cursor.peek_byte() == Some(b'*') {
            let pending = self.builder.start_node(SyntaxKind::CatchAllKey, start);
            let _ = self.cursor.bump_byte();
            let tok = self.commit_token(
                SyntaxKind::StarToken,
                Span::new(start, self.cursor.offset()),
            );
            self.builder.push_token_edge(tok);
            let end = self.cursor.offset();
            return self.builder.finish_node(pending, end);
        }
        let pending = self.builder.start_node(SyntaxKind::VariantKey, start);
        let lit = self.parse_literal();
        self.builder.push_node_edge(lit);
        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn parse_complex_body(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::ComplexBody, start);
        if self.cursor.peek_byte() == Some(b'{') && self.cursor.peek_byte_at(1) == Some(b'{') {
            let qp = self.parse_quoted_pattern();
            self.builder.push_node_edge(qp);
        } else if matches!(detect_keyword(&self.cursor), Some(crate::scanner::KeywordHit::Match)) {
            let matcher_start = self.cursor.offset();
            let matcher = self.parse_match_body_recovered(matcher_start);
            self.builder.push_node_edge(matcher);
        } else {
            self.diagnostics.push(
                self.source,
                Span::new(start, self.cursor.offset()),
                DiagnosticCode::MissingComplexBody,
            );
        }
        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn parse_quoted_pattern(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::QuotedPattern, start);

        // Opening `{{`
        let open_start = self.cursor.offset();
        self.cursor.set_offset(open_start + 2);
        let tok = self.commit_token(
            SyntaxKind::LeftDoubleBraceToken,
            Span::new(open_start, self.cursor.offset()),
        );
        self.builder.push_token_edge(tok);

        // Inner pattern (stops at `}}` or single `}`)
        let pattern = self.parse_pattern(PatternMode::Quoted);
        self.builder.push_node_edge(pattern);

        // Closing `}}`
        if self.cursor.peek_byte() == Some(b'}') && self.cursor.peek_byte_at(1) == Some(b'}') {
            let close_start = self.cursor.offset();
            self.cursor.set_offset(close_start + 2);
            let tok = self.commit_token(
                SyntaxKind::RightDoubleBraceToken,
                Span::new(close_start, self.cursor.offset()),
            );
            self.builder.push_token_edge(tok);
        } else {
            self.diagnostics.push(
                self.source,
                Span::new(start, self.cursor.offset()),
                DiagnosticCode::UnclosedQuotedPattern,
            );
        }

        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    // ───────────────────────── trivia helpers ──────────────────────────

    /// Scan a run of `ws` / `bidi` characters. When `collect_trivia` is on,
    /// each one is committed as a trivia record and the accumulated leading-
    /// trivia range is attached to whichever token is committed next.
    ///
    /// Returns counts of `ws` and `bidi` consumed so callers can enforce the
    /// `s = *bidi ws o` requirement without re-scanning the source.
    fn eat_trivia(&mut self, mode: TriviaMode) -> TriviaConsumed {
        let _ = mode; // mode currently informational; classifier matches both
        let mut stats = TriviaConsumed::default();
        if !self.options.collect_trivia {
            loop {
                match self.cursor.peek_byte() {
                    None => break,
                    Some(b) if b < 0x80 => {
                        if !is_ws_byte(b) {
                            break;
                        }
                        let _ = self.cursor.bump_byte();
                        stats.ws_runs += 1;
                    }
                    Some(_) => {
                        let Some((c, len)) = self.cursor.peek_char() else { break };
                        if is_ws_char(c) {
                            self.cursor.set_offset(self.cursor.offset() + len);
                            stats.ws_runs += 1;
                        } else if is_bidi_char(c) {
                            self.cursor.set_offset(self.cursor.offset() + len);
                            stats.bidi_runs += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
            return stats;
        }

        let trivia_start = self.builder.lengths().trivia;
        loop {
            let span_start = self.cursor.offset();
            match self.cursor.peek_byte() {
                None => break,
                Some(b) if b < 0x80 => {
                    if !is_ws_byte(b) {
                        break;
                    }
                    let _ = self.cursor.bump_byte();
                    let _ = self.builder.push_trivia(
                        SyntaxKind::WhitespaceTrivia,
                        self.source,
                        Span::new(span_start, self.cursor.offset()),
                    );
                    stats.ws_runs += 1;
                }
                Some(_) => {
                    let Some((c, len)) = self.cursor.peek_char() else { break };
                    if is_ws_char(c) {
                        self.cursor.set_offset(self.cursor.offset() + len);
                        let _ = self.builder.push_trivia(
                            SyntaxKind::WhitespaceTrivia,
                            self.source,
                            Span::new(span_start, self.cursor.offset()),
                        );
                        stats.ws_runs += 1;
                    } else if is_bidi_char(c) {
                        self.cursor.set_offset(self.cursor.offset() + len);
                        let _ = self.builder.push_trivia(
                            SyntaxKind::BidiTrivia,
                            self.source,
                            Span::new(span_start, self.cursor.offset()),
                        );
                        stats.bidi_runs += 1;
                    } else {
                        break;
                    }
                }
            }
        }
        if stats.ws_runs + stats.bidi_runs > 0 {
            self.pending_trivia_start = trivia_start;
        }
        stats
    }

    /// Commit a token whose `first_trivia` / `leading_trivia_count` are
    /// resolved from the trivia accumulated since the last commit.
    fn commit_token(&mut self, kind: SyntaxKind, span: Span) -> TokenId {
        let trivia_len_now = self.builder.lengths().trivia;
        let first_trivia = self.pending_trivia_start;
        let leading_count = trivia_len_now.saturating_sub(first_trivia) as u16;
        let id = self.builder.push_token(
            kind,
            self.source,
            span,
            first_trivia,
            leading_count,
            0,
        );
        self.pending_trivia_start = trivia_len_now;
        id
    }
}

/// Pattern parsing flavour. Quoted patterns stop at `}` while simple
/// patterns run to EOF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatternMode {
    Simple,
    Quoted,
}

impl PatternMode {
    #[inline]
    fn is_quoted(self) -> bool {
        matches!(self, PatternMode::Quoted)
    }
}

