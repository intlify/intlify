// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Portable Source Locator admission: a candidate admitted as a file
//! entry's `relative-path` locator, or as a Programmatic Entry's or Call-Site
//! Source's `safe-label` or `safe-uri` locator, or treated exactly as locator
//! absence (D15:747, 762-766, 796; 015-081, 015-171).
//!
//! The adapter or frontend constructs the canonical form and classifies it;
//! the core only validates the variant, the classification, the bound, and
//! the payload, and never repairs, converts, or guesses one. A file entry's
//! locator is a `relative-path`: a non-empty configuration-root-relative path
//! whose `/` separated segments hold no NUL or backslash and are neither
//! empty, `.`, nor `..`, with no root prefix and no drive prefix, an ASCII
//! letter and `:` at the start. A programmatic locator is a `safe-label`, a
//! non-empty string, or a `safe-uri`, an absolute ASCII URI already in
//! RFC 3986's syntax-based normal form, with no query, fragment, or user
//! information, a scheme other than `file`, a host that escapes no byte past
//! ASCII, since a Unicode host must already be in its A-label form, and a port
//! only when it is the shortest decimal of one that is not its scheme's
//! default: `80` for `http` and `ws`, `443` for `https` and `wss`.
//! `bootstrapOnly.evidenceMetadata.maxLocatorBytes` bounds the payload's
//! UTF-8 bytes. A candidate that is missing, of another variant, unclassified,
//! over the bound, or malformed is absence: it never fails the entry, emits no
//! Finding, and leaves nothing behind.
//!
//! Evidence compares admitted locators by variant rank, `relative-path`,
//! `safe-label`, then `safe-uri`, and then by exact payload bytes, after their
//! absence, and views one as a single-member object keyed by its variant
//! (D15:840, 2496).

use serde_json::{json, Value};

use crate::artifacts::count;
use crate::construction::validator::uri::Reference;
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
    admit(candidate?, bound, |variant, payload| {
        variant == LocatorVariant::RelativePath && relative_path(payload)
    })
}

/// A Programmatic Entry's or Call-Site Source's locator under the
/// capability's bound: the admitted `safe-label` or `safe-uri`, or `None` for
/// absence.
pub(crate) fn programmatic_locator(
    candidate: Option<LocatorCandidate<'_>>,
    capability: &ImplementationCapability,
) -> Option<Locator> {
    read_programmatic_locator(
        candidate,
        capability
            .bootstrap_only()
            .evidence_metadata
            .max_locator_bytes,
    )
}

/// The admission under `bound`, which [`programmatic_locator`] takes from
/// the capability.
pub(crate) fn read_programmatic_locator(
    candidate: Option<LocatorCandidate<'_>>,
    bound: Bound,
) -> Option<Locator> {
    admit(candidate?, bound, |variant, payload| match variant {
        LocatorVariant::RelativePath => false,
        LocatorVariant::SafeLabel => !payload.is_empty(),
        LocatorVariant::SafeUri => safe_uri(payload),
    })
}

