// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

#[path = "build/toolchain.rs"]
mod toolchain;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=build/toolchain.rs");
    // Reading a record never asks the compiler anything; only acquisition does.
    if std::env::var_os("CARGO_FEATURE_ACQUISITION").is_some() {
        toolchain::emit();
    }
}
