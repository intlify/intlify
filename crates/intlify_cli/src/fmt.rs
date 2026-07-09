// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::str;

use glob::{glob_with, MatchOptions, Pattern};
use intlify_format::{format_message, FormatOptions, FormatSuccess};
use ox_mf2_parser::{Diagnostic, DiagnosticSeverity};
use serde::Serialize;
use serde_json::{json, Value};

use crate::config::{self, FormatterMode, LoadedProjectConfig};
use crate::error::{CliError, OperationalError};
use crate::output::{render_operational_error, serialize_json_envelope, CliRunResult, Reporter};

const COMMAND: &str = "fmt";
const SUPPORTED_EXTENSIONS: [&str; 1] = [".mf2"];
const DEFAULT_OPERAND: &str = ".";
const DEFAULT_EXCLUDED_DIRS: [&str; 8] = [
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "vendor",
    "target",
    "dist",
    "coverage",
];

pub(crate) fn is_fmt_invocation(args: &[String]) -> bool {
    command_index(args).is_some()
}

pub(crate) fn argv_requests_stdin(args: &[String]) -> bool {
    let Some(command_index) = command_index(args) else {
        return false;
    };

    let mut index = command_index + 1;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            return false;
        }
        if arg == "--stdin-filepath" || arg.starts_with("--stdin-filepath=") {
            return true;
        }
        index += option_stride(arg);
    }

    false
}

pub(crate) fn render_stdin_read_error(
    raw_args: &[String],
    cwd: &Path,
    error: &io::Error,
) -> CliRunResult {
    let reporter = scan_json_reporter(raw_args).unwrap_or(Reporter::Text);
    let project_root = config::discover_project_root(cwd);
    render_operational_error(
        OperationalError {
            kind: "io",
            code: "input_read_failed",
            message: "Stdin could not be read.".to_owned(),
            path: None,
            details: Some(io_error_details(error)),
        },
        reporter,
        COMMAND,
        &project_root,
    )
}

pub(crate) fn run(raw_args: &[String], cwd: &Path, stdin: &[u8]) -> CliRunResult {
    let parsed = parse_fmt_args(raw_args);
    let project_root = config::discover_project_root(cwd);

    if let Some(error) = parsed.error {
        return render_operational_error(error.into(), parsed.reporter, COMMAND, &project_root);
    }

    // CLI argument shape is resolved before config I/O, matching the Phase 3A
    // help/error precedence and keeping argv-only failures file-independent.
    let loaded = match config::load_project_config(cwd, parsed.config_path.as_deref()) {
        Ok(loaded) => loaded,
        Err(error) => {
            return render_operational_error(error.into(), parsed.reporter, COMMAND, &project_root);
        }
    };

    let mode = parsed.mode.unwrap_or(loaded.config.fmt.mode);
    let options = FormatOptions {
        mode: mode.to_format_mode(),
    };
    let operation = if parsed.stdin_filepath.is_some() {
        if parsed.check {
            Operation::StdinCheck
        } else {
            Operation::Stdin
        }
    } else if parsed.check || parsed.list_different {
        Operation::Check
    } else {
        Operation::Write
    };

    let ignore_matcher = match IgnoreMatcher::load(
        &loaded.project_root,
        &parsed.ignore_paths,
        &loaded.config.fmt.ignore_patterns,
    ) {
        Ok(matcher) => matcher,
        Err(error) => {
            return render_operational_error(error, parsed.reporter, COMMAND, &loaded.project_root);
        }
    };

    if let Some(stdin_filepath) = parsed.stdin_filepath.as_deref() {
        return run_stdin(
            stdin_filepath,
            stdin,
            parsed.reporter,
            FormatExecution {
                operation,
                mode,
                options,
            },
            &loaded,
            &ignore_matcher,
        );
    }

    run_files(
        cwd,
        parsed,
        operation,
        mode,
        options,
        &loaded,
        &ignore_matcher,
    )
}

#[derive(Debug, Clone)]
struct FmtArgs {
    reporter: Reporter,
    config_path: Option<String>,
    mode: Option<FormatterMode>,
    check: bool,
    list_different: bool,
    stdin_filepath: Option<String>,
    ignore_paths: Vec<String>,
    operands: Vec<String>,
    error: Option<CliError>,
}

impl Default for FmtArgs {
    fn default() -> Self {
        Self {
            reporter: Reporter::Text,
            config_path: None,
            mode: None,
            check: false,
            list_different: false,
            stdin_filepath: None,
            ignore_paths: Vec::new(),
            operands: Vec::new(),
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Write,
    Check,
    Stdin,
    StdinCheck,
}

#[derive(Debug, Clone, Copy)]
struct FormatExecution {
    operation: Operation,
    mode: FormatterMode,
    options: FormatOptions,
}

impl Operation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Check => "check",
            Self::Stdin => "stdin",
            Self::StdinCheck => "stdin-check",
        }
    }

    const fn is_check(self) -> bool {
        matches!(self, Self::Check | Self::StdinCheck)
    }
}

