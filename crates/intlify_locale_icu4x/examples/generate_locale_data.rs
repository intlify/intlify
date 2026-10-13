// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Developer-only writer and freshness check for the committed artifact in
//! `data/`: the marker blob, the conformance payload, and the manifest.

use std::env;
use std::error::Error;
use std::path::Path;
use std::process::ExitCode;

fn run() -> Result<(), Box<dyn Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let artifact = intlify_locale_icu4x::generate::artifact()
        .map_err(|failure| format!("cannot write the artifact: {failure:?}"))?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    match args.as_slice() {
        [flag] if flag == "--check" => {
            for (name, bytes) in artifact.files() {
                if std::fs::read(directory.join(name))? != bytes {
                    return Err(format!("data/{name} is stale; regenerate it with --write").into());
                }
            }
        }
        [flag] if flag == "--write" => {
            for (name, bytes) in artifact.files() {
                std::fs::write(directory.join(name), bytes)?;
            }
        }
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
