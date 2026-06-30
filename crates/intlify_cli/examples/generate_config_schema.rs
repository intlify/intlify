// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::env;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let check = args.first().is_some_and(|arg| arg == "--check");
    if check {
        args.remove(0);
    }

    let path = args
        .first()
        .map_or(intlify_cli::schema::CONFIG_SCHEMA_ARTIFACT, String::as_str);
    let result = if check {
        intlify_cli::schema::check_config_schema(Path::new(path))
    } else {
        intlify_cli::schema::write_config_schema(Path::new(path))
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
