// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmParseInput {
    pub source: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub base_offset: Option<u32>,
}
