//! `CstView`: lazy accessor surface over [`crate::CstTables`].
//!
//! `CstView` does not materialise a recursive object graph. Each accessor
//! returns a small `Copy` view (`CstNodeView`, `CstTokenView`, `CstTriviaView`)
//! that references the underlying record by index, so traversal stays
//! allocation-free for formatters, linters, and snapshot writers.

use crate::source::SourceStore;
use crate::span::{EdgeId, NodeId, SourceId, Span, TokenId, TriviaId};
use crate::syntax_kind::SyntaxKind;
use crate::tables::{CstEdgeKind, CstTables};

/// Lightweight accessor over a parsed [`CstTables`].
#[derive(Debug, Clone, Copy)]
pub struct CstView<'a> {
    pub(crate) source: SourceId,
    pub(crate) sources: &'a SourceStore,
    pub(crate) tables: &'a CstTables,
}

impl<'a> CstView<'a> {
    pub fn new(sources: &'a SourceStore, source: SourceId, tables: &'a CstTables) -> Self {
        Self {
            source,
            sources,
            tables,
        }
    }

    pub fn source(&self) -> SourceId {
        self.source
    }

    pub fn tables(&self) -> &'a CstTables {
        self.tables
    }

    pub fn sources(&self) -> &'a SourceStore {
        self.sources
    }

    /// Root node, if any.
    pub fn root(self) -> Option<CstNodeView<'a>> {
        let id = self.tables.root_id()?;
        Some(CstNodeView::new(self, id))
    }

    /// Lookup a node view by `id`. Returns `None` if `id` is out of range.
    pub fn node(self, id: NodeId) -> Option<CstNodeView<'a>> {
        if id.is_none() || id.index() >= self.tables.node_count() {
            return None;
        }
        Some(CstNodeView::new(self, id))
    }

    pub fn token(self, id: TokenId) -> Option<CstTokenView<'a>> {
        if id.is_none() || id.index() >= self.tables.token_count() {
            return None;
        }
        Some(CstTokenView::new(self, id))
    }

    pub fn trivia(self, id: TriviaId) -> Option<CstTriviaView<'a>> {
        if id.is_none() || id.index() >= self.tables.trivia_count() {
            return None;
        }
        Some(CstTriviaView::new(self, id))
    }

    pub fn kind(&self, node: NodeId) -> SyntaxKind {
        self.node(node).map_or(SyntaxKind::Tombstone, |n| n.kind())
    }

    pub fn span(&self, node: NodeId) -> Span {
        self.node(node).map_or(Span::EMPTY, |n| n.span())
    }

    pub fn token_kind(&self, token: TokenId) -> SyntaxKind {
        self.token(token).map_or(SyntaxKind::Tombstone, |t| t.kind())
    }

    pub fn token_span(&self, token: TokenId) -> Span {
        self.token(token).map_or(Span::EMPTY, |t| t.span())
    }

    pub fn source_slice(&self, span: Span) -> &'a str {
        self.sources.slice_in(self.source, span)
    }
}

/// Reference from an edge to either a node or a token child.
#[derive(Debug, Clone, Copy)]
pub enum CstChild<'a> {
    Node(CstNodeView<'a>),
    Token(CstTokenView<'a>),
}

impl<'a> CstChild<'a> {
    pub fn span(&self) -> Span {
        match self {
            CstChild::Node(n) => n.span(),
            CstChild::Token(t) => t.span(),
        }
    }

    pub fn kind(&self) -> SyntaxKind {
        match self {
            CstChild::Node(n) => n.kind(),
            CstChild::Token(t) => t.kind(),
        }
    }
}

/// Lazy node view: holds the node id + a reference back to the owning view.
#[derive(Debug, Clone, Copy)]
pub struct CstNodeView<'a> {
    pub(crate) id: NodeId,
    pub(crate) view: CstView<'a>,
}

