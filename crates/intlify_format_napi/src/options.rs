// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_format::{FormatMode, FormatOptions};
use napi_derive::napi;

/// Format options that have already passed JavaScript-side validation.
#[napi(object)]
#[derive(Default)]
pub struct JsFormatOptions {
    pub mode: Option<String>,
}

impl JsFormatOptions {
    pub(crate) fn format_options(&self) -> FormatOptions {
        FormatOptions {
            mode: match self.mode.as_deref() {
                Some("preserve") => FormatMode::Preserve,
                _ => FormatMode::Standard,
            },
        }
    }
}
