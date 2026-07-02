// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

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
use crate::diagnostic::{DiagnosticCode, DiagnosticSink};
use crate::scanner::Cursor;
use crate::scanner::{
    detect_keyword, is_bidi_char, is_ws_byte, is_ws_char, peek_trivia, scan_name,
    scan_quoted_text_run, scan_text_run, scan_unquoted_literal, PeekTrivia, TriviaMode,
};
use crate::semantic::MessageMode;
use crate::source::SourceStore;
use crate::span::{usize_to_u32, NodeId, SourceId, Span, TokenId};
use crate::syntax_kind::SyntaxKind;
use crate::tables::{CstBuilder, CstEdgeKind};
use crate::workspace::ParseWorkspace;

/// Result of [`Parser::eat_trivia`]: how many `ws` and `bidi` runs were
/// consumed. Returned for parity with [`crate::scanner::PeekTrivia`] —
/// hot-path speculative branches now peek instead of consuming, but the
/// real commit still exposes the counts for any future caller that needs
/// them (e.g. lint passes over committed trivia).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct TriviaConsumed {
    pub ws_runs: u32,
    #[allow(dead_code)] // mirrored from PeekTrivia; reserved for future
    // span-aware `s` validation that distinguishes bidi-
    // only runs from bidi+ws sequences.
    pub bidi_runs: u32,
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

    run_parse_text(&file.text, source_id, workspace, options);
}

