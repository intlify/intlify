// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! WASM binding for the dedicated formatter package.
//!
//! JavaScript validates raw public inputs before they cross this boundary. The
//! WASM crate decodes snapshots, calls `intlify_format`, and serializes the
//! same discriminated result shape exposed by the N-API formatter package.

mod error;
mod options;
mod result;

use intlify_format::OperationalError;
use intlify_format::{check_format as core_check_format, check_snapshot as core_check_snapshot};
use intlify_format::{
    format_message as core_format_message, format_snapshot as core_format_snapshot,
};
use ox_mf2_parser::{decode_snapshot_owned, DecodeError, DecodeErrorCode};
use wasm_bindgen::prelude::*;

use crate::error::js_error;
use crate::options::WasmFormatOptions;
use crate::result::{WasmNativeFormatCheckResult, WasmNativeFormatResult};

#[wasm_bindgen(js_name = formatMessage)]
pub fn format_message_js(source: String, options: JsValue) -> Result<JsValue, JsValue> {
    let options: WasmFormatOptions = from_js_value_or_default(options)?;
    to_js_value(&WasmNativeFormatResult::from_format_result(
        core_format_message(&source, options.format_options()),
    ))
}

#[wasm_bindgen(js_name = checkFormat)]
pub fn check_format_js(source: String, options: JsValue) -> Result<JsValue, JsValue> {
    let options: WasmFormatOptions = from_js_value_or_default(options)?;
    to_js_value(&WasmNativeFormatCheckResult::from_check_result(
        core_check_format(&source, options.format_options()),
    ))
}

#[wasm_bindgen(js_name = formatSnapshot)]
pub fn format_snapshot_js(
    snapshot: Vec<u8>,
    source: String,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let options: WasmFormatOptions = from_js_value_or_default(options)?;
    let snapshot = match decode_snapshot_owned(snapshot) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return to_js_value(&WasmNativeFormatResult::from_operational_error(
                decode_error_to_invalid_snapshot(error),
            ));
        }
    };
    to_js_value(&WasmNativeFormatResult::from_format_result(
        core_format_snapshot(&source, snapshot.view(), options.format_options()),
    ))
}

#[wasm_bindgen(js_name = checkSnapshot)]
pub fn check_snapshot_js(
    snapshot: Vec<u8>,
    source: String,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let options: WasmFormatOptions = from_js_value_or_default(options)?;
    let snapshot = match decode_snapshot_owned(snapshot) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return to_js_value(&WasmNativeFormatCheckResult::from_operational_error(
                decode_error_to_invalid_snapshot(error),
            ));
        }
    };
    to_js_value(&WasmNativeFormatCheckResult::from_check_result(
        core_check_snapshot(&source, snapshot.view(), options.format_options()),
    ))
}

fn decode_error_to_invalid_snapshot(error: DecodeError) -> OperationalError {
    let reason = match error.code {
        DecodeErrorCode::UnsupportedMajorVersion | DecodeErrorCode::UnsupportedMinorVersion => {
            "unsupported_version"
        }
        _ => "corrupt",
    };
    OperationalError::invalid_snapshot(error.to_string(), reason)
        .with_detail("decodeCode", error.code.name())
}

fn from_js_value<T>(value: JsValue) -> Result<T, JsValue>
where
    T: serde::de::DeserializeOwned,
{
    serde_wasm_bindgen::from_value(value).map_err(|error| js_error(error.to_string()))
}

fn from_js_value_or_default<T>(value: JsValue) -> Result<T, JsValue>
where
    T: serde::de::DeserializeOwned + Default,
{
    if value.is_undefined() || value.is_null() {
        return Ok(T::default());
    }
    from_js_value(value)
}

fn to_js_value<T>(value: &T) -> Result<JsValue, JsValue>
where
    T: serde::Serialize,
{
    serde_wasm_bindgen::to_value(value).map_err(|error| js_error(error.to_string()))
}
