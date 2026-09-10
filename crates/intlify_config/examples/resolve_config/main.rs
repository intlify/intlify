// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use intlify_config::example_support::{resolve, MAX_FILE_BYTES};
use serde_json::Value;

const HELP: &str = "Usage: resolve_config FILE [--profile NAME] [--json]

Read an explicit project-profile configuration file without modifying it.
  --profile NAME  Select a declared profile (required when there are several).
  --json          Print an unstable, display-only JSON result.
  -h, --help      Show this help.
  --              Treat subsequent arguments as file paths.

This developer example uses finite fixture locale data, not production locale
data. It does not produce a LocalizationProjectProfile or resolve Policy/Target
bodies, fallback, or negotiation. See examples/resolve_config/README.md.

Exit codes: 0 = minimum locale core resolved; 1 = configuration rejected;
            2 = usage, file I/O, or internal error.
";

#[derive(Debug, PartialEq, Eq)]
struct Arguments {
    path: PathBuf,
    profile: Option<String>,
    json: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Resolve(Arguments),
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let mut path = None;
    let mut profile = None;
    let mut json = false;
    let mut positional_only = false;
    while let Some(arg) = args.next() {
        if !positional_only {
            match arg.to_str() {
                Some("--help" | "-h") => return Ok(Command::Help),
                Some("--") => {
                    positional_only = true;
                    continue;
                }
                Some("--json") => {
                    if json {
                        return Err("Pass --json only once.".to_owned());
                    }
                    json = true;
                    continue;
                }
                Some("--profile") => {
                    if profile.is_some() {
                        return Err("Pass --profile only once.".to_owned());
                    }
                    let value = args
                        .next()
                        .ok_or("--profile requires a name.")?
                        .into_string()
                        .map_err(|_| "--profile requires a Unicode name.")?;
                    if value.starts_with('-') || value.is_empty() {
                        return Err("--profile requires a name.".to_owned());
                    }
                    profile = Some(value);
                    continue;
                }
                _ if arg.to_string_lossy().starts_with('-') => {
                    return Err(
                        "Unknown option. Use --help, or -- before a file path starting with '-'."
                            .to_owned(),
                    );
                }
                _ => {}
            }
        }
        if path.replace(PathBuf::from(arg)).is_some() {
            return Err("Pass exactly one configuration file.".to_owned());
        }
    }
    Ok(Command::Resolve(Arguments {
        path: path.ok_or("A configuration file is required. Use --help for usage.")?,
        profile,
        json,
    }))
}

fn read_source(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a regular file",
        ));
    }
    // The extra byte lets the resolver reject oversized input before parsing;
    // a truncated prefix can never be mistaken for a complete valid document.
    let mut source = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut source)?;
    Ok(source)
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("(not declared)")
}

fn render(result: &Value, json: bool) -> String {
    if json {
        return format!(
            "{}\n",
            serde_json::to_string_pretty(result).expect("display value serializes")
        );
    }
    let mut lines = vec![
        "Intlify configuration example".to_owned(),
        text(&result["notice"]).to_owned(),
        String::new(),
        format!(
            "Status: {} (stage: {})",
            text(&result["status"]),
            text(&result["stage"])
        ),
    ];
    if result["selectedProfile"].is_string() {
        lines.push(format!("Profile: {}", text(&result["selectedProfile"])));
    }
    if result["locales"].is_object() {
        let locales = &result["locales"];
        lines.extend([
            format!("Source locale: {}", text(&locales["defaultSourceLocale"])),
            format!("Requested locales: {}", locales["requestedLocales"]),
            format!(
                "Default requested locale: {}",
                text(&locales["defaultRequestedLocale"])
            ),
        ]);
    }
    if let Some(corrections) = result["corrections"]
        .as_array()
        .filter(|items| !items.is_empty())
    {
        lines.push("\nSuggested canonical spellings (file is unchanged):".to_owned());
        for correction in corrections {
            lines.push(format!(
                "  {} -> {}",
                text(&correction["pointer"]),
                text(&correction["replacement"])
            ));
        }
    }
    if let Some(diagnostics) = result["diagnostics"]
        .as_array()
        .filter(|items| !items.is_empty())
    {
        lines.push("\nDiagnostics:".to_owned());
        for item in diagnostics {
            lines.push(format!(
                "  [{}] {}",
                text(&item["code"]),
                text(&item["message"])
            ));
            if item["pointer"].is_string() {
                lines.push(format!("    at {}", text(&item["pointer"])));
            }
            if item["position"].is_object() {
                lines.push(format!(
                    "    line {}, byte column {}",
                    item["position"]["line"], item["position"]["byteColumn"]
                ));
            }
            if let Some(pointers) = item["relatedPointers"].as_array() {
                lines.push(format!(
                    "    occurrences: {}",
                    pointers.iter().map(text).collect::<Vec<_>>().join(", ")
                ));
            }
            if item["relatedPosition"].is_object() {
                lines.push(format!(
                    "    related: line {}, byte column {}",
                    item["relatedPosition"]["line"], item["relatedPosition"]["byteColumn"]
                ));
            }
        }
    }
    lines.join("\n") + "\n"
}

fn run(args: impl IntoIterator<Item = OsString>) -> Result<(u8, String), String> {
    let Command::Resolve(args) = parse_args(args)? else {
        return Ok((0, HELP.to_owned()));
    };
    let source =
        read_source(&args.path).map_err(|e| format!("Cannot read {}: {e}", args.path.display()))?;
    let result = resolve(&source, args.profile.as_deref())?;
    let code = u8::from(result["status"] != "resolved");
    Ok((code, render(&result, args.json)))
}

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1)) {
        Ok((code, output)) => match io::stdout().lock().write_all(output.as_bytes()) {
            Ok(()) => ExitCode::from(code),
            Err(error) => {
                eprintln!("resolve_config: cannot write output: {error}");
                ExitCode::from(2)
            }
        },
        Err(error) => {
            eprintln!("resolve_config: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests;
