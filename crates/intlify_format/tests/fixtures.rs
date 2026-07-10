// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::{
    fs,
    path::{Path, PathBuf},
};

use intlify_format::{format_message, FormatMode, FormatOptions};
use ox_mf2_parser::{parse_message, Diagnostic};
use serde::{Deserialize, Serialize};

const UPDATE_ENV: &str = "INTLIFY_UPDATE_FORMAT_FIXTURES";

// Core fixtures are intentionally directory-based so standard and preserve
// expectations can share one input while invalid fixtures keep diagnostic
// summaries separate from formatted output.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureManifest {
    cases: Vec<FixtureCase>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureCase {
    name: String,
    options: FixtureOptions,
    expected: Option<String>,
    #[serde(rename = "expectedDiagnostics")]
    expected_diagnostics: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FixtureOptions {
    mode: Option<String>,
}

#[test]
fn fixture_outputs_match_expected_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let entries = fs::read_dir(&root).expect("fixtures directory exists");

    for entry in entries {
        let entry = entry.expect("fixture directory entry is readable");
        if !entry
            .file_type()
            .expect("fixture type is readable")
            .is_dir()
        {
            continue;
        }

        run_fixture(&entry.path());
    }
}

fn run_fixture(path: &Path) {
    let manifest_path = path.join("options.json");
    let manifest = fs::read_to_string(&manifest_path).expect("fixture manifest is readable");
    let manifest: FixtureManifest =
        serde_json::from_str(&manifest).expect("fixture manifest is valid json");
    let input = read_message_fixture(&path.join("input.mf2"));
    let update = std::env::var_os(UPDATE_ENV).is_some_and(|value| value == "1");
    let fixture_kind = fixture_kind(&manifest.cases);

    for case in manifest.cases {
        assert_eq!(
            fixture_kind,
            case.kind(),
            "fixture directory must not mix valid and invalid cases: {}",
            path.display()
        );

        let options = FormatOptions {
            mode: case
                .options
                .mode
                .as_deref()
                .map_or(FormatMode::Standard, mode),
        };

        match fixture_kind {
            FixtureKind::Valid => run_valid_case(path, &case, &input, options, update),
            FixtureKind::Invalid => run_invalid_case(path, &case, &input, options, update),
        }
    }
}

// Valid fixtures lock the observable formatter contract: output equality,
// idempotency, and reparse-without-diagnostics.
fn run_valid_case(
    path: &Path,
    case: &FixtureCase,
    input: &str,
    options: FormatOptions,
    update: bool,
) {
    let expected_path = path.join(
        case.expected
            .as_deref()
            .expect("valid fixture case declares expected"),
    );
    let actual = format_message(input, options)
        .unwrap_or_else(|failure| panic!("fixture case {} failed: {failure:?}", case.name))
        .code;

    if update {
        write_message_fixture(&expected_path, &actual);
    }

    let expected = read_message_fixture(&expected_path);
    assert_eq!(actual, expected, "fixture case {}", case.name);

    let idempotent = format_message(&expected, options)
        .unwrap_or_else(|failure| {
            panic!(
                "fixture case {} expected output failed to reformat: {failure:?}",
                case.name
            )
        })
        .code;
    assert_eq!(
        idempotent, expected,
        "fixture case {} is idempotent",
        case.name
    );

    let reparse = parse_message(&expected).expect("formatted output parses");
    assert!(
        reparse.diagnostics.is_empty(),
        "fixture case {} expected output reparses with diagnostics: {:?}",
        case.name,
        reparse.diagnostics
    );
}

// Invalid fixtures exercise the strict diagnostics policy: parser diagnostics
// are exposed, while formatted output is never produced.
fn run_invalid_case(
    path: &Path,
    case: &FixtureCase,
    input: &str,
    options: FormatOptions,
    update: bool,
) {
    let diagnostics_path = path.join(
        case.expected_diagnostics
            .as_deref()
            .expect("invalid fixture case declares expectedDiagnostics"),
    );
    let failure = format_message(input, options)
        .expect_err("invalid fixture case must not produce formatted output");

    assert!(
        failure.errors.is_empty(),
        "invalid fixture case {} should fail with parser diagnostics, got errors: {:?}",
        case.name,
        failure.errors
    );

    let actual = summarize_diagnostics(&failure.diagnostics);
    if update {
        write_diagnostics_fixture(&diagnostics_path, &actual);
    }

    let expected = read_diagnostics_fixture(&diagnostics_path);
    assert_eq!(actual, expected, "fixture case {}", case.name);
}

fn mode(value: &str) -> FormatMode {
    match value {
        "standard" => FormatMode::Standard,
        "preserve" => FormatMode::Preserve,
        other => panic!("unknown fixture format mode: {other}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureKind {
    Valid,
    Invalid,
}

impl FixtureCase {
    fn kind(&self) -> FixtureKind {
        match (&self.expected, &self.expected_diagnostics) {
            (Some(_), None) => FixtureKind::Valid,
            (None, Some(_)) => FixtureKind::Invalid,
            _ => panic!(
                "fixture case {} must declare exactly one of expected or expectedDiagnostics",
                self.name
            ),
        }
    }
}

fn fixture_kind(cases: &[FixtureCase]) -> FixtureKind {
    assert!(!cases.is_empty(), "fixture manifest must contain cases");
    cases[0].kind()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiagnosticSummary {
    code: String,
    severity: String,
    span: SpanSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpanSummary {
    start: u32,
    end: u32,
}

fn summarize_diagnostics(diagnostics: &[Diagnostic]) -> Vec<DiagnosticSummary> {
    diagnostics
        .iter()
        .map(|diagnostic| DiagnosticSummary {
            code: format!("{:?}", diagnostic.code),
            severity: format!("{:?}", diagnostic.severity),
            span: SpanSummary {
                start: diagnostic.span.start,
                end: diagnostic.span.end,
            },
        })
        .collect()
}

fn read_message_fixture(path: &Path) -> String {
    let source = fs::read_to_string(path).expect("message fixture is readable");
    strip_fixture_final_newline(&source).to_owned()
}

fn write_message_fixture(path: &Path, source: &str) {
    fs::write(path, format!("{source}\n")).expect("message fixture is writable");
}

// Core fixtures use CLI file framing on disk, but the formatter API receives
// exactly one message string. The harness removes the final file newline before
// calling the message-level API.
fn strip_fixture_final_newline(source: &str) -> &str {
    if let Some(stripped) = source.strip_suffix("\r\n") {
        stripped
    } else if let Some(stripped) = source.strip_suffix('\n') {
        stripped
    } else {
        source
    }
}

fn read_diagnostics_fixture(path: &Path) -> Vec<DiagnosticSummary> {
    let source = fs::read_to_string(path).expect("diagnostics fixture is readable");
    serde_json::from_str(&source).expect("diagnostics fixture is valid json")
}

fn write_diagnostics_fixture(path: &Path, diagnostics: &[DiagnosticSummary]) {
    let source =
        serde_json::to_string_pretty(diagnostics).expect("diagnostics fixture is serializable");
    fs::write(path, format!("{source}\n")).expect("diagnostics fixture is writable");
}
