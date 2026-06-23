// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use napi_derive::napi;

#[napi(object)]
#[allow(dead_code)]
pub struct JsSpan {
    pub start: u32,
    pub end: u32,
}
