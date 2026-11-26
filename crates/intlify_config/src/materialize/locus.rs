// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Where a materialized value, or a member's key, is in the input its
//! document came from. A locus is opaque: only code that needs a file's byte
//! coordinates asks for them, and handles a locus without them.

use super::{ByteSpan, NodeId};

/// Where a value, or a member's key, is in the input its document came
/// from. A file document's locus is its token's byte span, which
/// Materialized Value Evidence retains (D15:791). A Snapshot document's is
/// its node, or the key of the member whose value is that node, and has no
/// bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Locus(Kind);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    /// A token's byte span in a file.
    File(ByteSpan),
    /// A node of a Snapshot's document, or with `key` the key of the member
    /// whose value it is.
    Snapshot { node: NodeId, key: bool },
}

impl Locus {
    /// The locus of the file token at `span`.
    pub(super) const fn file(span: ByteSpan) -> Self {
        Self(Kind::File(span))
    }

    /// The locus of the Snapshot document's `node`, or with `key` of the key
    /// of the member whose value it is.
    pub(super) const fn snapshot(node: NodeId, key: bool) -> Self {
        Self(Kind::Snapshot { node, key })
    }

    /// The byte span of the file token at the locus; none without file
    /// coordinates.
    pub(crate) const fn bytes(self) -> Option<ByteSpan> {
        match self.0 {
            Kind::File(span) => Some(span),
            Kind::Snapshot { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::materialize::{materialize_file, ByteSpan, NodeKind};
    use crate::materialize_tests::limits;

    /// A file document's locus is its token's byte span: a value's, a
    /// container's from its opening to its closing token, and a member's key.
    #[test]
    fn a_file_documents_locus_is_its_tokens_byte_span() {
        let source = br#"{"key": [true]}"#;
        let doc = materialize_file(Arc::from(&source[..]), limits()).unwrap();
        let root = doc.node(doc.root());
        assert_eq!(root.locus().bytes(), Some(ByteSpan::new(0, 15)));
        let NodeKind::Object(members) = root.kind() else {
            panic!("an object");
        };
        let member = &members["key"];
        assert_eq!(member.key().bytes(), Some(ByteSpan::new(1, 6)));
        let value = doc.node(member.value());
        assert_eq!(value.locus().bytes(), Some(ByteSpan::new(8, 14)));
        let NodeKind::Array(items) = value.kind() else {
            panic!("an array");
        };
        assert_eq!(
            doc.node(items[0]).locus().bytes(),
            Some(ByteSpan::new(9, 13))
        );
    }
}
