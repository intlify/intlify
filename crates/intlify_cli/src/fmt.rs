// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::borrow::Cow;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::str;
use std::sync::Arc;

use glob::{MatchOptions, Pattern};
use intlify_format::{format_message, format_parsed, FormatOptions, FormatSuccess};
use intlify_resource::{
    preflight_host_bytes, ExtractedCatalog, FormattedEntry, HostFormatRegistry, MessageEntry,
    ResolvedHostFormat, ResourcePhase, Utf8ByteSpan, WriteBackOutcome, MAX_HOST_BYTES,
};
use ox_mf2_parser::{
    parse_source, Diagnostic, DiagnosticLabel, DiagnosticSeverity, ParseOptions, SourceFileInput,
    SourceStore, Span,
};
use serde::Serialize;
use serde_json::{json, Value};

use crate::config::{self, FormatterMode, LoadedProjectConfig};
use crate::error::{CliError, OperationalError};
use crate::input::{
    self, CatalogSelection, ExecutionUnit, InputIgnore, StdinSelection, WorkflowClassification,
};
use crate::output::{render_operational_error, serialize_json_envelope, CliRunResult, Reporter};
use crate::resource::{entry_key_value, offset_map_error, resource_error, HostLineIndex};

const COMMAND: &str = "fmt";
const DEFAULT_OPERAND: &str = ".";

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

