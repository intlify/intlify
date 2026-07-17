// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Validation boundary for in-memory parser artifacts.

use ox_mf2_parser::{
    CstChild, CstTriviaRange, CstView, NodeId, ParseResult, SourceStore, Span, TokenId, TriviaId,
    NONE_U32,
};

use crate::{FormatMode, FormatOptions, OperationalError};

/// Borrowed formatter view over one parser-owned source/result pair.
///
/// The view retains the original tables and source text without copying them.
/// Numeric source ids cannot prove historical ownership, so construction
/// rejects every attachment inconsistency observable in the retained data.
#[derive(Clone, Copy)]
pub(crate) struct ParsedFormatInput<'a> {
    sources: &'a SourceStore,
    result: &'a ParseResult,
    source: &'a str,
}

impl<'a> ParsedFormatInput<'a> {
    pub(crate) fn new(
        sources: &'a SourceStore,
        result: &'a ParseResult,
    ) -> Result<Self, OperationalError> {
        let source = sources
            .get(result.source)
            .map(|file| file.text.as_str())
            .ok_or_else(parsed_artifact_attachment_error)?;

        validate_cst_attachment(sources, result, source)?;
        validate_diagnostic_attachment(sources, result, source)?;

        if !result.trivia_collected && result.cst.trivia_count() != 0 {
            return Err(parsed_artifact_attachment_error());
        }

        Ok(Self {
            sources,
            result,
            source,
        })
    }

    pub(crate) fn validate_options(self, options: FormatOptions) -> Result<Self, OperationalError> {
        if options.mode == FormatMode::Preserve && !self.result.trivia_collected {
            return Err(parsed_artifact_attachment_error());
        }
        Ok(self)
    }

    pub(crate) const fn sources(self) -> &'a SourceStore {
        self.sources
    }

    pub(crate) const fn result(self) -> &'a ParseResult {
        self.result
    }

    pub(crate) const fn source(self) -> &'a str {
        self.source
    }
}

fn validate_cst_attachment(
    sources: &SourceStore,
    result: &ParseResult,
    source: &str,
) -> Result<(), OperationalError> {
    let view = CstView::new(sources, result.source, &result.cst);
    if view.root().is_none() {
        return Err(parsed_artifact_attachment_error());
    }

    let mut observed_edges = 0usize;
    for index in 0..result.cst.node_count() {
        let node = view
            .node(NodeId::new(table_id(index)?))
            .ok_or_else(parsed_artifact_attachment_error)?;
        if !span_resolves(source, node.span()) {
            return Err(parsed_artifact_attachment_error());
        }

        for child in node.children() {
            observed_edges = observed_edges
                .checked_add(1)
                .ok_or_else(parsed_artifact_attachment_error)?;
            let resolves = match child {
                CstChild::Node(child) => view.node(child.id()).is_some(),
                CstChild::Token(child) => view.token(child.id()).is_some(),
            };
            if !resolves {
                return Err(parsed_artifact_attachment_error());
            }
        }
    }
    if observed_edges != result.cst.edge_count() {
        return Err(parsed_artifact_attachment_error());
    }

    for index in 0..result.cst.token_count() {
        let token = view
            .token(TokenId::new(table_id(index)?))
            .ok_or_else(parsed_artifact_attachment_error)?;
        if token.source_id() != result.source || !span_resolves(source, token.span()) {
            return Err(parsed_artifact_attachment_error());
        }
        if !trivia_range_resolves(token.leading_trivia())
            || !trivia_range_resolves(token.trailing_trivia())
        {
            return Err(parsed_artifact_attachment_error());
        }
    }

    for index in 0..result.cst.trivia_count() {
        let trivia = view
            .trivia(TriviaId::new(table_id(index)?))
            .ok_or_else(parsed_artifact_attachment_error)?;
        if trivia.source_id() != result.source || !span_resolves(source, trivia.span()) {
            return Err(parsed_artifact_attachment_error());
        }
    }

    Ok(())
}

fn validate_diagnostic_attachment(
    sources: &SourceStore,
    result: &ParseResult,
    source: &str,
) -> Result<(), OperationalError> {
    for diagnostic in &result.diagnostics {
        if diagnostic.source != result.source
            || !span_resolves(source, diagnostic.span)
            || diagnostic.location != sources.location(diagnostic.source, diagnostic.span)
            || diagnostic.severity != diagnostic.code.severity()
            || diagnostic.message != diagnostic.code.static_message()
        {
            return Err(parsed_artifact_attachment_error());
        }

        for label in &diagnostic.labels {
            if label.source != result.source || !span_resolves(source, label.span) {
                return Err(parsed_artifact_attachment_error());
            }
        }
    }

    Ok(())
}

fn trivia_range_resolves(range: CstTriviaRange<'_>) -> bool {
    let expected = range.len();
    range.count() == expected
}

fn span_resolves(source: &str, span: Span) -> bool {
    span.start <= span.end && source.get(span.start as usize..span.end as usize).is_some()
}

fn table_id(index: usize) -> Result<u32, OperationalError> {
    u32::try_from(index)
        .ok()
        .filter(|value| *value != NONE_U32)
        .ok_or_else(parsed_artifact_attachment_error)
}

fn parsed_artifact_attachment_error() -> OperationalError {
    OperationalError::internal("parsed formatter input is not attached consistently")
        .with_detail("reason", "formatter_invariant_failed")
        .with_detail("phase", "parsed_artifact_attachment")
}