fn parse_fmt_args(raw_args: &[String]) -> FmtArgs {
    let mut parsed = FmtArgs {
        reporter: scan_json_reporter(raw_args).unwrap_or(Reporter::Text),
        ..FmtArgs::default()
    };
    let Some(command_index) = command_index(raw_args) else {
        parsed.error = Some(CliError::unknown_command(COMMAND));
        return parsed;
    };

    let mut seen_config = false;
    let mut seen_reporter = false;
    let mut seen_mode = false;
    let mut seen_check = false;
    let mut seen_list_different = false;
    let mut seen_stdin_filepath = false;

    let mut index = 0;
    while index < raw_args.len() {
        if index == command_index {
            index += 1;
            continue;
        }

        let arg = &raw_args[index];
        if arg == "--" {
            parsed
                .operands
                .extend(raw_args[index + 1..].iter().cloned());
            break;
        }

        match arg.as_str() {
            "--help" | "-h" | "--version" | "-V" => {
                index += 1;
            }
            "--config" => {
                let (next, value) = parse_string_option(
                    raw_args,
                    index,
                    "--config",
                    &mut seen_config,
                    &mut parsed.error,
                );
                if let Some(value) = value {
                    parsed.config_path = Some(value);
                }
                index = next;
            }
            "--reporter" => {
                let (next, value) = parse_string_option(
                    raw_args,
                    index,
                    "--reporter",
                    &mut seen_reporter,
                    &mut parsed.error,
                );
                if let Some(value) = value {
                    parse_reporter(&value, &mut parsed);
                }
                index = next;
            }
            "--mode" => {
                let (next, value) = parse_string_option(
                    raw_args,
                    index,
                    "--mode",
                    &mut seen_mode,
                    &mut parsed.error,
                );
                if let Some(value) = value {
                    parse_mode(&value, &mut parsed);
                }
                index = next;
            }
            "--stdin-filepath" => {
                let (next, value) = parse_string_option(
                    raw_args,
                    index,
                    "--stdin-filepath",
                    &mut seen_stdin_filepath,
                    &mut parsed.error,
                );
                if let Some(value) = value {
                    parsed.stdin_filepath = Some(value);
                }
                index = next;
            }
            "--ignore-path" => {
                let mut repeatable = false;
                let (next, value) = parse_string_option(
                    raw_args,
                    index,
                    "--ignore-path",
                    &mut repeatable,
                    &mut parsed.error,
                );
                if let Some(value) = value {
                    parsed.ignore_paths.push(value);
                }
                index = next;
            }
            "--check" => {
                record_flag("--check", &mut seen_check, &mut parsed.error);
                parsed.check = true;
                index += 1;
            }
            "--list-different" => {
                record_flag(
                    "--list-different",
                    &mut seen_list_different,
                    &mut parsed.error,
                );
                parsed.list_different = true;
                index += 1;
            }
            _ if arg.starts_with("--config=") => {
                if let Some(value) =
                    record_equals_option(arg, "--config", &mut seen_config, &mut parsed.error)
                {
                    parsed.config_path = Some(value);
                }
                index += 1;
            }
            _ if arg.starts_with("--reporter=") => {
                if let Some(value) =
                    record_equals_option(arg, "--reporter", &mut seen_reporter, &mut parsed.error)
                {
                    parse_reporter(&value, &mut parsed);
                }
                index += 1;
            }
            _ if arg.starts_with("--mode=") => {
                if let Some(value) =
                    record_equals_option(arg, "--mode", &mut seen_mode, &mut parsed.error)
                {
                    parse_mode(&value, &mut parsed);
                }
                index += 1;
            }
            _ if arg.starts_with("--stdin-filepath=") => {
                if let Some(value) = record_equals_option(
                    arg,
                    "--stdin-filepath",
                    &mut seen_stdin_filepath,
                    &mut parsed.error,
                ) {
                    parsed.stdin_filepath = Some(value);
                }
                index += 1;
            }
            _ if arg.starts_with("--ignore-path=") => {
                let mut repeatable = false;
                if let Some(value) =
                    record_equals_option(arg, "--ignore-path", &mut repeatable, &mut parsed.error)
                {
                    parsed.ignore_paths.push(value);
                }
                index += 1;
            }
            _ if arg.starts_with('-') => {
                record_first_error(&mut parsed.error, CliError::unknown_option(arg));
                index += 1;
            }
            _ => {
                parsed.operands.push(arg.to_owned());
                index += 1;
            }
        }
    }

    validate_arg_combinations(&mut parsed);
    parsed
}

fn parse_string_option(
    args: &[String],
    index: usize,
    option: &'static str,
    seen: &mut bool,
    error: &mut Option<CliError>,
) -> (usize, Option<String>) {
    if *seen {
        record_first_error(error, CliError::duplicate_option(option));
        return (index + 1, None);
    }
    *seen = true;

    match args.get(index + 1) {
        Some(value) if !value.starts_with('-') => (index + 2, Some(value.to_owned())),
        _ => {
            record_first_error(error, CliError::missing_option_value(option));
            (index + 1, None)
        }
    }
}

fn record_equals_option(
    arg: &str,
    option: &'static str,
    seen: &mut bool,
    error: &mut Option<CliError>,
) -> Option<String> {
    if *seen {
        record_first_error(error, CliError::duplicate_option(option));
        return None;
    }
    *seen = true;

    let value = arg
        .strip_prefix(option)
        .and_then(|suffix| suffix.strip_prefix('='))
        .unwrap_or_default();
    if value.is_empty() {
        record_first_error(error, CliError::missing_option_value(option));
        None
    } else {
        Some(value.to_owned())
    }
}

fn record_flag(option: &'static str, seen: &mut bool, error: &mut Option<CliError>) {
    if *seen {
        record_first_error(error, CliError::duplicate_option(option));
    }
    *seen = true;
}

fn parse_reporter(value: &str, parsed: &mut FmtArgs) {
    match value {
        "text" => parsed.reporter = Reporter::Text,
        "json" => parsed.reporter = Reporter::Json,
        reporter => record_first_error(
            &mut parsed.error,
            CliError::reporter_not_supported(reporter),
        ),
    }
}

