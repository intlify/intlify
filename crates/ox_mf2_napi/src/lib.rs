// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! N-API binding foundation for ox-mf2.
//!
//! This crate is intentionally thin. It delegates parser, snapshot writer,
//! decoder, diagnostics, and source text policy to `ox_mf2_parser`.

mod diagnostics;
mod error;
mod handles;
mod input;
mod options;
mod snapshot;
mod source_text;

use napi::bindgen_prelude::Uint8Array;
use napi_derive::napi;
use ox_mf2_parser::{
    decode_snapshot_owned, parse_batch_to_snapshot, parse_message_to_snapshot, ParseInput,
    SnapshotSourceMetadata,
};

use crate::error::{decode, snapshot_write};
use crate::input::{JsParseBatchInput, JsParseMessageInput};
use crate::options::{JsDecodeSnapshotOptions, JsParseBatchOptions, JsParseMessageOptions};
use crate::snapshot::{JsNativeSnapshotResult, JsSnapshotHandle};

#[napi]
pub fn parse_message(
    input: JsParseMessageInput,
    options: Option<JsParseMessageOptions>,
) -> napi::Result<JsNativeSnapshotResult> {
    let options = options.unwrap_or_default();
    let metadata = SnapshotSourceMetadata {
        path: input.path.as_deref(),
        locale: input.locale.as_deref(),
        message_id: input.message_id.as_deref(),
        base_offset: input.base_offset,
    };
    let snapshot = parse_message_to_snapshot(
        &input.source,
        Some(metadata),
        options.parse_options(),
        options.snapshot_options(),
    )
    .map_err(snapshot_write)?;
    Ok(JsNativeSnapshotResult {
        bytes: Uint8Array::from(snapshot.bytes),
        roots: vec![snapshot.root.raw()],
        execution: None,
        degraded: None,
    })
}

#[napi]
pub fn parse_batch(
    items: Vec<JsParseBatchInput>,
    options: Option<JsParseBatchOptions>,
) -> napi::Result<JsNativeSnapshotResult> {
    let options = options.unwrap_or_default();
    let inputs: Vec<ParseInput<'_>> = items
        .iter()
        .map(|item| ParseInput {
            source: &item.source,
            path: item.path.as_deref(),
            locale: item.locale.as_deref(),
            message_id: item.message_id.as_deref(),
            base_offset: item.base_offset,
        })
        .collect();
    let snapshot =
        parse_batch_to_snapshot(&inputs, options.batch_options(), options.snapshot_options())
            .map_err(snapshot_write)?;
    Ok(JsNativeSnapshotResult {
        bytes: Uint8Array::from(snapshot.bytes),
        roots: snapshot.roots.into_iter().map(|root| root.raw()).collect(),
        execution: Some(match snapshot.execution {
            ox_mf2_parser::BatchExecution::Sequential => "sequential".to_string(),
            ox_mf2_parser::BatchExecution::Parallel => "parallel".to_string(),
            _ => "sequential".to_string(),
        }),
        degraded: Some(snapshot.degraded),
    })
}

#[napi]
pub fn decode_snapshot(
    bytes: Uint8Array,
    _options: Option<JsDecodeSnapshotOptions>,
) -> napi::Result<JsNativeSnapshotResult> {
    let bytes = bytes.to_vec();
    let view = decode_snapshot_owned(bytes.clone()).map_err(decode)?;
    let root_count = view.view().root_count();
    Ok(JsNativeSnapshotResult {
        bytes: Uint8Array::from(bytes),
        roots: (0..root_count).collect(),
        execution: None,
        degraded: None,
    })
}

#[napi]
pub fn snapshot_to_bytes(snapshot: &JsSnapshotHandle) -> napi::Result<Uint8Array> {
    Ok(Uint8Array::from(snapshot.bytes().to_vec()))
}
