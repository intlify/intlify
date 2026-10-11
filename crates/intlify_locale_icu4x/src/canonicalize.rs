// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! ICU4X 2.2.0's extended canonicalization over the blob's marker families,
//! exactly as ICU4X behaves. This is the engine design 015's conformance
//! layer wraps, not that layer: it checks no validity, applies no Intlify
//! correction, and passes through forms revision "0" rejects, such as `zz`
//! or `en-x-brand`. It never adds likely subtags.

use icu_locale_core::Locale;

use crate::markers::MarkerBlob;

/// Why ICU4X gives no canonical spelling. It keeps no part of the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawFailure {
    /// ICU4X does not parse the input as a Unicode locale identifier.
    Unparsable,
}

impl MarkerBlob {
    /// ICU4X's canonical spelling of `input`, before any conformance layer.
    pub fn canonicalize_raw(&self, input: &str) -> Result<String, RawFailure> {
        let mut locale = input
            .parse::<Locale>()
            .map_err(|_| RawFailure::Unparsable)?;
        self.canonicalizer().canonicalize(&mut locale);
        Ok(locale.to_string())
    }
}

#[cfg(test)]
mod tests;
