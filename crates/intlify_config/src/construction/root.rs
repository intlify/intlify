// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The built-in Resolver Construction Root Package.
//!
//! Before reading any construction authority, the implementation fixes one
//! Root Package: five members in fixed roles, which close the construction
//! bounds, the admitted identities, the envelope schemas, the JSON Schema
//! dialect, the failure precedence, and the cross-authority rules. It is not a
//! sixth component, nothing can select or override it, and it does not
//! validate itself.
//!
//! What the implementation does check, on first use, is that the package it was
//! built with is the one it pins: the artifact's file digest, each member's
//! file digest as the artifact lists it, and the package's canonical digest. A
//! failure there is an implementation failure, never a construction result.
//!
//! The canonical digest is SHA-256 over the domain separator followed by one
//! framed record: the package identity, its revision, and the members in role
//! order, each a record of its role and its member digest. Paths enter neither
//! digest's preimage beyond the artifact's own bytes.

use std::sync::OnceLock;

use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::token::IntegrityDigest;
use serde_json::Value;

use crate::framing::{collection, Record};

/// The Root Package's identity.
pub(crate) const IDENTITY: &str = "intlify-resolver-construction-root-package";

/// The Root Package's revision.
pub(crate) const REVISION: &str = "0";

/// The domain separator of the canonical digest's preimage.
const CANONICAL_DOMAIN: &[u8] = b"intlify.resolver-construction-root-package.v0";

/// The artifact's path below the conformance suite's root.
pub(crate) const PATH: &str = "construction/root-package-v0.json";

/// The artifact's file digest, which the implementation compiles in.
pub(crate) const DIGEST: &str =
    "sha256:5d0ce7e61ff704357d8622b2ecd772710bcd36ad0eed23cf5ac18a7027bc8377";

/// The package's canonical digest, which the implementation compiles in.
pub(crate) const CANONICAL_DIGEST: &str =
    "sha256:43691bdd64c5ee490d9532fddd6d637da86e4dcf3a668a3439188e630aad1cef";

const BYTES: &[u8] = include_bytes!("../../conformance/construction/root-package-v0.json");

/// One fixed role of the Root Package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootRole {
    ConstructionAdmissionSpecification,
    ConstructionAdmissionSchema,
    AuthorityEnvelopeSchemaPackage,
    JsonSchemaMetaSchemaPackage,
    CrossAuthorityRules,
}

impl RootRole {
    /// Every role, in the fixed order.
    pub(crate) const ALL: [Self; 5] = [
        Self::ConstructionAdmissionSpecification,
        Self::ConstructionAdmissionSchema,
        Self::AuthorityEnvelopeSchemaPackage,
        Self::JsonSchemaMetaSchemaPackage,
        Self::CrossAuthorityRules,
    ];

    /// The role's spelling.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::ConstructionAdmissionSpecification => "construction-admission-specification",
            Self::ConstructionAdmissionSchema => "construction-admission-schema",
            Self::AuthorityEnvelopeSchemaPackage => "authority-envelope-schema-package",
            Self::JsonSchemaMetaSchemaPackage => "json-schema-meta-schema-package",
            Self::CrossAuthorityRules => "cross-authority-rules",
        }
    }

    /// The member's path below the conformance suite's root.
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Self::ConstructionAdmissionSpecification => {
                "construction/construction-admission-v0.json"
            }
            Self::ConstructionAdmissionSchema => {
                "construction/construction-admission-v0.schema.json"
            }
            Self::AuthorityEnvelopeSchemaPackage => {
                "construction/authority-envelope-schemas-v0.json"
            }
            Self::JsonSchemaMetaSchemaPackage => "construction/json-schema-meta-schemas-v0.json",
            Self::CrossAuthorityRules => "construction/cross-authority-rules-v0.json",
        }
    }

    /// The member's bytes, as compiled in.
    const fn built_in(self) -> &'static [u8] {
        match self {
            Self::ConstructionAdmissionSpecification => {
                include_bytes!("../../conformance/construction/construction-admission-v0.json")
            }
            Self::ConstructionAdmissionSchema => include_bytes!(
                "../../conformance/construction/construction-admission-v0.schema.json"
            ),
            Self::AuthorityEnvelopeSchemaPackage => {
                include_bytes!("../../conformance/construction/authority-envelope-schemas-v0.json")
            }
            Self::JsonSchemaMetaSchemaPackage => {
                include_bytes!("../../conformance/construction/json-schema-meta-schemas-v0.json")
            }
            Self::CrossAuthorityRules => {
                include_bytes!("../../conformance/construction/cross-authority-rules-v0.json")
            }
        }
    }
}

