// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Open the conformance suite at the Suite Index the one argument names, and
//! run its cases.
//!
//! The suite root is the directory that holds the index. The runner reads the
//! Suite Index Schema at its fixed path below that root and what the index
//! names, and nothing else; there is no default index. It prints each body it
//! verified with its file digest, or the Conformance harness failure that
//! stopped it, and then each case with whether the implementation did what
//! the case expects. It fails when the suite does not open or a case does not
//! pass.

use std::path::Path;
use std::process::ExitCode;

use intlify_config::conformance_support::open;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let (Some(index), None) = (args.next(), args.next()) else {
        eprintln!("usage: run_conformance_suite <suite-index>");
        return ExitCode::FAILURE;
    };
    let index = Path::new(&index);
    let opened = match open(index) {
        Ok(opened) => opened,
        Err(failure) => {
            eprintln!("conformance harness failure: {failure}");
            return ExitCode::FAILURE;
        }
    };
    for (path, digest) in &opened.verified {
        println!("{digest} {path}");
    }
    println!(
        "the suite at {} opens: {} bodies verified",
        index.display(),
        opened.verified.len()
    );
    let outcomes = opened.run();
    let mut failed = 0;
    for outcome in &outcomes {
        if outcome.passed() {
            println!("pass {}", outcome.id);
        } else {
            failed += 1;
            eprintln!(
                "FAIL {}: expected {}, got {}",
                outcome.id, outcome.expected, outcome.actual
            );
        }
    }
    println!("{} cases run, {failed} failed", outcomes.len());
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
