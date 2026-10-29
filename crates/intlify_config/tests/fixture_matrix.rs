// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Phase 2, 3, and 4 fixture matrices point at tests that exist.
//!
//! `fixtures/phase2/README.md` maps design 015's Phase 2 coverage to the tests
//! that check it, in this crate and in the reference adapter, whose tests it
//! marks. `fixtures/phase3/README.md` and `fixtures/phase4/README.md` do the
//! same for Phases 3 and 4, whose tests are all in this crate. A test that was
//! renamed or removed would leave a row pointing at nothing, so every name a
//! matrix gives is looked up where its row says.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Every `.rs` file under `dir`, in path order.
fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("a readable directory")
        .map(|entry| entry.expect("a directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// The name of every `#[test]` function in one crate's sources.
fn test_names(crate_dir: &Path) -> BTreeSet<String> {
    let mut files = Vec::new();
    for dir in ["src", "tests", "benches"] {
        let dir = crate_dir.join(dir);
        if dir.is_dir() {
            sources(&dir, &mut files);
        }
    }
    let mut names = BTreeSet::new();
    for file in &files {
        let text = std::fs::read_to_string(file).expect("a readable source");
        for after in text.split("#[test]").skip(1) {
            // Other attributes may sit between the marker and the function.
            let Some(start) = after.find("fn ") else {
                continue;
            };
            let name: String = after[start + 3..]
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
                .collect();
            names.insert(name);
        }
    }
    names
}

/// Each test name in the last cell of a matrix row, and whether
/// `_(adapter)_` follows it. A test name is snake case; file names,
/// commands, and literals in the same cells are not.
fn named_tests(matrix: &str) -> Vec<(String, bool)> {
    let mut named = Vec::new();
    for row in matrix.lines().filter(|line| line.starts_with('|')) {
        let Some(checked) = row.trim_end_matches('|').rsplit(" | ").next() else {
            continue;
        };
        let mut rest = checked;
        while let Some(open) = rest.find('`') {
            let after = &rest[open + 1..];
            let close = after.find('`').expect("a closed code span");
            let name = &after[..close];
            rest = &after[close + 1..];
            let snake = name.contains('_')
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
            if snake {
                named.push((name.to_owned(), rest.starts_with(" _(adapter)_")));
            }
        }
    }
    named
}

fn matrix(phase: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(root.join(format!("fixtures/{phase}/README.md")))
        .expect("the fixture matrix")
}

#[test]
fn every_test_the_phase_2_matrix_names_is_a_test_where_it_says() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let here = test_names(root);
    let adapter = test_names(&root.join("../intlify_locale_icu4x"));
    assert!(here.len() > 300 && adapter.len() > 130);

    let named = named_tests(&matrix("phase2"));
    for (name, in_adapter) in &named {
        let (names, place) = if *in_adapter {
            (&adapter, "intlify_locale_icu4x")
        } else {
            (&here, "intlify_config")
        };
        assert!(
            names.contains(name),
            "the matrix names `{name}`, which is no test in {place}"
        );
    }
    assert!(
        named.len() > 140,
        "the matrix was read: {} names",
        named.len()
    );
}

#[test]
fn every_test_the_phase_3_matrix_names_is_a_test_in_this_crate() {
    let here = test_names(Path::new(env!("CARGO_MANIFEST_DIR")));
    let named = named_tests(&matrix("phase3"));
    for (name, in_adapter) in &named {
        assert!(!in_adapter, "Phase 3 names no adapter test: `{name}`");
        assert!(
            here.contains(name),
            "the matrix names `{name}`, which is no test in intlify_config"
        );
    }
    assert!(
        named.len() > 200,
        "the matrix was read: {} names",
        named.len()
    );
}

#[test]
fn every_test_the_phase_4_matrix_names_is_a_test_in_this_crate() {
    let here = test_names(Path::new(env!("CARGO_MANIFEST_DIR")));
    // Phase 3's file names only tests here too, so make sure this is Phase 4's.
    let file = matrix("phase4");
    assert!(
        file.contains("\n# Phase 4 fixtures\n"),
        "the Phase 4 matrix"
    );
    let named = named_tests(&file);
    for (name, in_adapter) in &named {
        assert!(!in_adapter, "Phase 4 names no adapter test: `{name}`");
        assert!(
            here.contains(name),
            "the matrix names `{name}`, which is no test in intlify_config"
        );
    }
    assert!(
        named.len() > 100,
        "the matrix was read: {} names",
        named.len()
    );
}