/// Why the built-in package is not the one the implementation pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootPackageFailure {
    /// The artifact's bytes do not have the pinned file digest.
    DigestMismatch,
    /// The artifact is not a closed `{ members }` object of `{ digest }`
    /// records.
    Malformed,
    /// The members are not exactly the five roles in their fixed order.
    RoleDisagreement,
    /// A member's bytes do not have the file digest the artifact lists.
    MemberDigestMismatch(RootRole),
    /// The recomputed canonical digest is not the pinned one.
    CanonicalDigestMismatch,
}

/// A verified Root Package and its members' bodies.
#[derive(Debug)]
pub(crate) struct RootPackage<'a> {
    canonical_digest: [u8; 32],
    members: [(RootRole, &'a [u8]); 5],
}

impl<'a> RootPackage<'a> {
    /// Borrow the built-in package, verified once on its first use.
    pub(crate) fn built_in() -> Result<&'static RootPackage<'static>, RootPackageFailure> {
        static BUILT_IN: OnceLock<Result<RootPackage<'static>, RootPackageFailure>> =
            OnceLock::new();
        BUILT_IN
            .get_or_init(|| {
                verify(
                    BYTES,
                    RootRole::ALL.map(|role| (role, role.built_in())),
                    DIGEST,
                    CANONICAL_DIGEST,
                )
            })
            .as_ref()
            .map_err(|failure| *failure)
    }

    /// The package's canonical digest.
    pub(crate) const fn canonical_digest(&self) -> [u8; 32] {
        self.canonical_digest
    }

    /// One member's verified body.
    pub(crate) fn member(&self, role: RootRole) -> &'a [u8] {
        self.members
            .iter()
            .find(|(member, _)| *member == role)
            .map(|(_, bytes)| *bytes)
            .expect("a verified package holds every role")
    }
}

/// Read a `sha256:` presentation back into its 32 bytes.
fn digest_bytes_of(text: &str) -> Option<[u8; 32]> {
    let hex = text.strip_prefix("sha256:")?;
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let pair = hex.get(index * 2..index * 2 + 2)?;
        if !pair
            .bytes()
            .all(|digit| digit.is_ascii_digit() || (b'a'..=b'f').contains(&digit))
        {
            return None;
        }
        *byte = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(bytes)
}

/// The canonical digest of a package with these members, in this order.
pub(crate) fn canonical_digest(members: &[(RootRole, [u8; 32])]) -> [u8; 32] {
    let items = members
        .iter()
        .map(|(role, digest)| {
            Record::new()
                .field(role.name().as_bytes())
                .field(digest)
                .finish()
        })
        .collect::<Vec<_>>();
    let record = Record::new()
        .field(IDENTITY.as_bytes())
        .field(REVISION.as_bytes())
        .field(&collection(items.iter().map(Vec::as_slice)))
        .finish();
    let mut preimage = CANONICAL_DOMAIN.to_vec();
    preimage.extend_from_slice(&record);
    digest_bytes(&preimage)
}

/// The member digests a Root Package artifact lists, in role order.
fn listed(bytes: &[u8]) -> Result<[(RootRole, [u8; 32]); 5], RootPackageFailure> {
    let value = intlify_shared_json::decode::value(bytes, true)
        .map_err(|_| RootPackageFailure::Malformed)?;
    let Value::Object(root) = value else {
        return Err(RootPackageFailure::Malformed);
    };
    let Some(Value::Object(members)) = root.get("members") else {
        return Err(RootPackageFailure::Malformed);
    };
    if root.len() != 1 {
        return Err(RootPackageFailure::Malformed);
    }
    if !members
        .keys()
        .map(String::as_str)
        .eq(RootRole::ALL.map(RootRole::name))
    {
        return Err(RootPackageFailure::RoleDisagreement);
    }
    let mut listed = RootRole::ALL.map(|role| (role, [0; 32]));
    for ((_, digest), entry) in listed.iter_mut().zip(members.values()) {
        let Value::Object(entry) = entry else {
            return Err(RootPackageFailure::Malformed);
        };
        if entry.len() != 1 {
            return Err(RootPackageFailure::Malformed);
        }
        *digest = entry
            .get("digest")
            .and_then(Value::as_str)
            .and_then(digest_bytes_of)
            .ok_or(RootPackageFailure::Malformed)?;
    }
    Ok(listed)
}

