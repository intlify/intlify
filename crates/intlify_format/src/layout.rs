// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use ox_mf2_parser::{
    snapshot::{
        ChildView as SnapshotChild, NodeView as SnapshotNodeView, TokenView as SnapshotTokenView,
    },
    CstChild, CstNodeView, CstTokenView, CstView, ParseResult, RootId, SnapshotView, SourceStore,
    Span, SyntaxKind,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    document::{Document, GroupMode},
    error::OperationalError,
    options::{FormatMode, FormatOptions},
};

/// Formatter-owned layout root.
///
/// This type keeps the public parser view out of rendering. Traversal lowers
/// parser CST nodes into source-backed Document IR so user spelling is copied by
/// span while formatter-controlled punctuation and spacing are generated.
pub(crate) struct LayoutDocument {
    document: Document,
}

impl LayoutDocument {
    pub(crate) fn from_parse(
        source: &str,
        sources: &SourceStore,
        parse: &ParseResult,
        options: FormatOptions,
    ) -> Result<Self, OperationalError> {
        let view = CstView::new(sources, parse.source, &parse.cst);
        let root = view
            .root()
            .ok_or_else(|| internal_error("parse result does not contain a root node"))?;
        let formatter = Formatter { source, options };
        Ok(Self {
            document: formatter.format_root(root)?,
        })
    }

    pub(crate) fn from_snapshot(
        source: &str,
        snapshot: SnapshotView<'_>,
        options: FormatOptions,
    ) -> Result<Self, OperationalError> {
        let root = snapshot
            .root(RootId::new(0))
            .ok_or_else(|| internal_error("snapshot does not contain root 0"))?;
        let formatter = Formatter { source, options };
        Ok(Self {
            document: formatter.format_root(root.node())?,
        })
    }

    pub(crate) fn into_document(self) -> Document {
        self.document
    }
}

struct Formatter<'source> {
    source: &'source str,
    options: FormatOptions,
}

enum SyntaxChild<N, T> {
    Node(N),
    Token(T),
}

trait FormatToken: Copy {
    fn kind(self) -> SyntaxKind;
}

trait FormatNode: Copy + Sized {
    type Token: FormatToken;

    fn kind(self) -> SyntaxKind;
    fn span(self) -> Span;
    fn children(self) -> Vec<SyntaxChild<Self, Self::Token>>;
}

impl FormatToken for CstTokenView<'_> {
    fn kind(self) -> SyntaxKind {
        CstTokenView::kind(&self)
    }
}

impl<'a> FormatNode for CstNodeView<'a> {
    type Token = CstTokenView<'a>;

    fn kind(self) -> SyntaxKind {
        CstNodeView::kind(&self)
    }

    fn span(self) -> Span {
        CstNodeView::span(&self)
    }

    fn children(self) -> Vec<SyntaxChild<Self, Self::Token>> {
        CstNodeView::children(&self)
            .map(|child| match child {
                CstChild::Node(node) => SyntaxChild::Node(node),
                CstChild::Token(token) => SyntaxChild::Token(token),
            })
            .collect()
    }
}

impl FormatToken for SnapshotTokenView<'_> {
    fn kind(self) -> SyntaxKind {
        SnapshotTokenView::kind(&self)
    }
}

impl<'a> FormatNode for SnapshotNodeView<'a> {
    type Token = SnapshotTokenView<'a>;

    fn kind(self) -> SyntaxKind {
        SnapshotNodeView::kind(&self)
    }

    fn span(self) -> Span {
        SnapshotNodeView::span(&self)
    }

    fn children(self) -> Vec<SyntaxChild<Self, Self::Token>> {
        SnapshotNodeView::children(&self)
            .map(|child| match child {
                SnapshotChild::Node(node) => SyntaxChild::Node(node),
                SnapshotChild::Token(token) => SyntaxChild::Token(token),
            })
            .collect()
    }
}

