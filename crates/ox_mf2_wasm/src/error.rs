// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use wasm_bindgen::prelude::JsValue;

pub(crate) fn js_error(message: impl AsRef<str>) -> JsValue {
    JsValue::from_str(message.as_ref())
}
