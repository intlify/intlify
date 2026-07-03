// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

// The first implementation slice keeps Document IR intentionally small.
// Later formatting-rule PRs expand this into line, concat, indent, and group
// primitives without changing the public formatter API.
pub(crate) struct Document<'source> {
    text: &'source str,
}

impl<'source> Document<'source> {
    pub(crate) const fn text(text: &'source str) -> Self {
        Self { text }
    }

    pub(crate) const fn as_str(&self) -> &'source str {
        self.text
    }
}