fn parse_mode(value: &str, parsed: &mut FmtArgs) {
    match value {
        "standard" => parsed.mode = Some(FormatterMode::Standard),
        "preserve" => parsed.mode = Some(FormatterMode::Preserve),
        mode => record_first_error(
            &mut parsed.error,
            CliError::invalid_argument(
                format!("Invalid formatter mode: {mode}"),
                json!({
                    "option": "--mode",
                    "value": mode,
                    "allowedValues": ["standard", "preserve"]
                }),
            ),
        ),
    }
}

fn validate_arg_combinations(parsed: &mut FmtArgs) {
    if parsed.list_different && parsed.reporter == Reporter::Json {
        record_first_error(
            &mut parsed.error,
            CliError::invalid_argument(
                "--list-different cannot be combined with --reporter json.",
                json!({
                    "option": "--list-different",
                    "reason": "text_only_mode",
                    "conflictsWith": ["--reporter json"]
                }),
            ),
        );
    }

    if parsed.list_different && parsed.stdin_filepath.is_some() {
        record_first_error(
            &mut parsed.error,
            CliError::invalid_argument(
                "--list-different cannot be combined with stdin mode.",
                json!({
                    "option": "--list-different",
                    "reason": "stdin_not_supported",
                    "conflictsWith": ["--stdin-filepath"]
                }),
            ),
        );
    }

    if parsed.stdin_filepath.is_some() && !parsed.operands.is_empty() {
        record_first_error(
            &mut parsed.error,
            CliError::invalid_argument(
                "--stdin-filepath cannot be combined with file operands.",
                json!({
                    "option": "--stdin-filepath",
                    "reason": "conflicts_with_operands"
                }),
            ),
        );
    }
}

fn record_first_error(target: &mut Option<CliError>, error: CliError) {
    if target.is_none() {
        *target = Some(error);
    }
}

fn command_index(args: &[String]) -> Option<usize> {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--" => return None,
            "--help" | "-h" | "--version" | "-V" => index += 1,
            "--config" | "--reporter" => index += 2,
            _ if arg.starts_with("--config=") || arg.starts_with("--reporter=") => index += 1,
            _ if arg.starts_with('-') => return None,
            "fmt" => return Some(index),
            _ => return None,
        }
    }
    None
}

fn option_stride(arg: &str) -> usize {
    match arg {
        "--config" | "--reporter" | "--mode" | "--stdin-filepath" | "--ignore-path" => 2,
        _ => 1,
    }
}

fn scan_json_reporter(args: &[String]) -> Option<Reporter> {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            return None;
        }
        if arg == "--reporter=json" {
            return Some(Reporter::Json);
        }
        if arg == "--reporter" && args.get(index + 1).is_some_and(|value| value == "json") {
            return Some(Reporter::Json);
        }
        index += 1;
    }
    None
}

fn run_stdin(
    stdin_filepath: &str,
    stdin: &[u8],
    reporter: Reporter,
    execution: FormatExecution,
    loaded: &LoadedProjectConfig,
    ignore_matcher: &IgnoreMatcher,
) -> CliRunResult {
    let path = Path::new(stdin_filepath);
    let path_label = virtual_path_label(stdin_filepath);
    if !is_supported_mf2_path(path) {
        let error = unsupported_input_error(&path_label, path);
        return render_fmt_report(
            reporter,
            &loaded.project_root,
            FmtReport::empty(execution.operation, Some(execution.mode), vec![error]),
        );
    }

    if ignore_matcher.is_ignored(&path_label) {
        if reporter == Reporter::Text && !execution.operation.is_check() {
            return CliRunResult {
                exit_code: 0,
                stdout: String::from_utf8_lossy(stdin).into_owned(),
                stderr: String::new(),
            };
        }

        return render_fmt_report(
            reporter,
            &loaded.project_root,
            FmtReport::zero_targets(execution.operation, execution.mode),
        );
    }

    let result = match process_source(stdin, &path_label, execution.operation, execution.options) {
        ProcessedSource::Formatted {
            status,
            changed,
            output,
        } => {
            let stdout = if reporter == Reporter::Text {
                if execution.operation.is_check() && changed {
                    format!("{path_label}\n")
                } else if execution.operation.is_check() {
                    String::new()
                } else {
                    String::from_utf8(output).expect("formatter output is valid UTF-8")
                }
            } else {
                String::new()
            };
            let result = FmtResult::success(path_label, status, changed);
            FmtRunOutput {
                stdout,
                stderr: String::new(),
                report: FmtReport::from_results(
                    execution.operation,
                    execution.mode,
                    vec![result],
                    Vec::new(),
                ),
            }
        }
        ProcessedSource::Diagnostic { diagnostics } => {
            let stderr = render_diagnostics(&path_label, &diagnostics);
            let result = FmtResult::diagnostic(path_label, diagnostics);
            FmtRunOutput {
                stdout: String::new(),
                stderr,
                report: FmtReport::from_results(
                    execution.operation,
                    execution.mode,
                    vec![result],
                    Vec::new(),
                ),
            }
        }
        ProcessedSource::Error { error } => {
            let stderr = render_text_errors(std::slice::from_ref(&error));
            let result = FmtResult::error(path_label, error);
            FmtRunOutput {
                stdout: String::new(),
                stderr,
                report: FmtReport::from_results(
                    execution.operation,
                    execution.mode,
                    vec![result],
                    Vec::new(),
                ),
            }
        }
    };

    render_run_output(reporter, &loaded.project_root, result)
}

