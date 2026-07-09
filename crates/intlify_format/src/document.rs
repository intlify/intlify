// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::borrow::Cow;

use ox_mf2_parser::Span;

/// Fixed group rendering mode.
///
/// Phase 3B keeps group decisions deterministic. The enum exists now so the
/// formatter can model future line wrapping without changing traversal output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupMode {
    Flat,
    Break,
}

/// Small source-backed document IR lowered from MF2 layout decisions.
///
/// `SourceSlice` is the key boundary: source-owned user spelling, such as
/// pattern text, identifiers, variables, and literals, is copied by span during
/// rendering. Formatter-generated punctuation and spacing are represented as
/// `Text`, `Space`, and `HardLine`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Document {
    Empty,
    Text(Cow<'static, str>),
    SourceSlice(Span),
    Space,
    HardLine,
    #[allow(dead_code)]
    SoftLine,
    Concat(Vec<Document>),
    #[allow(dead_code)]
    Indent(Box<Document>),
    Group {
        mode: GroupMode,
        doc: Box<Document>,
    },
}

impl Document {
    pub(crate) fn text(text: &'static str) -> Self {
        Self::Text(Cow::Borrowed(text))
    }

    pub(crate) fn owned_text(text: String) -> Self {
        Self::Text(Cow::Owned(text))
    }

    pub(crate) const fn source(span: Span) -> Self {
        Self::SourceSlice(span)
    }

    pub(crate) const fn space() -> Self {
        Self::Space
    }

    pub(crate) const fn hard_line() -> Self {
        Self::HardLine
    }

    #[allow(dead_code)]
    pub(crate) const fn soft_line() -> Self {
        Self::SoftLine
    }

    pub(crate) fn concat(parts: Vec<Self>) -> Self {
        let mut flattened = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                Self::Empty => {}
                Self::Concat(nested) => flattened.extend(nested),
                part => flattened.push(part),
            }
        }

        match flattened.len() {
            0 => Self::Empty,
            1 => flattened.pop().expect("one document part"),
            _ => Self::Concat(flattened),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn indent(doc: Self) -> Self {
        Self::Indent(Box::new(doc))
    }

    pub(crate) fn group(mode: GroupMode, doc: Self) -> Self {
        Self::Group {
            mode,
            doc: Box::new(doc),
        }
    }

    pub(crate) fn join(separator: &Self, parts: Vec<Self>) -> Self {
        let mut docs = Vec::with_capacity(parts.len().saturating_mul(2));
        for (index, part) in parts.into_iter().enumerate() {
            if index > 0 {
                docs.push(separator.clone());
            }
            docs.push(part);
        }
        Self::concat(docs)
    }
}