impl<'a> CstNodeView<'a> {
    pub fn new(view: CstView<'a>, id: NodeId) -> Self {
        Self { id, view }
    }

    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn kind(&self) -> SyntaxKind {
        let rec = self.view.tables.node(self.id).expect("valid node id");
        kind_from_u16(rec.kind)
    }

    pub fn span(&self) -> Span {
        let rec = self.view.tables.node(self.id).expect("valid node id");
        Span::new(rec.span_start, rec.span_end)
    }

    pub fn child_count(&self) -> u32 {
        self.view
            .tables
            .node(self.id)
            .map_or(0, |rec| rec.child_count)
    }

    /// Iterator over all children — nodes or tokens — in source order.
    pub fn children(&self) -> CstChildren<'a> {
        let rec = self.view.tables.node(self.id).expect("valid node id");
        CstChildren {
            view: self.view,
            start: rec.first_child,
            end: rec.first_child + rec.child_count,
            cursor: rec.first_child,
        }
    }

    /// Convenience iterator that yields only the token children of this node.
    pub fn tokens(&self) -> CstNodeTokens<'a> {
        CstNodeTokens {
            children: self.children(),
        }
    }
}

/// Lazy token view.
#[derive(Debug, Clone, Copy)]
pub struct CstTokenView<'a> {
    pub(crate) id: TokenId,
    pub(crate) view: CstView<'a>,
}

impl<'a> CstTokenView<'a> {
    pub fn new(view: CstView<'a>, id: TokenId) -> Self {
        Self { id, view }
    }

    pub fn id(&self) -> TokenId {
        self.id
    }

    pub fn kind(&self) -> SyntaxKind {
        let rec = self.view.tables.token(self.id).expect("valid token id");
        kind_from_u16(rec.kind)
    }

    pub fn span(&self) -> Span {
        let rec = self.view.tables.token(self.id).expect("valid token id");
        Span::new(rec.span_start, rec.span_end)
    }

    pub fn source_id(&self) -> SourceId {
        let rec = self.view.tables.token(self.id).expect("valid token id");
        SourceId::new(rec.source_id)
    }

    pub fn text(&self) -> &'a str {
        let rec = self.view.tables.token(self.id).expect("valid token id");
        self.view
            .sources
            .slice_in(SourceId::new(rec.source_id), Span::new(rec.span_start, rec.span_end))
    }

    /// Compact leading-trivia range belonging to this token.
    pub fn leading_trivia(&self) -> CstTriviaRange<'a> {
        let rec = self.view.tables.token(self.id).expect("valid token id");
        CstTriviaRange {
            view: self.view,
            start: rec.first_trivia,
            end: rec.first_trivia + u32::from(rec.leading_trivia_count),
            cursor: rec.first_trivia,
        }
    }

    /// Compact trailing-trivia range belonging to this token.
    pub fn trailing_trivia(&self) -> CstTriviaRange<'a> {
        let rec = self.view.tables.token(self.id).expect("valid token id");
        let start = rec.first_trivia + u32::from(rec.leading_trivia_count);
        let end = start + u32::from(rec.trailing_trivia_count);
        CstTriviaRange {
            view: self.view,
            start,
            end,
            cursor: start,
        }
    }
}

/// Lazy trivia view.
#[derive(Debug, Clone, Copy)]
pub struct CstTriviaView<'a> {
    pub(crate) id: TriviaId,
    pub(crate) view: CstView<'a>,
}

impl<'a> CstTriviaView<'a> {
    pub fn new(view: CstView<'a>, id: TriviaId) -> Self {
        Self { id, view }
    }

    pub fn id(&self) -> TriviaId {
        self.id
    }

    pub fn kind(&self) -> SyntaxKind {
        let rec = self.view.tables.trivia(self.id).expect("valid trivia id");
        kind_from_u16(rec.kind)
    }

    pub fn span(&self) -> Span {
        let rec = self.view.tables.trivia(self.id).expect("valid trivia id");
        Span::new(rec.span_start, rec.span_end)
    }

