// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Permanent parser conformance smoke over the vendored Unicode `MessageFormat`
//! WG test corpus.
//!
//! The WG corpus covers formatting and data-model behavior too. This parser
//! test only checks the syntax boundary: tests expecting `syntax-error` must
//! produce at least one parser diagnostic, while tests without `syntax-error`
//! must parse with zero parser diagnostics. Semantic/function errors are left
//! to later phases.

use ox_mf2_parser::{parse_source, ParseOptions, SourceFileInput, SourceStore};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn wg_tests_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/spec/message-format-wg/tests")
}

fn discover_json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read WG test directory") {
        let path = entry.expect("read WG test entry").path();
        if path.is_dir() {
            discover_json_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path);
        }
    }
}

fn exp_errors<'a>(test: &'a Value, defaults: &'a Value) -> Option<&'a [Value]> {
    test.get("expErrors")
        .or_else(|| defaults.get("expErrors"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
}

fn expects_syntax_error(test: &Value, defaults: &Value) -> bool {
    exp_errors(test, defaults).is_some_and(|errors| {
        errors
            .iter()
            .any(|error| error.get("type").and_then(Value::as_str) == Some("syntax-error"))
    })
}

#[test]
fn wg_message_format_tests_match_parser_syntax_boundary() {
    let mut files = Vec::new();
    discover_json_files(&wg_tests_root(), &mut files);
    files.sort();
    assert!(!files.is_empty(), "no WG test JSON files discovered");

    let mut failures = Vec::new();
    let mut total = 0usize;

    for file in files {
        let content = fs::read_to_string(&file).expect("read WG test JSON");
        let suite: Value = serde_json::from_str(&content).expect("parse WG test JSON");
        let defaults = suite.get("defaultTestProperties").unwrap_or(&Value::Null);
        let tests = suite
            .get("tests")
            .and_then(Value::as_array)
            .expect("WG test suite has tests");

        for (index, test) in tests.iter().enumerate() {
            total += 1;
            let Some(source) = test
                .get("src")
                .or_else(|| defaults.get("src"))
                .and_then(Value::as_str)
            else {
                failures.push(format!("{}#{index}: missing src", file.display()));
                continue;
            };

            let mut sources = SourceStore::new();
            let id = sources.add(SourceFileInput {
                source,
                path: file.to_str(),
                ..Default::default()
            });
            let options = ParseOptions {
                parse_semantic: false,
                ..Default::default()
            };
            let result = parse_source(&sources, id, options);
            let wants_syntax_error = expects_syntax_error(test, defaults);
            let has_parser_error = !result.diagnostics.is_empty();

            if wants_syntax_error != has_parser_error {
                let description = test
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("<no description>");
                failures.push(format!(
                    "{}#{index} {description:?}: expected syntax_error={wants_syntax_error}, got diagnostics {:?} for source {:?}",
                    file.display(),
                    result
                        .diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic.code)
                        .collect::<Vec<_>>(),
                    source
                ));
            }
        }
    }

    assert!(total > 0, "WG test corpus was empty");
    assert!(
        failures.is_empty(),
        "{} of {total} WG parser conformance tests failed:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