enum StdinInput<'a> {
    Provided(&'a [u8]),
    Reader(&'a mut dyn Read),
}

pub(crate) fn run(raw_args: &[String], cwd: &Path, stdin: &[u8]) -> CliRunResult {
    run_with_stdin_input(raw_args, cwd, StdinInput::Provided(stdin))
}

pub(crate) fn run_with_reader(
    raw_args: &[String],
    cwd: &Path,
    stdin: &mut dyn Read,
) -> CliRunResult {
    run_with_stdin_input(raw_args, cwd, StdinInput::Reader(stdin))
}

fn run_with_stdin_input(raw_args: &[String], cwd: &Path, stdin: StdinInput<'_>) -> CliRunResult {
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
    let registry = HostFormatRegistry::new();

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
    let runtime = FmtRuntime {
        loaded: &loaded,
        ignore_matcher: &ignore_matcher,
        registry: &registry,
    };

    if let Some(stdin_filepath) = parsed.stdin_filepath.as_deref() {
        return run_stdin(
            cwd,
            stdin_filepath,
            stdin,
            parsed.reporter,
            FormatExecution {
                operation,
                mode,
                options,
            },
            &runtime,
        );
    }

    run_files(
        cwd,
        parsed,
        FormatExecution {
            operation,
            mode,
            options,
        },
        &runtime,
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

struct FmtRuntime<'a> {
    loaded: &'a LoadedProjectConfig,
    ignore_matcher: &'a IgnoreMatcher,
    registry: &'a HostFormatRegistry,
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
    cwd: &Path,
    stdin_filepath: &str,
    stdin: StdinInput<'_>,
    reporter: Reporter,
    execution: FormatExecution,
    runtime: &FmtRuntime<'_>,
) -> CliRunResult {
    let loaded = runtime.loaded;
    let ignore_matcher = runtime.ignore_matcher;
    let registry = runtime.registry;
    let (path_label, classification) = match input::select_stdin_input(
        cwd,
        &loaded.project_root,
        stdin_filepath,
        CatalogSelection::Enabled {
            resources: &loaded.resolved_resources,
            registry,
            config_path: loaded.config_path.as_deref(),
        },
    ) {
        StdinSelection::Selected {
            label,
            classification,
        } => (label, classification),
        StdinSelection::Skipped { label } => {
            let stdin = match read_stdin_input(stdin, false) {
                Ok(stdin) => stdin,
                Err(error) => {
                    return render_stdin_stream_error(
                        &label,
                        &error,
                        reporter,
                        execution,
                        &loaded.project_root,
                    );
                }
            };
            return render_skipped_stdin(&stdin, &label, reporter, execution, &loaded.project_root);
        }
        StdinSelection::Error(error) => {
            return render_fmt_report(
                reporter,
                &loaded.project_root,
                FmtReport::empty(execution.operation, Some(execution.mode), vec![error]),
            );
        }
    };

    if ignore_matcher.is_ignored(&path_label) {
        let stdin = match read_stdin_input(stdin, false) {
            Ok(stdin) => stdin,
            Err(error) => {
                return render_stdin_stream_error(
                    &path_label,
                    &error,
                    reporter,
                    execution,
                    &loaded.project_root,
                );
            }
        };
        return render_skipped_stdin(
            &stdin,
            &path_label,
            reporter,
            execution,
            &loaded.project_root,
        );
    }

    let bounded = matches!(classification, WorkflowClassification::Catalog { .. });
    let stdin = match read_stdin_input(stdin, bounded) {
        Ok(stdin) => stdin,
        Err(error) => {
            return render_stdin_stream_error(
                &path_label,
                &error,
                reporter,
                execution,
                &loaded.project_root,
            );
        }
    };

    let result = match process_selected_source(
        &stdin,
        &path_label,
        &classification,
        execution,
        reporter,
        registry,
    ) {
        ProcessedSource::StandaloneFormatted {
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
            let result = FmtResult::standalone_success(path_label, status, changed);
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
        ProcessedSource::StandaloneDiagnostic { diagnostics } => {
            let stderr = render_diagnostics(&path_label, &diagnostics);
            let result = FmtResult::standalone_diagnostic(path_label, diagnostics);
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
        ProcessedSource::Catalog(processed) => {
            let stdout = if reporter == Reporter::Text {
                if execution.operation.is_check() && processed.changed {
                    format!("{path_label}\n")
                } else if execution.operation.is_check() {
                    String::new()
                } else {
                    processed.output.source().to_owned()
                }
            } else {
                String::new()
            };
            let stderr = render_diagnostics(&path_label, &processed.diagnostics);
            let result = FmtResult::catalog_success(path_label, processed);
            FmtRunOutput {
                stdout,
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
            let result = FmtResult::error_for(path_label, error, &classification);
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

fn read_stdin_input(input: StdinInput<'_>, bounded_catalog: bool) -> io::Result<Cow<'_, [u8]>> {
    match input {
        StdinInput::Provided(bytes) => Ok(Cow::Borrowed(bytes)),
        StdinInput::Reader(reader) if bounded_catalog => {
            read_at_most(reader, MAX_HOST_BYTES).map(Cow::Owned)
        }
        StdinInput::Reader(reader) => {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes)?;
            Ok(Cow::Owned(bytes))
        }
    }
}

fn render_stdin_stream_error(
    path_label: &str,
    error: &io::Error,
    reporter: Reporter,
    execution: FormatExecution,
    project_root: &Path,
) -> CliRunResult {
    render_fmt_report(
        reporter,
        project_root,
        FmtReport::empty(
            execution.operation,
            Some(execution.mode),
            vec![input_read_error(path_label, error)],
        ),
    )
}

fn render_skipped_stdin(
    stdin: &[u8],
    path_label: &str,
    reporter: Reporter,
    execution: FormatExecution,
    project_root: &Path,
) -> CliRunResult {
    let original = match str::from_utf8(stdin) {
        Ok(source) => source,
        Err(error) => {
            return render_fmt_report(
                reporter,
                project_root,
                FmtReport::empty(
                    execution.operation,
                    Some(execution.mode),
                    vec![input_decode_error(path_label, error)],
                ),
            );
        }
    };
    if reporter == Reporter::Text && !execution.operation.is_check() {
        return CliRunResult {
            exit_code: 0,
            stdout: original.to_owned(),
            stderr: String::new(),
        };
    }

    render_fmt_report(
        reporter,
        project_root,
        FmtReport::zero_targets(execution.operation, execution.mode),
    )
}

fn run_files(
    cwd: &Path,
    parsed: FmtArgs,
    execution: FormatExecution,
    runtime: &FmtRuntime<'_>,
) -> CliRunResult {
    let FormatExecution {
        operation,
        mode,
        options,
    } = execution;
    let loaded = runtime.loaded;
    let ignore_matcher = runtime.ignore_matcher;
    let registry = runtime.registry;
    let operands = if parsed.operands.is_empty() {
        vec![DEFAULT_OPERAND.to_owned()]
    } else {
        parsed.operands
    };
    let selection = input::select_file_inputs(
        cwd,
        &loaded.project_root,
        &operands,
        ignore_matcher,
        CatalogSelection::Enabled {
            resources: &loaded.resolved_resources,
            registry,
            config_path: loaded.config_path.as_deref(),
        },
    );
    let mut stdout = String::new();
    let mut stderr = render_text_errors(&selection.errors);
    let mut results = Vec::new();
    debug_assert!(!selection.aborted || selection.units.is_empty());

    for unit in selection.units {
        match unit {
            ExecutionUnit::TargetError(failure) => {
                let label = failure.target.candidate.label.clone();
                stderr.push_str(&render_text_errors(std::slice::from_ref(&failure.error)));
                results.push(FmtResult::error_for(
                    label,
                    failure.error,
                    &failure.target.classification,
                ));
            }
            ExecutionUnit::Group(group) => {
                let mut blocked_by = None;
                for target in group.aliases {
                    let label = target.candidate.label.clone();
                    if let Some(predecessor) = blocked_by.as_deref() {
                        let error = alias_processing_blocked_error(&label, predecessor);
                        stderr.push_str(&render_text_errors(std::slice::from_ref(&error)));
                        results.push(FmtResult::error_for(label, error, &target.classification));
                        continue;
                    }

                    match process_file(&target, operation, options, parsed.reporter, registry) {
                        ProcessedFile::StandaloneFormatted {
                            status,
                            changed,
                            output,
                        } => {
                            if operation == Operation::Write && changed {
                                match fs::write(&target.candidate.logical_path, output) {
                                    Ok(()) => {
                                        stdout.push_str(&label);
                                        stdout.push('\n');
                                        results.push(FmtResult::standalone_success(
                                            label, status, changed,
                                        ));
                                    }
                                    Err(error) => {
                                        let error = output_write_error(&label, &error);
                                        stderr.push_str(&render_text_errors(std::slice::from_ref(
                                            &error,
                                        )));
                                        blocked_by = Some(label.clone());
                                        results.push(FmtResult::standalone_error(label, error));
                                    }
                                }
                            } else {
                                if operation.is_check() && changed {
                                    stdout.push_str(&label);
                                    stdout.push('\n');
                                }
                                results.push(FmtResult::standalone_success(label, status, changed));
                            }
                        }
                        ProcessedFile::StandaloneDiagnostic { diagnostics } => {
                            stderr.push_str(&render_diagnostics(&label, &diagnostics));
                            results.push(FmtResult::standalone_diagnostic(label, diagnostics));
                        }
                        ProcessedFile::Catalog(processed) => {
                            if operation == Operation::Write && processed.changed {
                                match fs::write(
                                    &target.candidate.logical_path,
                                    processed.output.source().as_bytes(),
                                ) {
                                    Ok(()) => {
                                        stdout.push_str(&label);
                                        stdout.push('\n');
                                        stderr.push_str(&render_diagnostics(
                                            &label,
                                            &processed.diagnostics,
                                        ));
                                        results.push(FmtResult::catalog_success(label, processed));
                                    }
                                    Err(error) => {
                                        let error = output_write_error(&label, &error);
                                        stderr.push_str(&render_text_errors(std::slice::from_ref(
                                            &error,
                                        )));
                                        blocked_by = Some(label.clone());
                                        results.push(FmtResult::catalog_error(label, error));
                                    }
                                }
                            } else {
                                if operation.is_check() && processed.changed {
                                    stdout.push_str(&label);
                                    stdout.push('\n');
                                }
                                stderr
                                    .push_str(&render_diagnostics(&label, &processed.diagnostics));
                                results.push(FmtResult::catalog_success(label, processed));
                            }
                        }
                        ProcessedFile::Error { error } => {
                            stderr.push_str(&render_text_errors(std::slice::from_ref(&error)));
                            results.push(FmtResult::error_for(
                                label,
                                error,
                                &target.classification,
                            ));
                        }
                    }
                }
            }
        }
    }

    let report = FmtReport::from_results(operation, mode, results, selection.errors);
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
enum ProcessedFile {
    StandaloneFormatted {
        status: &'static str,
        changed: bool,
        output: Vec<u8>,
    },
    StandaloneDiagnostic {
        diagnostics: Vec<Diagnostic>,
    },
    Catalog(ProcessedCatalog),
    Error {
        error: OperationalError,
    },
}

#[derive(Debug)]
enum ProcessedSource {
    StandaloneFormatted {
        status: &'static str,
        changed: bool,
        output: Vec<u8>,
    },
    StandaloneDiagnostic {
        diagnostics: Vec<Diagnostic>,
    },
    Catalog(ProcessedCatalog),
    Error {
        error: OperationalError,
    },
}

#[derive(Debug)]
struct ProcessedCatalog {
    status: &'static str,
    changed: bool,
    output: ExtractedCatalog,
    entries: Vec<CatalogEntryResult>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug)]
struct PendingCatalogEntry {
    status: &'static str,
    changed: bool,
    read_only: bool,
    diagnostics: Vec<Diagnostic>,
    formatted: Option<String>,
}

fn process_file(
    target: &input::SelectedTarget,
    operation: Operation,
    options: FormatOptions,
    reporter: Reporter,
    registry: &HostFormatRegistry,
) -> ProcessedFile {
    match &target.classification {
        WorkflowClassification::StandaloneMf2 => {
            let bytes = match fs::read(&target.candidate.logical_path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return ProcessedFile::Error {
                        error: input_read_error(&target.candidate.label, &error),
                    };
                }
            };
            match process_standalone_source(&bytes, &target.candidate.label, operation, options) {
                ProcessedSource::StandaloneFormatted {
                    status,
                    changed,
                    output,
                } => ProcessedFile::StandaloneFormatted {
                    status,
                    changed,
                    output,
                },
                ProcessedSource::StandaloneDiagnostic { diagnostics } => {
                    ProcessedFile::StandaloneDiagnostic { diagnostics }
                }
                ProcessedSource::Error { error } => ProcessedFile::Error { error },
                ProcessedSource::Catalog(_) => {
                    unreachable!("standalone processing stays standalone")
                }
            }
        }
        WorkflowClassification::Catalog { resolved, .. } => {
            let bytes =
                match read_catalog_file(&target.candidate.logical_path, &target.candidate.label) {
                    Ok(bytes) => bytes,
                    Err(error) => return ProcessedFile::Error { error },
                };
            match process_catalog_bytes(
                &bytes,
                &target.candidate.label,
                resolved.clone(),
                operation,
                options,
                reporter,
                registry,
            ) {
                ProcessedSource::Catalog(catalog) => ProcessedFile::Catalog(catalog),
                ProcessedSource::Error { error } => ProcessedFile::Error { error },
                ProcessedSource::StandaloneFormatted { .. }
                | ProcessedSource::StandaloneDiagnostic { .. } => {
                    unreachable!("catalog processing stays catalog")
                }
            }
        }
    }
}

fn process_selected_source(
    bytes: &[u8],
    path_label: &str,
    classification: &WorkflowClassification,
    execution: FormatExecution,
    reporter: Reporter,
    registry: &HostFormatRegistry,
) -> ProcessedSource {
    match classification {
        WorkflowClassification::StandaloneMf2 => {
            process_standalone_source(bytes, path_label, execution.operation, execution.options)
        }
        WorkflowClassification::Catalog { resolved, .. } => process_catalog_bytes(
            bytes,
            path_label,
            resolved.clone(),
            execution.operation,
            execution.options,
            reporter,
            registry,
        ),
    }
}

fn process_standalone_source(
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
        Err(failure) if !failure.diagnostics.is_empty() => ProcessedSource::StandaloneDiagnostic {
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

#[allow(clippy::too_many_arguments)]
fn process_catalog_bytes(
    bytes: &[u8],
    path_label: &str,
    resolved: ResolvedHostFormat,
    operation: Operation,
    options: FormatOptions,
    reporter: Reporter,
    registry: &HostFormatRegistry,
) -> ProcessedSource {
    if let Err(error) = preflight_host_bytes(bytes.len(), ResourcePhase::Extract) {
        return ProcessedSource::Error {
            error: resource_error(path_label, None, &error),
        };
    }
    let source = match str::from_utf8(bytes) {
        Ok(source) => Arc::<str>::from(source),
        Err(error) => {
            return ProcessedSource::Error {
                error: input_decode_error(path_label, error),
            };
        }
    };
    let error_source = Arc::clone(&source);
    let catalog = match registry.extract(resolved, source) {
        Ok(catalog) => catalog,
        Err(error) => {
            return ProcessedSource::Error {
                error: resource_error(path_label, Some(&error_source), &error),
            };
        }
    };

    match process_catalog(catalog, path_label, operation, options, reporter) {
        Ok(processed) => ProcessedSource::Catalog(processed),
        Err(error) => ProcessedSource::Error { error },
    }
}

fn process_catalog(
    catalog: ExtractedCatalog,
    path_label: &str,
    operation: Operation,
    options: FormatOptions,
    reporter: Reporter,
) -> Result<ProcessedCatalog, OperationalError> {
    process_catalog_with(
        catalog,
        path_label,
        operation,
        options,
        reporter,
        format_catalog_entry,
    )
}

fn process_catalog_with<F>(
    catalog: ExtractedCatalog,
    path_label: &str,
    operation: Operation,
    options: FormatOptions,
    reporter: Reporter,
    mut format_entry: F,
) -> Result<ProcessedCatalog, OperationalError>
where
    F: FnMut(&MessageEntry, FormatOptions) -> Result<FormatSuccess, CatalogEntryFailure>,
{
    let mut admission = Some(catalog.begin_candidate_message_admission());
    let mut admission_error = None;
    let mut formatter_error = None;
    let mut pending = Vec::with_capacity(catalog.entries().len());

    for entry in catalog.entries() {
        let formatted = format_entry(entry, options);
        match formatted {
            Ok(success) => {
                let read_only = entry.is_read_only();
                if let Some(active) = admission.as_mut() {
                    let result = if read_only {
                        active.admit_original(entry.handle())
                    } else {
                        active.admit_formatted_bytes(
                            entry.handle(),
                            u64::try_from(success.code.len())
                                .expect("formatter output length fits u64"),
                        )
                    };
                    if let Err(error) = result {
                        admission_error = Some(error);
                        admission = None;
                    }
                }

                let changed = !read_only && success.changed;
                pending.push(PendingCatalogEntry {
                    status: if read_only {
                        "skipped"
                    } else {
                        entry_success_status(changed, operation)
                    },
                    changed,
                    read_only,
                    diagnostics: Vec::new(),
                    formatted: (!read_only && admission_error.is_none()).then_some(success.code),
                });
            }
            Err(CatalogEntryFailure::Diagnostics(diagnostics)) => {
                if let Some(active) = admission.as_mut() {
                    if let Err(error) = active.admit_original(entry.handle()) {
                        admission_error = Some(error);
                        admission = None;
                    }
                }
                pending.push(PendingCatalogEntry {
                    status: "diagnostic",
                    changed: false,
                    read_only: entry.is_read_only(),
                    diagnostics,
                    formatted: None,
                });
            }
            Err(CatalogEntryFailure::Operational(error)) => {
                if formatter_error.is_none() {
                    formatter_error = Some(attach_entry_key(error, path_label, entry));
                }
                pending.push(PendingCatalogEntry {
                    status: "error",
                    changed: false,
                    read_only: entry.is_read_only(),
                    diagnostics: Vec::new(),
                    formatted: None,
                });
            }
        }
    }

    if let Some(error) = formatter_error {
        return Err(error);
    }
    if let Some(error) = admission_error {
        return Err(resource_error(path_label, Some(catalog.source()), &error));
    }
    if let Err(error) = admission
        .expect("admission remains active without an admission error")
        .finish()
    {
        return Err(resource_error(path_label, Some(catalog.source()), &error));
    }

    let formatted_entries = catalog
        .entries()
        .iter()
        .zip(&pending)
        .filter_map(|(entry, pending)| {
            pending
                .formatted
                .as_deref()
                .map(|formatted_message| FormattedEntry {
                    entry: entry.handle(),
                    formatted_message,
                })
        })
        .collect::<Vec<_>>();
    let candidate = match catalog.build_and_validate_write_back(&formatted_entries) {
        Ok(WriteBackOutcome::Unchanged) => None,
        Ok(WriteBackOutcome::Changed(write_back)) => Some(write_back.into_candidate()),
        Err(error) => return Err(resource_error(path_label, Some(catalog.source()), &error)),
    };
    let changed = candidate.is_some();
    let map_candidate = operation == Operation::Write
        || (operation == Operation::Stdin && reporter == Reporter::Text);
    let mapping_catalog = if map_candidate {
        candidate.as_ref().unwrap_or(&catalog)
    } else {
        &catalog
    };
    let entries = map_catalog_entries(&catalog, mapping_catalog, pending, path_label)?;
    let diagnostics = entries
        .iter()
        .flat_map(|entry| entry.mapped_diagnostics.iter().cloned())
        .collect::<Vec<_>>();
    let has_diagnostics = !diagnostics.is_empty();
    let status = catalog_status(changed, has_diagnostics, operation);
    let output = candidate.unwrap_or(catalog);

    Ok(ProcessedCatalog {
        status,
        changed,
        output,
        entries,
        diagnostics,
    })
}

enum CatalogEntryFailure {
    Diagnostics(Vec<Diagnostic>),
    Operational(intlify_format::OperationalError),
}

fn format_catalog_entry(
    entry: &MessageEntry,
    options: FormatOptions,
) -> Result<FormatSuccess, CatalogEntryFailure> {
    let mut sources = SourceStore::with_capacity(1);
    let source_id = sources.add(SourceFileInput {
        source: entry.message_text(),
        message_id: Some(entry.key().structural_path().as_str()),
        ..SourceFileInput::default()
    });
    let result = parse_source(&sources, source_id, ParseOptions::default()).map_err(|error| {
        CatalogEntryFailure::Operational(formatter_invariant_error(
            format!("MF2 parser failed: {error}"),
            "parsed_artifact_attachment",
        ))
    })?;
    match format_parsed(&sources, &result, options) {
        Ok(success) => Ok(success),
        Err(failure) => {
            if let Some(error) = failure.errors.into_iter().next() {
                Err(CatalogEntryFailure::Operational(error))
            } else if !failure.diagnostics.is_empty() {
                Err(CatalogEntryFailure::Diagnostics(failure.diagnostics))
            } else {
                Err(CatalogEntryFailure::Operational(formatter_invariant_error(
                    "formatter returned an empty failure",
                    "document_ir_render",
                )))
            }
        }
    }
}

fn formatter_invariant_error(
    message: impl Into<String>,
    phase: &'static str,
) -> intlify_format::OperationalError {
    intlify_format::OperationalError::internal(message)
        .with_detail("reason", "formatter_invariant_failed")
        .with_detail("phase", phase)
}

fn map_catalog_entries(
    original: &ExtractedCatalog,
    mapping: &ExtractedCatalog,
    pending: Vec<PendingCatalogEntry>,
    path_label: &str,
) -> Result<Vec<CatalogEntryResult>, OperationalError> {
    map_catalog_entries_with(
        original,
        mapping,
        pending,
        path_label,
        map_catalog_diagnostic,
    )
}

fn map_catalog_entries_with<F>(
    original: &ExtractedCatalog,
    mapping: &ExtractedCatalog,
    pending: Vec<PendingCatalogEntry>,
    path_label: &str,
    mut map_diagnostic: F,
) -> Result<Vec<CatalogEntryResult>, OperationalError>
where
    F: FnMut(
        Diagnostic,
        &MessageEntry,
        &HostLineIndex,
    ) -> Result<Diagnostic, intlify_resource::OffsetMapError>,
{
    debug_assert_eq!(original.entries().len(), mapping.entries().len());
    debug_assert_eq!(original.entries().len(), pending.len());
    let line_index = pending
        .iter()
        .any(|entry| !entry.diagnostics.is_empty())
        .then(|| HostLineIndex::new(mapping.source()));
    original
        .entries()
        .iter()
        .zip(mapping.entries())
        .zip(pending)
        .map(|((original_entry, mapping_entry), pending)| {
            let mapped_diagnostics = pending
                .diagnostics
                .into_iter()
                .map(|diagnostic| {
                    map_diagnostic(
                        diagnostic,
                        mapping_entry,
                        line_index
                            .as_ref()
                            .expect("diagnostic mapping constructed a host line index"),
                    )
                    .map_err(|_| offset_map_error(path_label, original_entry.key()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CatalogEntryResult {
                key: EntryKeyOutput::from(original_entry.key()),
                display_key: original_entry.display_key().map(str::to_owned),
                status: pending.status,
                changed: pending.changed,
                read_only: pending.read_only,
                diagnostics: mapped_diagnostics
                    .iter()
                    .cloned()
                    .map(DiagnosticOutput::from)
                    .collect(),
                mapped_diagnostics,
            })
        })
        .collect()
}

fn map_catalog_diagnostic(
    diagnostic: Diagnostic,
    entry: &MessageEntry,
    line_index: &HostLineIndex,
) -> Result<Diagnostic, intlify_resource::OffsetMapError> {
    let mapped = entry.offset_map().map_span(Utf8ByteSpan::new(
        diagnostic.span.start,
        diagnostic.span.end,
    ))?;
    let labels = diagnostic
        .labels
        .into_iter()
        .map(|label| {
            let span = entry
                .offset_map()
                .map_span(Utf8ByteSpan::new(label.span.start, label.span.end))?;
            Ok(DiagnosticLabel {
                source: label.source,
                span: Span::new(span.start(), span.end()),
                message: label.message,
            })
        })
        .collect::<Result<Vec<_>, intlify_resource::OffsetMapError>>()?;
    Ok(Diagnostic {
        source: diagnostic.source,
        span: Span::new(mapped.start(), mapped.end()),
        location: line_index.diagnostic_location(mapped.start()),
        severity: diagnostic.severity,
        code: diagnostic.code,
        message: diagnostic.message,
        labels,
    })
}

fn attach_entry_key(
    error: intlify_format::OperationalError,
    path_label: &str,
    entry: &MessageEntry,
) -> OperationalError {
    let mut error = OperationalError::from(error);
    error.path = Some(path_label.to_owned());
    let mut details = match error.details.take() {
        Some(Value::Object(details)) => details,
        Some(_) | None => serde_json::Map::new(),
    };
    details.insert("entryKey".to_owned(), entry_key_value(entry.key()));
    error.details = Some(Value::Object(details));
    error
}

fn entry_success_status(changed: bool, operation: Operation) -> &'static str {
    if changed && operation.is_check() {
        "would_format"
    } else if changed {
        "formatted"
    } else {
        "unchanged"
    }
}

fn catalog_status(changed: bool, has_diagnostics: bool, operation: Operation) -> &'static str {
    if changed && operation.is_check() {
        "would_format"
    } else if changed {
        "formatted"
    } else if has_diagnostics {
        "diagnostic"
    } else {
        "unchanged"
    }
}

fn read_catalog_file(path: &Path, path_label: &str) -> Result<Vec<u8>, OperationalError> {
    let mut file = fs::File::open(path).map_err(|error| input_read_error(path_label, &error))?;
    read_at_most(&mut file, MAX_HOST_BYTES).map_err(|error| input_read_error(path_label, &error))
}

fn read_at_most(reader: &mut dyn Read, inclusive_limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(inclusive_limit + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
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

    ProcessedSource::StandaloneFormatted {
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

impl InputIgnore for IgnoreMatcher {
    fn is_ignored(&self, path_label: &str) -> bool {
        Self::is_ignored(self, path_label)
    }

    fn can_prune_directory(&self, path_label: &str) -> bool {
        self.is_ignored_for_directory_prune(path_label)
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
#[serde(untagged)]
enum FmtResult {
    Standalone(StandaloneFmtResult),
    Catalog(CatalogFmtResult),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StandaloneFmtResult {
    path: String,
    status: &'static str,
    changed: bool,
    diagnostics: Vec<DiagnosticOutput>,
    errors: Vec<OperationalError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogFmtResult {
    path: String,
    status: &'static str,
    changed: bool,
    entries: Vec<CatalogEntryResult>,
    errors: Vec<OperationalError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogEntryResult {
    key: EntryKeyOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_key: Option<String>,
    status: &'static str,
    changed: bool,
    read_only: bool,
    diagnostics: Vec<DiagnosticOutput>,
    #[serde(skip)]
    mapped_diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize)]
struct EntryKeyOutput {
    path: String,
    occurrence: u32,
}

impl From<&intlify_resource::EntryKey> for EntryKeyOutput {
    fn from(key: &intlify_resource::EntryKey) -> Self {
        Self {
            path: key.structural_path().as_str().to_owned(),
            occurrence: key.occurrence(),
        }
    }
}

impl FmtResult {
    fn standalone_success(path: String, status: &'static str, changed: bool) -> Self {
        Self::Standalone(StandaloneFmtResult {
            path,
            status,
            changed,
            diagnostics: Vec::new(),
            errors: Vec::new(),
        })
    }

    fn standalone_diagnostic(path: String, diagnostics: Vec<Diagnostic>) -> Self {
        Self::Standalone(StandaloneFmtResult {
            path,
            status: "diagnostic",
            changed: false,
            diagnostics: diagnostics
                .into_iter()
                .map(DiagnosticOutput::from)
                .collect(),
            errors: Vec::new(),
        })
    }

    fn standalone_error(path: String, error: OperationalError) -> Self {
        Self::Standalone(StandaloneFmtResult {
            path,
            status: "error",
            changed: false,
            diagnostics: Vec::new(),
            errors: vec![error],
        })
    }

    fn catalog_success(path: String, processed: ProcessedCatalog) -> Self {
        Self::Catalog(CatalogFmtResult {
            path,
            status: processed.status,
            changed: processed.changed,
            entries: processed.entries,
            errors: Vec::new(),
        })
    }

    fn catalog_error(path: String, error: OperationalError) -> Self {
        Self::Catalog(CatalogFmtResult {
            path,
            status: "error",
            changed: false,
            entries: Vec::new(),
            errors: vec![error],
        })
    }

    fn error_for(
        path: String,
        error: OperationalError,
        classification: &WorkflowClassification,
    ) -> Self {
        match classification {
            WorkflowClassification::StandaloneMf2 => Self::standalone_error(path, error),
            WorkflowClassification::Catalog { .. } => Self::catalog_error(path, error),
        }
    }

    const fn status(&self) -> &'static str {
        match self {
            Self::Standalone(result) => result.status,
            Self::Catalog(result) => result.status,
        }
    }

    const fn changed(&self) -> bool {
        match self {
            Self::Standalone(result) => result.changed,
            Self::Catalog(result) => result.changed,
        }
    }

    fn diagnostic_count(&self) -> usize {
        match self {
            Self::Standalone(result) => result.diagnostics.len(),
            Self::Catalog(result) => result
                .entries
                .iter()
                .map(|entry| entry.diagnostics.len())
                .sum(),
        }
    }

    fn error_count(&self) -> usize {
        match self {
            Self::Standalone(result) => result.errors.len(),
            Self::Catalog(result) => result.errors.len(),
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    labels: Vec<DiagnosticLabelOutput>,
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
            labels: diagnostic
                .labels
                .into_iter()
                .map(|label| DiagnosticLabelOutput {
                    span: DiagnosticSpan {
                        start: label.span.start,
                        end: label.span.end,
                    },
                    message: label.message,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct DiagnosticLabelOutput {
    span: DiagnosticSpan,
    message: &'static str,
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
            .filter(|result| result.status() == "unchanged")
            .count();
        let diagnostic_files = results
            .iter()
            .filter(|result| result.diagnostic_count() > 0)
            .count();
        let diagnostic_count = results
            .iter()
            .map(FmtResult::diagnostic_count)
            .sum::<usize>();
        let result_error_count = results.iter().map(FmtResult::error_count).sum::<usize>();
        let error_count = errors.len() + result_error_count;
        let formatted_files = results
            .iter()
            .filter(|result| result.status() == "formatted")
            .count();
        let different_files = results.iter().filter(|result| result.changed()).count();
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
            let results = report.results;
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

fn alias_processing_blocked_error(path_label: &str, predecessor_path: &str) -> OperationalError {
    OperationalError {
        kind: "io",
        code: "alias_processing_blocked",
        message: format!(
            "Input processing was blocked by a prior alias write failure: {path_label}"
        ),
        path: Some(path_label.to_owned()),
        details: Some(json!({
            "reason": "prior_alias_write_failed",
            "predecessorPath": predecessor_path
        })),
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

fn has_glob_meta(input: &str) -> bool {
    input.contains('*') || input.contains('?') || input.contains('[')
}

fn display_path(project_root: &Path, path: &Path) -> String {
    config::config_error_path(project_root, path)
}

fn resolve_ignore_path(project_root: &Path, ignore_path: &str) -> PathBuf {
    let path = Path::new(ignore_path);
    if path.is_absolute() {
        normalize_path(path)
    } else {
        normalize_path(&project_root.join(path))
    }
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use intlify_format::{
        FormatOptions, FormatSuccess, OperationalError as FormatOperationalError,
    };
    use intlify_resource::{
        ExtractedCatalog, HostFormatRegistry, OffsetMapError, MAX_MESSAGE_BYTES,
    };
    use ox_mf2_parser::{
        Diagnostic, DiagnosticCode, DiagnosticLabel, DiagnosticSeverity, SourceId, SourceLocation,
        Span,
    };

    use super::{
        map_catalog_diagnostic, map_catalog_entries_with, process_catalog_with, read_at_most,
        CatalogEntryFailure, HostLineIndex, Operation, PendingCatalogEntry, Reporter,
    };

    fn extract_catalog(source: &str) -> ExtractedCatalog {
        let registry = HostFormatRegistry::new();
        let resolved = registry
            .resolve_direct_extension(".json")
            .expect("JSON should be a shipped direct format");
        registry
            .extract(resolved, Arc::from(source))
            .expect("test catalog should extract")
    }

    fn diagnostic(span: Span, labels: Vec<DiagnosticLabel>) -> Diagnostic {
        Diagnostic {
            source: SourceId::new(0),
            span,
            location: SourceLocation::default(),
            severity: DiagnosticSeverity::Error,
            code: DiagnosticCode::UnexpectedEndOfInput,
            message: DiagnosticCode::UnexpectedEndOfInput.static_message(),
            labels,
        }
    }

    #[test]
    fn bounded_catalog_read_stops_at_the_first_byte_over() {
        let mut over = Cursor::new(b"abcdef".as_slice());
        assert_eq!(read_at_most(&mut over, 4).unwrap(), b"abcde");

        let mut boundary = Cursor::new(b"abcdef".as_slice());
        assert_eq!(read_at_most(&mut boundary, 6).unwrap(), b"abcdef");
    }

    #[test]
    fn formatter_operational_error_wins_over_an_earlier_admission_error() {
        let catalog = extract_catalog(r#"{"first":"one","second":"two"}"#);
        let mut calls = 0;

        let error = process_catalog_with(
            catalog,
            "messages.json",
            Operation::Write,
            FormatOptions::default(),
            Reporter::Json,
            |_entry, _options| {
                let index = calls;
                calls += 1;
                if index == 0 {
                    Ok(FormatSuccess {
                        changed: true,
                        code: "x".repeat(
                            usize::try_from(MAX_MESSAGE_BYTES + 1)
                                .expect("message limit fits usize"),
                        ),
                    })
                } else {
                    Err(CatalogEntryFailure::Operational(
                        FormatOperationalError::internal("synthetic formatter failure")
                            .with_detail("reason", "formatter_invariant_failed")
                            .with_detail("phase", "document_ir_render"),
                    ))
                }
            },
        )
        .expect_err("formatter operational error should win");

        assert_eq!(calls, 2);
        assert_eq!(error.code, "internal_error");
        let details = error.details.expect("formatter error details");
        assert_eq!(details["reason"], "formatter_invariant_failed");
        assert_eq!(details["phase"], "document_ir_render");
        assert_eq!(details["entryKey"]["path"], "/second");
        assert_eq!(details["entryKey"]["occurrence"], 0);
    }

    #[test]
    fn candidate_admission_error_uses_the_resource_limit_schema() {
        let catalog = extract_catalog(r#"{"message":"one"}"#);

        let error = process_catalog_with(
            catalog,
            "messages.json",
            Operation::Check,
            FormatOptions::default(),
            Reporter::Json,
            |_entry, _options| {
                Ok(FormatSuccess {
                    changed: true,
                    code: "x".repeat(
                        usize::try_from(MAX_MESSAGE_BYTES + 1).expect("message limit fits usize"),
                    ),
                })
            },
        )
        .expect_err("candidate message should exceed the fixed limit");

        assert_eq!(error.code, "resource_limit_exceeded");
        let details = error.details.expect("resource limit details");
        assert_eq!(details["phase"], "validate_write_back");
        assert_eq!(details["resource"], "message_bytes");
        assert_eq!(details["limit"], MAX_MESSAGE_BYTES);
        assert_eq!(details["actual"], MAX_MESSAGE_BYTES + 1);
        assert_eq!(details["entryKey"]["path"], "/message");
        assert_eq!(details["entryKey"]["occurrence"], 0);
    }

    #[test]
    fn diagnostic_mapping_checks_the_primary_span_before_labels() {
        let catalog = extract_catalog(r#"{"message":"abc"}"#);
        let entry = &catalog.entries()[0];
        let line_index = HostLineIndex::new(catalog.source());
        let diagnostic = diagnostic(
            Span::new(0, 4),
            vec![DiagnosticLabel {
                source: SourceId::new(0),
                span: Span::new(0, 5),
                message: "label",
            }],
        );

        assert_eq!(
            map_catalog_diagnostic(diagnostic, entry, &line_index),
            Err(OffsetMapError::OutOfBounds {
                end: 4,
                message_len: 3,
            })
        );
    }

    #[test]
    fn diagnostic_mapping_checks_labels_in_stored_order() {
        let catalog = extract_catalog(r#"{"message":"abc"}"#);
        let entry = &catalog.entries()[0];
        let line_index = HostLineIndex::new(catalog.source());
        let diagnostic = diagnostic(
            Span::new(0, 1),
            vec![
                DiagnosticLabel {
                    source: SourceId::new(0),
                    span: Span::new(0, 5),
                    message: "first label",
                },
                DiagnosticLabel {
                    source: SourceId::new(0),
                    span: Span::new(0, 4),
                    message: "second label",
                },
            ],
        );

        assert_eq!(
            map_catalog_diagnostic(diagnostic, entry, &line_index),
            Err(OffsetMapError::OutOfBounds {
                end: 5,
                message_len: 3,
            })
        );
    }

    #[test]
    fn catalog_mapping_visits_entries_and_diagnostics_in_raw_order() {
        let catalog = extract_catalog(r#"{"first":"abc","second":"xyz"}"#);
        let pending = vec![
            PendingCatalogEntry {
                status: "diagnostic",
                changed: false,
                read_only: false,
                diagnostics: vec![
                    diagnostic(Span::new(0, 1), Vec::new()),
                    diagnostic(Span::new(1, 2), Vec::new()),
                ],
                formatted: None,
            },
            PendingCatalogEntry {
                status: "diagnostic",
                changed: false,
                read_only: false,
                diagnostics: vec![diagnostic(Span::new(0, 1), Vec::new())],
                formatted: None,
            },
        ];
        let mut visits = Vec::new();

        let error = map_catalog_entries_with(
            &catalog,
            &catalog,
            pending,
            "messages.json",
            |diagnostic, entry, _line_index| {
                visits.push((
                    entry.key().structural_path().as_str().to_owned(),
                    diagnostic.span.start,
                ));
                if visits.len() == 3 {
                    Err(OffsetMapError::OutOfBounds {
                        end: 4,
                        message_len: 3,
                    })
                } else {
                    Ok(diagnostic)
                }
            },
        )
        .expect_err("the injected third mapping should fail");

        assert_eq!(
            visits,
            vec![
                ("/first".to_owned(), 0),
                ("/first".to_owned(), 1),
                ("/second".to_owned(), 0),
            ]
        );
        assert_eq!(error.code, "internal_error");
        let details = error.details.expect("mapping error details");
        assert_eq!(details["reason"], "resource_offset_map_failed");
        assert_eq!(details["phase"], "map");
        assert_eq!(details["entryKey"]["path"], "/second");
        assert_eq!(details["entryKey"]["occurrence"], 0);
    }
}