    pub fn source_id(&self) -> SourceId {
        let rec = self.view.tables.trivia(self.id).expect("valid trivia id");
        SourceId::new(rec.source_id)
    }

    pub fn text(&self) -> &'a str {
        let rec = self.view.tables.trivia(self.id).expect("valid trivia id");
        self.view
            .sources
            .slice_in(SourceId::new(rec.source_id), Span::new(rec.span_start, rec.span_end))
    }
}

/// Iterator over a contiguous edge range yielding [`CstChild`] values.
#[derive(Debug, Clone, Copy)]
pub struct CstChildren<'a> {
    pub(crate) view: CstView<'a>,
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) cursor: u32,
}

impl<'a> Iterator for CstChildren<'a> {
    type Item = CstChild<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.end {
            return None;
        }
        let edge_id = EdgeId::new(self.cursor);
        self.cursor += 1;
        let edge = self.view.tables.edge(edge_id)?;
        let kind = if edge.kind == CstEdgeKind::Token as u16 {
            CstChild::Token(CstTokenView::new(self.view, TokenId::new(edge.ref_id)))
        } else {
            CstChild::Node(CstNodeView::new(self.view, NodeId::new(edge.ref_id)))
        };
        Some(kind)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.end - self.cursor) as usize;
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for CstChildren<'a> {}

impl<'a> CstChildren<'a> {
    pub fn reset(&mut self) {
        self.cursor = self.start;
    }
}

/// Convenience iterator that yields only the token children of a node.
#[derive(Debug, Clone, Copy)]
pub struct CstNodeTokens<'a> {
    pub(crate) children: CstChildren<'a>,
}

impl<'a> Iterator for CstNodeTokens<'a> {
    type Item = CstTokenView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        for child in self.children.by_ref() {
            if let CstChild::Token(t) = child {
                return Some(t);
            }
        }
        None
    }
}

/// Iterator over a compact trivia range belonging to a token.
#[derive(Debug, Clone, Copy)]
pub struct CstTriviaRange<'a> {
    pub(crate) view: CstView<'a>,
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) cursor: u32,
}

impl<'a> Iterator for CstTriviaRange<'a> {
    type Item = CstTriviaView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.end {
            return None;
        }
        let id = TriviaId::new(self.cursor);
        self.cursor += 1;
        if self.view.tables.trivia(id).is_none() {
            return None;
        }
        Some(CstTriviaView::new(self.view, id))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.end - self.cursor) as usize;
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for CstTriviaRange<'a> {}

impl<'a> CstTriviaRange<'a> {
    pub fn reset(&mut self) {
        self.cursor = self.start;
    }
}

