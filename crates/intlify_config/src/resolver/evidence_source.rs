// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's configuration source evidence: the Portable Source Identity
//! that a source's exact identity projects to, and the Portable Source Span
//! admitted over that source's exact snapshot, with the order and view of each
//! (D15:732-737, 745, 840, 2481, 2495; Decision 015-185). The Portable Source
//! Locator keeps the form and order that `boundary::locator` gives it.
//!
//! The adapter or frontend that owns an identity domain supplies, with each
//! exact identity, whether it marks the identity non-secret. A marked identity
//! is retained as `safe-opaque`: its exact bytes, the tag, `:`, and the
//! payload. Any other is `redacted`: the Redacted evidence token of its
//! identity-domain tag under the origin of the input that supplied it, so the
//! unmarked identities of one domain and origin share one token. The token is
//! assigned with every other evidence token of the invocation, so a projected
//! identity keeps its component until then. No revision enters either variant.
//!
//! A span is a half-open byte interval admitted only over the exact snapshot
//! of its own source: `start <= end <= length`, a zero-width interval at any
//! byte boundary included. Any other candidate is absence.
//!
//! Evidence compares a source identity by variant rank, `safe-opaque` before
//! `redacted`, then by exact bytes with a shorter equal prefix first, or by the
//! complete digest bytes and then the collision ordinal, absence first. It
//! compares a span by `start` and then `end`, after its absence. The derived
//! orders here are those comparisons. Nothing builds Evidence yet.

use std::fmt::Write as _;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::artifacts::count;
use crate::boundary::source::ExactIdentity;

use super::evidence_token::{EvidenceTokenInvariant, EvidenceTokens, Redacted};
use super::origin::SafeOriginKind;
use super::token::{RedactedToken, Specification};

/// A source's identity as Evidence projects it, before the invocation's
/// evidence tokens are assigned: the exact identity its owner marked
/// non-secret, or the component a token replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceIdentity {
    SafeOpaque(Arc<str>),
    Redacted(Redacted),
}

impl SourceIdentity {
    /// Project `identity`, which an input of `origin` supplied and whose owner
    /// marked it non-secret when `marked` (D15:734-737).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an origin that supplies no source identity,
    /// whether or not the identity is marked.
    pub(crate) fn project(
        origin: SafeOriginKind,
        identity: &ExactIdentity,
        marked: bool,
    ) -> Result<Self, EvidenceTokenInvariant> {
        let redacted = Redacted::source_identity(origin, identity)?;
        Ok(if marked {
            Self::SafeOpaque(Arc::from(identity.as_str()))
        } else {
            Self::Redacted(redacted)
        })
    }

    /// The component a token replaces, when the identity is not marked.
    pub(crate) const fn redacted(&self) -> Option<&Redacted> {
        match self {
            Self::SafeOpaque(_) => None,
            Self::Redacted(component) => Some(component),
        }
    }

    /// The Portable Source Identity Evidence retains, `None` when the identity
    /// is redacted and `tokens` was not assigned its component.
    pub(crate) fn retained(
        &self,
        specification: Specification<'_>,
        tokens: &EvidenceTokens,
    ) -> Option<PortableSourceIdentity> {
        match self {
            Self::SafeOpaque(identity) => Some(PortableSourceIdentity::SafeOpaque(Arc::from(
                identity.as_bytes(),
            ))),
            Self::Redacted(component) => tokens
                .token(specification, component)
                .map(PortableSourceIdentity::Redacted),
        }
    }
}

/// A retained Portable Source Identity, in 015's order: `safe-opaque` before
/// `redacted`, exact bytes with a shorter equal prefix first, and the complete
/// digest bytes before the collision ordinal, absence first.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PortableSourceIdentity {
    SafeOpaque(Arc<[u8]>),
    Redacted(RedactedToken),
}

impl PortableSourceIdentity {
    /// `{ "safe-opaque": hex }` or `{ "redacted": Token }`, the bytes in
    /// lowercase hexadecimal (D15:2452, 2495).
    pub(crate) fn view(&self) -> Value {
        match self {
            Self::SafeOpaque(bytes) => json!({ "safe-opaque": hex(bytes) }),
            Self::Redacted(token) => json!({ "redacted": token.view() }),
        }
    }
}

/// An admitted Portable Source Span, the half-open byte interval
/// `[start, end)` over its source's exact snapshot, in 015's order: `start`,
/// then `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SourceSpan {
    start: u64,
    end: u64,
}