/// Parse a borrowed source string directly.
///
/// This is used by one-shot convenience APIs that do not need to retain a
/// `SourceStore` after parsing. Callers that need source metadata,
/// diagnostics mapping across files, or public `CstView` construction should
/// keep using [`run_parse`].
pub(crate) fn run_parse_text(
    source: &str,
    source_id: SourceId,
    workspace: &mut ParseWorkspace,
    options: ParseOptions,
) {
    let cursor = Cursor::new(source);

    let mut builder = CstBuilder::new();
    // Move pre-allocated tables AND staging stacks out of the workspace into
    // the builder so the parser can grow them without aliasing trouble; we
    // swap them back when parsing finishes. Keeping the staging stacks in
    // the workspace lets repeated parses reuse their capacity. `labels` is
    // taken by mutable reference further down rather than swapped because
    // it has no aliasing problem (only the diagnostic sink touches it).
    core::mem::swap(&mut builder.tables, &mut workspace.parser.tables);
    core::mem::swap(
        &mut builder.pending_edges,
        &mut workspace.parser.pending_edges,
    );
    core::mem::swap(
        &mut builder.frame_starts,
        &mut workspace.parser.frame_starts,
    );

    {
        let (diagnostics_buf, labels_buf) = (
            &mut workspace.parser.diagnostics,
            &mut workspace.parser.labels,
        );
        let sink = DiagnosticSink::new(diagnostics_buf, labels_buf);
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
    core::mem::swap(
        &mut builder.pending_edges,
        &mut workspace.parser.pending_edges,
    );
    core::mem::swap(
        &mut builder.frame_starts,
        &mut workspace.parser.frame_starts,
    );
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
            let Some((c, len)) = peek.peek_char() else {
                break;
            };
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
        if peek.peek_byte() == Some(b'.') {
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
                    // In a quoted pattern, `{` opens a placeholder; the
                    // `}}` closing sequence is handled by the outer
                    // `parse_quoted_pattern` after `parse_pattern` returns.
                    // The lookahead at `{}` is informational only — let
                    // `parse_placeholder` produce an empty-expression
                    // diagnostic in that case instead of branching here.
                    let placeholder = self.parse_placeholder();
                    self.builder.push_node_edge(placeholder);
                }
                Some(b'}') if mode.is_quoted() => break,
                Some(_) => {
                    let Some(id) = self.parse_text() else {
                        break;
                    };
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
            let peek = peek_trivia(&self.cursor);
            if !Self::is_option_start_byte(peek.next_byte) {
                break;
            }
            if !peek.satisfies_required_s() {
                let at = peek.end_offset;
                self.diagnostics.push(
                    self.source,
                    Span::new(at, at),
                    DiagnosticCode::MissingRequiredWhitespace,
                );
            }
            // Reuse the just-measured peek instead of re-scanning trivia.
            self.commit_peeked_trivia(peek, TriviaMode::Required);
            let opt = self.parse_option();
            self.builder.push_node_edge(opt);
        }

        // *(s attribute)
        self.parse_attributes_zero_or_more();

        // [o "/"] — only valid on the open form (turns it into standalone).
        // Lookahead past optional `o` so the negative path (no slash) costs
        // nothing beyond a cursor copy.
        if is_open {
            let peek = peek_trivia(&self.cursor);
            if peek.next_byte == Some(b'/') {
                self.commit_peeked_trivia(peek, TriviaMode::Optional);
                let slash_start = self.cursor.offset();
                let _ = self.cursor.bump_byte();
                let tok = self.commit_token(
                    SyntaxKind::SlashToken,
                    Span::new(slash_start, self.cursor.offset()),
                );
                self.builder.push_token_edge(tok);
                let boundary = peek_trivia(&self.cursor);
                if boundary.next_byte == Some(b'}') && boundary.end_offset != self.cursor.offset() {
                    self.diagnostics.push(
                        self.source,
                        Span::new(slash_start, boundary.end_offset),
                        DiagnosticCode::InvalidMarkupBoundary,
                    );
                }
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
        } else {
            self.diagnostics.push(
                self.source,
                Span::new(var_start, var_start),
                DiagnosticCode::UnexpectedToken,
            );
        }
        let name = self.parse_name();
        self.builder.push_node_edge(name);

        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    fn maybe_parse_function(&mut self) {
        // `[s function]` — the leading `s` requires at least one `ws`.
        // Lookahead non-destructively so the common "no function follows"
        // path costs no checkpoint/rollback and no trivia commit.
        let peek = peek_trivia(&self.cursor);
        if peek.next_byte != Some(b':') {
            return;
        }
        if !peek.satisfies_required_s() {
            let at = peek.end_offset;
            self.diagnostics.push(
                self.source,
                Span::new(at, at),
                DiagnosticCode::MissingRequiredWhitespace,
            );
        }
        // Reuse the just-measured peek instead of re-scanning trivia.
        self.commit_peeked_trivia(peek, TriviaMode::Required);
        let func = self.parse_function();
        self.builder.push_node_edge(func);
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
        // *(s option) — each option requires `s` before it. Non-destructive
        // lookahead drops the "no more options" branch off the hot path.
        loop {
            let peek = peek_trivia(&self.cursor);
            if !Self::is_option_start_byte(peek.next_byte) {
                break;
            }
            if !peek.satisfies_required_s() {
                let at = peek.end_offset;
                self.diagnostics.push(
                    self.source,
                    Span::new(at, at),
                    DiagnosticCode::MissingRequiredWhitespace,
                );
            }
            // Reuse the just-measured peek instead of re-scanning trivia.
            self.commit_peeked_trivia(peek, TriviaMode::Required);
            let opt = self.parse_option();
            self.builder.push_node_edge(opt);
        }
        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    #[inline]
    fn is_option_start_byte(b: Option<u8>) -> bool {
        // option starts with `identifier`, which starts with name-start
        // (ASCII fast path) or any non-ASCII byte (deferred to scanner).
        match b {
            Some(b) if b < 0x80 => matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'+' | b'_'),
            Some(_) => true,
            None => false,
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
        // Non-destructive lookahead avoids paying checkpoint/rollback when
        // there is no attribute (the common case).
        loop {
            let peek = peek_trivia(&self.cursor);
            if peek.next_byte != Some(b'@') {
                break;
            }
            if !peek.satisfies_required_s() {
                let at = peek.end_offset;
                self.diagnostics.push(
                    self.source,
                    Span::new(at, at),
                    DiagnosticCode::MissingRequiredWhitespace,
                );
            }
            // Reuse the just-measured peek instead of re-scanning trivia.
            self.commit_peeked_trivia(peek, TriviaMode::Required);
            let attr = self.parse_attribute();
            self.builder.push_node_edge(attr);
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

        // Optional `[o "=" o literal]` — peek past optional `o` to see if
        // there's an `=`, no checkpoint/rollback needed.
        let peek = peek_trivia(&self.cursor);
        if peek.next_byte == Some(b'=') {
            // Reuse the just-measured peek instead of re-scanning trivia.
            self.commit_peeked_trivia(peek, TriviaMode::Optional);
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
                Some(
                    kw @ (crate::scanner::KeywordHit::Input | crate::scanner::KeywordHit::Local),
                ) => {
                    let decl = self.parse_declaration(kw);
                    self.builder.push_node_edge(decl);
                    self.eat_trivia(TriviaMode::Optional);
                }
                None if self.cursor.peek_byte() == Some(b'.') => {
                    let invalid = self.parse_invalid_declaration_start();
                    self.builder.push_node_edge(invalid);
                    self.eat_trivia(TriviaMode::Optional);
                }
                Some(crate::scanner::KeywordHit::Match) | None => break,
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

    fn parse_invalid_declaration_start(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Error, start);

        let _ = self.cursor.bump_byte(); // '.'
        let _ = scan_name(&mut self.cursor);
        let end = self.cursor.offset();
        let tok = self.commit_token(SyntaxKind::TextToken, Span::new(start, end));
        self.builder.push_token_edge(tok);
        self.diagnostics.push(
            self.source,
            Span::new(start, end),
            DiagnosticCode::InvalidDeclarationStart,
        );

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
                    // `input-declaration = input o variable-expression` —
                    // a literal, function, or markup placeholder after
                    // `.input` is a syntax error. The placeholder subtree is
                    // kept so tooling can still inspect the offending value.
                    if !self.placeholder_holds_variable_expression(expr) {
                        let rec = self.builder.tables.node_at(expr);
                        let expr_span = Span::new(rec.span_start, rec.span_end);
                        self.diagnostics.push(
                            self.source,
                            expr_span,
                            DiagnosticCode::InvalidInputDeclaration,
                        );
                    }
                } else {
                    self.diagnostics.push(
                        self.source,
                        Span::new(start, self.cursor.offset()),
                        DiagnosticCode::UnexpectedToken,
                    );
                }
            }
            KeywordHit::Local => {
                // `.local s variable` — `s` is required. Peek first so the
                // diagnostic anchors precisely at the variable's `$`.
                let peek = peek_trivia(&self.cursor);
                if !peek.satisfies_required_s() {
                    let at = peek.end_offset;
                    self.diagnostics.push(
                        self.source,
                        Span::new(at, at),
                        DiagnosticCode::MissingRequiredWhitespace,
                    );
                }
                // Reuse the just-measured peek instead of re-scanning trivia.
                self.commit_peeked_trivia(peek, TriviaMode::Required);
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

    /// `true` when the finished placeholder's expression child is a
    /// `VariableExpression`, i.e. the `.input` value is `{$name ...}`.
    fn placeholder_holds_variable_expression(&self, placeholder: NodeId) -> bool {
        let rec = self.builder.tables.node_at(placeholder);
        self.builder.tables.edges_for(rec).iter().any(|edge| {
            edge.kind == CstEdgeKind::Node as u16
                && self.builder.tables.node_at(NodeId::new(edge.ref_id)).kind
                    == SyntaxKind::VariableExpression as u16
        })
    }

    fn parse_match_body_recovered(&mut self, start: u32) -> NodeId {
        let pending = self.builder.start_node(SyntaxKind::Matcher, start);
        let kw_start = self.cursor.offset();
        self.cursor.set_offset(kw_start + 6); // ".match" length
        let kw_span = Span::new(kw_start, self.cursor.offset());
        let tok = self.commit_token(SyntaxKind::MatchKeyword, kw_span);
        self.builder.push_token_edge(tok);

        // 1*(s selector) — each selector requires `s` before it. Lookahead
        // first so the "no more selectors" exit is checkpoint-free.
        let mut selector_count = 0usize;
        loop {
            let peek = peek_trivia(&self.cursor);
            if peek.next_byte != Some(b'$') {
                break;
            }
            if !peek.satisfies_required_s() {
                let at = peek.end_offset;
                self.diagnostics.push(
                    self.source,
                    Span::new(at, at),
                    DiagnosticCode::MissingRequiredWhitespace,
                );
            }
            // Reuse the just-measured peek instead of re-scanning trivia.
            self.commit_peeked_trivia(peek, TriviaMode::Required);
            let sel_start = self.cursor.offset();
            let sel_pending = self.builder.start_node(SyntaxKind::Selector, sel_start);
            let var = self.parse_variable();
            self.builder.push_node_edge(var);
            let sel_end = self.cursor.offset();
            let sel_id = self.builder.finish_node(sel_pending, sel_end);
            self.builder.push_node_edge(sel_id);
            selector_count += 1;
        }

        let mut variant_count = 0usize;

        // `s variant` — the first variant must be separated from the
        // match-statement by required whitespace.
        let peek = peek_trivia(&self.cursor);
        if Self::is_variant_start_byte(peek.next_byte) {
            if !peek.satisfies_required_s() {
                let at = peek.end_offset;
                self.diagnostics.push(
                    self.source,
                    Span::new(at, at),
                    DiagnosticCode::MissingRequiredWhitespace,
                );
            }
            self.commit_peeked_trivia(peek, TriviaMode::Required);
            let variant = self.parse_variant();
            self.builder.push_node_edge(variant);
            variant_count += 1;
        }

        // *(o variant) — `o` between following variants is optional.
        loop {
            let peek = peek_trivia(&self.cursor);
            if !Self::is_variant_start_byte(peek.next_byte) {
                break;
            }
            // Reuse the just-measured peek instead of re-scanning trivia.
            self.commit_peeked_trivia(peek, TriviaMode::Optional);
            let variant = self.parse_variant();
            self.builder.push_node_edge(variant);
            variant_count += 1;
        }

        // `matcher = match-statement 1*variant` with
        // `match-statement = match 1*(s selector)` — both lists are
        // required. A selector-less matcher also covers non-variable
        // selector input such as `.match |x|`, because only `$` enters the
        // selector loop. One diagnostic anchored at the `.match` keyword;
        // no cascade when both lists are empty.
        if selector_count == 0 || variant_count == 0 {
            self.diagnostics
                .push(self.source, kw_span, DiagnosticCode::InvalidMatcherSyntax);
        }

        let end = self.cursor.offset();
        self.builder.finish_node(pending, end)
    }

    #[inline]
    fn is_variant_start_byte(b: Option<u8>) -> bool {
        // variant-key = `*` (catch-all) | quoted-literal (`|...|`) |
        // unquoted-literal (`1*name-char`). `name-char` includes DIGIT,
        // `-`, and `.`, so the lookahead must accept those too — otherwise
        // exact numeric keys like `1 {{one}}` would be rejected.
        match b {
            Some(b'*' | b'|') => true,
            Some(b) if b < 0x80 => matches!(
                b,
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'_' | b'-' | b'.'
            ),
            Some(_) => true,
            None => false,
        }
    }

    fn parse_variant(&mut self) -> NodeId {
        let start = self.cursor.offset();
        let pending = self.builder.start_node(SyntaxKind::Variant, start);

        // key *(s key) — adjacent keys require `s`. Lookahead so the loop
        // exits without paying checkpoint/rollback when the next token is
        // the quoted-pattern boundary `{{`.
        loop {
            let key = self.parse_variant_key();
            self.builder.push_node_edge(key);

            let peek = peek_trivia(&self.cursor);
            if !Self::is_variant_key_start_byte(peek.next_byte) {
                break;
            }
            if !peek.satisfies_required_s() {
                let at = peek.end_offset;
                self.diagnostics.push(
                    self.source,
                    Span::new(at, at),
                    DiagnosticCode::MissingRequiredWhitespace,
                );
            }
            // Reuse the just-measured peek instead of re-scanning trivia.
            self.commit_peeked_trivia(peek, TriviaMode::Required);
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

    #[inline]
    fn is_variant_key_start_byte(b: Option<u8>) -> bool {
        // Same coverage as `is_variant_start_byte` — adjacent variant keys
        // may begin with any `name-char`, not just `name-start`.
        match b {
            Some(b) => {
                matches!(
                    b,
                    b'*' | b'|'
                        | b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'_' | b'-' | b'.'
                ) || b >= 0x80
            }
            None => false,
        }
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
        } else if matches!(
            detect_keyword(&self.cursor),
            Some(crate::scanner::KeywordHit::Match)
        ) {
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
    /// each contiguous same-kind run is committed as a *single* trivia
    /// record (NOT one per character) so the trivia table stays small even
    /// on whitespace-heavy inputs. The accumulated leading-trivia range is
    /// attached to whichever token is committed next.
    ///
    /// Returns counts of `ws` and `bidi` runs consumed so callers can enforce
    /// the `s = *bidi ws o` requirement (`ws_runs > 0`) without re-scanning
    /// the source.
    fn eat_trivia(&mut self, mode: TriviaMode) -> TriviaConsumed {
        let _ = mode; // mode currently informational; classifier matches both
        let mut stats = TriviaConsumed::default();
        if !self.options.collect_trivia {
            self.consume_trivia_skipping(&mut stats);
            return stats;
        }

        // Direct field read — the BuilderLengths struct path packed five
        // other fields we don't need on the hot trivia commit path.
        let trivia_start = usize_to_u32(self.builder.tables.trivia.len(), "trivia table length");
        // Active run state: kind being collected and the byte offset where
        // the current run started. `None` means we haven't started a run.
        let mut run_kind: Option<SyntaxKind> = None;
        let mut run_start: u32 = self.cursor.offset();
        loop {
            let Some(b) = self.cursor.peek_byte() else {
                break;
            };
            // Classify the next codepoint into the run kind it belongs to.
            let (kind, advance) = if b < 0x80 {
                if is_ws_byte(b) {
                    (SyntaxKind::WhitespaceTrivia, 1u32)
                } else {
                    break;
                }
            } else {
                let Some((c, len)) = self.cursor.peek_char() else {
                    break;
                };
                if is_ws_char(c) {
                    (SyntaxKind::WhitespaceTrivia, len)
                } else if is_bidi_char(c) {
                    (SyntaxKind::BidiTrivia, len)
                } else {
                    break;
                }
            };
            // Switch runs when the kind changes: flush the current run
            // record, then start a new one at the byte we are about to
            // consume.
            match run_kind {
                Some(cur) if cur != kind => {
                    let here = self.cursor.offset();
                    let _ = self
                        .builder
                        .push_trivia(cur, self.source, Span::new(run_start, here));
                    Self::bump_run_count(&mut stats, cur);
                    run_start = here;
                }
                Some(_) => {}
                None => {
                    run_start = self.cursor.offset();
                }
            }
            run_kind = Some(kind);
            self.cursor.set_offset(self.cursor.offset() + advance);
        }
        if let Some(cur) = run_kind {
            let here = self.cursor.offset();
            if here > run_start {
                let _ = self
                    .builder
                    .push_trivia(cur, self.source, Span::new(run_start, here));
                Self::bump_run_count(&mut stats, cur);
            }
        }
        if stats.ws_runs + stats.bidi_runs > 0 {
            self.pending_trivia_start = trivia_start;
        }
        stats
    }

    /// Commit trivia whose extent was already measured by [`peek_trivia`].
    ///
    /// On the speculative trivia path (`option` / `attribute` / `function`
    /// / selector / variant loops) the parser first peeks to decide
    /// whether the construct exists, then calls `eat_trivia` to actually
    /// consume the whitespace. That second call re-scans the same bytes
    /// — wasted work on whitespace-heavy inputs.
    ///
    /// When `collect_trivia = false` (parser-core baseline benchmarks and
    /// any caller that does not care about trivia records) the second
    /// scan adds nothing: this helper just advances the cursor to the
    /// already-known `peek.end_offset` and reuses the peek's run counts.
    /// When `collect_trivia = true` source-fidelity requires preserving
    /// per-run trivia records, so the implementation falls back to the
    /// usual scanning commit path; the speedup there is bounded by the
    /// scanner work that has to happen anyway.
    fn commit_peeked_trivia(&mut self, peek: PeekTrivia, mode: TriviaMode) -> TriviaConsumed {
        if !self.options.collect_trivia {
            // Fast path: peek already proved no trivia records are needed
            // and where the next significant byte lives.
            self.cursor.set_offset(peek.end_offset);
            return TriviaConsumed {
                ws_runs: peek.ws_runs,
                bidi_runs: peek.bidi_runs,
            };
        }
        self.eat_trivia(mode)
    }

    /// Skip-only trivia consumption (used when `collect_trivia = false`).
    /// Counts runs in the same way the collecting path does so callers can
    /// still enforce `s = *bidi ws o`.
    fn consume_trivia_skipping(&mut self, stats: &mut TriviaConsumed) {
        let mut current: Option<SyntaxKind> = None;
        loop {
            let Some(b) = self.cursor.peek_byte() else {
                break;
            };
            let (kind, advance) = if b < 0x80 {
                if is_ws_byte(b) {
                    (SyntaxKind::WhitespaceTrivia, 1u32)
                } else {
                    break;
                }
            } else {
                let Some((c, len)) = self.cursor.peek_char() else {
                    break;
                };
                if is_ws_char(c) {
                    (SyntaxKind::WhitespaceTrivia, len)
                } else if is_bidi_char(c) {
                    (SyntaxKind::BidiTrivia, len)
                } else {
                    break;
                }
            };
            if current != Some(kind) {
                if let Some(prev) = current {
                    Self::bump_run_count(stats, prev);
                }
                current = Some(kind);
            }
            self.cursor.set_offset(self.cursor.offset() + advance);
        }
        if let Some(prev) = current {
            Self::bump_run_count(stats, prev);
        }
    }

    #[inline]
    fn bump_run_count(stats: &mut TriviaConsumed, kind: SyntaxKind) {
        if matches!(kind, SyntaxKind::WhitespaceTrivia) {
            stats.ws_runs += 1;
        } else {
            stats.bidi_runs += 1;
        }
    }

    /// Commit a token whose `first_trivia` / `leading_trivia_count` are
    /// resolved from the trivia accumulated since the last commit.
    fn commit_token(&mut self, kind: SyntaxKind, span: Span) -> TokenId {
        // Direct field read — commit_token runs per committed token, the
        // BuilderLengths struct path was packing five other fields each
        // call only to discard them.
        let trivia_len_now = usize_to_u32(self.builder.tables.trivia.len(), "trivia table length");
        let first_trivia = self.pending_trivia_start;
        let leading_count = trivia_len_now.saturating_sub(first_trivia);
        let id = self
            .builder
            .push_token(kind, self.source, span, first_trivia, leading_count, 0);
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
