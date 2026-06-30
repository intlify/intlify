// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::HashSet;

use crate::error::CliError;
use crate::output::Reporter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParsedCommand {
    Reserved(&'static str),
    Unknown(String),
}

impl ParsedCommand {
    pub(crate) const fn reserved_name(&self) -> Option<&'static str> {
        match self {
            Self::Reserved(command) => Some(command),
            Self::Unknown(_) => None,
        }
    }

    pub(crate) fn resolved_name(&self) -> Option<&str> {
        match self {
            Self::Reserved(command) => Some(command),
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

        if arg.starts_with('-') {
            record_first_error(&mut parsed, CliError::unknown_option(arg));
            index += 1;
            continue;
        }

        if parsed.command.is_none() && parsed.error.is_none() {
            parsed.command = Some(parse_command(arg));
        }
        index += 1;
    }

    parsed
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
    // Store the path even though Phase 3A reserved commands do not load config;
    // Phase 3B/3C can thread this through without changing parser behavior.
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

fn parse_command(command: &str) -> ParsedCommand {
    match command {
        "fmt" => ParsedCommand::Reserved("fmt"),
        "lint" => ParsedCommand::Reserved("lint"),
        "check" => ParsedCommand::Reserved("check"),
        "init" => ParsedCommand::Reserved("init"),
        command => ParsedCommand::Unknown(command.to_owned()),
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
}
