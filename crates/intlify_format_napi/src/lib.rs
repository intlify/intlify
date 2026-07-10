// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! N-API binding for the dedicated formatter package.
//!
//! This crate keeps JavaScript distribution concerns thin: raw JS argument
//! validation lives in `@intlify/format-napi`, while native code decodes
//! snapshots, calls `intlify_format`, and maps core results to JS objects.

mod options;
mod result;

use intlify_format::OperationalError;
use intlify_format::{check_format as core_check_format, check_snapshot as core_check_snapshot};
use intlify_format::{
    format_message as core_format_message, format_snapshot as core_format_snapshot,
};
use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
use ox_mf2_parser::snapshot::{SNAPSHOT_MAJOR_VERSION, SNAPSHOT_MINOR_VERSION};
use ox_mf2_parser::{decode_snapshot_owned, DecodeError, DecodeErrorCode};

use crate::options::JsFormatOptions;
use crate::result::{JsNativeFormatCheckResult, JsNativeFormatResult};

#[napi]
pub fn format_message(
    source: String,
    options: Option<JsFormatOptions>,
) -> napi::Result<JsNativeFormatResult> {
    let options = options.unwrap_or_default().format_options();
    Ok(JsNativeFormatResult::from_format_result(
        core_format_message(&source, options),
    ))
}

#[napi]
pub fn check_format(
    source: String,
    options: Option<JsFormatOptions>,
) -> napi::Result<JsNativeFormatCheckResult> {
    let options = options.unwrap_or_default().format_options();
    Ok(JsNativeFormatCheckResult::from_check_result(
        core_check_format(&source, options),
    ))
}

#[napi]
pub fn format_snapshot(
    snapshot: Uint8Array,
    source: String,
    options: Option<JsFormatOptions>,
) -> napi::Result<JsNativeFormatResult> {
    let options = options.unwrap_or_default().format_options();
    let snapshot = match decode_snapshot_owned(snapshot.to_vec()) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return Ok(JsNativeFormatResult::from_operational_error(
                decode_error_to_invalid_snapshot(error),
            ));
        }
    };
    Ok(JsNativeFormatResult::from_format_result(
        core_format_snapshot(&source, snapshot.view(), options),
    ))
}

#[napi]
pub fn check_snapshot(
    snapshot: Uint8Array,
    source: String,
    options: Option<JsFormatOptions>,
) -> napi::Result<JsNativeFormatCheckResult> {
    let options = options.unwrap_or_default().format_options();
    let snapshot = match decode_snapshot_owned(snapshot.to_vec()) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return Ok(JsNativeFormatCheckResult::from_operational_error(
                decode_error_to_invalid_snapshot(error),
            ));
        }
    };
    Ok(JsNativeFormatCheckResult::from_check_result(
        core_check_snapshot(&source, snapshot.view(), options),
    ))
}

fn decode_error_to_invalid_snapshot(error: DecodeError) -> OperationalError {
    let operational_error = match error.code {
        DecodeErrorCode::UnsupportedMajorVersion | DecodeErrorCode::UnsupportedMinorVersion => {
            let version = error
                .version
                .expect("unsupported version errors carry the decoded header version");
            OperationalError::unsupported_snapshot_version(
                error.to_string(),
                version.major,
                version.minor,
                &[(SNAPSHOT_MAJOR_VERSION, SNAPSHOT_MINOR_VERSION)],
            )
        }
        _ => OperationalError::invalid_snapshot(error.to_string(), "corrupt"),
    };
    operational_error.with_detail("decodeCode", error.code.name())
}
