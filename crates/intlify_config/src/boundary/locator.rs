// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Portable Source Locator admission for a file entry: a candidate
//! admitted as the entry's `relative-path` locator, or treated exactly as
//! locator absence (D15:762-766; 015-081, 015-171).
//!
//! The adapter constructs the canonical form and classifies it; the core only
//! validates the variant, the classification, the bound, and the payload, and
//! never repairs, converts, or guesses one. A file entry's locator is a
//! `relative-path`: a non-empty configuration-root-relative path whose `/`
//! separated segments hold no NUL or backslash and are neither empty, `.`, nor
//! `..`, with no root prefix and no drive prefix, an ASCII letter and `:` at
//! the start. `bootstrapOnly.evidenceMetadata.maxLocatorBytes` bounds the
//! payload's UTF-8 bytes. A candidate that is missing, of another variant,
//! unclassified, over the bound, or malformed is absence: it never fails the
//! entry, emits no Finding, and leaves nothing behind.
//!
//! Evidence compares admitted locators by variant rank, `relative-path`,
//! `safe-label`, then `safe-uri`, and then by exact payload bytes, after their
//! absence, and views one as a single-member object keyed by its variant
//! (D15:840, 2496).

use serde_json::{json, Value};

use crate::artifacts::count;
use crate::input_limits::Bound;

use super::capability::ImplementationCapability;

/// The closed locator variants, in their fixed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LocatorVariant {
    RelativePath,
    SafeLabel,
    SafeUri,
}

impl LocatorVariant {
    /// 015's name of the variant.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::RelativePath => "relative-path",
            Self::SafeLabel => "safe-label",
            Self::SafeUri => "safe-uri",
        }
    }
}

/// A locator as an adapter supplies it: its variant, its payload in the
/// variant's canonical form, and whether the adapter classified the payload,
/// and for a path its root, as non-secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocatorCandidate<'a> {
    pub(crate) variant: LocatorVariant,
    pub(crate) payload: &'a str,
    pub(crate) non_secret: bool,
}

/// An admitted locator: its variant and its exact payload, in 015's order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Locator {
    variant: LocatorVariant,
    payload: String,
}

impl Locator {
    pub(crate) const fn variant(&self) -> LocatorVariant {
        self.variant
    }

    pub(crate) fn payload(&self) -> &str {
        &self.payload
    }

    /// `{ variant: payload }` (D15:2496).
    pub(crate) fn view(&self) -> Value {
        json!({ self.variant.id(): self.payload })
    }
}

/// A file entry's locator under the capability's bound: the admitted
/// `relative-path`, or `None` for absence.
pub(crate) fn file_locator(
    candidate: Option<LocatorCandidate<'_>>,
    capability: &ImplementationCapability,
) -> Option<Locator> {
    read_file_locator(
        candidate,
        capability
            .bootstrap_only()
            .evidence_metadata
            .max_locator_bytes,
    )
}

/// The admission under `bound`, which [`file_locator`] takes from the
/// capability.
pub(crate) fn read_file_locator(
    candidate: Option<LocatorCandidate<'_>>,
    bound: Bound,
) -> Option<Locator> {
    let candidate = candidate?;
    let admitted = candidate.variant == LocatorVariant::RelativePath
        && candidate.non_secret
        && count(candidate.payload.len()) <= bound.get()
        && relative_path(candidate.payload);
    admitted.then(|| Locator {
        variant: candidate.variant,
        payload: candidate.payload.to_owned(),
    })
}

