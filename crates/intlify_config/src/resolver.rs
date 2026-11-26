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
pub(crate) mod call_site_span;
pub(crate) mod capability;
pub(crate) mod coverage;
pub(crate) mod decision_evidence;
pub(crate) mod declaration_fields;
pub(crate) mod declaration_fragments;
pub(crate) mod declaration_observer;
pub(crate) mod delivery;
pub(crate) mod dependency;
pub(crate) mod entry;
pub(crate) mod evaluation;
pub(crate) mod evidence;
pub(crate) mod evidence_component;
pub(crate) mod evidence_index;
pub(crate) mod evidence_metadata;
pub(crate) mod evidence_path;
pub(crate) mod evidence_selector;
pub(crate) mod evidence_source;
pub(crate) mod evidence_token;
pub(crate) mod exact_resolution;
pub(crate) mod fallback;
pub(crate) mod finalization;
pub(crate) mod groups;
pub(crate) mod handoff;
pub(crate) mod hydration;
pub(crate) mod identity;
pub(crate) mod invocation;
pub(crate) mod limits;
pub(crate) mod locale_data;
pub(crate) mod locales;
pub(crate) mod materialized;
pub(crate) mod negotiation;
pub(crate) mod occurrence;
pub(crate) mod origin;
pub(crate) mod outcome;
pub(crate) mod profile_resolution;
pub(crate) mod profile_selection;
pub(crate) mod profile_view;
pub(crate) mod record_view;
pub(crate) mod replay;
pub(crate) mod reporting;
pub(crate) mod reporting_plan;
pub(crate) mod resolution_evidence;
pub(crate) mod resource_policy;
pub(crate) mod snapshot_entry;
pub(crate) mod specification;
pub(crate) mod staleness;
pub(crate) mod structural_admission;
pub(crate) mod subject;
pub(crate) mod tally;
pub(crate) mod targets;
pub(crate) mod token;
pub(crate) mod value_admissions;
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

    /// What reads a file, the network, the environment, or a process, or
    /// embeds a file.
    const ACQUIRES: [&str; 17] = [
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
    ];

    /// What resolves or converts a name, or dereferences a URI.
    const NAME_WORK: [&str; 7] = [
        "idna",
        "punycode",
        "ToSocketAddrs",
        "to_socket_addrs",
        "lookup_host",
        "reqwest",
        "hyper",
    ];

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
            for forbidden in ACQUIRES {
                assert!(
                    !code.contains(forbidden),
                    "{} uses `{forbidden}`",
                    file.display()
                );
            }
        }
    }

    /// Locator admission only reads the candidate's text: neither it, its URI
    /// reader, nor a resolver module dereferences a URI, looks a name up,
    /// converts a host, reaches the network, or touches the filesystem
    /// (D15:765, 767, 3502).
    #[test]
    fn locator_admission_performs_no_uri_dns_idna_network_or_file_operation() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![
            root.join("boundary/locator.rs"),
            root.join("construction/validator/uri.rs"),
            root.join("resolver.rs"),
        ];
        sources(&root.join("resolver"), &mut files);
        assert!(files.len() >= 21);
        for file in &files {
            let text = std::fs::read_to_string(file).unwrap();
            let code = code_before_tests(file, &text);
            for forbidden in ACQUIRES.iter().chain(&NAME_WORK) {
                assert!(
                    !code.contains(forbidden),
                    "{} uses `{forbidden}`",
                    file.display()
                );
            }
        }
    }

    /// A module's code without the top-level items behind `#[cfg(test)]`,
    /// wherever they are. Each item is skipped through its attributes, its
    /// first line, and the lines that continue it; the first other line ends
    /// the skip, so a form this does not know leaves more code to scan, not
    /// less.
    fn code_outside_tests(text: &str) -> String {
        let mut code = String::new();
        let mut lines = text.lines().peekable();
        while let Some(line) = lines.next() {
            if line != "#[cfg(test)]" {
                code.push_str(line);
                code.push('\n');
                continue;
            }
            while lines
                .next_if(|line| line.starts_with("#[") || line.starts_with("///"))
                .is_some()
            {}
            lines.next();
            while lines.next_if(|line| continues_an_item(line)).is_some() {}
        }
        code
    }

    /// Whether a line can be part of a top-level item past its first line:
    /// rustfmt indents the rest of an item and leaves in column 0 only blank
    /// lines, the lines that close a bracket, and `where` or `{` after a
    /// signature.
    fn continues_an_item(line: &str) -> bool {
        line.is_empty()
            || line.starts_with([' ', '}', ')', ']', '>'])
            || line == "where"
            || line == "{"
    }

    /// Nothing a resolution runs through discovers its inputs: beside the
    /// resolver, no module it calls reads a file, reaches the network, the
    /// environment, or another process, or looks a name up, so equivalent
    /// declarations resolve through the collection their caller already
    /// acquired (D15:3760). Construction embeds the one built-in package its
    /// implementation fixes (D15:913) and acquires nothing when it runs. The
    /// benchmark harness, test modules, fixtures, and vectors are no part of a
    /// resolution.
    #[test]
    fn no_module_a_resolution_runs_through_discovers_anything() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        sources(&root, &mut files);
        let compiled_in = ["include_bytes!", "include_str!", "env!"];
        let outside = ["benchmark", "minimum_tests", "fixtures", "profile_fixtures"];
        let mut scanned = 0;
        for file in &files {
            let parts: Vec<&str> = file
                .strip_prefix(&root)
                .unwrap()
                .iter()
                .map(|part| part.to_str().unwrap().trim_end_matches(".rs"))
                .collect();
            if outside.contains(&parts[0])
                || parts.contains(&"vectors")
                || parts[parts.len() - 1].ends_with("tests")
            {
                continue;
            }
            let code = code_outside_tests(&std::fs::read_to_string(file).unwrap());
            for forbidden in ACQUIRES.iter().chain(&NAME_WORK) {
                if parts[0] == "construction" && compiled_in.contains(forbidden) {
                    continue;
                }
                assert!(
                    !code.contains(forbidden),
                    "{} uses `{forbidden}`",
                    file.display()
                );
            }
            scanned += 1;
        }
        assert!(scanned >= 130, "{scanned}");
    }

    /// Skipping a test-only item hides none of the code after it: not after
    /// one that fits on its first line, one that ends on an indented line, or
    /// one whose signature spreads over several lines.
    #[test]
    fn a_test_only_item_hides_no_code_after_it() {
        let after = "fn discover() {\n    std::fs::read(\"x\");\n}\n";
        for item in [
            "fn fixture() {}\n",
            "/// A fixture.\nfn fixture() {}\n",
            "const DIGEST: &str =\n    \"sha256:0\";\n",
            "const NAMES: [&str; 1] = [\n    \"a\",\n];\n",
            "#[allow(dead_code)]\nfn helper<\n    P: AsRef<Path>,\n>(\n    path: P,\n) -> Vec<u8>\n\
             where\n    P: Copy,\n{\n    std::fs::read(path).unwrap()\n}\n",
            "mod tests;\n",
            "mod tests {\n    fn read() {\n        std::fs::read(\"y\");\n    }\n\n    fn more() {}\n}\n",
        ] {
            let text = format!("use std::path::Path;\n\n#[cfg(test)]\n{item}{after}");
            assert_eq!(
                code_outside_tests(&text),
                format!("use std::path::Path;\n\n{after}"),
                "{item}"
            );
        }
    }
}
