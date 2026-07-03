// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::document::Document;

pub(crate) fn render(document: &Document<'_>) -> String {
    document.as_str().to_owned()
}
