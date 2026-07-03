// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Public formatter entry points.

use ox_mf2_parser::{parse_message, SnapshotView};

use crate::{
    error::OperationalError,
    format::format_parse_result,
    result::{FormatCheckResult, FormatCheckSuccess, FormatFailure, FormatResult, FormatSuccess},
    FormatOptions,
};

/// Format one complete MF2 message.
///
/// Parser diagnostics are a hard stop: invalid input returns
/// [`FormatFailure`] with diagnostics and never exposes partially formatted
/// output.
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

/// Check whether one complete MF2 message would change after formatting.
///
/// This follows the same parser-diagnostics gate as [`format_message`] but
/// omits formatted text from the success result.
pub fn check_format(source: &str, options: FormatOptions) -> FormatCheckResult {
    format_message(source, options).map(|success| FormatCheckSuccess {
        changed: success.changed,
    })
}

/// Format using a precomputed binary snapshot.
///
/// PR 1 defines the public API shape only. Snapshot-backed formatting is
/// implemented with the real snapshot capability checks in the core rules PR.
pub fn format_snapshot(
    _source: &str,
    _snapshot: SnapshotView<'_>,
    _options: FormatOptions,
) -> FormatResult {
    Err(FormatFailure::from_error(
        OperationalError::missing_snapshot_capability("formatSnapshot is not implemented yet"),
    ))
}

/// Check whether snapshot-backed formatting would change the supplied source.
///
/// This delegates to [`format_snapshot`] so the snapshot validation and
/// diagnostics behavior stay identical between format and check APIs.
pub fn check_snapshot(
    source: &str,
    snapshot: SnapshotView<'_>,
    options: FormatOptions,
) -> FormatCheckResult {
    format_snapshot(source, snapshot, options).map(|success| FormatCheckSuccess {
        changed: success.changed,
    })
}
