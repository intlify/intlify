// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Semantic fixture runner for parser-owned MF2 data-model diagnostics.
//!
//! Each source under `fixtures/semantic/` must parse without syntax
//! diagnostics. The runner then builds a source-backed semantic model and
//! locks diagnostic code, severity, primary span, and report order in the
//! adjacent `.snap` file. Set `UPDATE_SNAPSHOTS=1` to refresh snapshots after
//! an intentional semantic-contract change.

use ox_mf2_parser::{
    build_semantic_model, parse_source, validate_semantics, ParseOptions, SourceFileInput,
    SourceStore,
};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("semantic")
}

fn discover_fixtures() -> Vec<PathBuf> {
    let mut fixtures = fs::read_dir(fixture_root())
        .expect("semantic fixture directory exists")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "mf2"))
        .collect::<Vec<_>>();
    fixtures.sort();
    fixtures
}

fn render(source: &str, path: &Path) -> String {
    let mut sources = SourceStore::new();
    let source_id = sources.add(SourceFileInput {
        source,
        path: path.to_str(),
        ..Default::default()
    });
    let result = parse_source(&sources, source_id, ParseOptions::default())
        .expect("semantic fixture parses");
    assert!(
        result.diagnostics.is_empty(),
        "semantic fixture {} has parser diagnostics: {:?}",
        path.display(),
        result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>()
    );
    let model = build_semantic_model(&sources, &result).expect("semantic model builds");
    let diagnostics = validate_semantics(&model).expect("semantic validation succeeds");

    let mut snapshot = String::from("--semantic-diagnostics--\n");
    if diagnostics.is_empty() {
        snapshot.push_str("(none)\n");
    } else {
        for diagnostic in diagnostics {
            writeln!(
                snapshot,
                "{} {:?} @ {}..{}",
                diagnostic.code().json_code(),
                diagnostic.severity(),
                diagnostic.span().start,
                diagnostic.span().end,
            )
            .expect("write semantic snapshot");
        }
    }
    snapshot
}

#[test]
fn semantic_fixture_corpus_matches_snapshots() {
    let update = std::env::var("UPDATE_SNAPSHOTS").is_ok();
    let fixtures = discover_fixtures();
    assert!(!fixtures.is_empty(), "no semantic fixtures discovered");

    let mut failures = Vec::new();
    for fixture in fixtures {
        let source = fs::read_to_string(&fixture).expect("read semantic fixture");
        let actual = render(&source, &fixture);
        let snapshot = fixture.with_extension("snap");
        if update {
            fs::write(&snapshot, actual).expect("write semantic snapshot");
            continue;
        }
        match fs::read_to_string(&snapshot) {
            Ok(expected) if expected == actual => {}
            Ok(expected) => failures.push(format!(
                "{} differs\n--- expected ---\n{expected}--- actual ---\n{actual}",
                fixture.display()
            )),
            Err(_) => failures.push(format!(
                "{} is missing; rerun with UPDATE_SNAPSHOTS=1",
                snapshot.display()
            )),
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