/// Verify a Root Package against the digests the implementation pins.
///
/// The checks run in this order: the artifact's file digest, its shape, its
/// roles, each member's file digest in role order, and the canonical digest.
pub(crate) fn verify<'a>(
    bytes: &[u8],
    members: [(RootRole, &'a [u8]); 5],
    digest: &str,
    canonical: &str,
) -> Result<RootPackage<'a>, RootPackageFailure> {
    if IntegrityDigest::from_hash(digest_bytes(bytes)).as_str() != digest {
        return Err(RootPackageFailure::DigestMismatch);
    }
    let listed = listed(bytes)?;
    for ((role, listed), (member, body)) in listed.iter().zip(&members) {
        if role != member {
            return Err(RootPackageFailure::RoleDisagreement);
        }
        if digest_bytes(body) != *listed {
            return Err(RootPackageFailure::MemberDigestMismatch(*role));
        }
    }
    let computed = canonical_digest(&listed);
    if IntegrityDigest::from_hash(computed).as_str() != canonical {
        return Err(RootPackageFailure::CanonicalDigestMismatch);
    }
    Ok(RootPackage {
        canonical_digest: computed,
        members,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hash(bytes: &[u8]) -> String {
        IntegrityDigest::from_hash(digest_bytes(bytes))
            .as_str()
            .to_owned()
    }

    fn built_in_members() -> [(RootRole, &'static [u8]); 5] {
        RootRole::ALL.map(|role| (role, role.built_in()))
    }

    /// Verify an artifact against its own file digest, so only the later checks
    /// can fail.
    fn verify_self_pinned(bytes: &[u8]) -> Result<RootPackage<'static>, RootPackageFailure> {
        verify(bytes, built_in_members(), &hash(bytes), CANONICAL_DIGEST)
    }

    fn committed() -> serde_json::Value {
        serde_json::from_slice(BYTES).unwrap()
    }

    #[test]
    fn the_built_in_package_is_the_one_the_implementation_pins() {
        let package = RootPackage::built_in().unwrap();
        assert_eq!(
            IntegrityDigest::from_hash(package.canonical_digest()).as_str(),
            CANONICAL_DIGEST
        );
        assert_eq!(hash(BYTES), DIGEST);
        for role in RootRole::ALL {
            assert_eq!(package.member(role), role.built_in(), "{role:?}");
        }
    }

    #[test]
    fn the_roles_are_015s_five_in_their_fixed_order() {
        assert_eq!(
            RootRole::ALL.map(RootRole::name),
            [
                "construction-admission-specification",
                "construction-admission-schema",
                "authority-envelope-schema-package",
                "json-schema-meta-schema-package",
                "cross-authority-rules",
            ]
        );
        assert_eq!(
            committed()["members"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            RootRole::ALL.map(RootRole::name)
        );
    }

    #[test]
    fn the_canonical_digest_frames_the_identity_the_revision_and_each_member() {
        let members = RootRole::ALL.map(|role| (role, digest_bytes(role.built_in())));
        // Written out frame by frame: tag, eight-byte big-endian length, payload.
        let frame = |tag: u8, payload: &[u8]| {
            let mut bytes = vec![tag];
            bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
            bytes.extend_from_slice(payload);
            bytes
        };
        let mut items = (members.len() as u64).to_be_bytes().to_vec();
        for (role, digest) in &members {
            let item = [frame(1, role.name().as_bytes()), frame(2, digest)].concat();
            items.extend_from_slice(&(item.len() as u64).to_be_bytes());
            items.extend_from_slice(&item);
        }
        let preimage = [
            b"intlify.resolver-construction-root-package.v0".to_vec(),
            frame(1, b"intlify-resolver-construction-root-package"),
            frame(2, b"0"),
            frame(3, &items),
        ]
        .concat();
        assert_eq!(canonical_digest(&members), digest_bytes(&preimage));
        assert_eq!(
            IntegrityDigest::from_hash(canonical_digest(&members)).as_str(),
            CANONICAL_DIGEST
        );
    }

    #[test]
    fn the_canonical_digest_commits_every_role_and_member_but_no_path() {
        let members = RootRole::ALL.map(|role| (role, digest_bytes(role.built_in())));
        let original = canonical_digest(&members);
        let mut changed = members;
        changed[4].1[31] ^= 1;
        assert_ne!(canonical_digest(&changed), original);
        let mut swapped = members;
        swapped.swap(0, 1);
        assert_ne!(canonical_digest(&swapped), original);
        // The artifact's own bytes name no path, so moving the files would not
        // change either digest.
        let text = std::str::from_utf8(BYTES).unwrap();
        assert!(!text.contains("construction/"));
    }

    #[test]
    fn a_changed_artifact_is_refused_by_its_file_digest() {
        let mut bytes = BYTES.to_vec();
        bytes.push(b'\n');
        assert_eq!(
            verify(&bytes, built_in_members(), DIGEST, CANONICAL_DIGEST).map(|_| ()),
            Err(RootPackageFailure::DigestMismatch)
        );
    }

    #[test]
    fn a_member_whose_bytes_changed_is_named() {
        for (index, role) in RootRole::ALL.iter().enumerate() {
            let mut body = role.built_in().to_vec();
            body.push(b' ');
            let mut members = RootRole::ALL.map(|role| (role, role.built_in().to_vec()));
            members[index].1 = body;
            let borrowed = [0, 1, 2, 3, 4].map(|at| (members[at].0, members[at].1.as_slice()));
            assert_eq!(
                verify(BYTES, borrowed, DIGEST, CANONICAL_DIGEST).map(|_| ()),
                Err(RootPackageFailure::MemberDigestMismatch(*role))
            );
        }
    }

    #[test]
    fn the_roles_must_be_the_five_in_their_fixed_order() {
        let mut value = committed();
        let members = value["members"].as_object().unwrap().clone();
        let mut reordered = serde_json::Map::new();
        for (role, entry) in members.iter().rev() {
            reordered.insert(role.clone(), entry.clone());
        }
        value["members"] = serde_json::Value::Object(reordered);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            verify_self_pinned(&bytes).map(|_| ()),
            Err(RootPackageFailure::RoleDisagreement)
        );
        let mut missing = committed();
        missing["members"]
            .as_object_mut()
            .unwrap()
            .remove("cross-authority-rules");
        let bytes = serde_json::to_vec(&missing).unwrap();
        assert_eq!(
            verify_self_pinned(&bytes).map(|_| ()),
            Err(RootPackageFailure::RoleDisagreement)
        );
    }

    #[test]
    fn members_handed_over_out_of_role_order_disagree_with_the_artifact() {
        // The artifact lists the roles in order; bodies handed over in another
        // order would be checked against the wrong listed digests.
        let mut members = built_in_members();
        members.swap(0, 1);
        assert_eq!(
            verify(BYTES, members, DIGEST, CANONICAL_DIGEST).map(|_| ()),
            Err(RootPackageFailure::RoleDisagreement)
        );
    }

    #[test]
    fn a_malformed_artifact_is_refused() {
        let digest = committed()["members"]["cross-authority-rules"]["digest"].clone();
        for value in [
            json!([]),
            json!({ "members": [] }),
            json!({ "members": {}, "extra": {} }),
        ] {
            let bytes = serde_json::to_vec(&value).unwrap();
            assert_eq!(
                verify_self_pinned(&bytes).map(|_| ()),
                Err(RootPackageFailure::Malformed),
                "{value}"
            );
        }
        for entry in [
            json!({ "digest": "SHA256:00" }),
            json!({ "digest": digest, "path": "construction/x.json" }),
            json!({ "digest": 1 }),
            json!("sha256:00"),
        ] {
            let mut value = committed();
            value["members"]["construction-admission-schema"] = entry.clone();
            let bytes = serde_json::to_vec(&value).unwrap();
            assert_eq!(
                verify_self_pinned(&bytes).map(|_| ()),
                Err(RootPackageFailure::Malformed),
                "{entry}"
            );
        }
        // A duplicate member is not one of two readings.
        let text = std::str::from_utf8(BYTES).unwrap();
        let duplicated = text.replacen(
            "\"members\": {",
            "\"members\": {\"cross-authority-rules\": {},",
            1,
        );
        assert_eq!(
            verify_self_pinned(duplicated.as_bytes()).map(|_| ()),
            Err(RootPackageFailure::Malformed)
        );
    }

    #[test]
    fn a_canonical_digest_that_is_not_the_pinned_one_is_refused() {
        let other = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
        assert_eq!(
            verify(BYTES, built_in_members(), DIGEST, other).map(|_| ()),
            Err(RootPackageFailure::CanonicalDigestMismatch)
        );
    }

    #[test]
    fn a_digest_is_read_back_only_from_its_exact_presentation() {
        let text = DIGEST;
        assert_eq!(
            digest_bytes_of(text)
                .map(|bytes| IntegrityDigest::from_hash(bytes).as_str().to_owned()),
            Some(text.to_owned())
        );
        let hex = text.strip_prefix("sha256:").unwrap();
        for invalid in [
            "sha256:".to_owned(),
            text.to_uppercase(),
            // Uppercase digits under the right prefix are still not the
            // presentation: a digest is spelled in lowercase alone.
            format!("sha256:{}", hex.to_uppercase()),
            text.replacen("sha256:", "sha512:", 1),
            format!("{text}0"),
            text.replacen('0', "g", 1),
            text.replacen("sha256:", "", 1),
        ] {
            assert_eq!(digest_bytes_of(&invalid), None, "{invalid}");
        }
    }
}