fn run_files(
    cwd: &Path,
    parsed: FmtArgs,
    operation: Operation,
    mode: FormatterMode,
    options: FormatOptions,
    loaded: &LoadedProjectConfig,
    ignore_matcher: &IgnoreMatcher,
) -> CliRunResult {
    let operands = if parsed.operands.is_empty() {
        vec![DEFAULT_OPERAND.to_owned()]
    } else {
        parsed.operands
    };
    let discovery = discover_targets(cwd, &loaded.project_root, &operands, ignore_matcher);
    let mut stdout = String::new();
    let mut stderr = render_text_errors(&discovery.errors);
    let mut results = Vec::new();

    for target in discovery.targets {
        match process_file(&target, operation, options) {
            ProcessedFile::Formatted {
                status,
                changed,
                output,
            } => {
                if operation == Operation::Write && changed {
                    match fs::write(&target.path, output) {
                        Ok(()) => {
                            stdout.push_str(&target.label);
                            stdout.push('\n');
                            results.push(FmtResult::success(target.label, status, changed));
                        }
                        Err(error) => {
                            let error = output_write_error(&target.label, &error);
                            stderr.push_str(&render_text_errors(std::slice::from_ref(&error)));
                            results.push(FmtResult::error(target.label, error));
                        }
                    }
                } else {
                    if operation.is_check() && changed {
                        stdout.push_str(&target.label);
                        stdout.push('\n');
                    }
                    results.push(FmtResult::success(target.label, status, changed));
                }
            }
            ProcessedFile::Diagnostic { diagnostics } => {
                stderr.push_str(&render_diagnostics(&target.label, &diagnostics));
                results.push(FmtResult::diagnostic(target.label, diagnostics));
            }
            ProcessedFile::Error { error } => {
                stderr.push_str(&render_text_errors(std::slice::from_ref(&error)));
                results.push(FmtResult::error(target.label, error));
            }
        }
    }

    let report = FmtReport::from_results(operation, mode, results, discovery.errors);
    render_run_output(
        parsed.reporter,
        &loaded.project_root,
        FmtRunOutput {
            stdout,
            stderr,
            report,
        },
    )
}

#[derive(Debug)]
struct Discovery {
    targets: Vec<Target>,
    errors: Vec<OperationalError>,
}

#[derive(Debug, Clone)]
struct Target {
    path: PathBuf,
    label: String,
}

fn discover_targets(
    cwd: &Path,
    project_root: &Path,
    operands: &[String],
    ignore_matcher: &IgnoreMatcher,
) -> Discovery {
    let mut targets = BTreeMap::<String, Target>::new();
    let mut errors = Vec::new();

    // Deterministic output order and duplicate suppression share one key:
    // slash-normalized absolute paths gathered from explicit, dir, and glob inputs.
    for operand in operands {
        if has_glob_meta(operand) {
            discover_glob(
                cwd,
                project_root,
                operand,
                ignore_matcher,
                &mut targets,
                &mut errors,
            );
        } else {
            discover_path(
                cwd,
                project_root,
                operand,
                ignore_matcher,
                &mut targets,
                &mut errors,
            );
        }
    }

    Discovery {
        targets: targets.into_values().collect(),
        errors,
    }
}

fn discover_path(
    cwd: &Path,
    project_root: &Path,
    operand: &str,
    ignore_matcher: &IgnoreMatcher,
    targets: &mut BTreeMap<String, Target>,
    errors: &mut Vec<OperationalError>,
) {
    let path = resolve_operand_path(cwd, operand);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        errors.push(unmatched_input_error(operand, "path"));
        return;
    };

    if metadata.file_type().is_symlink() {
        match fs::metadata(&path) {
            Ok(target_metadata) if target_metadata.is_file() => {
                add_explicit_file(
                    project_root,
                    operand,
                    &path,
                    ignore_matcher,
                    targets,
                    errors,
                );
            }
            Ok(_) => {}
            Err(error) => errors.push(input_read_error(&display_path(project_root, &path), &error)),
        }
        return;
    }

    if metadata.is_dir() {
        collect_directory(project_root, &path, ignore_matcher, targets, errors);
    } else if metadata.is_file() {
        add_explicit_file(
            project_root,
            operand,
            &path,
            ignore_matcher,
            targets,
            errors,
        );
    } else {
        errors.push(unsupported_input_error(
            &display_path(project_root, &path),
            &path,
        ));
    }
}

fn add_explicit_file(
    project_root: &Path,
    operand: &str,
    path: &Path,
    ignore_matcher: &IgnoreMatcher,
    targets: &mut BTreeMap<String, Target>,
    errors: &mut Vec<OperationalError>,
) {
    if !is_supported_mf2_path(path) {
        errors.push(unsupported_input_error(
            &display_path(project_root, path),
            Path::new(operand),
        ));
        return;
    }

    add_target(project_root, path, ignore_matcher, targets);
}

fn collect_directory(
    project_root: &Path,
    dir: &Path,
    ignore_matcher: &IgnoreMatcher,
    targets: &mut BTreeMap<String, Target>,
    errors: &mut Vec<OperationalError>,
) {
    if dir != project_root && should_skip_discovered_dir_path(project_root, dir, ignore_matcher) {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(input_read_error(&display_path(project_root, dir), &error));
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(input_read_error(&display_path(project_root, dir), &error));
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                errors.push(input_read_error(&display_path(project_root, &path), &error));
                continue;
            }
        };

        if file_type.is_symlink() {
            if !should_skip_discovered_file_path(project_root, &path, ignore_matcher)
                && is_supported_mf2_path(&path)
                && fs::metadata(&path).is_ok_and(|metadata| metadata.is_file())
            {
                add_target(project_root, &path, ignore_matcher, targets);
            }
            continue;
        }

        if file_type.is_dir() {
            if should_skip_discovered_dir_path(project_root, &path, ignore_matcher) {
                continue;
            }
            collect_directory(project_root, &path, ignore_matcher, targets, errors);
        } else if file_type.is_file()
            && !should_skip_discovered_file_path(project_root, &path, ignore_matcher)
            && is_supported_mf2_path(&path)
        {
            add_target(project_root, &path, ignore_matcher, targets);
        }
    }
}

