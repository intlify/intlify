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
mod tests {
    use crate::markers::tests::committed;
    use crate::RawFailure;

    #[test]
    fn icu4x_canonicalizes_case_aliases_and_order_without_adding_subtags() {
        let blob = committed();
        for (input, expected) in [
            ("en", "en"),
            ("en-US", "en-US"),
            ("ja", "ja"),
            ("EN-latn-us", "en-Latn-US"),
            ("zh-Hant-TW", "zh-Hant-TW"),
            ("iw-IL", "he-IL"),
            ("sgn-GR", "gss"),
            ("cmn", "zh"),
            ("en-u-nu-latn-ca-gregory", "en-u-ca-gregory-nu-latn"),
        ] {
            assert_eq!(
                blob.canonicalize_raw(input).as_deref(),
                Ok(expected),
                "{input}"
            );
        }
        assert_ne!(blob.canonicalize_raw("en"), blob.canonicalize_raw("en-US"));
    }

    #[test]
    fn forms_revision_0_rejects_still_pass_through_the_raw_engine() {
        // The conformance layer rejects these before ICU4X, or corrects them
        // after: invalid or unregistered subtags, a reserved language, private
        // use, a non-u/t extension, and a calendar alias ICU4X 2.2.0 leaves alone.
        let blob = committed();
        for input in [
            "zz",
            "qfz",
            "en-u-ca-madeup",
            "en-u-zz-abc",
            "en-x-brand",
            "en-a-foo",
            "und-u-ca-islamicc",
        ] {
            assert_eq!(
                blob.canonicalize_raw(input).as_deref(),
                Ok(input),
                "{input}"
            );
        }
    }

    #[test]
    fn inputs_icu4x_does_not_parse_are_unparsable() {
        let blob = committed();
        for input in ["", "en_US", "root", "en--US", "-en", "en-", "日本語"] {
            assert_eq!(
                blob.canonicalize_raw(input),
                Err(RawFailure::Unparsable),
                "{input}"
            );
        }
    }

    #[test]
    fn every_canonical_spelling_is_a_fixed_point() {
        let blob = committed();
        for input in [
            "EN-latn-us",
            "iw-IL",
            "sgn-GR",
            "cmn",
            "en-u-nu-latn-ca-gregory",
        ] {
            let once = blob.canonicalize_raw(input).unwrap();
            assert_eq!(blob.canonicalize_raw(&once).unwrap(), once, "{input}");
        }
    }
}