/// Reverse the numeric `SyntaxKind` back into the enum, falling back to
/// `Unknown` for unrecognised future kinds. Defensive against snapshot
/// decoding paths that might surface a wider value.
#[inline]
fn kind_from_u16(value: u16) -> SyntaxKind {
    // SAFETY: `SyntaxKind` is `#[repr(u16)]` and `#[non_exhaustive]`. We map
    // known values explicitly through a table; unknown values go to
    // `SyntaxKind::Unknown` so traversal stays well-defined.
    match value {
        v if v == SyntaxKind::Tombstone as u16 => SyntaxKind::Tombstone,
        v if v == SyntaxKind::Root as u16 => SyntaxKind::Root,
        v if v == SyntaxKind::SimpleMessage as u16 => SyntaxKind::SimpleMessage,
        v if v == SyntaxKind::ComplexMessage as u16 => SyntaxKind::ComplexMessage,
        v if v == SyntaxKind::Pattern as u16 => SyntaxKind::Pattern,
        v if v == SyntaxKind::Text as u16 => SyntaxKind::Text,
        v if v == SyntaxKind::QuotedPattern as u16 => SyntaxKind::QuotedPattern,
        v if v == SyntaxKind::Placeholder as u16 => SyntaxKind::Placeholder,
        v if v == SyntaxKind::LiteralExpression as u16 => SyntaxKind::LiteralExpression,
        v if v == SyntaxKind::VariableExpression as u16 => SyntaxKind::VariableExpression,
        v if v == SyntaxKind::FunctionExpression as u16 => SyntaxKind::FunctionExpression,
        v if v == SyntaxKind::Function as u16 => SyntaxKind::Function,
        v if v == SyntaxKind::Option as u16 => SyntaxKind::Option,
        v if v == SyntaxKind::Attribute as u16 => SyntaxKind::Attribute,
        v if v == SyntaxKind::LocalDeclaration as u16 => SyntaxKind::LocalDeclaration,
        v if v == SyntaxKind::InputDeclaration as u16 => SyntaxKind::InputDeclaration,
        v if v == SyntaxKind::ComplexBody as u16 => SyntaxKind::ComplexBody,
        v if v == SyntaxKind::Matcher as u16 => SyntaxKind::Matcher,
        v if v == SyntaxKind::Selector as u16 => SyntaxKind::Selector,
        v if v == SyntaxKind::Variant as u16 => SyntaxKind::Variant,
        v if v == SyntaxKind::VariantKey as u16 => SyntaxKind::VariantKey,
        v if v == SyntaxKind::CatchAllKey as u16 => SyntaxKind::CatchAllKey,
        v if v == SyntaxKind::Markup as u16 => SyntaxKind::Markup,
        v if v == SyntaxKind::MarkupOpen as u16 => SyntaxKind::MarkupOpen,
        v if v == SyntaxKind::MarkupStandalone as u16 => SyntaxKind::MarkupStandalone,
        v if v == SyntaxKind::MarkupClose as u16 => SyntaxKind::MarkupClose,
        v if v == SyntaxKind::QuotedLiteral as u16 => SyntaxKind::QuotedLiteral,
        v if v == SyntaxKind::UnquotedLiteral as u16 => SyntaxKind::UnquotedLiteral,
        v if v == SyntaxKind::Name as u16 => SyntaxKind::Name,
        v if v == SyntaxKind::Identifier as u16 => SyntaxKind::Identifier,
        v if v == SyntaxKind::Variable as u16 => SyntaxKind::Variable,
        v if v == SyntaxKind::LeftBraceToken as u16 => SyntaxKind::LeftBraceToken,
        v if v == SyntaxKind::RightBraceToken as u16 => SyntaxKind::RightBraceToken,
        v if v == SyntaxKind::LeftDoubleBraceToken as u16 => SyntaxKind::LeftDoubleBraceToken,
        v if v == SyntaxKind::RightDoubleBraceToken as u16 => SyntaxKind::RightDoubleBraceToken,
        v if v == SyntaxKind::DotToken as u16 => SyntaxKind::DotToken,
        v if v == SyntaxKind::AtToken as u16 => SyntaxKind::AtToken,
        v if v == SyntaxKind::PipeToken as u16 => SyntaxKind::PipeToken,
        v if v == SyntaxKind::EqualsToken as u16 => SyntaxKind::EqualsToken,
        v if v == SyntaxKind::ColonToken as u16 => SyntaxKind::ColonToken,
        v if v == SyntaxKind::DollarToken as u16 => SyntaxKind::DollarToken,
        v if v == SyntaxKind::SlashToken as u16 => SyntaxKind::SlashToken,
        v if v == SyntaxKind::StarToken as u16 => SyntaxKind::StarToken,
        v if v == SyntaxKind::HashToken as u16 => SyntaxKind::HashToken,
        v if v == SyntaxKind::InputKeyword as u16 => SyntaxKind::InputKeyword,
        v if v == SyntaxKind::LocalKeyword as u16 => SyntaxKind::LocalKeyword,
        v if v == SyntaxKind::MatchKeyword as u16 => SyntaxKind::MatchKeyword,
        v if v == SyntaxKind::NameToken as u16 => SyntaxKind::NameToken,
        v if v == SyntaxKind::TextToken as u16 => SyntaxKind::TextToken,
        v if v == SyntaxKind::QuotedTextToken as u16 => SyntaxKind::QuotedTextToken,
        v if v == SyntaxKind::EscapeToken as u16 => SyntaxKind::EscapeToken,
        v if v == SyntaxKind::WhitespaceTrivia as u16 => SyntaxKind::WhitespaceTrivia,
        v if v == SyntaxKind::BidiTrivia as u16 => SyntaxKind::BidiTrivia,
        v if v == SyntaxKind::Error as u16 => SyntaxKind::Error,
        v if v == SyntaxKind::Missing as u16 => SyntaxKind::Missing,
        _ => SyntaxKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{SourceFileInput, SourceStore};
    use crate::tables::CstBuilder;

    fn build_simple_pattern() -> (SourceStore, CstTables) {
        let mut sources = SourceStore::new();
        let source_id = sources.add(SourceFileInput {
            source: "Hello",
            ..Default::default()
        });

        let mut b = CstBuilder::new();
        let text_pending = b.start_node(SyntaxKind::Text, 0);
        let token_id = b.push_token(
            SyntaxKind::TextToken,
            source_id,
            Span::new(0, 5),
            0,
            0,
            0,
        );
        b.push_token_edge(token_id);
        let text_id = b.finish_node(text_pending, 5);

        let pattern_pending = b.start_node(SyntaxKind::Pattern, 0);
        b.push_node_edge(text_id);
        let pattern_id = b.finish_node(pattern_pending, 5);

        let simple_pending = b.start_node(SyntaxKind::SimpleMessage, 0);
        b.push_node_edge(pattern_id);
        let simple_id = b.finish_node(simple_pending, 5);

        let root_pending = b.start_node(SyntaxKind::Root, 0);
        b.push_node_edge(simple_id);
        let _ = b.finish_node(root_pending, 5);

        (sources, b.tables)
    }

    #[test]
    fn root_resolves_to_last_committed_node() {
        let (sources, tables) = build_simple_pattern();
        let view = CstView::new(&sources, SourceId::new(0), &tables);
        let root = view.root().expect("root node exists");
        assert_eq!(root.kind(), SyntaxKind::Root);
        assert_eq!(root.span(), Span::new(0, 5));
        assert_eq!(root.child_count(), 1);
    }

    #[test]
    fn children_iterator_visits_node_and_token_edges() {
        let (sources, tables) = build_simple_pattern();
        let view = CstView::new(&sources, SourceId::new(0), &tables);
        let root = view.root().unwrap();

        // Root has 1 child (SimpleMessage), which has 1 child (Pattern),
        // which has 1 child (Text), which has 1 token (TextToken).
        let simple = match root.children().next().unwrap() {
            CstChild::Node(n) => n,
            CstChild::Token(_) => panic!("expected node child"),
        };
        let pattern = match simple.children().next().unwrap() {
            CstChild::Node(n) => n,
            CstChild::Token(_) => panic!("expected node child"),
        };
        let text = match pattern.children().next().unwrap() {
            CstChild::Node(n) => n,
            CstChild::Token(_) => panic!("expected node child"),
        };
        let token = match text.children().next().unwrap() {
            CstChild::Node(_) => panic!("expected token child"),
            CstChild::Token(t) => t,
        };
        assert_eq!(token.kind(), SyntaxKind::TextToken);
        assert_eq!(token.span(), Span::new(0, 5));
        assert_eq!(token.text(), "Hello");
    }

    #[test]
    fn token_text_reads_from_source_store() {
        let (sources, tables) = build_simple_pattern();
        let view = CstView::new(&sources, SourceId::new(0), &tables);
        let token = view.token(TokenId::new(0)).unwrap();
        assert_eq!(view.source_slice(token.span()), "Hello");
        assert_eq!(token.text(), "Hello");
    }
}