fn discover_glob(
    cwd: &Path,
    project_root: &Path,
    operand: &str,
    ignore_matcher: &IgnoreMatcher,
    targets: &mut BTreeMap<String, Target>,
    errors: &mut Vec<OperationalError>,
) {
    let pattern = resolve_glob_pattern(cwd, operand);
    let options = glob_match_options();
    let Ok(entries) = glob_with(&pattern, options) else {
        errors.push(invalid_glob_error(operand));
        return;
    };

    let mut matched_entries = 0usize;
    for entry in entries {
        matched_entries += 1;
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                if should_skip_discovered_file_path(project_root, error.path(), ignore_matcher) {
                    continue;
                }
                let label = display_path(project_root, error.path());
                errors.push(input_read_error(&label, error.error()));
                continue;
            }
        };
        if should_skip_discovered_file_path(project_root, &path, ignore_matcher) {
            continue;
        }
        if fs::metadata(&path).is_ok_and(|metadata| metadata.is_file())
            && is_supported_mf2_path(&path)
        {
            add_target(project_root, &path, ignore_matcher, targets);
        }
    }

    if matched_entries == 0 {
        errors.push(unmatched_input_error(operand, "glob"));
    }
}

fn add_target(
    project_root: &Path,
    path: &Path,
    ignore_matcher: &IgnoreMatcher,
    targets: &mut BTreeMap<String, Target>,
) {
    let absolute = normalize_path(path);
    let label = display_path(project_root, &absolute);
    if ignore_matcher.is_ignored(&label) {
        return;
    }
    let key = config::slash_normalize_path(&absolute);
    targets.entry(key).or_insert(Target {
        path: absolute,
        label,
    });
}

#[derive(Debug)]
enum ProcessedFile {
    Formatted {
        status: &'static str,
        changed: bool,
        output: Vec<u8>,
    },
    Diagnostic {
        diagnostics: Vec<Diagnostic>,
    },
    Error {
        error: OperationalError,
    },
}

#[derive(Debug)]
enum ProcessedSource {
    Formatted {
        status: &'static str,
        changed: bool,
        output: Vec<u8>,
    },
    Diagnostic {
        diagnostics: Vec<Diagnostic>,
    },
    Error {
        error: OperationalError,
    },
}

fn process_file(target: &Target, operation: Operation, options: FormatOptions) -> ProcessedFile {
    let bytes = match fs::read(&target.path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return ProcessedFile::Error {
                error: input_read_error(&target.label, &error),
            };
        }
    };

    match process_source(&bytes, &target.label, operation, options) {
        ProcessedSource::Formatted {
            status,
            changed,
            output,
        } => ProcessedFile::Formatted {
            status,
            changed,
            output,
        },
        ProcessedSource::Diagnostic { diagnostics } => ProcessedFile::Diagnostic { diagnostics },
        ProcessedSource::Error { error } => ProcessedFile::Error { error },
    }
}

fn process_source(
    bytes: &[u8],
    path_label: &str,
    operation: Operation,
    options: FormatOptions,
) -> ProcessedSource {
    let framed = match FramedSource::from_bytes(bytes) {
        Ok(source) => source,
        Err(error) => {
            return ProcessedSource::Error {
                error: input_decode_error(path_label, error),
            };
        }
    };

    match format_message(&framed.message, options) {
        Ok(success) => formatted_source_result(&success, bytes, operation),
        Err(failure) if !failure.diagnostics.is_empty() => ProcessedSource::Diagnostic {
            diagnostics: failure.diagnostics,
        },
        Err(failure) => {
            let error = failure
                .errors
                .into_iter()
                .next()
                .map_or_else(|| internal_error(path_label), OperationalError::from);
            ProcessedSource::Error { error }
        }
    }
}

fn formatted_source_result(
    success: &FormatSuccess,
    original: &[u8],
    operation: Operation,
) -> ProcessedSource {
    let output = framed_output(&success.code);
    let changed = output != original;
    let status = if changed && operation.is_check() {
        "would_format"
    } else if changed {
        "formatted"
    } else {
        "unchanged"
    };

    ProcessedSource::Formatted {
        status,
        changed,
        output,
    }
}

#[derive(Debug)]
struct FramedSource {
    message: String,
}

impl FramedSource {
    fn from_bytes(bytes: &[u8]) -> Result<Self, str::Utf8Error> {
        // File framing is a CLI boundary. The formatter core receives only
        // message text, so BOM and final-newline handling stays out here.
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        let bytes = bytes
            .strip_suffix(b"\r\n")
            .or_else(|| bytes.strip_suffix(b"\n"))
            .unwrap_or(bytes);
        let message = str::from_utf8(bytes)?.to_owned();
        Ok(Self { message })
    }
}

fn framed_output(code: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(code.len() + 1);
    // Per CLI file framing, a leading UTF-8 BOM is removed at read time and
    // intentionally not emitted again; only one final LF is reconstructed.
    output.extend_from_slice(code.as_bytes());
    output.push(b'\n');
    output
}

#[derive(Debug, Clone)]
struct IgnoreMatcher {
    rules: Vec<IgnoreRule>,
}

