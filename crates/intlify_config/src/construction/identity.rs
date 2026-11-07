// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Resolver Construction Identity.
//!
//! Only a successful construction has an identity (D15:967). It is the
//! SHA-256 of an exact ASCII domain separator followed by one canonical
//! top-level record of eight fields, framed as design 015 frames its typed
//! records ([`framing`](crate::framing)): the Root Package by its canonical
//! digest; each component by its identity, revision, and file digest, except
//! the Profile Specification, by its semantic digest; the set's association;
//! every configuration mapping, in ascending unsigned UTF-8 `schemaVersion`
//! order; and the six registry members, in role order (D15:967-987). Strings
//! contribute their exact UTF-8 bytes and digests their 32 bytes, and the
//! top-level record is not itself framed. The identity is presented as
//! `rci0_` and the unpadded base64url of its 32 bytes.
//!
//! The construction interval ends when the authorities are admitted and
//! excludes the identity (D15:2766), so the identity is derived from the
//! admitted authorities as a step of its own.

use intlify_shared_json::encoding::digest_bytes;

use crate::base64url;
use crate::framing::{collection, Record};

use super::component::Declared;
use super::flow::AdmittedAuthorities;
use super::registry::RegistryRole;
use super::root::{self, digest_bytes_of, ConstructionRoot};

/// The domain separator the identity's preimage starts with.
pub(crate) const DOMAIN: &[u8] = b"intlify.resolver-construction-identity.v0";

/// A Resolver Construction Identity: the SHA-256 of its preimage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConstructionIdentity([u8; 32]);

impl ConstructionIdentity {
    /// The identity's 32 bytes.
    pub(crate) const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The identity's text: `rci0_` and its unpadded base64url.
    pub(crate) fn text(&self) -> String {
        base64url::construction_identity(&self.0)
    }
}

/// The 32 bytes of a digest construction admitted, which is always a
/// `sha256:` presentation.
fn digest(text: &str) -> [u8; 32] {
    digest_bytes_of(text).expect("construction admits only sha256 digests")
}

/// A component's record: its identity, its revision, and its file digest.
fn component(declared: &Declared) -> Vec<u8> {
    Record::new()
        .field(declared.identity.as_bytes())
        .field(declared.revision.as_bytes())
        .field(&digest(&declared.digest))
        .finish()
}

/// The identity's preimage for a construction that admitted these
/// authorities under this root.
pub(crate) fn preimage(root: &ConstructionRoot, admitted: &AdmittedAuthorities) -> Vec<u8> {
    let profile = &admitted.profile_specification;
    let set = &admitted.configuration_schema_authority_set;
    let association = &set.profile_specification;
    let mappings = set
        .members
        .iter()
        .map(|(version, admitted)| {
            let member = &admitted.member;
            Record::new()
                .field(version.as_bytes())
                .field(member.identity.as_bytes())
                .field(member.revision.as_bytes())
                .field(member.dialect.as_bytes())
                .field(&digest(&member.digest))
                .finish()
        })
        .collect::<Vec<_>>();
    let members = RegistryRole::ALL
        .into_iter()
        .zip(&admitted.registry_package.members)
        .map(|(role, member)| {
            Record::new()
                .field(role.name().as_bytes())
                .field(member.identity.as_bytes())
                .field(member.revision.as_bytes())
                .field(&digest(&member.digest))
                .finish()
        })
        .collect::<Vec<_>>();
    let record = Record::new()
        .field(
            &Record::new()
                .field(root::IDENTITY.as_bytes())
                .field(root::REVISION.as_bytes())
                .field(&root.canonical_digest())
                .finish(),
        )
        .field(&component(&admitted.profile_specification_schema.declared))
        .field(
            &Record::new()
                .field(profile.declared.identity.as_bytes())
                .field(profile.declared.revision.as_bytes())
                .field(&digest(profile.semantic_digest.as_str()))
                .finish(),
        )
        .field(
            &Record::new()
                .field(set.declared.identity.as_bytes())
                .field(set.declared.revision.as_bytes())
                .field(&digest(&set.declared.digest))
                .field(
                    &Record::new()
                        .field(association.identity.as_bytes())
                        .field(association.revision.as_bytes())
                        .field(&digest(&association.semantic_digest))
                        .finish(),
                )
                .finish(),
        )
        .field(&collection(mappings.iter().map(Vec::as_slice)))
        .field(&component(
            &admitted.resolver_capability_specification.declared,
        ))
        .field(&component(&admitted.registry_package.declared))
        .field(&collection(members.iter().map(Vec::as_slice)))
        .finish();
    let mut preimage = DOMAIN.to_vec();
    preimage.extend_from_slice(&record);
    preimage
}