/// Whether `payload` is a canonical `relative-path`. An empty payload is one
/// empty segment and a root prefix leaves an empty first one, so the segment
/// check refuses both.
fn relative_path(payload: &str) -> bool {
    let bytes = payload.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    !drive
        && !payload.contains(['\0', '\\'])
        && payload
            .split('/')
            .all(|segment| !matches!(segment, "" | "." | ".."))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::capability::{
        IDENTITY as CAPABILITY, REVISION as CAPABILITY_REVISION,
    };
    use crate::locale::VersionedIdentity;
    use crate::resolver::limits::fixture_limits;

    fn path(payload: &str) -> LocatorCandidate<'_> {
        LocatorCandidate {
            variant: LocatorVariant::RelativePath,
            payload,
            non_secret: true,
        }
    }

    fn roomy(candidate: LocatorCandidate<'_>) -> Option<Locator> {
        read_file_locator(Some(candidate), Bound::new(u64::MAX - 1).unwrap())
    }

    #[test]
    fn a_canonical_relative_path_is_admitted_as_given() {
        for payload in [
            "intlify.config.json",
            "config/intlify.json",
            "ab:c/intlify.json",
            "config/C:x/intlify.json",
            "a b/.hidden/c..d/...",
            "設定/intlify.json",
        ] {
            let locator = roomy(path(payload)).unwrap();
            assert_eq!(locator.variant(), LocatorVariant::RelativePath);
            assert_eq!(locator.payload(), payload);
        }
        // Equality is exact bytes: composed and decomposed spellings stay two
        // locators, neither normalized into the other.
        let composed = roomy(path("\u{c5}.json")).unwrap();
        let decomposed = roomy(path("A\u{30a}.json")).unwrap();
        assert_ne!(composed, decomposed);
        assert_eq!(decomposed.payload(), "A\u{30a}.json");
    }

    #[test]
    fn a_malformed_relative_path_is_absence() {
        for payload in [
            "",
            "/config/intlify.json",
            "//server/share/intlify.json",
            "C:/config/intlify.json",
            "c:intlify.json",
            "Z:",
            "config\\intlify.json",
            "config//intlify.json",
            "config/",
            "./intlify.json",
            "config/./intlify.json",
            "../intlify.json",
            "config/../intlify.json",
            ".",
            "..",
            "config/intlify\0.json",
        ] {
            assert_eq!(roomy(path(payload)), None, "{payload:?}");
        }
    }

    #[test]
    fn another_variant_or_an_unclassified_path_is_absence() {
        for variant in [LocatorVariant::SafeLabel, LocatorVariant::SafeUri] {
            let candidate = LocatorCandidate {
                variant,
                ..path("config/intlify.json")
            };
            assert_eq!(roomy(candidate), None, "{variant:?}");
        }
        let unclassified = LocatorCandidate {
            non_secret: false,
            ..path("config/intlify.json")
        };
        assert_eq!(roomy(unclassified), None);
        assert_eq!(
            read_file_locator(None, Bound::new(u64::MAX - 1).unwrap()),
            None
        );
    }

    /// The bound counts the payload's UTF-8 bytes, admitting its exact value
    /// and refusing one more.
    #[test]
    fn the_bound_counts_the_payload_bytes() {
        let payload = "設定/intlify.json";
        let bound = |value: usize| Bound::new(count(value)).unwrap();
        assert!(read_file_locator(Some(path(payload)), bound(payload.len())).is_some());
        assert_eq!(
            read_file_locator(Some(path(payload)), bound(payload.len() - 1)),
            None
        );
    }

    fn admitted(variant: LocatorVariant, payload: &str) -> Locator {
        Locator {
            variant,
            payload: payload.to_owned(),
        }
    }

    #[test]
    fn a_locator_is_viewed_under_its_variant() {
        assert_eq!(
            roomy(path("config/intlify.json")).unwrap().view(),
            json!({ "relative-path": "config/intlify.json" })
        );
        assert_eq!(
            admitted(LocatorVariant::SafeLabel, "storefront config").view(),
            json!({ "safe-label": "storefront config" })
        );
        assert_eq!(
            admitted(LocatorVariant::SafeUri, "https://example.com/config").view(),
            json!({ "safe-uri": "https://example.com/config" })
        );
    }

    /// Absence, then variant rank, then exact payload bytes with a shorter
    /// equal prefix first, never a normalized or case-folded spelling.
    #[test]
    fn locators_compare_by_variant_then_payload_bytes() {
        use LocatorVariant::{RelativePath, SafeLabel, SafeUri};
        let mut locators = vec![
            Some(admitted(SafeUri, "a:b")),
            Some(admitted(SafeLabel, "b")),
            Some(admitted(RelativePath, "z/a")),
            None,
            Some(admitted(RelativePath, "z")),
            Some(admitted(RelativePath, "Z")),
            Some(admitted(SafeLabel, "A\u{30a}")),
            Some(admitted(SafeLabel, "\u{c5}")),
        ];
        locators.sort();
        assert_eq!(
            locators,
            [
                None,
                Some(admitted(RelativePath, "Z")),
                Some(admitted(RelativePath, "z")),
                Some(admitted(RelativePath, "z/a")),
                Some(admitted(SafeLabel, "A\u{30a}")),
                Some(admitted(SafeLabel, "b")),
                Some(admitted(SafeLabel, "\u{c5}")),
                Some(admitted(SafeUri, "a:b")),
            ]
        );
    }

    #[test]
    fn the_capability_bounds_the_locator() {
        let payload = "config/intlify.json";
        let capability = |bound: usize| {
            let mut value = ImplementationCapability::fixture_owned(fixture_limits(1)).normalized();
            value["bootstrapOnly"]["evidenceMetadata"]["maxLocatorBytes"] =
                json!(bound.to_string());
            let constructed = VersionedIdentity {
                identity: CAPABILITY,
                revision: CAPABILITY_REVISION,
            };
            crate::boundary::capability::admit(Some(&value), &constructed).unwrap()
        };
        assert!(file_locator(Some(path(payload)), &capability(payload.len())).is_some());
        assert_eq!(
            file_locator(Some(path(payload)), &capability(payload.len() - 1)),
            None
        );
    }
}