impl IgnoreMatcher {
    fn load(
        project_root: &Path,
        ignore_paths: &[String],
        config_patterns: &[String],
    ) -> Result<Self, OperationalError> {
        let mut rules = Vec::new();
        // Later rules override earlier rules, so loading order is the
        // observable ignore precedence: .gitignore, CLI files, then config.
        let gitignore = project_root.join(".gitignore");
        if let Ok(source) = fs::read_to_string(&gitignore) {
            for line in source.lines() {
                if let Some(rule) = IgnoreRule::parse(line).transpose().ok().flatten() {
                    rules.push(rule);
                }
            }
        }

        for ignore_path in ignore_paths {
            let path = resolve_ignore_path(project_root, ignore_path);
            let path_label = display_path(project_root, &path);
            let source = read_ignore_file(&path, &path_label)?;
            for (index, line) in effective_ignore_lines(&source) {
                let rule = IgnoreRule::parse(line).transpose().map_err(|reason| {
                    invalid_ignore_pattern_error(&path_label, line, index, reason)
                })?;
                if let Some(rule) = rule {
                    rules.push(rule);
                }
            }
        }

        for pattern in config_patterns {
            if let Some(rule) = IgnoreRule::parse(pattern).transpose().ok().flatten() {
                rules.push(rule);
            }
        }

        Ok(Self { rules })
    }

    fn is_ignored(&self, path_label: &str) -> bool {
        let mut ignored = false;
        for rule in &self.rules {
            if rule.matches(path_label) {
                ignored = !rule.negated;
            }
        }
        ignored
    }

    fn is_ignored_for_directory_prune(&self, path_label: &str) -> bool {
        self.is_ignored(path_label)
            && !self
                .rules
                .iter()
                .any(|rule| rule.negated && rule.may_match_descendant_of(path_label))
    }
}

#[derive(Debug, Clone)]
struct IgnoreRule {
    negated: bool,
    directory_only: bool,
    pattern_sources: Vec<String>,
    patterns: Vec<Pattern>,
}

impl IgnoreRule {
    fn parse(line: &str) -> Option<Result<Self, &'static str>> {
        if line.trim().is_empty() || line.starts_with('#') {
            return None;
        }

        let (negated, body) = if let Some(body) = line.strip_prefix(r"\!") {
            (false, format!("!{body}"))
        } else if let Some(body) = line.strip_prefix(r"\#") {
            (false, format!("#{body}"))
        } else if let Some(body) = line.strip_prefix('!') {
            (true, body.to_owned())
        } else {
            (false, line.to_owned())
        };

        if body.is_empty() {
            return Some(Err("empty_negation"));
        }

        let anchored = body.starts_with('/');
        let directory_only = body.ends_with('/');
        let body = body
            .strip_prefix('/')
            .unwrap_or(&body)
            .strip_suffix('/')
            .unwrap_or_else(|| body.strip_prefix('/').unwrap_or(&body));

        if body.is_empty() {
            return Some(Err("empty_pattern"));
        }

        let mut pattern_sources = Vec::new();
        pattern_sources.push(body.to_owned());
        if !anchored {
            pattern_sources.push(format!("**/{body}"));
        }

        let patterns = pattern_sources
            .iter()
            .map(|pattern| Pattern::new(pattern).map_err(|_| "invalid_glob"))
            .collect::<Result<Vec<_>, _>>();

        Some(patterns.map(|patterns| Self {
            negated,
            directory_only,
            pattern_sources,
            patterns,
        }))
    }

    fn matches(&self, path_label: &str) -> bool {
        let path = path_label.trim_start_matches('/');
        let options = glob_match_options();
        if self.directory_only {
            return parent_dirs(path).iter().any(|dir| {
                self.patterns
                    .iter()
                    .any(|pattern| pattern.matches_with(dir, options))
            });
        }

        self.patterns
            .iter()
            .any(|pattern| pattern.matches_with(path, options))
    }

    fn may_match_descendant_of(&self, dir_label: &str) -> bool {
        let dir = dir_label.trim_matches('/');
        if dir.is_empty() {
            return true;
        }

        let dir_prefix = format!("{dir}/");
        self.pattern_sources.iter().any(|source| {
            source.starts_with("**/")
                || has_glob_meta(source)
                || source == dir
                || source.starts_with(&dir_prefix)
        })
    }
}

fn effective_ignore_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .enumerate()
}

