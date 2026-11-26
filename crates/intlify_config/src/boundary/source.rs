// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's two identity-revision boundaries: `file-source-identity-revision`,
//! which admits a file entry's exact logical source identity and
//! Source-content revision against the raw bytes they name, and
//! `programmatic-entry-identity-revision`, which admits a Programmatic Entry
//! identity and revision against the digest of the admitted Snapshot. Each
//! refuses with one of the seven reasons of its row, and the two rows spell
//! the same checks (D15:744, 3151, 3152; 015-149, 015-170, 015-201).
//!
//! An identity is a versioned identity-domain tag, `:`, and a non-empty
//! payload. The tag is a name, `.v`, and a decimal version without a leading
//! zero; the name holds lowercase ASCII letters, digits, `.`, and `-`, and
//! begins and ends with a letter or digit. The domain's adapter defines the
//! payload, so the boundary only requires one. A revision is derived from the
//! content: a file's is SHA-256 over the domain separator and a one-field
//! record holding the exact bytes, and a Programmatic Entry's is the
//! snapshot-content digest, each presented as `sha256:` and 64 lowercase
//! hexadecimal digits. A path, a timestamp, or an adapter's assertion is never
//! a revision.
//!
//! The checks run in the row's order: an identity, then a revision, is
//! present; each is within its `bootstrapOnly.evidenceMetadata` bound; the
//! identity has the form; the revision has the presentation; and last the
//! revision is the one computed from the content. The first check that fails
//! decides. A Programmatic Entry's revision is compared with the digest of
//! its admitted Snapshot, so the pair is admitted only after the Snapshot is.
//! The `programmatic-call-site-evidence-input` boundary runs the same checks
//! on a Call-Site Source pair, spelled as the file row spells them, against
//! the Call-Site Source revision of the sidecar's source bytes; its row puts
//! the two bounds before the rest of the sidecar and the other checks after
//! it ([`super::call_site`]).

use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::token::hex;

use crate::artifacts::count;
use crate::construction::profile::{CALL_SITE_SOURCE_REVISION_DOMAIN, FILE_SOURCE_REVISION_DOMAIN};
use crate::framing::frame;
use crate::input_limits::Bound;
use crate::references::valid_semantic_digest;

use super::capability::ImplementationCapability;
use super::snapshot::AdmittedSnapshot;

/// The bounds on one pair: the capability's, or a test's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceLimits {
    pub(crate) identity_bytes: Bound,
    pub(crate) revision_bytes: Bound,
}

impl SourceLimits {
    pub(super) const fn of(capability: &ImplementationCapability) -> Self {
        let bounds = &capability.bootstrap_only().evidence_metadata;
        Self {
            identity_bytes: bounds.max_source_identity_bytes,
            revision_bytes: bounds.max_source_revision_bytes,
        }
    }
}

/// Why a boundary refuses a pair: one of the seven checks of its row, in that
/// order.
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
    /// The revision is not the one computed from the content: the bytes'
    /// Source-content revision, or the admitted Snapshot's digest.
    SnapshotMismatch,
}

impl SourceRefusal {
    /// The reason as 015's `file-source-identity-revision` row spells it.
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

