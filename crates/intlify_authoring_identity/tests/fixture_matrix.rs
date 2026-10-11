// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Phase 3 fixture matrix points at tests that exist.
//!
//! `fixtures/phase3/README.md` maps each fixture family to the tests that pin
//! it, across this crate and the crates whose tests it marks. A test that was
//! renamed or removed would leave a row pointing at nothing, so every name the
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

#[test]
fn every_test_the_phase_3_matrix_names_is_a_test_where_it_says() {
    // Each name in a row's last cell is looked up in this crate, or in the
    // one its `_(local host)_`, `_(authoring)_` or `_(measurement)_` names.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let matrix = std::fs::read_to_string(root.join("fixtures/phase3/README.md"))
        .expect("the fixture matrix");
    let here = test_names(root);
    let local_host = test_names(&root.join("../intlify_local_host"));
    let authoring = test_names(&root.join("../intlify_authoring"));
    let measurement = test_names(&root.join("../intlify_measurement"));
    assert!(
        here.len() > 200
            && local_host.len() > 50
            && authoring.len() > 100
            && measurement.len() > 50
    );

    let mut named = 0;
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
            // A test name is snake case; file names, type paths and commands
            // in the same cells are not.
            let snake = name.contains('_')
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
            if !snake {
                continue;
            }
            let (names, place) = if rest.starts_with(" _(local host)_") {
                (&local_host, "intlify_local_host")
            } else if rest.starts_with(" _(authoring)_") {
                (&authoring, "intlify_authoring")
            } else if rest.starts_with(" _(measurement)_") {
                (&measurement, "intlify_measurement")
            } else {
                (&here, "intlify_authoring_identity")
            };
            assert!(
                names.contains(name),
                "the matrix names `{name}`, which is no test in {place}"
            );
            named += 1;
        }
    }
    assert!(named > 280, "the matrix was read: {named} names");
}
