// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Public formatter entry points.

use ox_mf2_parser::{
    parse_message, snapshot::RootView, Diagnostic, ParseResult, RootId, SectionKind, SnapshotView,
    SourceFileInput, SourceStore,
};

use crate::{
    error::OperationalError,
    format::{format_parse_result, format_parsed_input, format_snapshot_view},
    parsed::ParsedFormatInput,
    result::{FormatCheckResult, FormatCheckSuccess, FormatFailure, FormatResult, FormatSuccess},
    FormatOptions,
};

/// Format one complete MF2 message.
///
/// Parser diagnostics are a hard stop: invalid input returns
/// [`FormatFailure`] with diagnostics and never exposes partially formatted
/// output.
pub fn format_message(source: &str, options: FormatOptions) -> FormatResult {
    #[cfg(test)]
    test_observer::record_source_parse();
    let parse = parse_message(source).map_err(|error| {
        FormatFailure::from_error(
            OperationalError::internal(format!("MF2 parser failed: {error}"))
                .with_detail("parser_code", error.code().name()),
        )
    })?;
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

/// Format one complete MF2 message from its original in-memory parser artifacts.
///
/// This path validates the observable attachment between `sources` and
/// `result`, then enters the same Layout IR and renderer as [`format_message`]
/// without parsing again or converting through a binary snapshot.
/// Callers must retain the owner pair produced by one parse; rebuilding a
/// store with equal text or a coincidentally equal source id does not satisfy
/// that contract even when no inconsistency remains observable.
pub fn format_parsed(
    sources: &SourceStore,
    result: &ParseResult,
    options: FormatOptions,
) -> FormatResult {
    let input = ParsedFormatInput::new(sources, result).map_err(FormatFailure::from_error)?;
    if !result.diagnostics.is_empty() {
        return Err(FormatFailure::from_diagnostics(result.diagnostics.clone()));
    }
    let input = input
        .validate_options(options)
        .map_err(FormatFailure::from_error)?;
    let source = input.source();
    let code = format_parsed_input(input, options).map_err(FormatFailure::from_error)?;
    Ok(FormatSuccess {
        changed: code != source,
        code,
    })
}

/// Check whether attached in-memory parser artifacts would format differently.
///
/// This delegates to [`format_parsed`] so attachment validation, parser
/// diagnostics, and preserve-mode trivia requirements stay identical.
pub fn check_parsed(
    sources: &SourceStore,
    result: &ParseResult,
    options: FormatOptions,
) -> FormatCheckResult {
    format_parsed(sources, result, options).map(|success| FormatCheckSuccess {
        changed: success.changed,
    })
}

/// Format using a precomputed binary snapshot.
pub fn format_snapshot(
    source: &str,
    snapshot: SnapshotView<'_>,
    options: FormatOptions,
) -> FormatResult {
    #[cfg(test)]
    test_observer::record_snapshot_adapter();
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

#[cfg(test)]
mod test_observer {
    use std::cell::Cell;

    thread_local! {
        static SOURCE_PARSE_CALLS: Cell<usize> = const { Cell::new(0) };
        static SNAPSHOT_ADAPTER_CALLS: Cell<usize> = const { Cell::new(0) };
    }

    pub(super) fn record_source_parse() {
        SOURCE_PARSE_CALLS.set(SOURCE_PARSE_CALLS.get() + 1);
    }

    pub(super) fn record_snapshot_adapter() {
        SNAPSHOT_ADAPTER_CALLS.set(SNAPSHOT_ADAPTER_CALLS.get() + 1);
    }

    pub(super) fn reset() {
        SOURCE_PARSE_CALLS.set(0);
        SNAPSHOT_ADAPTER_CALLS.set(0);
    }

    pub(super) fn counts() -> (usize, usize) {
        (SOURCE_PARSE_CALLS.get(), SNAPSHOT_ADAPTER_CALLS.get())
    }
}

#[cfg(test)]
mod tests {
    use ox_mf2_parser::{parse_source, ParseOptions, SourceFileInput, SourceStore};

    use super::{check_parsed, format_parsed, test_observer};
    use crate::FormatOptions;

    #[test]
    fn parsed_apis_do_not_enter_source_parse_or_snapshot_adapters() {
        let source = ".input {$count :number}\n{{Value {$count}}}";
        let mut sources = SourceStore::with_capacity(1);
        let source_id = sources.add(SourceFileInput {
            source,
            ..SourceFileInput::default()
        });
        let result = parse_source(&sources, source_id, ParseOptions::default())
            .expect("source should parse");
        test_observer::reset();

        format_parsed(&sources, &result, FormatOptions::default())
            .expect("parsed format should succeed");
        check_parsed(&sources, &result, FormatOptions::default())
            .expect("parsed check should succeed");

        assert_eq!(test_observer::counts(), (0, 0));
    }
}
