// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

#[allow(dead_code)]
pub(crate) struct WasmDiagnostic {
    pub root_id: u32,
    pub source_id: u32,
    pub severity: u32,
    pub code: u32,
}
