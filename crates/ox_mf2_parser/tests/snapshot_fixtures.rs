// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Snapshot fixture runner: walks `fixtures/snapshot/valid/` and, for
//! each `.mf2` input, locks down both the binary snapshot bytes
//! (`.bin`) and the human-reviewable decoded dump (`.txt`).
//!
//! Run with `UPDATE_SNAPSHOTS=1` after an intentional snapshot format
//! or parser-output change. Don't forget to also update
//! `design/003-ox-mf2-binary-ast-format-changelog.md` for any wire
//! format change.

use std::fs;
use std::path::{Path, PathBuf};

use ox_mf2_parser::snapshot::{
    decode_snapshot, parse_result_to_snapshot, SnapshotOptions, SnapshotView,
};
use ox_mf2_parser::{parse_source, ParseOptions, SourceFileInput, SourceStore, SyntaxKind};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("snapshot")
        .join("valid")
}

fn discover() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(fixture_root()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "mf2") {
            out.push(path);
        }
    }
    out.sort();
    out
}

#[test]
fn snapshot_fixtures_round_trip() {
    let update = std::env::var("UPDATE_SNAPSHOTS").is_ok();
    let fixtures = discover();
    assert!(
        !fixtures.is_empty(),
        "no fixtures discovered under {}",
        fixture_root().display()
    );

    let mut failures = Vec::new();
    for path in &fixtures {
        let stem = path
            .file_stem()
            .expect("fixture stem")
            .to_string_lossy()
            .into_owned();
        let input = fs::read_to_string(path).expect("read .mf2");
        let mut sources = SourceStore::new();
        let id = sources.add(SourceFileInput {
            source: &input,
            ..Default::default()
        });
        let result = parse_source(&sources, id, ParseOptions::default());
        let snap = parse_result_to_snapshot(&sources, &result, SnapshotOptions::default())
            .expect("snapshot encode succeeds");

        let bin_path = path.with_extension("bin");
        let txt_path = path.with_extension("txt");

        if update {
            fs::write(&bin_path, &snap.bytes).expect("write .bin");
        } else {
            match fs::read(&bin_path) {
                Ok(expected) if expected == snap.bytes => (),
                Ok(expected) => {
                    failures.push(format!(
                        "[{stem}] .bin mismatch (expected {} bytes, got {} bytes)",
                        expected.len(),
                        snap.bytes.len()
                    ));
                }
                Err(_) => {
                    failures.push(format!(
                        "[{stem}] no .bin fixture at {} — rerun with UPDATE_SNAPSHOTS=1",
                        bin_path.display()
                    ));
                }
            }
        }

        let view = decode_snapshot(&snap.bytes).expect("decode succeeds");
        let dump = render_dump(&view);

        if update {
            fs::write(&txt_path, &dump).expect("write .txt");
        } else {
            match fs::read_to_string(&txt_path) {
                Ok(expected) if expected == dump => (),
                Ok(expected) => failures.push(format!(
                    "[{stem}] .txt mismatch\n--- expected ---\n{expected}--- actual ---\n{dump}"
                )),
                Err(_) => failures.push(format!(
                    "[{stem}] no .txt fixture at {} — rerun with UPDATE_SNAPSHOTS=1",
                    txt_path.display()
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

fn render_dump(view: &SnapshotView<'_>) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let sections = view.sections();
    let _ = writeln!(out, "SNAPSHOT v0.1");
    let _ = writeln!(out, "bytes: {}", view.as_bytes().len());
    let _ = writeln!(out, "section_count: {}", section_count(sections));

    let _ = writeln!(out);
    let _ = writeln!(out, "roots:");
    for i in 0..view.root_count() {
        let root = view.root(ox_mf2_parser::snapshot::RootId::new(i)).unwrap();
        let (start, count) = root.diagnostic_range();
        let _ = writeln!(
            out,
            "  [{i}] root=node#{} source=source#{} diags=[{start},{}) (count={count})",
            root.root_node().raw(),
            root.source_id().raw(),
            start + count
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "sources:");
    for i in 0..view.source_count() {
        let source = view
            .source(ox_mf2_parser::SourceId::new(i))
            .expect("source view");
        let text_display = source
            .text()
            .map_or_else(|| "-".to_owned(), |t| format!("{t:?}"));
        let _ = writeln!(
            out,
            "  [{i}] path={} locale={} message_id={} base_offset={} text={text_display}",
            opt_display(source.path()),
            opt_display(source.locale()),
            opt_display(source.message_id()),
            source.base_offset()
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "nodes:");
    for i in 0..view.node_count() {
        let node = view.node(ox_mf2_parser::NodeId::new(i)).unwrap();
        let _ = writeln!(
            out,
            "  [{i}] {:?} @ {}..{} children={}",
            node.kind(),
            node.span().start,
            node.span().end,
            node.child_count()
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "edges:");
    for i in 0..sections.edges.count {
        let off = sections.edges.offset as usize + i as usize * 8;
        let kind = u16::from_le_bytes(view.as_bytes()[off..off + 2].try_into().unwrap());
        let ref_id = u32::from_le_bytes(view.as_bytes()[off + 4..off + 8].try_into().unwrap());
        let label = match kind {
            ox_mf2_parser::snapshot::EDGE_KIND_NODE => format!("node#{ref_id}"),
            ox_mf2_parser::snapshot::EDGE_KIND_TOKEN => format!("token#{ref_id}"),
            other => format!("?#{ref_id}(kind={other})"),
        };
        let _ = writeln!(out, "  [{i}] -> {label}");
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "tokens:");
    for i in 0..view.token_count() {
        let token = view.token(ox_mf2_parser::TokenId::new(i)).unwrap();
        let lead_n = token.leading_trivia().count();
        let trail_n = token.trailing_trivia().count();
        let _ = writeln!(
            out,
            "  [{i}] {:?} @ {}..{} source=source#{} leading={lead_n} trailing={trail_n}",
            token.kind(),
            token.span().start,
            token.span().end,
            token.source_id().raw(),
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "trivia:");
    if view.trivia_count() == 0 {
        let _ = writeln!(out, "  (none)");
    } else {
        for i in 0..view.trivia_count() {
            let trivia = view
                .trivia(ox_mf2_parser::TriviaId::new(i))
                .expect("trivia view");
            let _ = writeln!(
                out,
                "  [{i}] {:?} @ {}..{} source=source#{}",
                trivia.kind(),
                trivia.span().start,
                trivia.span().end,
                trivia.source_id().raw(),
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "diagnostics:");
    if view.diagnostic_count() == 0 {
        let _ = writeln!(out, "  (none)");
    } else {
        for i in 0..view.diagnostic_count() {
            let diag = view.diagnostic(i).expect("diagnostic view");
            let (lstart, lcount) = diag.label_range();
            let _ = writeln!(
                out,
                "  [{i}] {:?} {:?} @ {}..{} source=source#{} labels=[{lstart},{}) msg={}",
                diag.severity(),
                diag.code(),
                diag.span().start,
                diag.span().end,
                diag.source_id().raw(),
                lstart + lcount,
                opt_display(diag.message()),
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "strings:");
    if sections.string_offsets.count == 0 {
        let _ = writeln!(out, "  (none)");
    } else {
        for i in 0..sections.string_offsets.count {
            let id = ox_mf2_parser::snapshot::StringId::new(i);
            let s = view.string(id).unwrap_or("<bad utf8>");
            let _ = writeln!(out, "  [{i}] {s:?}");
        }
    }
    // Suppress unused-import warnings on builds that don't see them
    // through the test surface.
    let _ = SyntaxKind::Tombstone;
    out
}

fn opt_display(value: Option<&str>) -> String {
    match value {
        Some(s) => format!("{s:?}"),
        None => "-".to_owned(),
    }
}

fn section_count(sections: &ox_mf2_parser::snapshot::SectionIndex) -> usize {
    let mut n = 0;
    // Required sections are always present.
    n += 7;
    if sections.trivia.is_some() {
        n += 1;
    }
    if sections.diagnostics.is_some() {
        n += 1;
    }
    if sections.diagnostic_labels.is_some() {
        n += 1;
    }
    if sections.source_text_data.is_some() {
        n += 1;
    }
    if sections.extended_data.is_some() {
        n += 1;
    }
    n
}
