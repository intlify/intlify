// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

#[cfg(feature = "benchmark")]
#[path = "build/metadata.rs"]
mod metadata;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    #[cfg(feature = "benchmark")]
    metadata::emit();
}
