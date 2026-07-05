// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use ox_mf2_parser::{ParseResult, SnapshotView, SourceFileInput, SourceStore};

use crate::{
    error::OperationalError, layout::LayoutDocument, options::FormatOptions, render::render,
};

/// Run the formatter pipeline after the public parser-diagnostics gate.
pub(crate) fn format_parse_result(
    source: &str,
    parse: &ParseResult,
    options: FormatOptions,
) -> Result<String, OperationalError> {
    ensure_parse_invariant(parse)?;

    let mut sources = SourceStore::with_capacity(1);
    let source_id = sources.add(SourceFileInput {
        source,
        ..Default::default()
    });
    debug_assert_eq!(source_id, parse.source);

    let layout = LayoutDocument::from_parse(source, &sources, parse, options)?;
    let document = layout.into_document();
    render(&document, source)
}

pub(crate) fn format_snapshot_view(
    source: &str,
    snapshot: SnapshotView<'_>,
    options: FormatOptions,
) -> Result<String, OperationalError> {
    let layout = LayoutDocument::from_snapshot(source, snapshot, options)?;
    let document = layout.into_document();
    render(&document, source)
}

// This is defensive because public APIs already reject parser diagnostics.
// Keeping the check here documents the IR boundary: formatter internals should
// only see syntactically valid parser output.
fn ensure_parse_invariant(parse: &ParseResult) -> Result<(), OperationalError> {
    if parse.diagnostics.is_empty() {
        Ok(())
    } else {
        Err(OperationalError::internal(
            "formatter received parser diagnostics after strict diagnostics gate",
        )
        .with_detail("phase", "layout_ir_construction"))
    }
}