/// Derive the identity of a construction that admitted these authorities.
pub(crate) fn derive(
    root: &ConstructionRoot,
    admitted: &AdmittedAuthorities,
) -> ConstructionIdentity {
    ConstructionIdentity(digest_bytes(&preimage(root, admitted)))
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::base64url::parse_construction_identity;
    use crate::construction::flow::testing::{
        committed, compact, profile_specification, value, PROFILE_SPECIFICATION,
    };
    use crate::construction::flow::{construct, ConstructionInput};
    use crate::construction::profile;
    use crate::framing::{items, record};

    const VECTORS: &[u8] =
        include_bytes!("../../conformance/construction/construction-vectors-v0.json");

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    fn admitted(input: &ConstructionInput) -> AdmittedAuthorities {
        construct(root(), input).unwrap()
    }

    fn hex(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        bytes
            .iter()
            .flat_map(|&byte| {
                [
                    DIGITS[usize::from(byte >> 4)],
                    DIGITS[usize::from(byte & 0x0f)],
                ]
            })
            .map(char::from)
            .collect()
    }

    #[test]
    fn the_committed_input_has_the_identity_the_vectors_pin() {
        let vectors = value(VECTORS);
        let expected = &vectors["constructionIdentity"];
        let admitted = admitted(&committed());
        let identity = derive(root(), &admitted);
        assert_eq!(hex(&preimage(root(), &admitted)), expected["preimage"]);
        assert_eq!(
            format!("sha256:{}", hex(identity.bytes())),
            expected["digest"]
        );
        assert_eq!(identity.text(), expected["text"]);
        assert_eq!(
            parse_construction_identity(&identity.text()),
            Ok(*identity.bytes())
        );
    }

    /// The preimage is the domain separator and one unframed record of
    /// 015's eight fields, each nested record and collection item restarting
    /// its tags, with every string's UTF-8 bytes and every digest's 32 bytes.
    #[test]
    fn the_preimage_is_015s_record_of_eight_fields() {
        let admitted = admitted(&committed());
        let preimage = preimage(root(), &admitted);
        let fields = record(preimage.strip_prefix(DOMAIN).unwrap(), 8).unwrap();
        let root_package = record(fields[0], 3).unwrap();
        assert_eq!(root_package[0], root::IDENTITY.as_bytes());
        assert_eq!(root_package[1], root::REVISION.as_bytes());
        assert_eq!(root_package[2], root().canonical_digest());
        let digest_of = |text: &str| digest_bytes_of(text).unwrap();
        let profile = record(fields[2], 3).unwrap();
        assert_eq!(profile[2], digest_of(profile::SEMANTIC_DIGEST));
        let set = record(fields[3], 4).unwrap();
        assert_eq!(
            record(set[3], 3).unwrap()[2],
            digest_of(profile::SEMANTIC_DIGEST)
        );
        let mappings = items(fields[4]).unwrap();
        assert_eq!(mappings.len(), 1);
        let mapping = record(mappings[0], 5).unwrap();
        assert_eq!(mapping[0], b"0");
        assert_eq!(mapping[3], b"http://json-schema.org/draft-07/schema#");
        for (field, declared) in [
            (fields[1], &admitted.profile_specification_schema.declared),
            (
                fields[5],
                &admitted.resolver_capability_specification.declared,
            ),
            (fields[6], &admitted.registry_package.declared),
        ] {
            let parts = record(field, 3).unwrap();
            assert_eq!(parts[0], declared.identity.as_bytes());
            assert_eq!(parts[2], digest_of(&declared.digest));
        }
        let members = items(fields[7]).unwrap();
        assert_eq!(members.len(), 6);
        for ((role, member), item) in RegistryRole::ALL
            .into_iter()
            .zip(&admitted.registry_package.members)
            .zip(members)
        {
            let parts = record(item, 4).unwrap();
            assert_eq!(parts[0], role.name().as_bytes());
            assert_eq!(parts[3], digest_of(&member.digest));
        }
    }

    /// Construction admits the same authorities from a Profile Specification
    /// in another layout and from tuples with their members in another
    /// order, and they have one identity.
    #[test]
    fn the_same_authorities_in_another_layout_have_one_identity() {
        let identity = |input: &ConstructionInput| derive(root(), &admitted(input));
        let expected = identity(&committed());
        let mut layout = committed();
        layout.profile_specification = Some(profile_specification(&compact(&value(
            PROFILE_SPECIFICATION,
        ))));
        assert_eq!(identity(&layout), expected);
        let reversed = |tuple: &Value| {
            let members = tuple.as_object().unwrap();
            Value::Object(
                members
                    .iter()
                    .rev()
                    .map(|(key, member)| (key.clone(), member.clone()))
                    .collect(),
            )
        };
        let mut order = committed();
        for component in [
            order.profile_specification_schema.as_mut(),
            order.profile_specification.as_mut(),
            order.resolver_capability_specification.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            component.tuple = reversed(&component.tuple);
        }
        assert_eq!(identity(&order), expected);
    }

    /// Every field of the record is part of the identity.
    #[test]
    fn every_field_moves_the_identity() {
        let expected = derive(root(), &admitted(&committed()));
        let changes: [fn(&mut AdmittedAuthorities); 8] = [
            |admitted| admitted.profile_specification_schema.declared.revision = "1".to_owned(),
            |admitted| {
                admitted.profile_specification.semantic_digest =
                    crate::construction::component::file_digest(b"other");
            },
            |admitted| {
                admitted
                    .configuration_schema_authority_set
                    .profile_specification
                    .identity
                    .push('x');
            },
            |admitted| {
                let member = admitted
                    .configuration_schema_authority_set
                    .members
                    .values_mut()
                    .next()
                    .unwrap();
                member.member.dialect.push('x');
            },
            |admitted| {
                admitted
                    .resolver_capability_specification
                    .declared
                    .identity
                    .push('x');
            },
            |admitted| admitted.registry_package.declared.revision = "1".to_owned(),
            |admitted| admitted.registry_package.members[5].identity.push('x'),
            |admitted| {
                admitted
                    .configuration_schema_authority_set
                    .declared
                    .identity
                    .push('x');
            },
        ];
        for (index, change) in changes.into_iter().enumerate() {
            let mut admitted = admitted(&committed());
            change(&mut admitted);
            assert_ne!(derive(root(), &admitted), expected, "change {index}");
        }
        assert_eq!(expected.text().len(), "rci0_".len() + 43);
    }
}
