// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

#[allow(dead_code)]
pub(crate) const fn validate_source_text(_source: &str) -> bool {
    // Rust `&str` is already valid UTF-8 and cannot contain unpaired
    // surrogates. JavaScript string validation happens before crossing
    // this boundary.
    true
}
