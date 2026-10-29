// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Crate-private state of 015's Semantic Profile Resolver and of its targets
//! and groups (015 Phases 3 and 4). Not a `LocalizationProjectProfile`, a
//! checked or blocked outcome, Resolution Evidence, or a registry: each
//! resolution phase records which of its checks it evaluated and why the others
//! were not, and hands later phases the values it admitted. Phases that need construction authority or a confirmed Resource
//! Limit Policy take them as values. Phase 4 confirms the policy, and
//! `profile_resolution` runs the phases in 015's order; a test of one phase
//! builds both.

pub(crate) mod artifact_resolution;
pub(crate) mod bootstrap;
pub(crate) mod coverage;
pub(crate) mod delivery;
pub(crate) mod evaluation;
pub(crate) mod exact_resolution;
pub(crate) mod fallback;
pub(crate) mod identity;
pub(crate) mod limits;
pub(crate) mod locales;
pub(crate) mod negotiation;
pub(crate) mod profile_resolution;
pub(crate) mod resource_policy;
pub(crate) mod specification;
pub(crate) mod tally;
pub(crate) mod targets;
pub(crate) mod vocabulary;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// Every `.rs` file under `dir`, in path order.
    fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
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

    /// What starts a top-level item, as rustfmt lays one out.
    const ITEM_STARTS: [&str; 15] = [
        "pub ",
        "pub(",
        "fn ",
        "mod ",
        "struct ",
        "enum ",
        "impl",
        "const ",
        "static ",
        "use ",
        "type ",
        "trait ",
        "macro_rules!",
        "unsafe ",
        "extern ",
    ];

    /// A module's code before its tests. The tests and their fixtures come
    /// last, each top-level item behind its own `#[cfg(test)]`, so no code of
    /// the module hides after them.
    fn code_before_tests<'t>(file: &Path, text: &'t str) -> &'t str {
        let Some((code, tests)) = text.split_once("\n#[cfg(test)]\n") else {
            assert!(
                !text.contains("#[cfg(test)]"),
                "{} has tests inside an item",
                file.display()
            );
            return text;
        };
        let mut attributed = true;
        for line in tests.lines() {
            if line == "#[cfg(test)]" {
                attributed = true;
            } else if ITEM_STARTS.iter().any(|start| line.starts_with(start)) {
                assert!(
                    attributed,
                    "{} has code after its tests: {line}",
                    file.display()
                );
                attributed = false;
            }
        }
        code
    }

    /// The resolver acquires nothing: no module of it reads a file, reaches the
    /// network, the environment, or another process, or embeds a file. Its
    /// inputs are values its caller already acquired, so it can query no Store
    /// and hold no credential either (plan 017's exit criteria for Phase 3).
    /// Tests may read fixtures; the scan leaves them out.
    #[test]
    fn no_resolver_module_reads_files_the_network_the_environment_or_processes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![root.join("resolver.rs")];
        sources(&root.join("resolver"), &mut files);
        assert!(files.len() >= 19);
        for file in &files {
            let text = std::fs::read_to_string(file).unwrap();
            let code = code_before_tests(file, &text);
            for forbidden in [
                "std::fs",
                "std::net",
                "std::env",
                "std::process",
                "std::io",
                "fs::",
                "net::",
                "process::",
                "env::",
                "io::",
                "File::",
                "TcpStream",
                "UdpSocket",
                "Command::",
                "include_bytes!",
                "include_str!",
                "env!",
            ] {
                assert!(
                    !code.contains(forbidden),
                    "{} uses `{forbidden}`",
                    file.display()
                );
            }
        }
    }
}
