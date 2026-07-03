// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::{document::Document, options::FormatOptions};

pub(crate) struct LayoutDocument<'source> {
    source: &'source str,
    options: FormatOptions,
}

impl<'source> LayoutDocument<'source> {
    pub(crate) const fn from_parse(source: &'source str, options: FormatOptions) -> Self {
        Self { source, options }
    }

    pub(crate) fn into_document(self) -> Document<'source> {
        match self.options.mode {
            crate::FormatMode::Standard | crate::FormatMode::Preserve => {
                Document::text(self.source)
            }
        }
    }
}
