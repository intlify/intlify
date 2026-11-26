// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Open the conformance suite at the Suite Index the one argument names.
//!
//! The suite root is the directory that holds the index. The runner reads the
//! Suite Index Schema at its fixed path below that root and what the index
//! names, and nothing else; there is no default index. It prints each body it
//! verified with its file digest, or the Conformance harness failure that
//! stopped it.

use std::path::Path;
use std::process::ExitCode;

use intlify_config::conformance_support::open;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(index), None) = (args.next(), args.next()) else {
        eprintln!("usage: run_conformance_suite <suite-index>");
        return ExitCode::FAILURE;
    };
    match open(Path::new(&index)) {
        Ok(opened) => {
            for (path, digest) in &opened.verified {
                println!("{digest} {path}");
            }
            println!(
                "the suite at {index} opens: {} bodies verified",
                opened.verified.len()
            );
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprintln!("conformance harness failure: {failure}");
            ExitCode::FAILURE
        }
    }
}
