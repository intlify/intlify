// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `file-source-identity-revision` boundary: a file entry's exact
//! logical source identity and Source-content revision, admitted against the
//! raw bytes they name or refused with one of the seven reasons of its row
//! (D15:743, 3146; 015-149, 015-170).
//!
//! An identity is a versioned identity-domain tag, `:`, and a non-empty
//! payload. The tag is a name, `.v`, and a decimal version without a leading
//! zero; the name holds lowercase ASCII letters, digits, `.`, and `-`, and
//! begins and ends with a letter or digit. The domain's adapter defines the
//! payload, so the boundary only requires one. A revision is derived from the
//! content: SHA-256 over the domain separator and a one-field record holding
//! the exact bytes, presented as `sha256:` and 64 lowercase hexadecimal
//! digits. A path, a timestamp, or an adapter's assertion is never a revision.
//!
//! The checks run in the row's order: an identity, then a revision, is
//! present; each is within its `bootstrapOnly.evidenceMetadata` bound; the
//! identity has the form; the revision has the presentation; and last the
//! revision is the one computed over the bytes. The first check that fails
//! decides. Programmatic Entry and Call-Site Source identities have the same
//! form, which [`identity_parts`] checks for them too.

use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::token::hex;

use crate::artifacts::count;
use crate::construction::profile::FILE_SOURCE_REVISION_DOMAIN;
use crate::framing::frame;
use crate::input_limits::Bound;
use crate::references::valid_semantic_digest;

use super::capability::ImplementationCapability;

/// The bounds on one pair: the capability's, or a test's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceLimits {
    pub(crate) identity_bytes: Bound,
    pub(crate) revision_bytes: Bound,
}

impl SourceLimits {
    const fn of(capability: &ImplementationCapability) -> Self {
        let bounds = &capability.bootstrap_only().evidence_metadata;
        Self {
            identity_bytes: bounds.max_source_identity_bytes,
            revision_bytes: bounds.max_source_revision_bytes,
        }
    }
}

/// Why the boundary refuses a pair: one of the seven reasons of its row, in
/// that order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceRefusal {
    MissingIdentity,
    MissingRevision,
    /// The identity's UTF-8 bytes are over `maxSourceIdentityBytes`.
    OverBoundIdentity,
    /// The revision's bytes are over `maxSourceRevisionBytes`.
    OverBoundRevision,
    /// The identity has no `:`, an invalid tag, or an empty payload.
    UnstableIdentity,
    /// The revision is not `sha256:` and 64 lowercase hexadecimal digits.
    UnboundRevision,
    /// The revision is not the one computed over the supplied bytes.
    SnapshotMismatch,
}

impl SourceRefusal {
    /// The reason as 015's boundary table spells it.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::MissingIdentity => "missing-source-identity",
            Self::MissingRevision => "missing-source-revision",
            Self::OverBoundIdentity => "over-bound-source-identity",
            Self::OverBoundRevision => "over-bound-source-revision",
            Self::UnstableIdentity => "unstable-source-identity",
            Self::UnboundRevision => "unbound-source-revision",
            Self::SnapshotMismatch => "source-snapshot-mismatch",
        }
    }
}

/// An admitted exact identity, written as its tag, `:`, and its payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExactIdentity {
    text: String,
    /// The byte offset of the first `:`, which ends the tag.
    colon: usize,
}

impl ExactIdentity {
    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }

    pub(crate) fn tag(&self) -> &str {
        &self.text[..self.colon]
    }

    pub(crate) fn payload(&self) -> &str {
        &self.text[self.colon + 1..]
    }
}

/// An admitted pair: the identity, and the revision of the bytes it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourcePair {
    identity: ExactIdentity,
    revision: String,
}

impl SourcePair {
    pub(crate) const fn identity(&self) -> &ExactIdentity {
        &self.identity
    }

    /// The revision as presented: `sha256:` and 64 lowercase hexadecimal
    /// digits.
    pub(crate) fn revision(&self) -> &str {
        &self.revision
    }
}

/// Admit a submitted pair, either part `None` when absent, against the raw
/// bytes it names, under the capability's bounds.
pub(crate) fn admit(
    identity: Option<&str>,
    revision: Option<&str>,
    bytes: &[u8],
    capability: &ImplementationCapability,
) -> Result<SourcePair, SourceRefusal> {
    read_pair(identity, revision, bytes, SourceLimits::of(capability))
}