    /// The reason as the `programmatic-entry-identity-revision` row spells it.
    pub(crate) const fn entry_reason(self) -> &'static str {
        match self {
            Self::MissingIdentity => "missing-entry-identity",
            Self::MissingRevision => "missing-entry-revision",
            Self::OverBoundIdentity => "over-bound-entry-identity",
            Self::OverBoundRevision => "over-bound-entry-revision",
            Self::UnstableIdentity => "unstable-entry-identity",
            Self::UnboundRevision => "unbound-entry-revision",
            Self::SnapshotMismatch => "snapshot-identity-mismatch",
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

    /// The identity that the entry a replay observes its retained value
    /// through names, which no boundary admitted: a replay has no entry
    /// source, and nothing that names this one leaves it (D15:2660).
    pub(crate) fn replayed() -> Self {
        let tag = "intlify-materialized-replay.v0";
        Self {
            text: format!("{tag}:value"),
            colon: tag.len(),
        }
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

/// The file boundary's checks under `limits`, which [`admit`] takes from the
/// capability.
pub(crate) fn read_pair(
    identity: Option<&str>,
    revision: Option<&str>,
    bytes: &[u8],
    limits: SourceLimits,
) -> Result<SourcePair, SourceRefusal> {
    check_pair(identity, revision, limits, |revision| {
        revision == file_source_revision(bytes)
    })
}

/// Admit a submitted Programmatic Entry pair, either part `None` when absent,
/// against the admitted Snapshot whose digest is the Programmatic Entry
/// revision, under the capability's bounds (015-201).
pub(crate) fn admit_entry(
    identity: Option<&str>,
    revision: Option<&str>,
    snapshot: &AdmittedSnapshot,
    capability: &ImplementationCapability,
) -> Result<SourcePair, SourceRefusal> {
    read_entry_pair(
        identity,
        revision,
        snapshot.digest(),
        SourceLimits::of(capability),
    )
}

/// The Programmatic Entry boundary's checks under `limits`, the revision
/// compared with the Snapshot's `digest`.
pub(crate) fn read_entry_pair(
    identity: Option<&str>,
    revision: Option<&str>,
    digest: &str,
    limits: SourceLimits,
) -> Result<SourcePair, SourceRefusal> {
    check_pair(identity, revision, limits, |revision| revision == digest)
}

/// The Call-Site Source pair's checks under `limits`, the revision compared
/// with the Call-Site Source revision of `source`, the sidecar's exact source
/// bytes (D15:742, 744).
pub(crate) fn read_call_site_pair(
    identity: Option<&str>,
    revision: Option<&str>,
    source: &[u8],
    limits: SourceLimits,
) -> Result<SourcePair, SourceRefusal> {
    check_pair(identity, revision, limits, |revision| {
        revision == call_site_source_revision(source)
    })
}

/// The seven checks in the rows' order; `derived` says whether a revision in
/// the presentation is the one computed from the content.
fn check_pair(
    identity: Option<&str>,
    revision: Option<&str>,
    limits: SourceLimits,
    derived: impl FnOnce(&str) -> bool,
) -> Result<SourcePair, SourceRefusal> {
    use SourceRefusal::{
        MissingIdentity, MissingRevision, SnapshotMismatch, UnboundRevision, UnstableIdentity,
    };

    let identity = identity.ok_or(MissingIdentity)?;
    let revision = revision.ok_or(MissingRevision)?;
    check_bounds(Some(identity), Some(revision), limits)?;
    let (tag, _) = identity_parts(identity).ok_or(UnstableIdentity)?;
    if !valid_semantic_digest(revision) {
        return Err(UnboundRevision);
    }
    if !derived(revision) {
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

/// The two bound checks, each on its part when present: the identity's UTF-8
/// bytes, and then the revision's.
pub(super) fn check_bounds(
    identity: Option<&str>,
    revision: Option<&str>,
    limits: SourceLimits,
) -> Result<(), SourceRefusal> {
    let over =
        |part: Option<&str>, bound: Bound| part.is_some_and(|part| count(part.len()) > bound.get());
    if over(identity, limits.identity_bytes) {
        return Err(SourceRefusal::OverBoundIdentity);
    }
    if over(revision, limits.revision_bytes) {
        return Err(SourceRefusal::OverBoundRevision);
    }
    Ok(())
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
/// whose one field `0x01` holds the exact bytes (D15:744, 988).
pub(crate) fn file_source_revision(bytes: &[u8]) -> String {
    content_revision(FILE_SOURCE_REVISION_DOMAIN, bytes)
}

/// The Call-Site Source revision of a sidecar's exact source bytes, as
/// presented: the same over
/// `intlify.project-profile.call-site-source-revision.v0` (D15:744).
pub(crate) fn call_site_source_revision(bytes: &[u8]) -> String {
    content_revision(CALL_SITE_SOURCE_REVISION_DOMAIN, bytes)
}

/// SHA-256 over `domain` followed by the record whose one field `0x01` holds
/// `bytes`, presented as `sha256:` and 64 lowercase hexadecimal digits.
fn content_revision(domain: &str, bytes: &[u8]) -> String {
    let mut preimage = Vec::with_capacity(domain.len() + 9 + bytes.len());
    preimage.extend_from_slice(domain.as_bytes());
    frame(0x01, bytes, &mut preimage);
    format!("sha256:{}", hex(digest_bytes(&preimage)))
}

#[cfg(test)]
pub(super) mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::capability::{
        IDENTITY as CAPABILITY, REVISION as CAPABILITY_REVISION,
    };
    use crate::locale::VersionedIdentity;
    use crate::resolver::limits::fixture_limits;
    use crate::snapshot::Node;

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

    /// Both source-revision domains, each over a one-field record of the
    /// exact bytes: frozen preimages and digests that a Python script computed
    /// apart from this crate from D15:744 and the field framing
    /// (`.plans/drafts/025-pr7/t2-vectors.py`; D15:3444).
    #[test]
    fn both_revision_domains_digest_their_frozen_preimages() {
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../../fixtures/evidence-records/vectors.json"))
                .unwrap();
        let unhex = |text: &str| -> Vec<u8> {
            (0..text.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
                .collect()
        };
        let rows = vectors["sourceRevisions"].as_array().unwrap();
        assert_eq!(rows.len(), 6);
        for row in rows {
            let domain = row["domain"].as_str().unwrap();
            let bytes = unhex(row["bytes"].as_str().unwrap());
            let preimage = unhex(row["preimage"].as_str().unwrap());
            let revision = row["revision"].as_str().unwrap();
            let length = u64::try_from(bytes.len()).unwrap().to_be_bytes();
            assert_eq!(
                preimage,
                [domain.as_bytes(), &[0x01], &length, &bytes].concat(),
                "{domain}"
            );
            assert_eq!(
                format!("sha256:{}", hex(digest_bytes(&preimage))),
                revision,
                "{domain}"
            );
            let computed = match domain {
                "intlify.project-profile.file-source-revision.v0" => file_source_revision(&bytes),
                "intlify.project-profile.call-site-source-revision.v0" => {
                    call_site_source_revision(&bytes)
                }
                other => panic!("no such domain: {other}"),
            };
            assert_eq!(computed, revision, "{domain}");
        }
    }

    /// Call-Site Source revisions computed the same way outside this crate,
    /// over the call-site domain: the same bytes have a different revision in
    /// each domain.
    #[test]
    fn the_call_site_revision_is_the_same_over_its_own_domain() {
        let source = "defineConfig({ profiles: { app: {} } }) // 設定\n".as_bytes();
        for (bytes, revision) in [
            (
                &b""[..],
                "sha256:1120c36577e1a30cf623314968e78e9dd17ab18ab89d34b37fd631b30d44cd4e",
            ),
            (
                &b"{}"[..],
                "sha256:8774e34a24fda7565ff9f2a25f360a7f77fbbbec7e4c467d6c79d627ec6d4af7",
            ),
            (
                BYTES,
                "sha256:b94dfb77c4a2e23192f4562dd12c627059b57af38c36b9b62acf6026123c0f05",
            ),
            (
                source,
                "sha256:cce9511720b93089d1cc228ba37453adc6325618bb012c46ebad55190bffb3a0",
            ),
        ] {
            assert_eq!(call_site_source_revision(bytes), revision, "{bytes:?}");
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

    /// Each bound holds its part only when the part is present, the identity
    /// first.
    #[test]
    fn the_bounds_hold_only_the_parts_present() {
        let one = Bound::new(1).unwrap();
        let tight = SourceLimits {
            identity_bytes: one,
            revision_bytes: one,
        };
        assert_eq!(check_bounds(None, None, tight), Ok(()));
        assert_eq!(check_bounds(Some("x"), Some("y"), tight), Ok(()));
        assert_eq!(
            check_bounds(Some("xy"), None, tight),
            Err(SourceRefusal::OverBoundIdentity)
        );
        assert_eq!(
            check_bounds(None, Some("xy"), tight),
            Err(SourceRefusal::OverBoundRevision)
        );
        assert_eq!(
            check_bounds(Some("xy"), Some("xy"), tight),
            Err(SourceRefusal::OverBoundIdentity)
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

    const ENTRY: &str = "intlify-programmatic-entry.v0:storefront";

    /// Identities outside the form: no `:`, an empty payload, a tag without
    /// `.v` and a version, an empty name, a name that begins or ends with `.`
    /// or `-` or holds an uppercase letter or `_`, and a version with a
    /// leading zero (D15:744, 3514).
    pub(in crate::boundary) const UNSTABLE: [&str; 21] = [
        "", "a.v0", "a.v0:", ":", ":p", "a:p", "a.v:p", "a.vx:p", "a.V0:p", ".v0:p", ".a.v0:p",
        "a..v0:p", "-a.v0:p", "a-.v0:p", "A.v0:p", "aB.v0:p", "a_b.v0:p", "a b.v0:p", "a.v01:p",
        "a.v00:p", "a.v0x:p",
    ];

    /// The contrasts: a name with `.` and `-` inside, a longer or zero
    /// version, and payloads that hold a further `:` (D15:744, 3514).
    pub(in crate::boundary) const STABLE: [(&str, &str, &str); 7] = [
        ("x.y-z.v3:p", "x.y-z.v3", "p"),
        ("a.v12:p", "a.v12", "p"),
        ("0.v0:p", "0.v0", "p"),
        ("a.v.v0:p", "a.v.v0", "p"),
        ("a.v0:x:y", "a.v0", "x:y"),
        ("a.v0::", "a.v0", ":"),
        ("a.v0: ", "a.v0", " "),
    ];

    /// What a host knows of a source, none of which alone establishes a pair:
    /// a normalized path, a URI, a modification timestamp, filesystem
    /// metadata, a process-local object address, a stack frame, and a value
    /// random per invocation (D15:742, 3513).
    pub(in crate::boundary) const HOST_FACTS: [&str; 14] = [
        "config/intlify.json",
        "/repository/intlify.config.json",
        "C:/repository/intlify.config.json",
        "https://example.com/intlify.config.json",
        "file:///repository/intlify.config.json",
        "2026-11-10T00:00:00Z",
        "1762732800",
        "dev:16777220,ino:12345",
        "mode:0644",
        "0x7ffee4b2a9c0",
        "heap:0x7ffee4b2a9c0",
        "at defineConfig (app.ts:12:3)",
        "app.ts:12:3",
        "f47ac10b-58cc-4372-a567-0e02b2c3d479",
    ];

    /// `identity` and `revision` read as each of the three pairs, a `None`
    /// revision standing for the one each pair's content derives.
    fn each_pair(
        identity: &str,
        revision: Option<&str>,
    ) -> [(&'static str, Result<SourcePair, SourceRefusal>); 3] {
        let digest = Node::Null.digest();
        let source = call_site_source_revision(BYTES);
        [
            (
                "file",
                read_pair(
                    Some(identity),
                    Some(revision.unwrap_or(REVISION)),
                    BYTES,
                    roomy(),
                ),
            ),
            (
                "entry",
                read_entry_pair(
                    Some(identity),
                    Some(revision.unwrap_or(&digest)),
                    &digest,
                    roomy(),
                ),
            ),
            (
                "call-site",
                read_call_site_pair(
                    Some(identity),
                    Some(revision.unwrap_or(&source)),
                    BYTES,
                    roomy(),
                ),
            ),
        ]
    }

    /// Every pair applies one form: each spelling outside it is the row's
    /// unstable identity, and each contrast is admitted with the first `:`
    /// as the end of its tag (D15:744, 3514).
    #[test]
    fn every_pair_applies_the_identity_form() {
        for identity in UNSTABLE {
            for (pair, read) in each_pair(identity, None) {
                assert_eq!(
                    read,
                    Err(SourceRefusal::UnstableIdentity),
                    "{pair} {identity:?}"
                );
            }
        }
        assert_eq!(
            SourceRefusal::UnstableIdentity.reason(),
            "unstable-source-identity"
        );
        assert_eq!(
            SourceRefusal::UnstableIdentity.entry_reason(),
            "unstable-entry-identity"
        );
        for (identity, tag, payload) in STABLE {
            for (pair, read) in each_pair(identity, None) {
                let admitted = read.unwrap_or_else(|_| panic!("{pair} {identity:?}"));
                assert_eq!(admitted.identity().as_str(), identity, "{pair}");
                assert_eq!(admitted.identity().tag(), tag, "{pair}");
                assert_eq!(admitted.identity().payload(), payload, "{pair}");
            }
        }
    }

    /// None of a host's facts establishes any pair, as its identity or as
    /// its revision, while the content-derived revision of each pair still
    /// establishes it under a stable identity (D15:742, 744, 3513).
    #[test]
    fn no_host_fact_alone_establishes_a_pair() {
        for fact in HOST_FACTS {
            for (pair, read) in each_pair(fact, None) {
                assert_eq!(read, Err(SourceRefusal::UnstableIdentity), "{pair} {fact}");
            }
            for (pair, read) in each_pair(IDENTITY, Some(fact)) {
                assert_eq!(read, Err(SourceRefusal::UnboundRevision), "{pair} {fact}");
            }
        }
        for (pair, read) in each_pair(IDENTITY, None) {
            assert!(read.is_ok(), "{pair}");
        }
    }

    /// A Programmatic Entry pair runs the same checks in the same order, its
    /// revision compared with the Snapshot's digest (D15:744, 3152).
    #[test]
    fn the_entry_pair_runs_the_same_checks_against_the_digest() {
        use SourceRefusal as R;

        let digest = Node::Null.digest();
        let bound = |value: usize| Bound::new(count(value)).unwrap();
        let exact = SourceLimits {
            identity_bytes: bound(ENTRY.len()),
            revision_bytes: bound(digest.len()),
        };
        let read = |identity: Option<&str>, revision: Option<&str>, limits: SourceLimits| {
            read_entry_pair(identity, revision, &digest, limits)
        };
        let pair = read(Some(ENTRY), Some(&digest), exact).unwrap();
        assert_eq!(pair.identity().tag(), "intlify-programmatic-entry.v0");
        assert_eq!(pair.revision(), digest);
        let short_identity = SourceLimits {
            identity_bytes: bound(ENTRY.len() - 1),
            ..exact
        };
        let short_revision = SourceLimits {
            revision_bytes: bound(digest.len() - 1),
            ..exact
        };
        let upper = digest.to_ascii_uppercase();
        let bytes = file_source_revision(b"null");
        for (identity, revision, limits, refusal) in [
            (None, None, exact, R::MissingIdentity),
            (Some(ENTRY), None, exact, R::MissingRevision),
            (
                Some(ENTRY),
                Some(&*digest),
                short_identity,
                R::OverBoundIdentity,
            ),
            (
                Some(ENTRY),
                Some(&*digest),
                short_revision,
                R::OverBoundRevision,
            ),
            (Some("entry"), Some(&*digest), exact, R::UnstableIdentity),
            (Some(ENTRY), Some(&*upper), exact, R::UnboundRevision),
            (Some(ENTRY), Some(&*bytes), exact, R::SnapshotMismatch),
        ] {
            assert_eq!(
                read(identity, revision, limits),
                Err(refusal),
                "{refusal:?}"
            );
        }
    }

    /// `admit_entry` reads the capability's bounds and compares the revision
    /// with the digest of the Snapshot it is given.
    #[test]
    fn the_capability_bounds_the_entry_pair() {
        let snapshot = AdmittedSnapshot::of(Node::Null);
        let digest = snapshot.digest().to_owned();
        let (identity, revision) = (ENTRY.len(), digest.len());
        let under = |snapshot: &AdmittedSnapshot, identity, revision| {
            admit_entry(
                Some(ENTRY),
                Some(&digest),
                snapshot,
                &capability(identity, revision),
            )
        };
        assert!(under(&snapshot, identity, revision).is_ok());
        assert_eq!(
            under(&snapshot, identity - 1, revision),
            Err(SourceRefusal::OverBoundIdentity)
        );
        assert_eq!(
            under(&snapshot, identity, revision - 1),
            Err(SourceRefusal::OverBoundRevision)
        );
        let other = AdmittedSnapshot::of(Node::Boolean(true));
        assert_eq!(
            under(&other, identity, revision),
            Err(SourceRefusal::SnapshotMismatch)
        );
    }

    #[test]
    fn each_refusal_has_its_entry_reason() {
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
            .map(R::entry_reason),
            [
                "missing-entry-identity",
                "missing-entry-revision",
                "over-bound-entry-identity",
                "over-bound-entry-revision",
                "unstable-entry-identity",
                "unbound-entry-revision",
                "snapshot-identity-mismatch",
            ]
        );
    }
}
