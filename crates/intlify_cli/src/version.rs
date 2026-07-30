// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Compile-time package version exposed by CLI output and machine envelopes.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