/// The boundary's checks under `limits`, which [`admit`] takes from the
/// capability.
pub(crate) fn read_pair(
    identity: Option<&str>,
    revision: Option<&str>,
    bytes: &[u8],
    limits: SourceLimits,
) -> Result<SourcePair, SourceRefusal> {
    use SourceRefusal::{
        MissingIdentity, MissingRevision, OverBoundIdentity, OverBoundRevision, SnapshotMismatch,
        UnboundRevision, UnstableIdentity,
    };

    let identity = identity.ok_or(MissingIdentity)?;
    let revision = revision.ok_or(MissingRevision)?;
    if count(identity.len()) > limits.identity_bytes.get() {
        return Err(OverBoundIdentity);
    }
    if count(revision.len()) > limits.revision_bytes.get() {
        return Err(OverBoundRevision);
    }
    let (tag, _) = identity_parts(identity).ok_or(UnstableIdentity)?;
    if !valid_semantic_digest(revision) {
        return Err(UnboundRevision);
    }
    if revision != file_source_revision(bytes) {
        return Err(SnapshotMismatch);
    }
    Ok(SourcePair {
        identity: ExactIdentity {
            text: identity.to_owned(),
            colon: tag.len(),
        },
        revision: revision.to_owned(),
    })
}

/// The tag and the payload of an identity in 015's form, or `None` when it
/// has no `:`, an invalid tag, or an empty payload. The tag has no `:`, so the
/// first one separates the two, and the payload may hold more.
pub(crate) fn identity_parts(identity: &str) -> Option<(&str, &str)> {
    let (tag, payload) = identity.split_once(':')?;
    (valid_tag(tag) && !payload.is_empty()).then_some((tag, payload))
}

/// A name, `.v`, and a version. A version holds no `.v`, so only the last one
/// can end the name.
fn valid_tag(tag: &str) -> bool {
    tag.rsplit_once(".v")
        .is_some_and(|(name, version)| valid_name(name) && valid_version(version))
}

/// Lowercase ASCII letters, digits, `.`, and `-`, beginning and ending with a
/// letter or digit (015-170).
fn valid_name(name: &str) -> bool {
    let edge = |byte: &u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    let bytes = name.as_bytes();
    bytes.first().is_some_and(edge)
        && bytes.last().is_some_and(edge)
        && bytes
            .iter()
            .all(|byte| edge(byte) || matches!(byte, b'.' | b'-'))
}

