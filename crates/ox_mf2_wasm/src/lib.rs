// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! WASM binding foundation for ox-mf2.
//!
//! This crate keeps the browser/edge boundary thin and delegates parser,
//! snapshot, diagnostics, and source text behavior to `ox_mf2_parser`.

mod diagnostics;
mod error;
mod handles;
mod input;
mod options;
mod source_text;

use wasm_bindgen::prelude::*;

use ox_mf2_parser::{
    decode_snapshot, parse_batch_to_snapshot, parse_message_to_snapshot, BatchExecution,
    BatchParseOptions, ParseInput, ParseOptions, SnapshotOptions, SnapshotSourceMetadata,
};

use crate::error::js_error;
use crate::input::WasmParseInput;
use crate::options::{WasmParseBatchOptions, WasmParseOptions};

#[wasm_bindgen(js_name = parseMessageToSnapshot)]
pub fn parse_message_to_snapshot_js(input: JsValue, options: JsValue) -> Result<Vec<u8>, JsValue> {
    let input: WasmParseInput = from_js_value(input)?;
    let options: WasmParseOptions = from_js_value_or_default(options)?;
    let metadata = SnapshotSourceMetadata {
        path: input.path.as_deref(),
        locale: input.locale.as_deref(),
        message_id: input.message_id.as_deref(),
        base_offset: input.base_offset,
    };
    let snapshot = parse_message_to_snapshot(
        &input.source,
        Some(metadata),
        parse_options(&options),
        snapshot_options(
            options.include_diagnostics,
            options.include_source_text,
            options.include_trivia,
        ),
    )
    .map_err(|error| js_error(error.to_string()))?;
    Ok(snapshot.bytes)
}

#[wasm_bindgen(js_name = parseBatchToSnapshot)]
pub fn parse_batch_to_snapshot_js(items: JsValue, options: JsValue) -> Result<Vec<u8>, JsValue> {
    let items: Vec<WasmParseInput> = from_js_value(items)?;
    let options: WasmParseBatchOptions = from_js_value_or_default(options)?;
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
    let snapshot = parse_batch_to_snapshot(
        &inputs,
        BatchParseOptions {
            execution: match options.batch_execution.as_str() {
                "parallel" => BatchExecution::Parallel,
                "sequential" => BatchExecution::Sequential,
                other => {
                    return Err(js_error(format!(
                        "Invalid batchExecution '{other}'. Expected 'sequential' or 'parallel'."
                    )))
                }
            },
            max_threads: None,
            preserve_order: true,
            parse: ParseOptions {
                recovery: true,
                parse_semantic: false,
                collect_trivia: options.collect_trivia,
            },
        },
        snapshot_options(
            options.include_diagnostics,
            options.include_source_text,
            options.include_trivia,
        ),
    )
    .map_err(|error| js_error(error.to_string()))?;
    Ok(snapshot.bytes)
}

#[wasm_bindgen(js_name = decodeSnapshotBytes)]
pub fn decode_snapshot_bytes(bytes: Vec<u8>) -> Result<Vec<u8>, JsValue> {
    decode_snapshot(&bytes).map_err(|error| js_error(error.to_string()))?;
    Ok(bytes)
}

fn parse_options(options: &WasmParseOptions) -> ParseOptions {
    ParseOptions {
        recovery: true,
        parse_semantic: false,
        collect_trivia: options.collect_trivia,
    }
}

const fn snapshot_options(
    include_diagnostics: bool,
    include_source_text: bool,
    include_trivia: bool,
) -> SnapshotOptions {
    SnapshotOptions {
        include_diagnostics,
        include_source_text,
        include_trivia,
    }
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
