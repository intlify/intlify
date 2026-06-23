// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use napi_derive::napi;

#[napi]
pub struct JsSnapshotHandle {
    bytes: Vec<u8>,
}

#[napi(object)]
pub struct JsNativeSnapshotResult {
    pub bytes: napi::bindgen_prelude::Uint8Array,
    pub roots: Vec<u32>,
    pub execution: Option<String>,
    pub degraded: Option<bool>,
}

impl JsSnapshotHandle {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