/// `0`, or decimal digits that do not begin with `0`.
fn valid_version(version: &str) -> bool {
    version == "0"
        || (version.starts_with(|digit: char| ('1'..='9').contains(&digit))
            && version.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The Source-content revision of `bytes`, as presented: SHA-256 over
/// `intlify.project-profile.file-source-revision.v0` followed by the record
/// whose one field `0x01` holds the exact bytes (D15:743, 987).
pub(crate) fn file_source_revision(bytes: &[u8]) -> String {
    let mut preimage = Vec::with_capacity(FILE_SOURCE_REVISION_DOMAIN.len() + 9 + bytes.len());
    preimage.extend_from_slice(FILE_SOURCE_REVISION_DOMAIN.as_bytes());
    frame(0x01, bytes, &mut preimage);
    format!("sha256:{}", hex(digest_bytes(&preimage)))
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

    const BYTES: &[u8] = b"{\"schemaVersion\":\"0\"}\n";
    /// The revision of `BYTES`, computed outside this crate with Python's
    /// `hashlib` over the domain, `0x01`, the eight-byte big-endian length,
    /// and the bytes.
    const REVISION: &str =
        "sha256:66bb712931d10dbd02539ce2c61126543e7623228c938e48a746bad813223ac4";
    const IDENTITY: &str = "intlify-file-source.v0:storefront/intlify.config.json";

    fn largest() -> Bound {
        Bound::new(u64::MAX - 1).unwrap()
    }

    fn roomy() -> SourceLimits {
        SourceLimits {
            identity_bytes: largest(),
            revision_bytes: largest(),
        }
    }

    fn read(identity: Option<&str>, revision: Option<&str>) -> Result<SourcePair, SourceRefusal> {
        read_pair(identity, revision, BYTES, roomy())
    }

    #[test]
    fn a_pair_the_bytes_derive_is_admitted() {
        let pair = read(Some(IDENTITY), Some(REVISION)).unwrap();
        assert_eq!(pair.identity().as_str(), IDENTITY);
        assert_eq!(pair.identity().tag(), "intlify-file-source.v0");
        assert_eq!(pair.identity().payload(), "storefront/intlify.config.json");
        assert_eq!(pair.revision(), REVISION);
    }

    /// Revisions computed outside this crate, so the framing is checked
    /// against an independent implementation and not against itself.
    #[test]
    fn the_revision_is_sha256_over_the_domain_and_one_framed_field() {
        for (bytes, revision) in [
            (
                &b""[..],
                "sha256:9b5ccfeead0f20b9f9f2fccd02711b5842b5da6223acc36078e1731ab33d18fc",
            ),
            (
                &b"{}"[..],
                "sha256:cd716b08f2eff0aa7a1ed8df694c6c6ddd8a5f66275e78761d8f62e9335b727d",
            ),
            (BYTES, REVISION),
        ] {
            assert_eq!(file_source_revision(bytes), revision, "{bytes:?}");
        }
    }

    #[test]
    fn an_absent_identity_then_an_absent_revision_is_missing() {
        assert_eq!(
            read(None, Some(REVISION)),
            Err(SourceRefusal::MissingIdentity)
        );
        assert_eq!(read(None, None), Err(SourceRefusal::MissingIdentity));
        assert_eq!(
            read(Some(IDENTITY), None),
            Err(SourceRefusal::MissingRevision)
        );
        assert_eq!(read(Some("x"), None), Err(SourceRefusal::MissingRevision));
    }

    /// Each bound admits its exact value and refuses one more byte, before the
    /// form, the presentation, or the content is looked at.
    #[test]
    fn each_bound_admits_its_exact_value_and_refuses_the_next() {
        let bound = |value: usize| Bound::new(count(value)).unwrap();
        let under = |identity_bytes, revision_bytes| SourceLimits {
            identity_bytes,
            revision_bytes,
        };
        let exact = under(bound(IDENTITY.len()), bound(REVISION.len()));
        assert!(read_pair(Some(IDENTITY), Some(REVISION), BYTES, exact).is_ok());
        let short = under(bound(IDENTITY.len() - 1), bound(REVISION.len()));
        assert_eq!(
            read_pair(Some(IDENTITY), Some(REVISION), BYTES, short),
            Err(SourceRefusal::OverBoundIdentity)
        );
        let short = under(bound(IDENTITY.len()), bound(REVISION.len() - 1));
        assert_eq!(
            read_pair(Some(IDENTITY), Some(REVISION), BYTES, short),
            Err(SourceRefusal::OverBoundRevision)
        );
        // The identity's UTF-8 bytes are counted, not its characters.
        let wide = "intlify-file-source.v0:設定.json";
        let exact = under(bound(wide.len()), bound(REVISION.len()));
        assert_eq!(
            read_pair(Some(wide), Some(REVISION), BYTES, exact)
                .map(|pair| pair.identity().payload().to_owned()),
            Ok("設定.json".to_owned())
        );
        let short = under(bound(wide.len() - 1), bound(REVISION.len()));
        assert_eq!(
            read_pair(Some(wide), Some(REVISION), BYTES, short),
            Err(SourceRefusal::OverBoundIdentity)
        );
    }

    #[test]
    fn an_identity_outside_the_form_is_unstable() {
        for identity in [
            "intlify-file-source.v0:x",
            "a.v12:p",
            "x.y-z.v3:p",
            "a.v.v0:p",
            "0.v0:p",
            "a.v0:x:y",
            "a.v0: ",
        ] {
            assert!(read(Some(identity), Some(REVISION)).is_ok(), "{identity}");
        }
        for identity in [
            "a.v0", ":p", "a.v0:", "a:p", "a.v:p", "a.vx:p", ".v0:p", "-a.v0:p", "a-.v0:p",
            ".a.v0:p", "a..v0:p", "A.v0:p", "a_b.v0:p", "a b.v0:p", "a.v01:p", "a.v0x:p", "a.V0:p",
        ] {
            assert_eq!(
                read(Some(identity), Some(REVISION)),
                Err(SourceRefusal::UnstableIdentity),
                "{identity}"
            );
        }
    }

    #[test]
    fn a_revision_outside_the_presentation_is_unbound() {
        let digits = &REVISION["sha256:".len()..];
        for revision in [
            format!("SHA256:{digits}"),
            format!("sha256:{}", digits.to_ascii_uppercase()),
            format!("sha256:{}", &digits[1..]),
            format!("sha256:{digits}0"),
            format!("sha-256:{digits}"),
            digits.to_owned(),
            "2026-11-10T00:00:00Z".to_owned(),
        ] {
            assert_eq!(
                read(Some(IDENTITY), Some(&revision)),
                Err(SourceRefusal::UnboundRevision),
                "{revision}"
            );
        }
    }

    #[test]
    fn a_revision_of_other_bytes_is_a_snapshot_mismatch() {
        let other = file_source_revision(b"{}");
        assert_eq!(
            read(Some(IDENTITY), Some(&other)),
            Err(SourceRefusal::SnapshotMismatch)
        );
        // The same bytes under another identity are still the same revision.
        let renamed = "intlify-file-source.v1:other/intlify.config.json";
        assert!(read(Some(renamed), Some(REVISION)).is_ok());
    }

    #[test]
    fn the_first_failing_check_decides() {
        let one = Bound::new(1).unwrap();
        let tight = SourceLimits {
            identity_bytes: one,
            revision_bytes: one,
        };
        assert_eq!(
            read_pair(None, Some("x"), BYTES, tight),
            Err(SourceRefusal::MissingIdentity)
        );
        assert_eq!(
            read_pair(Some("bad"), Some("bad"), BYTES, tight),
            Err(SourceRefusal::OverBoundIdentity)
        );
        assert_eq!(
            read(Some("bad"), Some("bad")),
            Err(SourceRefusal::UnstableIdentity)
        );
        let other = file_source_revision(b"{}");
        assert_eq!(
            read(Some("bad"), Some(&other)),
            Err(SourceRefusal::UnstableIdentity)
        );
        assert_eq!(
            read(Some(IDENTITY), Some("bad")),
            Err(SourceRefusal::UnboundRevision)
        );
    }

    /// The capability whose `evidenceMetadata` sets these two bounds, and every
    /// other bound as a fixture owner states it.
    fn capability(identity: usize, revision: usize) -> ImplementationCapability {
        let mut value = ImplementationCapability::fixture_owned(fixture_limits(1)).normalized();
        let bounds = &mut value["bootstrapOnly"]["evidenceMetadata"];
        bounds["maxSourceIdentityBytes"] = json!(identity.to_string());
        bounds["maxSourceRevisionBytes"] = json!(revision.to_string());
        let constructed = VersionedIdentity {
            identity: CAPABILITY,
            revision: CAPABILITY_REVISION,
        };
        crate::boundary::capability::admit(Some(&value), &constructed).unwrap()
    }

    #[test]
    fn the_capability_bounds_the_pair() {
        let (identity, revision) = (IDENTITY.len(), REVISION.len());
        let under = |identity, revision| {
            admit(
                Some(IDENTITY),
                Some(REVISION),
                BYTES,
                &capability(identity, revision),
            )
        };
        assert!(under(identity, revision).is_ok());
        assert_eq!(
            under(identity - 1, revision),
            Err(SourceRefusal::OverBoundIdentity)
        );
        assert_eq!(
            under(identity, revision - 1),
            Err(SourceRefusal::OverBoundRevision)
        );
    }

    #[test]
    fn each_refusal_has_its_reason() {
        use SourceRefusal as R;
        assert_eq!(
            [
                R::MissingIdentity,
                R::MissingRevision,
                R::OverBoundIdentity,
                R::OverBoundRevision,
                R::UnstableIdentity,
                R::UnboundRevision,
                R::SnapshotMismatch,
            ]
            .map(R::reason),
            [
                "missing-source-identity",
                "missing-source-revision",
                "over-bound-source-identity",
                "over-bound-source-revision",
                "unstable-source-identity",
                "unbound-source-revision",
                "source-snapshot-mismatch",
            ]
        );
    }
}
