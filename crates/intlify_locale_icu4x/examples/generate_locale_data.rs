// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Developer-only writer and freshness check for the committed marker blob.

use std::env;
use std::error::Error;
use std::path::Path;
use std::process::ExitCode;

fn run() -> Result<(), Box<dyn Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let blob = intlify_locale_icu4x::generate::markers_blob()
        .map_err(|failure| format!("cannot write the marker blob: {failure:?}"))?;
    let artifact = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/markers.postcard");
    match args.as_slice() {
        [flag] if flag == "--check" => {
            if std::fs::read(&artifact)? != blob {
                return Err("the marker blob is stale; regenerate it with --write".into());
            }
        }
        [flag] if flag == "--write" => std::fs::write(artifact, blob)?,
        _ => return Err("usage: generate_locale_data (--check | --write)".into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
