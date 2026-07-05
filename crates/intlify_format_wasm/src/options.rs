// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_format::{FormatMode, FormatOptions};
use serde::Deserialize;

/// Format options that have already passed JavaScript-side validation.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmFormatOptions {
    #[serde(default)]
    mode: Option<String>,
}

impl WasmFormatOptions {
    pub(crate) fn format_options(&self) -> FormatOptions {
        FormatOptions {
            mode: match self.mode.as_deref() {
                Some("preserve") => FormatMode::Preserve,
                _ => FormatMode::Standard,
            },
        }
    }
}
