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
