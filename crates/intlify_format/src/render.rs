// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::document::Document;

/// Render the Document IR into deterministic source text.
pub(crate) fn render(document: &Document<'_>) -> String {
    document.as_str().to_owned()
}