fn read_ignore_file(path: &Path, path_label: &str) -> Result<String, OperationalError> {
    let metadata = fs::metadata(path).map_err(|error| ignore_read_error(path_label, &error))?;
    if !metadata.is_file() {
        return Err(OperationalError {
            kind: "io",
            code: "ignore_file_read_failed",
            message: format!("Ignore path is not a file: {path_label}"),
            path: Some(path_label.to_owned()),
            details: Some(json!({ "reason": "not_file" })),
        });
    }
    fs::read_to_string(path).map_err(|error| ignore_read_error(path_label, &error))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FmtSummary {
    status: &'static str,
    operation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<&'static str>,
    matched_files: usize,
    unchanged_files: usize,
    diagnostic_files: usize,
    diagnostic_count: usize,
    error_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    formatted_files: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    different_files: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FmtResult {
    path: String,
    status: &'static str,
    changed: bool,
    diagnostics: Vec<DiagnosticOutput>,
    errors: Vec<OperationalError>,
}

impl FmtResult {
    fn success(path: String, status: &'static str, changed: bool) -> Self {
        Self {
            path,
            status,
            changed,
            diagnostics: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn diagnostic(path: String, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            path,
            status: "diagnostic",
            changed: false,
            diagnostics: diagnostics
                .into_iter()
                .map(DiagnosticOutput::from)
                .collect(),
            errors: Vec::new(),
        }
    }

    fn error(path: String, error: OperationalError) -> Self {
        Self {
            path,
            status: "error",
            changed: false,
            diagnostics: Vec::new(),
            errors: vec![error],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticOutput {
    severity: &'static str,
    code: u16,
    message: &'static str,
    span: DiagnosticSpan,
    location: DiagnosticLocation,
}

impl From<Diagnostic> for DiagnosticOutput {
    fn from(diagnostic: Diagnostic) -> Self {
        Self {
            severity: severity_label(diagnostic.severity),
            code: diagnostic.code.as_u16(),
            message: diagnostic.message,
            span: DiagnosticSpan {
                start: diagnostic.span.start,
                end: diagnostic.span.end,
            },
            location: DiagnosticLocation {
                line: diagnostic.location.line,
                column: diagnostic.location.column,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
struct DiagnosticSpan {
    start: u32,
    end: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct DiagnosticLocation {
    line: u32,
    column: u32,
}

#[derive(Debug)]
struct FmtReport {
    summary: FmtSummary,
    results: Vec<FmtResult>,
    errors: Vec<OperationalError>,
}

impl FmtReport {
    fn zero_targets(operation: Operation, mode: FormatterMode) -> Self {
        Self::empty(operation, Some(mode), Vec::new())
    }

    fn empty(
        operation: Operation,
        mode: Option<FormatterMode>,
        errors: Vec<OperationalError>,
    ) -> Self {
        let status = if errors.is_empty() {
            "success"
        } else {
            "error"
        };
        Self {
            summary: FmtSummary {
                status,
                operation: operation.as_str(),
                mode: mode.map(FormatterMode::as_str),
                matched_files: 0,
                unchanged_files: 0,
                diagnostic_files: 0,
                diagnostic_count: 0,
                error_count: errors.len(),
                formatted_files: None,
                different_files: None,
            },
            results: Vec::new(),
            errors,
        }
    }

    fn from_results(
        operation: Operation,
        mode: FormatterMode,
        results: Vec<FmtResult>,
        errors: Vec<OperationalError>,
    ) -> Self {
        let matched_files = results.len();
        let unchanged_files = results
            .iter()
            .filter(|result| result.status == "unchanged")
            .count();
        let diagnostic_files = results
            .iter()
            .filter(|result| result.status == "diagnostic")
            .count();
        let diagnostic_count = results
            .iter()
            .map(|result| result.diagnostics.len())
            .sum::<usize>();
        let result_error_count = results
            .iter()
            .map(|result| result.errors.len())
            .sum::<usize>();
        let error_count = errors.len() + result_error_count;
        let formatted_files = results
            .iter()
            .filter(|result| result.status == "formatted")
            .count();
        let different_files = results.iter().filter(|result| result.changed).count();
        // Encode exit priority through summary.status so text and JSON paths
        // cannot disagree on mixed operational-error/diagnostic outcomes.
        let status = if error_count > 0 {
            "error"
        } else if diagnostic_count > 0 || (operation.is_check() && different_files > 0) {
            "failure"
        } else {
            "success"
        };

        Self {
            summary: FmtSummary {
                status,
                operation: operation.as_str(),
                mode: Some(mode.as_str()),
                matched_files,
                unchanged_files,
                diagnostic_files,
                diagnostic_count,
                error_count,
                formatted_files: (operation == Operation::Write && matched_files > 0)
                    .then_some(formatted_files),
                different_files: (operation == Operation::Check && matched_files > 0)
                    .then_some(different_files),
            },
            results,
            errors,
        }
    }

    fn exit_code(&self) -> i32 {
        match self.summary.status {
            "error" => 2,
            "failure" => 1,
            _ => 0,
        }
    }
}

#[derive(Debug)]
struct FmtRunOutput {
    stdout: String,
    stderr: String,
    report: FmtReport,
}

fn render_run_output(
    reporter: Reporter,
    project_root: &Path,
    output: FmtRunOutput,
) -> CliRunResult {
    match reporter {
        Reporter::Text => CliRunResult {
            exit_code: output.report.exit_code(),
            stdout: output.stdout,
            stderr: output.stderr,
        },
        Reporter::Json => render_fmt_report(reporter, project_root, output.report),
    }
}

fn render_fmt_report(reporter: Reporter, project_root: &Path, report: FmtReport) -> CliRunResult {
    match reporter {
        Reporter::Text => CliRunResult {
            exit_code: report.exit_code(),
            stdout: String::new(),
            stderr: render_text_errors(&report.errors),
        },
        Reporter::Json => {
            let exit_code = report.exit_code();
            let summary = report.summary;
            let errors = report.errors;
            let results = report
                .results
                .into_iter()
                .map(|result| serde_json::to_value(result).expect("fmt result serializes"))
                .collect::<Vec<Value>>();
            CliRunResult {
                exit_code,
                stdout: format!(
                    "{}\n",
                    serialize_json_envelope(COMMAND, project_root, summary, results, errors)
                ),
                stderr: String::new(),
            }
        }
    }
}

fn render_diagnostics(path: &str, diagnostics: &[Diagnostic]) -> String {
    use std::fmt::Write as _;

    diagnostics
        .iter()
        .fold(String::new(), |mut output, diagnostic| {
            writeln!(
                &mut output,
                "{}:{}:{}: {} [{}]: {}",
                path,
                diagnostic.location.line,
                diagnostic.location.column,
                severity_label(diagnostic.severity),
                diagnostic.code.as_u16(),
                diagnostic.message
            )
            .expect("writing diagnostics to a string should not fail");
            output
        })
}

fn render_text_errors(errors: &[OperationalError]) -> String {
    errors
        .iter()
        .map(|error| {
            error.path.as_ref().map_or_else(
                || format!("error: {}\n", error.message),
                |path| format!("error: {path}: {}\n", error.message),
            )
        })
        .collect()
}

fn severity_label(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Information => "information",
        DiagnosticSeverity::Hint => "hint",
    }
}

fn unsupported_input_error(path_label: &str, path: &Path) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "unsupported_input_file",
        message: format!("Input file extension is not supported: {path_label}"),
        path: Some(path_label.to_owned()),
        details: Some(json!({
            "extension": extension_label(path),
            "supportedExtensions": SUPPORTED_EXTENSIONS
        })),
    }
}

fn invalid_glob_error(input: &str) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "invalid_cli_argument",
        message: format!("Input glob is invalid: {input}"),
        path: None,
        details: Some(json!({
            "input": input,
            "kind": "glob",
            "reason": "invalid_glob"
        })),
    }
}

fn unmatched_input_error(input: &str, kind: &'static str) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "unmatched_input",
        message: format!("Input did not match any filesystem entries: {input}"),
        path: None,
        details: Some(json!({
            "input": input,
            "kind": kind
        })),
    }
}

fn invalid_ignore_pattern_error(
    path_label: &str,
    pattern: &str,
    index: usize,
    reason: &'static str,
) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "invalid_ignore_pattern",
        message: format!("Ignore pattern is invalid: {pattern}"),
        path: Some(path_label.to_owned()),
        details: Some(json!({
            "pattern": pattern,
            "source": "ignore-path",
            "index": index,
            "reason": reason
        })),
    }
}

fn ignore_read_error(path_label: &str, error: &io::Error) -> OperationalError {
    OperationalError {
        kind: "io",
        code: "ignore_file_read_failed",
        message: format!("Ignore file could not be read: {path_label}"),
        path: Some(path_label.to_owned()),
        details: Some(json!({ "reason": io_reason(error) })),
    }
}

fn input_read_error(path_label: &str, error: &io::Error) -> OperationalError {
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: format!("Input file could not be read: {path_label}"),
        path: Some(path_label.to_owned()),
        details: Some(io_error_details(error)),
    }
}

fn input_decode_error(path_label: &str, error: str::Utf8Error) -> OperationalError {
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: format!("Input file is not valid UTF-8: {path_label}"),
        path: Some(path_label.to_owned()),
        details: Some(json!({
            "reason": "invalid_utf8",
            "validUpTo": error.valid_up_to()
        })),
    }
}

fn output_write_error(path_label: &str, error: &io::Error) -> OperationalError {
    OperationalError {
        kind: "io",
        code: "output_write_failed",
        message: format!("Formatted output could not be written: {path_label}"),
        path: Some(path_label.to_owned()),
        details: Some(io_error_details(error)),
    }
}

fn internal_error(path_label: &str) -> OperationalError {
    OperationalError {
        kind: "internal",
        code: "internal_error",
        message: "Formatter returned an empty failure.".to_owned(),
        path: Some(path_label.to_owned()),
        details: None,
    }
}

fn io_error_details(error: &io::Error) -> Value {
    let mut details = serde_json::Map::new();
    details.insert("ioKind".to_owned(), json!(format!("{:?}", error.kind())));
    if let Some(raw_os_error) = error.raw_os_error() {
        details.insert("rawOsError".to_owned(), json!(raw_os_error));
    }
    Value::Object(details)
}

fn io_reason(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::NotFound => "not_found",
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::InvalidData => "invalid_utf8",
        _ => "unknown",
    }
}