impl SourceSpan {
    /// Admit the candidate `[start, end)` over `snapshot`, the exact bytes of
    /// the source the span points into, or `None` for absence: an inverted
    /// interval, or one that ends past the snapshot (D15:745).
    pub(crate) fn admit(start: u64, end: u64, snapshot: &[u8]) -> Option<Self> {
        (start <= end && end <= count(snapshot.len())).then_some(Self { start, end })
    }

    /// `{ start, end }`, each as its decimal string (D15:2452, 2481).
    pub(crate) fn view(self) -> Value {
        json!({ "start": self.start.to_string(), "end": self.end.to_string() })
    }
}

/// `bytes` as lowercase hexadecimal digits, two per byte.
fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("writing to a String does not fail");
    }
    text
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::boundary::source::{file_source_revision, read_pair, ExactIdentity, SourceLimits};
    use crate::input_limits::Bound;
    use crate::resolver::evidence_token::{
        EvidenceTokenInvariant, EvidenceTokens, Redacted, TokenKind,
    };
    use crate::resolver::origin::SafeOriginKind as O;
    use crate::resolver::token::{Digest, Sha256, Specification};

    use super::{PortableSourceIdentity, SourceIdentity, SourceSpan};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };
    const FILE: &str = "intlify-file-source.v0:storefront/intlify.config.json";
    const PROGRAMMATIC: &str = "intlify-programmatic-entry.v0:storefront#config";
    /// The tokens of the unmarked identities below, computed apart from this
    /// crate by `.plans/drafts/025-pr5d2/vectors.py`. The file token is PR
    /// 5d-1's `source-identity` vector.
    const FILE_TOKEN: &str =
        "sha256:0b14760705ef603246b0f47ea23829ebfcccc8a47da7bc7fc19fc4b0d291612f";
    const PROGRAMMATIC_TOKEN: &str =
        "sha256:d85b6bd775bd8c8918471df34d4e1cb537a0c2d41f6064effbd1d5d6c2672823";
    const OTHER_DOMAIN_TOKEN: &str =
        "sha256:37edffcfb11c9d9b7ff1e1f1002eb8ae58d5d02efaa7d0ed583798a93ca3b2f2";
    /// `FILE`'s UTF-8 bytes in hexadecimal, and those of an identity whose
    /// payload is the byte `0x01`, computed by the same script.
    const FILE_HEX: &str = "696e746c6966792d66696c652d736f757263652e76303a73746f726566726f6e742f696e746c6966792e636f6e6669672e6a736f6e";
    const CONTROL_HEX: &str = "612e76303a01";

    /// A digest under which every record collides.
    struct Constant;

    impl Digest for Constant {
        fn digest(&self, _: &[u8]) -> [u8; 32] {
            [7; 32]
        }
    }

    /// `text` admitted as an exact identity, as the file boundary admits it.
    fn exact(text: &str) -> ExactIdentity {
        let largest = Bound::new(u64::MAX - 1).unwrap();
        let limits = SourceLimits {
            identity_bytes: largest,
            revision_bytes: largest,
        };
        read_pair(Some(text), Some(&file_source_revision(b"")), b"", limits)
            .unwrap()
            .identity()
            .clone()
    }

    fn project(origin: O, text: &str, marked: bool) -> SourceIdentity {
        SourceIdentity::project(origin, &exact(text), marked).unwrap()
    }

    /// The retained identities of `projected`, its tokens assigned together
    /// under `digest`.
    fn retain(projected: &[SourceIdentity], digest: &impl Digest) -> Vec<PortableSourceIdentity> {
        let tokens = EvidenceTokens::assign(
            PROFILE,
            projected.iter().filter_map(SourceIdentity::redacted),
            digest,
        );
        projected
            .iter()
            .map(|identity| identity.retained(PROFILE, &tokens).unwrap())
            .collect()
    }

    /// Every byte is two digits, so a byte below `0x10` keeps its zero.
    #[test]
    fn a_marked_identity_keeps_its_exact_bytes() {
        let projected = project(O::ConfigurationFile, FILE, true);
        assert_eq!(projected, SourceIdentity::SafeOpaque(FILE.into()));
        assert_eq!(projected.redacted(), None);
        let control = project(O::ConfigurationProgrammatic, "a.v0:\u{1}", true);
        let views: Vec<_> = retain(&[projected, control], &Sha256)
            .iter()
            .map(PortableSourceIdentity::view)
            .collect();
        assert_eq!(
            views,
            [
                json!({ "safe-opaque": FILE_HEX }),
                json!({ "safe-opaque": CONTROL_HEX }),
            ]
        );
    }

    /// An unmarked identity keeps only its domain and origin, so the unmarked
    /// identities of one domain and origin share one token.
    #[test]
    fn an_unmarked_identity_is_its_domain_token() {
        let projected = [
            project(O::ConfigurationFile, FILE, false),
            project(O::ConfigurationFile, "intlify-file-source.v0:other", false),
            project(O::ConfigurationProgrammatic, PROGRAMMATIC, false),
            project(
                O::ConfigurationFile,
                "acme-file-source.v2:storefront",
                false,
            ),
        ];
        assert_eq!(
            projected[0].redacted().map(Redacted::projected),
            Some(json!({ "domain": "intlify-file-source.v0" }))
        );
        let views: Vec<_> = retain(&projected, &Sha256)
            .iter()
            .map(PortableSourceIdentity::view)
            .collect();
        assert_eq!(
            views,
            [
                json!({ "redacted": { "token": FILE_TOKEN } }),
                json!({ "redacted": { "token": FILE_TOKEN } }),
                json!({ "redacted": { "token": PROGRAMMATIC_TOKEN } }),
                json!({ "redacted": { "token": OTHER_DOMAIN_TOKEN } }),
            ]
        );
    }

    /// The origin is checked whether or not the identity is marked.
    #[test]
    fn only_a_configuration_origin_supplies_a_source_identity() {
        for marked in [false, true] {
            assert_eq!(
                SourceIdentity::project(O::ProfileSelectorCli, &exact(FILE), marked),
                Err(EvidenceTokenInvariant::OriginOutsideKind(
                    TokenKind::SourceIdentity,
                    O::ProfileSelectorCli
                )),
                "{marked}"
            );
        }
    }

    #[test]
    fn a_redacted_identity_without_its_token_is_not_retained() {
        let projected = project(O::ConfigurationFile, FILE, false);
        let tokens = EvidenceTokens::assign(PROFILE, [], &Sha256);
        assert_eq!(projected.retained(PROFILE, &tokens), None);
    }

    /// `safe-opaque` before `redacted`; exact bytes with a shorter equal
    /// prefix first; digest bytes, then the collision ordinal, absence first.
    #[test]
    fn identities_compare_by_variant_then_bytes_or_token() {
        let safe = |text: &str| retain(&[project(O::ConfigurationFile, text, true)], &Sha256);
        let short = safe("a.v0:b").remove(0);
        let long = safe("a.v0:b/c").remove(0);
        let upper = safe("a.v0:B").remove(0);
        assert!(upper < short && short < long);
        let colliding = retain(
            &[
                project(O::ConfigurationFile, FILE, false),
                project(O::ConfigurationProgrammatic, PROGRAMMATIC, false),
            ],
            &Constant,
        );
        let alone = retain(&[project(O::ConfigurationFile, FILE, false)], &Constant).remove(0);
        assert!(
            long < alone,
            "every safe-opaque identity precedes every token"
        );
        assert!(alone < colliding[0] && colliding[0] < colliding[1]);
        assert_eq!(
            colliding
                .iter()
                .map(PortableSourceIdentity::view)
                .collect::<Vec<_>>(),
            [
                json!({ "redacted": { "token": format!("sha256:{}", "07".repeat(32)), "collision": "0" } }),
                json!({ "redacted": { "token": format!("sha256:{}", "07".repeat(32)), "collision": "1" } }),
            ]
        );
    }

    #[test]
    fn a_span_is_admitted_only_inside_its_snapshot() {
        let snapshot = b"{\"a\":1}";
        let span = |start, end| SourceSpan::admit(start, end, snapshot);
        assert_eq!(
            span(1, 4).map(SourceSpan::view),
            Some(json!({ "start": "1", "end": "4" }))
        );
        assert!(span(0, 0).is_some(), "zero width at the start");
        assert!(span(3, 3).is_some(), "zero width inside");
        assert!(span(7, 7).is_some(), "zero width at the end of input");
        assert!(span(0, 7).is_some(), "the whole snapshot");
        assert_eq!(span(4, 3), None, "inverted");
        assert_eq!(span(6, 8), None, "past the end");
        assert_eq!(span(8, 8), None, "zero width past the end");
        assert_eq!(SourceSpan::admit(0, 0, b""), span(0, 0));
        assert_eq!(SourceSpan::admit(0, 1, b""), None);
    }

    /// Absence, then `start`, then `end`.
    #[test]
    fn spans_compare_by_start_then_end() {
        let snapshot = [0_u8; 8];
        let span = |start, end| SourceSpan::admit(start, end, &snapshot);
        let mut spans = vec![span(2, 5), span(1, 6), None, span(1, 1), span(2, 3)];
        spans.sort();
        assert_eq!(
            spans,
            [None, span(1, 1), span(1, 6), span(2, 3), span(2, 5)]
        );
    }
}
