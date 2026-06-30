// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

mod args;
mod command;
pub mod config;
mod error;
mod output;
pub mod schema;
pub mod version;

use std::env;
use std::path::Path;

use args::{parse_args, ParsedCommand};
use command::{reserved_command, top_level_help};
use error::CliError;
use output::{render_error, render_reserved_command};

pub use output::CliRunResult;

pub fn run_env() -> CliRunResult {
    let args = env::args().skip(1);
    let cwd = env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    run(args, cwd)
}

pub fn run<I, S>(args: I, cwd: impl AsRef<Path>) -> CliRunResult
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let raw_args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    run_with_slice(&raw_args, cwd.as_ref())
}

fn run_with_slice(raw_args: &[String], cwd: &Path) -> CliRunResult {
    let parsed = parse_args(raw_args);

    // Help and version are intentionally resolved before argument errors so
    // discovery or invalid future command operands never block those exits.
    if parsed.help {
        return match parsed
            .command
            .as_ref()
            .and_then(ParsedCommand::reserved_name)
        {
            Some(command) => CliRunResult::success(command::reserved_help(command)),
            None => CliRunResult::success(top_level_help()),
        };
    }

    if parsed.version {
        return CliRunResult::success(format!("{}\n", version::VERSION));
    }

    if raw_args.is_empty() {
        return CliRunResult::success(top_level_help());
    }

    let project_root = config::discover_project_root(cwd);
    // Phase 3A records the explicit config path for the future loader but
    // reserved commands stop before config discovery, parsing, or validation.
    let _explicit_config_path = parsed
        .config_path
        .as_deref()
        .map(|path| config::resolve_explicit_config_path(cwd, path));

    if let Some(error) = parsed.error {
        let command = parsed
            .command
            .as_ref()
            .and_then(ParsedCommand::resolved_name)
            .unwrap_or("intlify");
        return render_error(error, parsed.reporter, command, &project_root);
    }

    if parsed.command.is_none() {
        return CliRunResult::success(top_level_help());
    }

    match parsed.command {
        // Formatter/linter engines land in later phases; these command names are
        // reserved now so integrations can depend on the public CLI surface.
        Some(ParsedCommand::Reserved(command)) => {
            render_reserved_command(reserved_command(command), parsed.reporter, &project_root)
        }
        Some(ParsedCommand::Unknown(command)) => render_error(
            CliError::unknown_command(&command),
            parsed.reporter,
            "intlify",
            &project_root,
        ),
        None => CliRunResult::success(top_level_help()),
    }
}
