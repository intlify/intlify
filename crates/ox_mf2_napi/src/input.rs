// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use napi_derive::napi;

#[napi(object)]
pub struct JsParseMessageInput {
    pub source: String,
    pub path: Option<String>,
    pub locale: Option<String>,
    pub message_id: Option<String>,
    pub base_offset: Option<u32>,
}

#[napi(object)]
pub struct JsParseBatchInput {
    pub source: String,
    pub path: Option<String>,
    pub locale: Option<String>,
    pub message_id: Option<String>,
    pub base_offset: Option<u32>,
}
