// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Public formatter entry points.

use ox_mf2_parser::{
    parse_message, snapshot::RootView, Diagnostic, RootId, SectionKind, SnapshotView,
    SourceFileInput, SourceStore,
};

use crate::{
    error::OperationalError,
    format::{format_parse_result, format_snapshot_view},
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
pub fn format_snapshot(
    source: &str,
    snapshot: SnapshotView<'_>,
    options: FormatOptions,
) -> FormatResult {
    validate_snapshot_request(source, snapshot, options).map_err(FormatFailure::from_error)?;
    let root = snapshot.root(RootId::new(0)).ok_or_else(|| {
        FormatFailure::from_error(OperationalError::invalid_snapshot(
            "formatter snapshot root is missing",
            "corrupt",
        ))
    })?;

    if root.diagnostic_range().1 > 0 {
        return Err(FormatFailure::from_diagnostics(snapshot_diagnostics(
            source, root,
        )));
    }

    let code =
        format_snapshot_view(source, snapshot, options).map_err(FormatFailure::from_error)?;
    Ok(FormatSuccess {
        changed: code != source,
        code,
    })
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

fn validate_snapshot_request(
    source: &str,
    snapshot: SnapshotView<'_>,
    options: FormatOptions,
) -> Result<(), OperationalError> {
    if snapshot.root_count() != 1 {
        return Err(OperationalError::invalid_snapshot(
            "formatter snapshots must contain exactly one root",
            "corrupt",
        ));
    }

    let root = snapshot.root(RootId::new(0)).ok_or_else(|| {
        OperationalError::invalid_snapshot("formatter snapshot root is missing", "corrupt")
    })?;
    let snapshot_source = snapshot.source(root.source_id()).ok_or_else(|| {
        OperationalError::invalid_snapshot("formatter snapshot source is missing", "corrupt")
    })?;

    if snapshot.section(SectionKind::Diagnostics).is_none() {
        return Err(OperationalError::missing_snapshot_capability(
            "formatter snapshots require diagnostic records",
        ));
    }

    if let Some(snapshot_text) = snapshot_source.text() {
        if snapshot_text != source {
            return Err(OperationalError::source_snapshot_mismatch(
                "snapshot source text does not match supplied source",
            ));
        }
    }

    if options.mode == crate::FormatMode::Preserve
        && snapshot.section(SectionKind::Trivia).is_none()
    {
        return Err(OperationalError::missing_snapshot_capability(
            "preserve mode requires snapshot trivia records",
        ));
    }

    Ok(())
}

fn snapshot_diagnostics(source: &str, root: RootView<'_>) -> Vec<Diagnostic> {
    let mut sources = SourceStore::with_capacity(1);
    let local_source = sources.add(SourceFileInput {
        source,
        ..Default::default()
    });

    root.diagnostics()
        .map(|diagnostic| {
            let code = diagnostic.code();
            let span = diagnostic.span();
            Diagnostic {
                source: diagnostic.source_id(),
                span,
                location: sources.location(local_source, span),
                severity: diagnostic.severity(),
                code,
                message: code.static_message(),
                labels: Vec::new(),
            }
        })
        .collect()
}