impl Formatter<'_> {
    fn format_root<N: FormatNode>(&self, root: N) -> Result<Document, OperationalError> {
        let message = self
            .first_node(root, SyntaxKind::SimpleMessage)
            .or_else(|| self.first_node(root, SyntaxKind::ComplexMessage))
            .ok_or_else(|| internal_error("root node does not contain a message"))?;
        self.format_message(message)
    }

    fn format_message<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        match node.kind() {
            SyntaxKind::SimpleMessage => {
                let pattern = self.required_node(node, SyntaxKind::Pattern, "simple message")?;
                self.format_pattern(pattern)
            }
            SyntaxKind::ComplexMessage => self.format_complex_message(node),
            _ => Err(internal_error("unexpected message node kind")),
        }
    }

    fn format_complex_message<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        let mut entries = Vec::new();
        for child in self.node_children(node) {
            match child.kind() {
                SyntaxKind::InputDeclaration | SyntaxKind::LocalDeclaration => {
                    entries.push(MajorEntry {
                        span: child.span(),
                        document: self.format_declaration(child)?,
                    });
                }
                SyntaxKind::ComplexBody => {
                    entries.push(MajorEntry {
                        span: child.span(),
                        document: self.format_complex_body(child)?,
                    });
                }
                _ => {}
            }
        }

        if entries.is_empty() {
            return Err(internal_error("complex message has no format entries"));
        }

        let preserve_flat = self.options.mode == FormatMode::Preserve
            && !self.span_contains_line_break(node.span())
            && entries
                .last()
                .is_some_and(|entry| self.span_has_kind(entry.span, SyntaxKind::QuotedPattern));

        if preserve_flat {
            return Ok(Document::group(
                GroupMode::Flat,
                Document::join(
                    &Document::space(),
                    entries.into_iter().map(|e| e.document).collect(),
                ),
            ));
        }

        Ok(Document::group(
            GroupMode::Break,
            self.join_major_entries(entries),
        ))
    }

    fn format_declaration<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        match node.kind() {
            SyntaxKind::InputDeclaration => {
                let value = self.required_node(node, SyntaxKind::Placeholder, ".input")?;
                Ok(Document::concat(vec![
                    Document::text(".input"),
                    Document::space(),
                    self.format_placeholder(value)?,
                ]))
            }
            SyntaxKind::LocalDeclaration => {
                let variable = self.required_node(node, SyntaxKind::Variable, ".local")?;
                let value = self.required_node(node, SyntaxKind::Placeholder, ".local")?;
                Ok(Document::concat(vec![
                    Document::text(".local"),
                    Document::space(),
                    self.format_variable(variable),
                    Document::space(),
                    Document::text("="),
                    Document::space(),
                    self.format_placeholder(value)?,
                ]))
            }
            _ => Err(internal_error("unexpected declaration node kind")),
        }
    }

    fn format_complex_body<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        let body = self
            .node_children(node)
            .into_iter()
            .find(|child| {
                matches!(
                    child.kind(),
                    SyntaxKind::QuotedPattern | SyntaxKind::Matcher
                )
            })
            .ok_or_else(|| internal_error("complex body has no supported child"))?;

        match body.kind() {
            SyntaxKind::QuotedPattern => self.format_quoted_pattern(body),
            SyntaxKind::Matcher => self.format_matcher(body),
            _ => Err(internal_error("unexpected complex body child")),
        }
    }

    fn format_pattern<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        let mut parts = Vec::new();
        for child in self.node_children(node) {
            match child.kind() {
                SyntaxKind::Text => parts.push(Document::source(child.span())),
                SyntaxKind::Placeholder => parts.push(self.format_placeholder(child)?),
                _ => return Err(internal_error("pattern contains unsupported child")),
            }
        }
        Ok(Document::concat(parts))
    }

    fn format_placeholder<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        let expression = self
            .node_children(node)
            .into_iter()
            .find(|child| child.kind().is_expression() || child.kind() == SyntaxKind::Markup)
            .ok_or_else(|| internal_error("placeholder has no expression"))?;
        self.format_expression(expression)
    }

    fn format_expression<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        match node.kind() {
            SyntaxKind::LiteralExpression
            | SyntaxKind::VariableExpression
            | SyntaxKind::FunctionExpression => self.format_regular_expression(node),
            SyntaxKind::Markup => self.format_markup(node),
            _ => Err(internal_error("unexpected expression node kind")),
        }
    }

    fn format_regular_expression<N: FormatNode>(
        &self,
        node: N,
    ) -> Result<Document, OperationalError> {
        let mut inner = Vec::new();
        for child in self.node_children(node) {
            let document = match child.kind() {
                SyntaxKind::Variable => self.format_variable(child),
                SyntaxKind::QuotedLiteral | SyntaxKind::UnquotedLiteral => {
                    self.format_literal(child)
                }
                SyntaxKind::Function => self.format_function(child)?,
                SyntaxKind::Attribute => self.format_attribute(child)?,
                _ => continue,
            };

            if !inner.is_empty() {
                inner.push(Document::space());
            }
            inner.push(document);
        }

        if inner.is_empty() {
            return Err(internal_error("expression has no printable children"));
        }

        Ok(Document::concat(vec![
            Document::text("{"),
            Document::concat(inner),
            Document::text("}"),
        ]))
    }

    fn format_function<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        let identifier = self.required_node(node, SyntaxKind::Identifier, "function")?;
        let mut parts = vec![Document::text(":"), self.format_identifier(identifier)];

        for option in self.direct_nodes(node, SyntaxKind::Option) {
            parts.push(Document::space());
            parts.push(self.format_option(option)?);
        }

        Ok(Document::concat(parts))
    }

    fn format_option<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        let identifier = self.required_node(node, SyntaxKind::Identifier, "option")?;
        let value = self
            .node_children(node)
            .into_iter()
            .find(|child| {
                matches!(
                    child.kind(),
                    SyntaxKind::Variable | SyntaxKind::QuotedLiteral | SyntaxKind::UnquotedLiteral
                )
            })
            .ok_or_else(|| internal_error("option has no value"))?;

        let value = match value.kind() {
            SyntaxKind::Variable => self.format_variable(value),
            SyntaxKind::QuotedLiteral | SyntaxKind::UnquotedLiteral => self.format_literal(value),
            _ => return Err(internal_error("unexpected option value")),
        };

        Ok(Document::concat(vec![
            self.format_identifier(identifier),
            Document::text("="),
            value,
        ]))
    }

    fn format_attribute<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        let identifier = self.required_node(node, SyntaxKind::Identifier, "attribute")?;
        let mut parts = vec![Document::text("@"), self.format_identifier(identifier)];

        if let Some(literal) = self.node_children(node).into_iter().find(|child| {
            matches!(
                child.kind(),
                SyntaxKind::QuotedLiteral | SyntaxKind::UnquotedLiteral
            )
        }) {
            parts.push(Document::text("="));
            parts.push(self.format_literal(literal));
        }

        Ok(Document::concat(parts))
    }

    fn format_markup<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        let identifier = self.required_node(node, SyntaxKind::Identifier, "markup")?;
        let close = self.first_markup_sigil(node) == Some(SyntaxKind::SlashToken);
        let standalone = !close && self.markup_has_standalone_slash(node);

        let mut parts = vec![Document::text("{")];
        parts.push(if close {
            Document::text("/")
        } else {
            Document::text("#")
        });
        parts.push(self.format_identifier(identifier));

        for option in self.direct_nodes(node, SyntaxKind::Option) {
            parts.push(Document::space());
            parts.push(self.format_option(option)?);
        }
        for attribute in self.direct_nodes(node, SyntaxKind::Attribute) {
            parts.push(Document::space());
            parts.push(self.format_attribute(attribute)?);
        }
        if standalone {
            parts.push(Document::space());
            parts.push(Document::text("/"));
        }
        parts.push(Document::text("}"));

        Ok(Document::concat(parts))
    }

    fn format_quoted_pattern<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        let pattern = self.required_node(node, SyntaxKind::Pattern, "quoted pattern")?;
        Ok(Document::concat(vec![
            Document::text("{{"),
            self.format_pattern(pattern)?,
            Document::text("}}"),
        ]))
    }

    fn format_matcher<N: FormatNode>(&self, node: N) -> Result<Document, OperationalError> {
        let selectors = self.direct_nodes(node, SyntaxKind::Selector);
        let variants = self.direct_nodes(node, SyntaxKind::Variant);

        if selectors.is_empty() || variants.is_empty() {
            return Err(internal_error("matcher is missing selectors or variants"));
        }

        let selector_docs = selectors
            .into_iter()
            .map(|selector| {
                let variable = self.required_node(selector, SyntaxKind::Variable, "selector")?;
                Ok(self.format_variable(variable))
            })
            .collect::<Result<Vec<_>, OperationalError>>()?;

        let mut rows = Vec::with_capacity(variants.len());
        for variant in variants {
            rows.push(self.format_matcher_row(variant)?);
        }

        let selector_count = selector_docs.len();
        let mut column_widths = vec![0usize; selector_count];
        for row in &rows {
            if row.keys.len() != selector_count {
                return Err(internal_error(
                    "matcher row key count does not match selectors",
                ));
            }
            for (index, key) in row.keys.iter().enumerate() {
                column_widths[index] = column_widths[index].max(key.width);
            }
        }

        let mut parts = vec![
            Document::text(".match"),
            Document::space(),
            Document::join(&Document::space(), selector_docs),
        ];

        let mut previous_end = node.span().start;
        for row in rows {
            parts.push(Document::hard_line());
            if self.options.mode == FormatMode::Preserve
                && self.has_blank_line_between(previous_end, row.span.start)
            {
                parts.push(Document::hard_line());
            }
            previous_end = row.span.end;
            parts.push(self.render_matcher_row(row, &column_widths));
        }

        Ok(Document::concat(parts))
    }

    fn format_matcher_row<N: FormatNode>(&self, node: N) -> Result<MatcherRow, OperationalError> {
        let mut keys = Vec::new();
        let mut value = None;

        for child in self.node_children(node) {
            match child.kind() {
                SyntaxKind::VariantKey | SyntaxKind::CatchAllKey => {
                    let span = if child.kind() == SyntaxKind::VariantKey {
                        self.required_literal(child, "variant key")?.span()
                    } else {
                        child.span()
                    };
                    keys.push(MatcherKey {
                        width: UnicodeWidthStr::width(self.source_slice(span)),
                        document: Document::source(span),
                    });
                }
                SyntaxKind::QuotedPattern => {
                    value = Some(self.format_quoted_pattern(child)?);
                }
                _ => {}
            }
        }

        let value = value.ok_or_else(|| internal_error("matcher row has no value"))?;
        Ok(MatcherRow {
            span: node.span(),
            keys,
            value,
        })
    }

    #[allow(clippy::unused_self)]
    fn render_matcher_row(&self, row: MatcherRow, column_widths: &[usize]) -> Document {
        let mut parts = Vec::new();
        for (index, key) in row.keys.into_iter().enumerate() {
            let padding = column_widths[index].saturating_sub(key.width) + 2;
            parts.push(key.document);
            parts.push(Document::owned_text(" ".repeat(padding)));
        }
        parts.push(row.value);
        Document::concat(parts)
    }

    #[allow(clippy::unused_self)]
    fn format_variable<N: FormatNode>(&self, node: N) -> Document {
        Document::source(node.span())
    }

    #[allow(clippy::unused_self)]
    fn format_identifier<N: FormatNode>(&self, node: N) -> Document {
        Document::source(node.span())
    }

    #[allow(clippy::unused_self)]
    fn format_literal<N: FormatNode>(&self, node: N) -> Document {
        Document::source(node.span())
    }

    fn join_major_entries(&self, entries: Vec<MajorEntry>) -> Document {
        let mut parts = Vec::new();
        let mut previous_end = entries.first().map_or(0, |entry| entry.span.start);

        for (index, entry) in entries.into_iter().enumerate() {
            if index > 0 {
                parts.push(Document::hard_line());
                if self.options.mode == FormatMode::Preserve
                    && self.has_blank_line_between(previous_end, entry.span.start)
                {
                    parts.push(Document::hard_line());
                }
            }
            previous_end = entry.span.end;
            parts.push(entry.document);
        }

        Document::concat(parts)
    }

    fn required_node<N: FormatNode>(
        &self,
        node: N,
        kind: SyntaxKind,
        owner: &'static str,
    ) -> Result<N, OperationalError> {
        self.first_node(node, kind)
            .ok_or_else(|| internal_error(format!("{owner} is missing {kind:?}")))
    }

    fn required_literal<N: FormatNode>(
        &self,
        node: N,
        owner: &'static str,
    ) -> Result<N, OperationalError> {
        self.node_children(node)
            .into_iter()
            .find(|child| {
                matches!(
                    child.kind(),
                    SyntaxKind::QuotedLiteral | SyntaxKind::UnquotedLiteral
                )
            })
            .ok_or_else(|| internal_error(format!("{owner} is missing literal")))
    }

    fn direct_nodes<N: FormatNode>(&self, node: N, kind: SyntaxKind) -> Vec<N> {
        self.node_children(node)
            .into_iter()
            .filter(|child| child.kind() == kind)
            .collect()
    }

    fn first_node<N: FormatNode>(&self, node: N, kind: SyntaxKind) -> Option<N> {
        self.node_children(node)
            .into_iter()
            .find(|child| child.kind() == kind)
    }

    #[allow(clippy::unused_self)]
    fn node_children<N: FormatNode>(&self, node: N) -> Vec<N> {
        node.children()
            .into_iter()
            .filter_map(|child| match child {
                SyntaxChild::Node(node) => Some(node),
                SyntaxChild::Token(_) => None,
            })
            .collect()
    }

    #[allow(clippy::unused_self)]
    fn token_children<N: FormatNode>(&self, node: N) -> Vec<N::Token> {
        node.children()
            .into_iter()
            .filter_map(|child| match child {
                SyntaxChild::Token(token) => Some(token),
                SyntaxChild::Node(_) => None,
            })
            .collect()
    }

    fn first_markup_sigil<N: FormatNode>(&self, node: N) -> Option<SyntaxKind> {
        self.token_children(node)
            .into_iter()
            .find_map(|token| match token.kind() {
                SyntaxKind::HashToken | SyntaxKind::SlashToken => Some(token.kind()),
                _ => None,
            })
    }

    fn markup_has_standalone_slash<N: FormatNode>(&self, node: N) -> bool {
        let tokens = self.token_children(node);
        let Some(hash_index) = tokens
            .iter()
            .position(|token| token.kind() == SyntaxKind::HashToken)
        else {
            return false;
        };

        tokens
            .iter()
            .skip(hash_index + 1)
            .any(|token| token.kind() == SyntaxKind::SlashToken)
    }

    fn span_has_kind(&self, span: Span, kind: SyntaxKind) -> bool {
        self.source
            .get(span.start as usize..span.end as usize)
            .is_some_and(|source| {
                // This is only used for the preserve-flat complex message
                // gate. The body span includes `{{` for quoted patterns.
                kind == SyntaxKind::QuotedPattern && source.trim_start().starts_with("{{")
            })
    }

    fn source_slice(&self, span: Span) -> &str {
        self.source
            .get(span.start as usize..span.end as usize)
            .unwrap_or("")
    }

    fn span_contains_line_break(&self, span: Span) -> bool {
        self.source_slice(span)
            .bytes()
            .any(|byte| matches!(byte, b'\n' | b'\r'))
    }

    fn has_blank_line_between(&self, previous_end: u32, next_start: u32) -> bool {
        if next_start <= previous_end {
            return false;
        }
        count_line_breaks(self.source_slice(Span::new(previous_end, next_start))) >= 2
    }
}

struct MajorEntry {
    span: Span,
    document: Document,
}

struct MatcherRow {
    span: Span,
    keys: Vec<MatcherKey>,
    value: Document,
}

struct MatcherKey {
    width: usize,
    document: Document,
}

fn count_line_breaks(source: &str) -> usize {
    let mut count = 0;
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                count += 1;
                index += if bytes.get(index + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
            }
            b'\n' => {
                count += 1;
                index += 1;
            }
            _ => index += 1,
        }
    }
    count
}

fn internal_error(message: impl Into<String>) -> OperationalError {
    OperationalError::internal(message).with_detail("phase", "layout_ir_construction")
}
