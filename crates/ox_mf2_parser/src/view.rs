//! `CstView`: lazy accessor surface over [`crate::CstTables`].
//!
//! Full traversal API lands in Milestone 2.

use crate::source::SourceStore;
use crate::span::{NodeId, Span, TokenId};
use crate::syntax_kind::SyntaxKind;
use crate::tables::CstTables;

/// Lightweight accessor over a parsed [`CstTables`].
///
/// Created lazily — traversal does not materialise a recursive object tree.
#[derive(Debug, Clone, Copy)]
pub struct CstView<'a> {
    pub(crate) sources: &'a SourceStore,
    pub(crate) tables: &'a CstTables,
}

impl<'a> CstView<'a> {
    pub fn new(sources: &'a SourceStore, tables: &'a CstTables) -> Self {
        Self { sources, tables }
    }

    pub fn root(self) -> Option<CstNodeView<'a>> {
        if self.tables.node_count() == 0 {
            None
        } else {
            Some(CstNodeView::new(self, NodeId::new(0)))
        }
    }

    pub fn kind(&self, _node: NodeId) -> SyntaxKind {
        self.tables.root_kind()
    }

    pub fn span(&self, _node: NodeId) -> Span {
        self.tables.root_span()
    }

    pub fn token_kind(&self, _token: TokenId) -> SyntaxKind {
        SyntaxKind::Tombstone
    }

    pub fn token_span(&self, _token: TokenId) -> Span {
        Span::EMPTY
    }

    pub fn source_slice(&self, span: Span) -> &str {
        self.sources.slice(span)
    }
}

/// Lazy node view that references a node record + edge range in [`CstTables`].
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
        self.view.kind(self.id)
    }

    pub fn span(&self) -> Span {
        self.view.span(self.id)
    }

    pub fn children(&self) -> CstChildren<'a> {
        CstChildren { view: self.view }
    }
}

/// Iterator over the children of a [`CstNodeView`].
#[derive(Debug, Clone, Copy)]
pub struct CstChildren<'a> {
    #[allow(dead_code)]
    pub(crate) view: CstView<'a>,
}

impl<'a> Iterator for CstChildren<'a> {
    type Item = CstNodeView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}
