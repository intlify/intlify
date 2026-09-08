// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::*;

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("crates/config");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("build")).unwrap();
    for (path, content) in [
        (root.join("Cargo.toml"), "package"),
        (root.join("README.md"), "readme"),
        (root.join("build.rs"), "fn main() {}"),
        (root.join("src/lib.rs"), "pub fn resolve() {}"),
        (root.join("src/expectations.json"), "{\"expected\":true}"),
        (root.join("build/metadata.rs"), "producer"),
        (directory.path().join("Cargo.toml"), "workspace"),
        (directory.path().join("Cargo.lock"), "locked"),
    ] {
        fs::write(path, content).unwrap();
    }
    (directory, root)
}

#[test]
fn source_observations_are_path_independent_and_cover_each_declared_input() {
    let (left, root) = fixture();
    let (right, other_root) = fixture();
    let original = source_snapshot(&root, left.path()).unwrap();
    assert_eq!(
        original,
        source_snapshot(&other_root, right.path()).unwrap()
    );
    assert_eq!(original.source["files"], "8");
    assert_eq!(
        original.lock,
        lock_snapshot(&left.path().join("Cargo.lock")).unwrap()
    );
    for relative in [
        "Cargo.toml",
        "README.md",
        "build.rs",
        "src/lib.rs",
        "src/expectations.json",
        "build/metadata.rs",
    ] {
        let path = root.join(relative);
        let old = fs::read(&path).unwrap();
        fs::write(&path, b"changed").unwrap();
        assert_ne!(
            original.source,
            source_snapshot(&root, left.path()).unwrap().source,
            "{relative}"
        );
        fs::write(&path, old).unwrap();
    }
    fs::write(left.path().join("Cargo.lock"), "new lock").unwrap();
    let changed = source_snapshot(&root, left.path()).unwrap();
    assert_ne!(original.source, changed.source);
    assert_ne!(original.lock, changed.lock);
    assert_eq!(
        changed.lock,
        lock_snapshot(&left.path().join("Cargo.lock")).unwrap()
    );
    fs::write(left.path().join("Cargo.toml"), "new workspace").unwrap();
    let manifest_changed = source_snapshot(&root, left.path()).unwrap();
    assert_ne!(changed.source, manifest_changed.source);
    assert_eq!(changed.lock, manifest_changed.lock);
    let text = serde_json::to_string(&original.source).unwrap();
    assert!(!text.contains(root.to_str().unwrap()));
    assert!(!text.contains("src/lib.rs"));
}

#[test]
fn enumeration_order_and_timestamps_do_not_change_the_source_observation() {
    let (directory, root) = fixture();
    fs::write(root.join("src/z.rs"), "last").unwrap();
    fs::write(root.join("src/a.rs"), "first").unwrap();
    let original = source_snapshot(&root, directory.path()).unwrap();
    fs::remove_file(root.join("src/z.rs")).unwrap();
    fs::remove_file(root.join("src/a.rs")).unwrap();
    fs::write(root.join("src/a.rs"), "first").unwrap();
    fs::write(root.join("src/z.rs"), "last").unwrap();
    assert_eq!(original, source_snapshot(&root, directory.path()).unwrap());
    fs::rename(root.join("src/a.rs"), root.join("src/renamed.rs")).unwrap();
    assert_ne!(
        original.source,
        source_snapshot(&root, directory.path()).unwrap().source
    );
}

#[test]
fn source_failure_has_no_partial_digest_and_read_limits_accept_exact_only() {
    let (directory, root) = fixture();
    let file = root.join("src/bound.rs");
    fs::write(&file, "abcd").unwrap();
    assert_eq!(read_bounded(&file, 4).unwrap(), b"abcd");
    assert_eq!(read_bounded(&file, 3), Err(Failure::SourceLimitExceeded));
    assert_eq!(read_bounded(&file, 0), Err(Failure::SourceLimitExceeded));
    let empty = root.join("src/empty.rs");
    fs::write(&empty, "").unwrap();
    assert!(read_bounded(&empty, 0).unwrap().is_empty());
    assert_eq!(
        read_bounded(&file, u64::MAX),
        Err(Failure::SourceLimitExceeded)
    );
    fs::remove_file(root.join("Cargo.toml")).unwrap();
    let result = observation(source_snapshot(&root, directory.path()).map(|inputs| inputs.source));
    assert_eq!(result, json!({"state": "unavailable", "reason": "io"}));
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains(root.to_str().unwrap()));
}

#[test]
fn bounded_directory_and_file_counts_fail_without_a_successful_prefix() {
    let (directory, root) = fixture();
    // The fixture starts with six crate files and two workspace files.
    for index in 0..MAX_SOURCE_FILES - 6 {
        fs::write(root.join(format!("src/file-{index}.rs")), "").unwrap();
    }
    assert_eq!(
        source_snapshot(&root, directory.path()).unwrap().source["files"],
        (MAX_SOURCE_FILES + 2).to_string()
    );
    fs::write(root.join("src/first-over.rs"), "").unwrap();
    assert_eq!(
        source_snapshot(&root, directory.path()),
        Err(Failure::SourceLimitExceeded)
    );
}

