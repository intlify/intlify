// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Top-level CLI argument recognition and precedence.
//!
//! This module owns global-option parsing, reserved-command recognition,
//! first-error selection, and the reporter pre-scan needed for machine-readable
//! argument failures. It performs no configuration I/O or command execution.

use std::collections::HashSet;

use crate::error::CliError;
use crate::output::Reporter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParsedCommand {
    Reserved(&'static str),
    Messages(MessagesCommand),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MessagesCommand {
    Namespace,
    Emit(MessagesEmitArgs),
    Prune,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MessagesEmitArgs {
    pub(crate) target: Option<String>,
    pub(crate) check: bool,
}

impl ParsedCommand {
    pub(crate) fn resolved_name(&self) -> Option<&str> {
        match self {
            Self::Reserved(command) => Some(command),
            Self::Messages(MessagesCommand::Namespace) => Some("messages"),
            Self::Messages(MessagesCommand::Emit(_)) => Some("messages.emit"),
            Self::Messages(MessagesCommand::Prune) => Some("messages.prune"),
            Self::Unknown(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedArgs {
    pub(crate) help: bool,
    pub(crate) version: bool,
    pub(crate) reporter: Reporter,
    pub(crate) config_path: Option<String>,
    pub(crate) command: Option<ParsedCommand>,
    pub(crate) error: Option<CliError>,
}

pub(crate) fn parse_args(args: &[String]) -> ParsedArgs {
    let help = args.iter().any(|arg| arg == "--help" || arg == "-h");
    let version = args.iter().any(|arg| arg == "--version" || arg == "-V");
    // Reporter detection is a pre-scan so input errors after `--reporter json`
    // can still be emitted as machine-readable envelopes.
    let mut parsed = ParsedArgs {
        help,
        version,
        reporter: scan_json_reporter(args).unwrap_or(Reporter::Text),
        config_path: None,
        command: None,
        error: None,
    };

    let mut seen_options = HashSet::<&'static str>::new();
    let mut target = None;
    let mut check = false;
    let mut emit_options = Vec::<String>::new();
    let mut positionals = Vec::<String>::new();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];

        if arg == "--help" || arg == "-h" {
            record_flag(&mut parsed, &mut seen_options, "--help");
            index += 1;
            continue;
        }

        if arg == "--version" || arg == "-V" {
            record_flag(&mut parsed, &mut seen_options, "--version");
            index += 1;
            continue;
        }

        if arg == "--config" {
            index = parse_config_option(&mut parsed, &mut seen_options, args, index);
            continue;
        }

        if let Some(value) = arg.strip_prefix("--config=") {
            record_config_value(&mut parsed, &mut seen_options, value);
            index += 1;
            continue;
        }

        if arg == "--reporter" {
            index = parse_value_option(
                &mut parsed,
                &mut seen_options,
                args,
                index,
                "--reporter",
                parse_reporter_value,
            );
            continue;
        }

        if let Some(value) = arg.strip_prefix("--reporter=") {
            record_value_option(
                &mut parsed,
                &mut seen_options,
                "--reporter",
                value,
                parse_reporter_value,
            );
            index += 1;
            continue;
        }

        if arg == "--target" {
            emit_options.push(arg.clone());
            index = parse_target_option(&mut parsed, &mut seen_options, &mut target, args, index);
            continue;
        }

        if let Some(value) = arg.strip_prefix("--target=") {
            emit_options.push("--target".to_owned());
            record_target_value(&mut parsed, &mut seen_options, &mut target, value);
            index += 1;
            continue;
        }

        if arg == "--check" {
            emit_options.push(arg.clone());
            record_flag(&mut parsed, &mut seen_options, "--check");
            check = true;
            index += 1;
            continue;
        }

        if arg == "--" {
            if positionals.first().is_some_and(|value| value == "messages")
                && positionals.get(1).is_some_and(|value| value == "emit")
            {
                positionals.extend(args[(index + 1)..].iter().cloned());
                break;
            }
            record_first_error(&mut parsed, CliError::unknown_option(arg));
            index += 1;
            continue;
        }

        if arg.starts_with('-') && arg != "-" {
            record_first_error(&mut parsed, CliError::unknown_option(arg));
            index += 1;
            continue;
        }

        positionals.push(arg.clone());
        index += 1;
    }

    parsed.command = parse_command_path(&positionals, target, check, &mut parsed);
    if !matches!(
        parsed.command,
        Some(ParsedCommand::Messages(MessagesCommand::Emit(_)))
    ) {
        if let Some(option) = emit_options.first() {
            record_first_error(&mut parsed, CliError::unknown_option(option));
        }
    }

    parsed
}

fn parse_target_option(
    parsed: &mut ParsedArgs,
    seen_options: &mut HashSet<&'static str>,
    target: &mut Option<String>,
    args: &[String],
    index: usize,
) -> usize {
    match args.get(index + 1) {
        Some(value) if !value.starts_with('-') => {
            record_target_value(parsed, seen_options, target, value);
            index + 2
        }
        _ => {
            record_first_error(parsed, CliError::missing_option_value("--target"));
            index + 1
        }
    }
}

fn record_target_value(
    parsed: &mut ParsedArgs,
    seen_options: &mut HashSet<&'static str>,
    target: &mut Option<String>,
    value: &str,
) {
    if !seen_options.insert("--target") {
        record_first_error(parsed, CliError::duplicate_option("--target"));
        return;
    }
    *target = Some(value.to_owned());
}

fn parse_value_option(
    parsed: &mut ParsedArgs,
    seen_options: &mut HashSet<&'static str>,
    args: &[String],
    index: usize,
    option: &'static str,
    parse_value: impl FnOnce(&str) -> Option<CliError>,
) -> usize {
    match args.get(index + 1) {
        Some(value) if !value.starts_with('-') => {
            record_value_option(parsed, seen_options, option, value, parse_value);
            index + 2
        }
        _ => {
            record_first_error(parsed, CliError::missing_option_value(option));
            index + 1
        }
    }
}

fn parse_config_option(
    parsed: &mut ParsedArgs,
    seen_options: &mut HashSet<&'static str>,
    args: &[String],
    index: usize,
) -> usize {
    match args.get(index + 1) {
        Some(value) if !value.starts_with('-') => {
            record_config_value(parsed, seen_options, value);
            index + 2
        }
        _ => {
            record_first_error(parsed, CliError::missing_option_value("--config"));
            index + 1
        }
    }
}

fn record_flag(
    parsed: &mut ParsedArgs,
    seen_options: &mut HashSet<&'static str>,
    option: &'static str,
) {
    if !seen_options.insert(option) {
        record_first_error(parsed, CliError::duplicate_option(option));
    }
}

fn record_config_value(
    parsed: &mut ParsedArgs,
    seen_options: &mut HashSet<&'static str>,
    value: &str,
) {
    // Preserve the explicit path even for reserved commands. Their eventual
    // implementations can reuse the parsed value without changing argv rules.
    if !seen_options.insert("--config") {
        record_first_error(parsed, CliError::duplicate_option("--config"));
        return;
    }

    parsed.config_path = Some(value.to_owned());
}

fn record_value_option(
    parsed: &mut ParsedArgs,
    seen_options: &mut HashSet<&'static str>,
    option: &'static str,
    value: &str,
    parse_value: impl FnOnce(&str) -> Option<CliError>,
) {
    if !seen_options.insert(option) {
        record_first_error(parsed, CliError::duplicate_option(option));
        return;
    }

    if let Some(error) = parse_value(value) {
        record_first_error(parsed, error);
    }
}

fn parse_reporter_value(value: &str) -> Option<CliError> {
    match value {
        "text" | "json" => None,
        reporter => Some(CliError::reporter_not_supported(reporter)),
    }
}

fn parse_command_path(
    positionals: &[String],
    target: Option<String>,
    check: bool,
    parsed: &mut ParsedArgs,
) -> Option<ParsedCommand> {
    let command = positionals.first()?;
    match command.as_str() {
        "fmt" => Some(ParsedCommand::Reserved("fmt")),
        "lint" => Some(ParsedCommand::Reserved("lint")),
        "check" => Some(ParsedCommand::Reserved("check")),
        "init" => Some(ParsedCommand::Reserved("init")),
        "messages" => match positionals.get(1).map(String::as_str) {
            None => Some(ParsedCommand::Messages(MessagesCommand::Namespace)),
            Some("emit") => {
                if let Some(target) = target.as_deref() {
                    if !crate::messages::is_valid_delivery_target_name(target) {
                        record_first_error(
                            parsed,
                            CliError::invalid_argument(
                                format!("The messages emit target name is invalid: {target}"),
                                serde_json::json!({
                                    "reason": "invalid_target_name",
                                    "option": "--target",
                                    "value": target,
                                }),
                            ),
                        );
                    }
                }
                if let Some(argument) = positionals.get(2) {
                    record_first_error(
                        parsed,
                        CliError::invalid_argument(
                            format!(
                                "The messages emit command does not accept operands: {argument}"
                            ),
                            serde_json::json!({ "argument": argument }),
                        ),
                    );
                }
                Some(ParsedCommand::Messages(MessagesCommand::Emit(
                    MessagesEmitArgs { target, check },
                )))
            }
            Some("prune") => Some(ParsedCommand::Messages(MessagesCommand::Prune)),
            Some(leaf) => Some(ParsedCommand::Unknown(format!("messages.{leaf}"))),
        },
        command => Some(ParsedCommand::Unknown(command.to_owned())),
    }
}

fn record_first_error(parsed: &mut ParsedArgs, error: CliError) {
    if parsed.error.is_none() {
        parsed.error = Some(error);
    }
}

fn scan_json_reporter(args: &[String]) -> Option<Reporter> {
    args.iter().enumerate().find_map(|(index, arg)| {
        if arg == "--reporter=json" {
            return Some(Reporter::Json);
        }
        if arg == "--reporter" && args.get(index + 1).is_some_and(|value| value == "json") {
            return Some(Reporter::Json);
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> ParsedArgs {
        parse_args(&args.iter().copied().map(str::to_owned).collect::<Vec<_>>())
    }

    #[test]
    fn parses_global_options_after_reserved_command() {
        let parsed = parse(&["fmt", "--reporter", "json"]);

        assert_eq!(parsed.command, Some(ParsedCommand::Reserved("fmt")));
        assert_eq!(parsed.reporter, Reporter::Json);
        assert!(parsed.error.is_none());
    }

    #[test]
    fn preserves_config_path_values() {
        let separated = parse(&["--config", "intlify.config.json", "fmt"]);
        let equals = parse(&["fmt", "--config=intlify.config.jsonc"]);

        assert_eq!(
            separated.config_path,
            Some("intlify.config.json".to_owned())
        );
        assert_eq!(equals.config_path, Some("intlify.config.jsonc".to_owned()));
    }

    #[test]
    fn keeps_first_unknown_positional_as_command() {
        let parsed = parse(&["file.mf2", "extra"]);

        assert_eq!(
            parsed.command,
            Some(ParsedCommand::Unknown("file.mf2".to_owned()))
        );
    }

    #[test]
    fn rejects_clustered_short_options() {
        let parsed = parse(&["-hV"]);

        assert!(matches!(
            parsed.error,
            Some(CliError {
                code: "unknown_cli_option",
                ..
            })
        ));
    }

    #[test]
    fn parses_nested_emit_options_in_any_position() {
        let parsed = parse(&[
            "--target",
            "web",
            "messages",
            "--check",
            "emit",
            "--reporter=json",
        ]);

        assert_eq!(
            parsed.command,
            Some(ParsedCommand::Messages(MessagesCommand::Emit(
                MessagesEmitArgs {
                    target: Some("web".to_owned()),
                    check: true,
                }
            )))
        );
        assert_eq!(parsed.reporter, Reporter::Json);
        assert!(parsed.error.is_none());
    }

    #[test]
    fn rejects_emit_operands_before_project_processing() {
        let parsed = parse(&["messages", "emit", "locales"]);

        assert!(matches!(
            parsed.error,
            Some(CliError {
                code: "invalid_cli_argument",
                ..
            })
        ));
        assert_eq!(
            parsed.command.unwrap().resolved_name(),
            Some("messages.emit")
        );
    }

    #[test]
    fn accepts_an_empty_end_of_options_marker_for_emit() {
        let parsed = parse(&["messages", "emit", "--"]);

        assert_eq!(
            parsed.command,
            Some(ParsedCommand::Messages(MessagesCommand::Emit(
                MessagesEmitArgs {
                    target: None,
                    check: false,
                }
            )))
        );
        assert!(parsed.error.is_none());
    }

    #[test]
    fn rejects_invalid_emit_target_before_project_processing() {
        for target in ["", "Web", ".web", "web/esm", &"a".repeat(256)] {
            let parsed = parse(&["messages", "emit", &format!("--target={target}")]);

            assert!(matches!(
                parsed.error,
                Some(CliError {
                    code: "invalid_cli_argument",
                    ..
                })
            ));
            assert_eq!(
                parsed.command.unwrap().resolved_name(),
                Some("messages.emit")
            );
        }
    }

    #[test]
    fn joins_unknown_message_leaf_for_command_evidence() {
        let parsed = parse(&["messages", "inspect"]);

        assert_eq!(
            parsed.command,
            Some(ParsedCommand::Unknown("messages.inspect".to_owned()))
        );
    }
}
