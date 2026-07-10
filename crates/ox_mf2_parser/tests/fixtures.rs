// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fixture runner: walks `crates/ox_mf2_parser/fixtures/{spec,recovery,
//! implementation}/`, parses each `.mf2`, and diffs the deterministic
//! snapshot against the matching `.snap`. Set `UPDATE_SNAPSHOTS=1` in the
//! environment to regenerate snapshots after an intentional change.

#![allow(
    clippy::format_push_string,
    clippy::field_reassign_with_default,
    clippy::manual_assert
)]

use ox_mf2_parser::{
    parse_source, CstChild, CstNodeView, CstView, ParseOptions, SourceFileInput, SourceStore,
    SyntaxKind,
};
use std::fs;
use std::path::{Path, PathBuf};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn buckets() -> [&'static str; 3] {
    ["spec", "recovery", "implementation"]
}

fn discover_fixtures() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for bucket in buckets() {
        let dir = fixture_root().join(bucket);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "mf2") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn render_snapshot(view: &CstView<'_>, diagnostics: &[ox_mf2_parser::Diagnostic]) -> String {
    let mut buf = String::new();
    if let Some(root) = view.root() {
        render_node(root, 0, &mut buf);
    } else {
        buf.push_str("<no root>\n");
    }
    if diagnostics.is_empty() {
        buf.push_str("--diagnostics--\n");
        buf.push_str("(none)\n");
    } else {
        buf.push_str("--diagnostics--\n");
        for d in diagnostics {
            buf.push_str(&format!(
                "{:?} @ {}..{} (line {}, col {}): {}\n",
                d.code, d.span.start, d.span.end, d.location.line, d.location.column, d.message
            ));
        }
    }
    buf
}

fn render_node(node: CstNodeView<'_>, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    let span = node.span();
    out.push_str(&format!(
        "{}{:?} @ {}..{}\n",
        indent,
        node.kind(),
        span.start,
        span.end
    ));
    for child in node.children() {
        match child {
            CstChild::Node(n) => render_node(n, depth + 1, out),
            CstChild::Token(t) => {
                let tspan = t.span();
                let text = t.text().replace('\n', "\\n");
                out.push_str(&format!(
                    "{}  TOKEN {:?} @ {}..{} {:?}\n",
                    indent,
                    t.kind(),
                    tspan.start,
                    tspan.end,
                    text
                ));
            }
        }
    }
    let _ = SyntaxKind::Tombstone; // suppress unused warning in some configs
}

fn parse_meta(path: &Path) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(content) = fs::read_to_string(path) else {
        return out;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, raw_value)) = line.split_once('=') {
            let key = key.trim().to_owned();
            let value = raw_value.trim().trim_matches('"').to_owned();
            out.insert(key, value);
        }
    }
    out
}

#[test]
fn fixture_corpus_matches_snapshots() {
    let update = std::env::var("UPDATE_SNAPSHOTS").is_ok();
    let mut failures = Vec::new();
    let mut total = 0usize;

    for fixture in discover_fixtures() {
        total += 1;
        let stem = fixture
            .file_stem()
            .expect("fixture stem")
            .to_string_lossy()
            .into_owned();
        let bucket = fixture
            .parent()
            .and_then(|p| p.file_name())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();

        let input = fs::read_to_string(&fixture).expect("read fixture");
        let mut sources = SourceStore::new();
        let id = sources.add(SourceFileInput {
            source: &input,
            path: fixture.to_str(),
            ..Default::default()
        });
        let mut options = ParseOptions::default();
        options.parse_semantic = false;
        let result = parse_source(&sources, id, options).expect("fixture parses");
        let view = CstView::new(&sources, id, &result.cst);
        let actual = render_snapshot(&view, &result.diagnostics);

        let meta_path = fixture.with_extension("meta");
        let meta = parse_meta(&meta_path);
        if let Some(expected) = meta.get("diagnostics_expected") {
            if let Ok(expected) = expected.parse::<usize>() {
                if expected != result.diagnostics.len() {
                    failures.push(format!(
                        "[{bucket}/{stem}] expected {expected} diagnostics, got {} ({:?})",
                        result.diagnostics.len(),
                        result
                            .diagnostics
                            .iter()
                            .map(|d| d.code)
                            .collect::<Vec<_>>(),
                    ));
                }
            }
        }

        let snap_path = fixture.with_extension("snap");
        if update {
            fs::write(&snap_path, &actual).expect("write snapshot");
            continue;
        }
        match fs::read_to_string(&snap_path) {
            Ok(expected) => {
                if expected != actual {
                    failures.push(format!(
                        "[{bucket}/{stem}] snapshot mismatch\n--- expected ---\n{expected}--- actual ---\n{actual}"
                    ));
                }
            }
            Err(_) => {
                failures.push(format!(
                    "[{bucket}/{stem}] no snapshot at {} — rerun with UPDATE_SNAPSHOTS=1",
                    snap_path.display()
                ));
            }
        }
    }

    assert!(total > 0, "no fixtures discovered under fixtures/");
    if !failures.is_empty() {
        panic!(
            "{} of {} fixtures failed:\n{}",
            failures.len(),
            total,
            failures.join("\n\n")
        );
    }
}
