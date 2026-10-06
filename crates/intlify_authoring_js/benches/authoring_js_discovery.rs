// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fixed developer smoke: actual capture, retained records, disk reread, and
//! owner-backed common admission. No average, threshold, or product CLI API.

use std::process::ExitCode;

use intlify_authoring_js::benchmark::facade::{AuthoringJsDiscovery, RUNNER};

fn main() -> ExitCode {
    RUNNER.main::<AuthoringJsDiscovery>(std::env::args_os().skip(1).collect(), || {
        Ok(tempfile::Builder::new()
            .prefix("intlify-authoring-js-smoke-")
            .tempdir()?
            .keep())
    })
}
