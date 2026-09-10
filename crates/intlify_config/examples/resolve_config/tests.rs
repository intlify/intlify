// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::*;

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn sample() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/resolve_config/intlify.config.json")
}

#[test]
fn arguments_select_output_and_profile() {
    assert_eq!(
        parse_args(args(&["--json", "config.json", "--profile", "app"])).unwrap(),
        Command::Resolve(Arguments {
            path: "config.json".into(),
            profile: Some("app".to_owned()),
            json: true
        })
    );
    assert_eq!(parse_args(args(&["--help"])).unwrap(), Command::Help);
    assert_eq!(
        parse_args(args(&["--", "-config.json"])).unwrap(),
        Command::Resolve(Arguments {
            path: "-config.json".into(),
            profile: None,
            json: false
        })
    );
}

#[test]
fn invalid_usage_is_not_silently_accepted() {
    for values in [
        vec![],
        vec!["--json"],
        vec!["a.json", "b.json"],
        vec!["--unknown", "a.json"],
        vec!["a.json", "--profile"],
        vec!["a.json", "--profile", "--json"],
        vec!["a.json", "--profile", ""],
        vec!["a.json", "--json", "--json"],
        vec!["a.json", "--profile", "app", "--profile", "other"],
    ] {
        assert!(parse_args(args(&values)).is_err(), "{values:?}");
    }
}

#[test]
fn sample_runs_in_text_and_json_modes_without_modifying_the_file() {
    let path = sample();
    let before = std::fs::read(&path).unwrap();
    let (code, output) = run([path.clone().into_os_string()]).unwrap();
    assert_eq!(code, 0);
    assert!(output.contains("Status: resolved"));
    assert!(output.contains("Requested locales: [\"en-US\",\"ja\"]"));
    assert!(output.contains("file is unchanged"));

    let (code, output) = run([path.clone().into_os_string(), "--json".into()]).unwrap();
    assert_eq!(code, 0);
    let value: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["status"], "resolved");
    assert_eq!(before, std::fs::read(path).unwrap());
}

#[test]
fn rejected_config_has_exit_code_one_and_json_diagnostics() {
    let (code, output) = run([
        sample().into_os_string(),
        "--profile".into(),
        "missing".into(),
        "--json".into(),
    ])
    .unwrap();
    assert_eq!(code, 1);
    let value: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["diagnostics"][0]["code"], "UnknownProfile");
}

#[test]
fn bounded_read_rejects_oversize_instead_of_accepting_a_prefix() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&std::fs::read(sample()).unwrap()).unwrap();
    file.as_file().set_len(MAX_FILE_BYTES + 100).unwrap();
    assert_eq!(
        read_source(file.path()).unwrap().len() as u64,
        MAX_FILE_BYTES + 1
    );
    let (code, output) = run([file.path().as_os_str().to_owned(), "--json".into()]).unwrap();
    assert_eq!(code, 1);
    let value: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["diagnostics"][0]["code"], "InputLimit");
    assert_eq!(value["stage"], "materialize");
}

#[test]
fn missing_files_and_directories_are_io_errors() {
    let dir = tempfile::tempdir().unwrap();
    assert!(run([dir.path().join("missing.json").into_os_string()]).is_err());
    assert!(run([dir.path().as_os_str().to_owned()]).is_err());
}

#[cfg(unix)]
#[test]
fn non_unicode_file_paths_are_preserved() {
    use std::os::unix::ffi::OsStringExt;
    let path = OsString::from_vec(b"config-\xff.json".to_vec());
    let Command::Resolve(parsed) = parse_args([path.clone()]).unwrap() else {
        panic!("expected a file");
    };
    assert_eq!(parsed.path.as_os_str(), path);
    assert!(parse_args([
        "--profile".into(),
        OsString::from_vec(vec![0xff]),
        "config.json".into()
    ])
    .is_err());
}
