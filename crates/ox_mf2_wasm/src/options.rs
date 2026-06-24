// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmParseOptions {
    #[serde(default = "default_true")]
    pub collect_trivia: bool,
    #[serde(default = "default_true")]
    pub include_trivia: bool,
    #[serde(default = "default_true")]
    pub include_diagnostics: bool,
    #[serde(default)]
    pub include_source_text: bool,
}

impl Default for WasmParseOptions {
    fn default() -> Self {
        Self {
            collect_trivia: true,
            include_trivia: true,
            include_diagnostics: true,
            include_source_text: false,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmParseBatchOptions {
    #[serde(default = "default_true")]
    pub collect_trivia: bool,
    #[serde(default = "default_true")]
    pub include_trivia: bool,
    #[serde(default = "default_true")]
    pub include_diagnostics: bool,
    #[serde(default)]
    pub include_source_text: bool,
    #[serde(default = "default_batch_execution")]
    pub batch_execution: String,
}

impl Default for WasmParseBatchOptions {
    fn default() -> Self {
        Self {
            collect_trivia: true,
            include_trivia: true,
            include_diagnostics: true,
            include_source_text: false,
            batch_execution: default_batch_execution(),
        }
    }
}

const fn default_true() -> bool {
    true
}

fn default_batch_execution() -> String {
    "sequential".to_string()
}