fn is_supported_mf2_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == "mf2")
}

fn extension_label(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map_or_else(String::new, |extension| format!(".{extension}"))
}

fn has_glob_meta(input: &str) -> bool {
    input.contains('*') || input.contains('?') || input.contains('[')
}

fn should_skip_discovered_dir(name: &str) -> bool {
    is_hidden_name(name) || DEFAULT_EXCLUDED_DIRS.contains(&name)
}

fn should_skip_discovered_file_path(
    project_root: &Path,
    path: &Path,
    ignore_matcher: &IgnoreMatcher,
) -> bool {
    let label = display_path(project_root, path);
    should_skip_discovered_label(&label) || ignore_matcher.is_ignored(&label)
}

fn should_skip_discovered_dir_path(
    project_root: &Path,
    path: &Path,
    ignore_matcher: &IgnoreMatcher,
) -> bool {
    let label = display_path(project_root, path);
    should_skip_discovered_label(&label) || ignore_matcher.is_ignored_for_directory_prune(&label)
}

fn should_skip_discovered_label(label: &str) -> bool {
    label
        .split('/')
        .any(|part| should_skip_discovered_dir(part) || is_hidden_name(part))
}

fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".."
}

fn display_path(project_root: &Path, path: &Path) -> String {
    config::config_error_path(project_root, path)
}

fn virtual_path_label(path: &str) -> String {
    config::slash_normalize_path(Path::new(path))
}

fn resolve_operand_path(cwd: &Path, operand: &str) -> PathBuf {
    let path = Path::new(operand);
    if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(&cwd.join(path))
    }
}

fn resolve_ignore_path(project_root: &Path, ignore_path: &str) -> PathBuf {
    let path = Path::new(ignore_path);
    if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(&project_root.join(path))
    }
}

fn resolve_glob_pattern(cwd: &Path, operand: &str) -> String {
    let path = Path::new(operand);
    let path = if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(&cwd.join(path))
    };
    config::slash_normalize_path(&path)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }

    normalized
}

fn glob_match_options() -> MatchOptions {
    MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    }
}

fn parent_dirs(path: &str) -> Vec<String> {
    let mut dirs = Vec::new();
    let mut current = PathBuf::new();
    let path = Path::new(path);
    let parent = if path.extension().is_some() {
        path.parent()
    } else {
        Some(path)
    };
    let Some(parent) = parent else {
        return dirs;
    };

    for component in parent.components() {
        if let Component::Normal(part) = component {
            current.push(part);
            dirs.push(config::slash_normalize_path(&current));
        }
    }

    dirs
}
