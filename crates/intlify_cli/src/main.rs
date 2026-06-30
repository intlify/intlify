// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let result = intlify_cli::run_env();
    // Treat broken pipes or failed redirects as process failures instead of
    // silently returning the command's logical exit code with truncated output.
    if write_streams(&result).is_err() {
        return ExitCode::FAILURE;
    }

    match u8::try_from(result.exit_code) {
        Ok(code) => ExitCode::from(code),
        Err(_) => ExitCode::FAILURE,
    }
}

fn write_streams(result: &intlify_cli::CliRunResult) -> io::Result<()> {
    if !result.stdout.is_empty() {
        io::stdout().write_all(result.stdout.as_bytes())?;
    }
    if !result.stderr.is_empty() {
        io::stderr().write_all(result.stderr.as_bytes())?;
    }
    Ok(())
}
