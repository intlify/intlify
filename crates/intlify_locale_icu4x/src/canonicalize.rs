// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! ICU4X 2.2.0's extended canonicalization over the blob's marker families,
//! exactly as ICU4X behaves. This is the engine design 015's conformance
//! layer wraps, not that layer: it checks no validity, applies no Intlify
//! correction, and passes through forms revision "0" rejects, such as `zz`
//! or `en-x-brand`. It never adds likely subtags.

use icu_locale_core::Locale;

use crate::markers::MarkerBlob;

/// The engine and its release: ICU4X 2.2.0, as design 015 pins it. A provider
/// reports it among the facts of what it admitted, never as profile
/// semantics.
pub const ENGINE: (&str, &str) = ("icu4x", "2.2.0");

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
    use crate::{RawFailure, ENGINE};

    #[test]
    fn the_engine_is_the_icu4x_release_cargo_locked() {
        let lock = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"),
        )
        .unwrap();
        for package in [
            "icu_locale",
            "icu_locale_core",
            "icu_provider",
            "icu_provider_blob",
        ] {
            let name = format!("name = \"{package}\"\n");
            let versions: Vec<_> = lock
                .split("[[package]]")
                .filter(|entry| entry.contains(&name))
                .filter_map(|entry| {
                    entry
                        .lines()
                        .find_map(|line| line.strip_prefix("version = "))
                })
                .collect();
            assert_eq!(versions, [format!("\"{}\"", ENGINE.1)], "{package}");
        }
        assert_eq!(ENGINE.0, "icu4x");
    }

    #[test]
    fn only_the_generators_link_icu4x_compiled_data() {
        let manifest = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
        )
        .unwrap();
        // Comments may mention compiled data; only the settings count.
        let settings: Vec<&str> = manifest
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        let setting = |name: &str| {
            settings
                .iter()
                .find(|line| line.starts_with(&format!("{name} = ")))
                .copied()
                .unwrap()
        };
        assert_eq!(setting("default"), "default = []");
        assert!(setting("icu_locale").contains("default-features = false"));
        assert!(setting("icu_locale_data").contains("optional = true"));
        // The generate feature is the one place that turns either on.
        let generate = settings
            .iter()
            .position(|line| *line == "generate = [")
            .unwrap();
        let end = generate
            + settings[generate..]
                .iter()
                .position(|line| *line == "]")
                .unwrap();
        for needle in ["compiled_data", "dep:icu_locale_data"] {
            let places: Vec<usize> = (0..settings.len())
                .filter(|&index| settings[index].contains(needle))
                .collect();
            assert_eq!(places.len(), 1, "{needle}");
            assert!(generate < places[0] && places[0] < end, "{needle}");
        }
    }

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
