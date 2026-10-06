// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Guarantees checked against the crate's structure rather than its output.
//!
//! Design 016's Phase 2 requires that no host code is executed and that each
//! source is read under an explicit profile. Neither is a property one run
//! can show, so these tests look at what the crate is built from and what it
//! exposes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Crates that embed a JavaScript engine, and so could evaluate host code.
const ENGINES: &[&str] = &[
    "boa_engine",
    "boa_interpreter",
    "deno_core",
    "deno_runtime",
    "javascriptcore-rs",
    "mozjs",
    "mozjs_sys",
    "nova_vm",
    "quick-js",
    "quickjs-rs",
    "rquickjs",
    "rquickjs-core",
    "rusty_v8",
    "v8",
];

/// Every package the lock file records, with the names it depends on.
fn locked_packages(lock: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut packages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for block in lock.split("[[package]]").skip(1) {
        let mut name = None;
        let mut dependencies = BTreeSet::new();
        let mut in_dependencies = false;
        for line in block.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("name = ") {
                name = Some(value.trim_matches('"').to_owned());
            } else if line.starts_with("dependencies = [") {
                in_dependencies = !line.ends_with(']');
            } else if in_dependencies && line == "]" {
                in_dependencies = false;
            } else if in_dependencies {
                // An entry is `"name"`, or `"name version"` when the lock holds
                // several versions of one crate.
                let entry = line.trim_end_matches(',').trim_matches('"');
                let dependency = entry.split(' ').next().unwrap_or(entry);
                dependencies.insert(dependency.to_owned());
            }
        }
        if let Some(name) = name {
            packages.entry(name).or_default().extend(dependencies);
        }
    }
    packages
}

#[test]
fn no_javascript_engine_is_anywhere_in_the_dependency_tree() {
    let lock =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"))
            .expect("the workspace lock file");
    let packages = locked_packages(&lock);
    // The lock records development dependencies too, so the closure is a
    // superset of what an ordinary build links.
    let mut reached = BTreeSet::new();
    let mut pending = vec!["intlify_authoring_js".to_owned()];
    while let Some(name) = pending.pop() {
        if reached.insert(name.clone()) {
            pending.extend(packages.get(&name).into_iter().flatten().cloned());
        }
    }
    // The walk really followed the tree: the parser and the shared crate are
    // in it.
    assert!(reached.contains("oxc_parser"));
    assert!(reached.contains("intlify_authoring"));
    for engine in ENGINES {
        assert!(!reached.contains(*engine), "{engine} is a dependency");
    }
}

/// Every `.rs` file under `dir`, in path order.
fn sources(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
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

#[test]
fn no_public_function_reads_a_grammar_from_a_path_or_a_suffix() {
    // A grammar is chosen by the caller and named by the snapshot. A function
    // that took a path, or worked from a file name or extension, would be a
    // way to guess one.
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() > 10, "the walk found the crate's sources");
    let mut signatures = 0;
    for file in &files {
        let text = std::fs::read_to_string(file).expect("a readable source");
        let lines: Vec<&str> = text.lines().collect();
        for (line, content) in lines.iter().enumerate() {
            let content = content.trim_start();
            if !(content.starts_with("pub fn ") || content.starts_with("pub const fn ")) {
                continue;
            }
            signatures += 1;
            // A signature may wrap; it ends where the body or a `;` begins.
            let signature: String =
                lines[line..]
                    .iter()
                    .take_while(|part| {
                        !part.trim_end().ends_with('{') && !part.trim_end().ends_with(';')
                    })
                    .chain(lines[line..].iter().find(|part| {
                        part.trim_end().ends_with('{') || part.trim_end().ends_with(';')
                    }))
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ");
            let lower = signature.to_ascii_lowercase();
            for word in [
                "path",
                "osstr",
                "extension",
                "suffix",
                "file_name",
                "filename",
            ] {
                assert!(
                    !lower.contains(word),
                    "{}:{}: {signature}",
                    file.display(),
                    line + 1
                );
            }
        }
    }
    assert!(signatures > 20, "the scan found the public functions");
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
fn every_test_the_phase_2_matrix_names_is_a_test_where_it_says() {
    // The matrix maps each fixture family to the tests that pin it. A row
    // naming a test that was renamed or removed would point at nothing, so
    // each name in a row's last cell is looked up in the crate the row says:
    // this one, or the one its `_(authoring)_` or `_(measurement)_` names.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let matrix = std::fs::read_to_string(root.join("fixtures/phase2/README.md"))
        .expect("the fixture matrix");
    let here = test_names(root);
    let authoring = test_names(&root.join("../intlify_authoring"));
    let measurement = test_names(&root.join("../intlify_measurement"));
    assert!(here.len() > 200 && authoring.len() > 100 && measurement.len() > 50);

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
            // A test name is snake case; file names, case names and commands
            // in the same cells are not.
            let snake = name.contains('_')
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
            if !snake {
                continue;
            }
            let (names, place) = if rest.starts_with(" _(authoring)_") {
                (&authoring, "intlify_authoring")
            } else if rest.starts_with(" _(measurement)_") {
                (&measurement, "intlify_measurement")
            } else {
                (&here, "intlify_authoring_js")
            };
            assert!(
                names.contains(name),
                "the matrix names `{name}`, which is no test in {place}"
            );
            named += 1;
        }
    }
    assert!(named > 250, "the matrix was read: {named} names");
}
