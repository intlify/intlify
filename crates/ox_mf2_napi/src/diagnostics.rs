// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use napi_derive::napi;

#[napi(object)]
#[allow(dead_code)]
pub struct JsDiagnostic {
    pub root_id: u32,
    pub source_id: u32,
    pub severity: u32,
    pub code: u32,
    pub message: Option<String>,
}