/// The candidate as a locator when the adapter classified it non-secret, its
/// payload is within `bound`, and `canonical` accepts its variant and payload.
fn admit(
    candidate: LocatorCandidate<'_>,
    bound: Bound,
    canonical: impl FnOnce(LocatorVariant, &str) -> bool,
) -> Option<Locator> {
    let admitted = candidate.non_secret
        && count(candidate.payload.len()) <= bound.get()
        && canonical(candidate.variant, candidate.payload);
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

/// Whether `payload` is a canonical `safe-uri` (D15:764): an absolute URI,
/// with no query or fragment, that RFC 3986's syntax-based normalization
/// leaves as it is, whose scheme is not `file`, and whose authority, when
/// present, has no user information, an ASCII host, and a canonical port.
fn safe_uri(payload: &str) -> bool {
    let Some(uri) = Reference::parse(payload) else {
        return false;
    };
    let Some(scheme) = uri.scheme() else {
        return false;
    };
    uri.query().is_none()
        && uri.fragment().is_none()
        && scheme != "file"
        && uri.authority().is_none_or(|authority| {
            !authority.contains('@')
                && ascii_host(authority)
                && canonical_port(scheme, port(authority))
        })
        && uri == uri.normalize()
}

/// Whether an authority with no user information escapes no byte past ASCII.
/// RFC 3986 spells a non-ASCII registered name as its escaped UTF-8 bytes, so
/// such a host is a Unicode one, which the adapter must already have converted
/// to its A-label form (D15:764, 3377). The grammar makes every `%` the start
/// of an escape, and a port holds none.
fn ascii_host(authority: &str) -> bool {
    !authority
        .as_bytes()
        .windows(2)
        .any(|pair| pair[0] == b'%' && !matches!(pair[1], b'0'..=b'7'))
}

/// The port of an authority with no user information, `None` when no `:`
/// follows the host. Only a port's `:` follows a registered name or an IP
/// literal's `]`.
fn port(authority: &str) -> Option<&str> {
    let host_end = authority.rfind(']').unwrap_or(0);
    authority[host_end..]
        .find(':')
        .map(|colon| &authority[host_end + colon + 1..])
}

/// Whether a port, `None` when absent, is the shortest decimal of a port that
/// is not the scheme's default. Revision `"0"` knows only the defaults of
/// `http`, `ws`, `https`, and `wss`.
fn canonical_port(scheme: &str, port: Option<&str>) -> bool {
    port.is_none_or(|port| {
        let shortest = port == "0" || port.starts_with(|digit: char| ('1'..='9').contains(&digit));
        let default = matches!(
            (scheme, port),
            ("http" | "ws", "80") | ("https" | "wss", "443")
        );
        shortest && !default
    })
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
        let label = label("storefront");
        assert!(programmatic_locator(Some(label), &capability(10)).is_some());
        assert_eq!(programmatic_locator(Some(label), &capability(9)), None);
    }

    fn label(payload: &str) -> LocatorCandidate<'_> {
        LocatorCandidate {
            variant: LocatorVariant::SafeLabel,
            ..path(payload)
        }
    }

    fn uri(payload: &str) -> LocatorCandidate<'_> {
        LocatorCandidate {
            variant: LocatorVariant::SafeUri,
            ..path(payload)
        }
    }

    fn programmatic(candidate: LocatorCandidate<'_>) -> Option<Locator> {
        read_programmatic_locator(Some(candidate), Bound::new(u64::MAX - 1).unwrap())
    }

    /// A `safe-label` is any non-empty string the frontend classified,
    /// compared by its exact bytes and never normalized (D15:763).
    #[test]
    fn a_classified_label_is_admitted_as_given() {
        for payload in ["storefront", "app.ts:12", " ", "設定 / config", "a\\b/../c"] {
            let locator = programmatic(label(payload)).unwrap();
            assert_eq!(locator.variant(), LocatorVariant::SafeLabel);
            assert_eq!(locator.payload(), payload);
        }
        let composed = programmatic(label("\u{c5}")).unwrap();
        let decomposed = programmatic(label("A\u{30a}")).unwrap();
        assert_ne!(composed, decomposed);
        assert_eq!(programmatic(label("")), None);
        let unclassified = LocatorCandidate {
            non_secret: false,
            ..label("storefront")
        };
        assert_eq!(programmatic(unclassified), None);
    }

    /// A `safe-uri` is admitted only already canonical: an absolute ASCII URI
    /// in syntax-based normal form, with a non-default port in its shortest
    /// decimal, and nothing that revision `"0"` removes (D15:764).
    #[test]
    fn a_canonical_uri_is_admitted_as_given() {
        for payload in [
            "https://example.com/config/app.ts",
            "https://example.com",
            "http://example.com:8080/x",
            "http://example.com:443/x",
            "https://example.com:80/x",
            "ws://example.com:0/",
            "https://[2001:db8::1]:8443/x",
            "https://[2001:db8::1]/x",
            "urn:isbn:0451450523",
            "vscode-remote://ssh-remote%2Bhost/src/app.ts",
            "https://example.com/%2F%C3%A9",
            // An adapter's A-label host is ASCII text the core keeps as given.
            "https://xn--bcher-kva.example/x",
            "https://www.xn--bcher-kva.example:8443/x",
            "https://xn--r8jz45g.xn--zckzah/",
            // Revision `"0"` removes only its four defaults, from no registry.
            "ftp://example.com:21/x",
        ] {
            let locator = programmatic(uri(payload)).unwrap();
            assert_eq!(locator.variant(), LocatorVariant::SafeUri, "{payload}");
            assert_eq!(locator.payload(), payload);
        }
    }

    #[test]
    fn a_non_canonical_uri_is_absence() {
        for payload in [
            "",
            "example.com/x",
            "//example.com/x",
            "/x",
            "HTTPS://example.com/x",
            "https://Example.com/x",
            "https://user@example.com/x",
            "https://user:secret@example.com/x",
            "https://example.com/x?q",
            "https://example.com/x?",
            "https://example.com/x#f",
            "https://example.com/x#",
            "file:///etc/intlify.json",
            "file://host/x",
            "http://example.com:80/x",
            "ws://example.com:80/",
            "https://example.com:443/x",
            "wss://example.com:443/",
            "https://example.com:0443/x",
            "https://example.com:00/x",
            "https://example.com:/x",
            "https://[2001:db8::1]:443/x",
            "https://[2001:DB8::1]/x",
            "https://example.com/a/../b",
            "https://example.com/./b",
            "https://example.com/%7Ex",
            "https://example.com/%2f",
            "https://example.com/é",
            "https://example.com/a b",
            "https://exa%mple.com/",
            "https://XN--bcher-kva.example/x",
            "https://bücher.example/x",
            "https://例え.テスト/",
            "https://example.com/%zz",
            "https://example.com/%2",
            "https://example.com/%",
            "https://ex%61mple.com/x",
            "https://b%C3%BCcher.example/x",
            "https://b%C3%BCcher.example:8443/x",
            "vscode-remote://ssh-remote%2B%E8%A8%AD%E5%AE%9A/src/app.ts",
            "https://%FF.example/x",
        ] {
            assert_eq!(programmatic(uri(payload)), None, "{payload}");
        }
    }

    /// `%7F` is the last ASCII byte and `%80` the first past it. An escape of
    /// another ASCII byte stays in a host, as `%2B` does (D15:764, 3377).
    #[test]
    fn a_host_may_escape_only_ascii_bytes() {
        for authority in ["ssh-remote%2Bhost", "a%7Fb", "example.com:8443"] {
            assert!(ascii_host(authority), "{authority}");
        }
        for authority in ["a%80b", "%FF", "b%C3%BCcher.example:8443"] {
            assert!(!ascii_host(authority), "{authority}");
        }
    }

    /// Each spelling an adapter must rewrite, beside the canonical URI its
    /// rewrite gives: the core admits the canonical one and refuses the other
    /// without repairing it. RFC 3986's normalization decodes an unreserved
    /// escape first and then removes the dot segments the decoding spells
    /// (D15:764, 766, 3376, 3377).
    #[test]
    fn a_spelling_the_adapter_must_rewrite_is_refused_and_not_repaired() {
        for (spelled, canonical) in [
            ("HTTPS://example.com/x", "https://example.com/x"),
            ("https://EXAMPLE.com/x", "https://example.com/x"),
            (
                "https://XN--BCHER-KVA.example/x",
                "https://xn--bcher-kva.example/x",
            ),
            ("https://example.com/%7Ex", "https://example.com/~x"),
            ("https://example.com/%41%2d", "https://example.com/A-"),
            ("https://example.com/%c3%a9", "https://example.com/%C3%A9"),
            ("https://example.com/%2f", "https://example.com/%2F"),
            ("https://example.com/a/../b", "https://example.com/b"),
            ("https://example.com/a/%2E%2E/b", "https://example.com/b"),
            ("https://example.com/a/%2e/b", "https://example.com/a/b"),
            ("https://example.com/a/%2E", "https://example.com/a/"),
        ] {
            assert_eq!(programmatic(uri(spelled)), None, "{spelled}");
            let rewritten = Reference::parse(spelled).unwrap().normalize();
            assert_eq!(rewritten.to_string(), canonical, "{spelled}");
            let admitted = programmatic(uri(canonical)).unwrap();
            assert_eq!(admitted.payload(), canonical);
        }
        // A default port is the one rewrite no scheme-independent
        // normalization makes, and a longer spelling of a port is another.
        for (spelled, canonical) in [
            ("http://example.com:80/x", "http://example.com/x"),
            ("ws://example.com:80/", "ws://example.com/"),
            ("https://example.com:443/x", "https://example.com/x"),
            ("wss://example.com:443/", "wss://example.com/"),
            ("https://example.com:08443/x", "https://example.com:8443/x"),
        ] {
            assert_eq!(programmatic(uri(spelled)), None, "{spelled}");
            let admitted = programmatic(uri(canonical)).unwrap();
            assert_eq!(admitted.payload(), canonical);
        }
    }

    /// A programmatic locator is a `safe-label` or a `safe-uri`, never a
    /// `relative-path`, and an unclassified, over-bound, or absent candidate
    /// is absence (D15:747, 766, 796).
    #[test]
    fn a_programmatic_locator_takes_only_its_two_variants() {
        assert_eq!(programmatic(path("config/intlify.json")), None);
        let unclassified = LocatorCandidate {
            non_secret: false,
            ..uri("https://example.com/x")
        };
        assert_eq!(programmatic(unclassified), None);
        let payload = "https://example.com/設定";
        assert_eq!(programmatic(uri(payload)), None);
        let payload = "https://example.com/x";
        let bound = |value: usize| Bound::new(count(value)).unwrap();
        assert!(read_programmatic_locator(Some(uri(payload)), bound(payload.len())).is_some());
        assert_eq!(
            read_programmatic_locator(Some(uri(payload)), bound(payload.len() - 1)),
            None
        );
        let payload = "設定";
        assert!(read_programmatic_locator(Some(label(payload)), bound(payload.len())).is_some());
        assert_eq!(
            read_programmatic_locator(Some(label(payload)), bound(payload.len() - 1)),
            None
        );
        assert_eq!(
            read_programmatic_locator(None, Bound::new(u64::MAX - 1).unwrap()),
            None
        );
    }
}