#[test]
fn traversal_depth_and_visited_entry_counters_accept_exact_and_reject_first_over() {
    let directory = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    assert!(gather(directory.path(), &mut paths, 64, &mut 0).is_ok());
    assert_eq!(
        gather(directory.path(), &mut paths, 65, &mut 0),
        Err(Failure::SourceLimitExceeded)
    );
    fs::write(directory.path().join("one.rs"), "").unwrap();
    let mut visited = 2047;
    assert!(gather(directory.path(), &mut paths, 0, &mut visited).is_ok());
    assert_eq!(visited, 2048);
    assert_eq!(
        gather(directory.path(), &mut paths, 0, &mut visited),
        Err(Failure::SourceLimitExceeded)
    );
}

#[cfg(unix)]
#[test]
fn symlinked_source_entries_are_not_followed_or_disclosed() {
    let (directory, root) = fixture();
    let secret = directory.path().join("private-token-file");
    fs::write(&secret, "do not retain this content").unwrap();
    std::os::unix::fs::symlink(&secret, root.join("src/link.rs")).unwrap();
    assert_eq!(
        source_snapshot(&root, directory.path()),
        Err(Failure::UnsupportedSourceEntry)
    );
    assert_eq!(
        read_bounded(&root.join("src/link.rs"), 100),
        Err(Failure::UnsupportedSourceEntry)
    );
}

#[test]
fn compiler_projection_retains_controlled_fields_without_host_or_path_text() {
    let output = b"rustc 1.95.0 (ignored)\nbinary: rustc\ncommit-hash: 0123456789abcdef0123456789abcdef01234567\ncommit-date: 2026-01-01\nhost: private-host-name\nrelease: 1.95.0\nLLVM version: 22.1.0\nprivate: /Users/private/repository\n";
    let result = parse_compiler(output).unwrap();
    assert_eq!(
        result,
        json!({"identity": "rustc", "release": "1.95.0", "commit": "0123456789abcdef0123456789abcdef01234567", "llvm": "22.1.0"})
    );
    let text = serde_json::to_string(&result).unwrap();
    assert!(!text.contains("private"));
    assert!(!text.contains("Users"));
    for release in ["1.95.0-nightly", "1.95.0-beta", "1.95.0-dev"] {
        assert!(compiler_release(release));
    }
    let unknown = b"rustc 1.95.0\ncommit-hash: unknown\nrelease: 1.95.0\nLLVM version: 22.1.0\n";
    assert_eq!(parse_compiler(unknown).unwrap()["commit"], Value::Null);
    let duplicate = [output.as_slice(), b"release: 1.95.0\n"].concat();
    assert_eq!(
        parse_compiler(&duplicate),
        Err(Failure::CompilerOutputUnsupported)
    );
    assert_eq!(
        parse_compiler(&vec![
            b'a';
            usize::try_from(MAX_COMPILER_OUTPUT).unwrap() + 1
        ]),
        Err(Failure::CompilerOutputLimit)
    );
    for bad in [
        b"bad compiler".as_slice(),
        b"rustc 1.95.0\nrelease: private-secret\ncommit-hash: unknown\nLLVM version: 22.0.0\n",
        &[0xff],
    ] {
        let reason = observation(parse_compiler(bad));
        assert_eq!(
            reason,
            json!({"state": "unavailable", "reason": "compiler-output-unsupported"})
        );
    }
    // A wrapper is not silently described as the inner unwrapped compiler.
    assert_eq!(
        compiler_snapshot(true),
        Err(Failure::CompilerWrappersPresent)
    );
}

#[test]
fn cargo_input_whitelists_do_not_echo_unknown_or_missing_environment_values() {
    assert_eq!(
        controlled(Some(OsStr::new("release")), &["debug", "release"]),
        Ok(json!("release"))
    );
    assert_eq!(
        controlled(None, &["debug", "release"]),
        Err(Failure::MissingInput)
    );
    let rejected = observation(controlled(
        Some(OsStr::new("/private/token-value")),
        &["debug", "release"],
    ));
    assert_eq!(
        rejected,
        json!({"state": "unavailable", "reason": "unsupported-input"})
    );
    assert!(!serde_json::to_string(&rejected)
        .unwrap()
        .contains("private"));
}

#[test]
fn source_framing_keeps_file_names_and_contents_unambiguous() {
    let hash = |left: &[u8], right: &[u8]| {
        let mut hasher = blake3::Hasher::new();
        frame(&mut hasher, left).unwrap();
        frame(&mut hasher, right).unwrap();
        hasher.finalize()
    };
    assert_ne!(hash(b"ab", b"c"), hash(b"a", b"bc"));
    assert_ne!(hash(b"", b"abc"), hash(b"abc", b""));
}
