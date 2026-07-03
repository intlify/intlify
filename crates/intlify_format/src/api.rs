// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use ox_mf2_parser::{parse_message, SnapshotView};

use crate::{
    error::OperationalError,
    format::format_parse_result,
    result::{FormatCheckResult, FormatCheckSuccess, FormatFailure, FormatResult, FormatSuccess},
    FormatOptions,
};

pub fn format_message(source: &str, options: FormatOptions) -> FormatResult {
    let parse = parse_message(source);
    if !parse.diagnostics.is_empty() {
        return Err(FormatFailure::from_diagnostics(parse.diagnostics));
    }

    let code = format_parse_result(source, &parse, options).map_err(FormatFailure::from_error)?;
    Ok(FormatSuccess {
        changed: code != source,
        code,
    })
}

pub fn check_format(source: &str, options: FormatOptions) -> FormatCheckResult {
    format_message(source, options).map(|success| FormatCheckSuccess {
        changed: success.changed,
    })
}

pub fn format_snapshot(
    _source: &str,
    _snapshot: SnapshotView<'_>,
    _options: FormatOptions,
) -> FormatResult {
    Err(FormatFailure::from_error(
        OperationalError::missing_snapshot_capability("formatSnapshot is not implemented yet"),
    ))
}

pub fn check_snapshot(
    source: &str,
    snapshot: SnapshotView<'_>,
    options: FormatOptions,
) -> FormatCheckResult {
    format_snapshot(source, snapshot, options).map(|success| FormatCheckSuccess {
        changed: success.changed,
    })
}
