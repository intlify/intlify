// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

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
