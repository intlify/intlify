// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's Intent Surface-Class Vocabulary Artifact and its reader.
//!
//! [`read`] is 015's `intent-surface-class-vocabulary-artifact` pre-invocation
//! boundary: the caller's five limits (017 step 1) and the strict, closed
//! decoding (step 2). A failure there has one of 015's closed reasons, and the
//! resolver is not invoked. [`FormalVocabulary::admit`] is the rest of the
//! reader (steps 3 to 7), whose typed failures 015's Phase 5 reports; whether
//! an intact vocabulary is the one the slot asserts is 015's check, in
//! `resolver::vocabulary`. Reading never appends, removes, renames, or infers
//! a member.

use intlify_shared_json::encoding::{hash, hash_with_excluded_member, Domain, EncodingFailure};
use intlify_shared_json::json::decode_unique_json;
use intlify_shared_json::token::{IntegrityDigest, Token};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::SemanticDigest;

/// The one artifact kind this reader implements.
pub(crate) const ARTIFACT_KIND: &str = "intent-surface-class-vocabulary";

/// The one artifact schema revision this reader implements.
pub(crate) const ARTIFACT_SCHEMA_REVISION: &str = "0";

/// The digest domain 017 registers for the artifact's integrity.
pub(crate) const INTEGRITY_DOMAIN: &str = "intent-surface-class-vocabulary-integrity";

/// The digest domain 017 registers for the artifact's semantic digest.
pub(crate) const SEMANTIC_DOMAIN: &str = "intent-surface-class-vocabulary-semantic-digest";

// Single-value types: each serializes as one fixed spelling and refuses every
// other, which is what makes the committed schema as strict as the reader.
// They carry no doc comments, which would become schema descriptions.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum ArtifactKind {
    #[serde(rename = "intent-surface-class-vocabulary")]
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum RevisionZero {
    #[serde(rename = "0")]
    Value,
}

/// One Intent Surface-Class Vocabulary Artifact: the complete finite set of
/// surface classes that coverage and authoring may use.
// A plain comment, since schemars would take a doc comment as a schema
// description: the boundary reads `members` as strings, so a member that is
// not an `IdentityToken` reaches the reader's step 4, while the schema states
// the token grammar 017 gives the members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct IntentSurfaceClassVocabularyArtifact {
    kind: ArtifactKind,
    schema_revision: RevisionZero,
    identity: Token,
    revision: Token,
    /// The surface classes, sorted by unsigned UTF-8 bytes and each named once.
    /// The set may be empty.
    #[schemars(with = "Vec<Token>")]
    members: Vec<String>,
    integrity_digest: IntegrityDigest,
}

impl IntentSurfaceClassVocabularyArtifact {
    /// The artifact of these members, as given, with its integrity digest
    /// computed over the rest.
    #[cfg(any(test, feature = "vectors"))]
    pub(crate) fn sealed(
        identity: &'static str,
        revision: &'static str,
        members: &[&str],
    ) -> Result<Self, EncodingFailure> {
        let mut artifact = Self {
            kind: ArtifactKind::Value,
            schema_revision: RevisionZero::Value,
            identity: Token::literal(identity),
            revision: Token::literal(revision),
            members: members.iter().map(|member| (*member).to_owned()).collect(),
            integrity_digest: IntegrityDigest::from_hash([0; 32]),
        };
        artifact.integrity_digest = artifact.integrity()?;
        Ok(artifact)
    }

    /// `H("intent-surface-class-vocabulary-integrity", artifact)` with only the
    /// top-level `integrityDigest` left out.
    fn integrity(&self) -> Result<IntegrityDigest, EncodingFailure> {
        hash_with_excluded_member(
            Domain::literal(INTEGRITY_DOMAIN),
            self,
            &["integrityDigest"],
        )
        .map(IntegrityDigest::from_hash)
    }

    /// `{ kind, schemaRevision, members }`: the members, without the name and
    /// revision the semantic digest leaves out.
    pub(crate) fn semantic_preimage(&self) -> Value {
        json!({
            "kind": ARTIFACT_KIND,
            "schemaRevision": ARTIFACT_SCHEMA_REVISION,
            "members": self.members,
        })
    }

    fn semantic_digest(&self) -> Result<SemanticDigest, EncodingFailure> {
        hash(Domain::literal(SEMANTIC_DOMAIN), &self.semantic_preimage())
            .map(SemanticDigest::from_hash)
    }
}

/// The caller's finite limits on one artifact (017 step 1), each checked before
/// the work it bounds. There are no defaults: 017 asks the caller for them, and
/// no 015 bound names the vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VocabularyLimits {
    /// The artifact's bytes, checked before it is decoded.
    pub(crate) artifact_bytes: u64,
    /// Its nesting of objects and arrays, checked before it is decoded.
    pub(crate) depth: u64,
    /// The number of its members.
    pub(crate) members: u64,
    /// The UTF-8 bytes of each member.
    pub(crate) member_bytes: u64,
    /// The UTF-8 bytes of all its members together.
    pub(crate) total_member_bytes: u64,
}

/// What a boundary limit bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnvelopeLimit {
    ArtifactBytes,
    Depth,
    Members,
    MemberBytes,
    TotalMemberBytes,
}

/// Why the boundary refuses an artifact. Each has one of 015's closed reasons,
/// which [`EnvelopeFailure::reason`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnvelopeFailure {
    /// No artifact was supplied.
    Missing,
    /// The artifact is over one of the caller's limits.
    OverBound(EnvelopeLimit),
    /// The bytes are not one JSON value of the closed artifact.
    Malformed,
    /// The `kind` or `schemaRevision` is not the one this reader implements.
    Unsupported,
    /// The artifact repeats a member.
    DuplicateMember,
    /// The artifact has a member the closed artifact does not.
    UnknownMember,
}

impl EnvelopeFailure {
    /// 015's closed reason at the `intent-surface-class-vocabulary-artifact`
    /// boundary. A `kind` or `schemaRevision` this reader does not implement is
    /// no envelope it can read, so it is `malformed-envelope` there.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing-input",
            Self::OverBound(_) => "over-bound-envelope",
            Self::Malformed | Self::Unsupported => "malformed-envelope",
            Self::DuplicateMember => "duplicate-member",
            Self::UnknownMember => "unknown-member",
        }
    }
}

/// An artifact past 015's pre-invocation boundary: within the caller's limits
/// and closed, with members that are strings. Only [`read`] makes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormalVocabulary(IntentSurfaceClassVocabularyArtifact);

/// Read an artifact at the pre-invocation boundary.
///
/// 017 applies every limit before anything is decoded, so the byte and depth
/// limits come first. The member limits need the members, so they are counted
/// on the decoded value before it is projected onto the closed type. The rest
/// follows 015's order: malformed, then a repeated member, then an unknown one.
/// A repeated member is found once the shape is known to be right, from a
/// reading that keeps one value per member; an unknown member is removed before
/// the shape is checked, so it is reported only when nothing else is wrong.
pub(crate) fn read(
    bytes: Option<&[u8]>,
    limits: &VocabularyLimits,
) -> Result<FormalVocabulary, EnvelopeFailure> {
    let bytes = bytes.ok_or(EnvelopeFailure::Missing)?;
    if count(bytes.len()) > limits.artifact_bytes {
        return Err(EnvelopeFailure::OverBound(EnvelopeLimit::ArtifactBytes));
    }
    if depth(bytes) > limits.depth {
        return Err(EnvelopeFailure::OverBound(EnvelopeLimit::Depth));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| EnvelopeFailure::Malformed)?;
    let mut value: Value = serde_json::from_str(text).map_err(|_| EnvelopeFailure::Malformed)?;
    within(&value, limits)?;
    select(&value)?;
    let unknown = strip(&mut value);
    let artifact = serde_json::from_value(value).map_err(|_| EnvelopeFailure::Malformed)?;
    // The text parsed above, so a repeated member is all that can fail here.
    if decode_unique_json(text).is_err() {
        return Err(EnvelopeFailure::DuplicateMember);
    }
    if unknown {
        return Err(EnvelopeFailure::UnknownMember);
    }
    Ok(FormalVocabulary(artifact))
}

fn count(length: usize) -> u64 {
    u64::try_from(length).expect("an in-memory length fits u64")
}

/// The deepest nesting of objects and arrays, counted outside strings without
/// decoding anything.
fn depth(bytes: &[u8]) -> u64 {
    let (mut depth, mut deepest) = (0_u64, 0_u64);
    let (mut quoted, mut escaped) = (false, false);
    for &byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'{' | b'[' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

/// The member limits, counted on the decoded value whatever its shape: a
/// `members` that is not an array has no members, and a member that is not a
/// string has no bytes. The closed type refuses both afterwards.
fn within(value: &Value, limits: &VocabularyLimits) -> Result<(), EnvelopeFailure> {
    let members = value
        .get("members")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    if count(members.len()) > limits.members {
        return Err(EnvelopeFailure::OverBound(EnvelopeLimit::Members));
    }
    let mut total = 0_u64;
    for member in members {
        let bytes = count(member.as_str().map_or(0, str::len));
        if bytes > limits.member_bytes {
            return Err(EnvelopeFailure::OverBound(EnvelopeLimit::MemberBytes));
        }
        total = total.saturating_add(bytes);
    }
    if total > limits.total_member_bytes {
        return Err(EnvelopeFailure::OverBound(EnvelopeLimit::TotalMemberBytes));
    }
    Ok(())
}

/// Select the exact `kind` and `schemaRevision` before the rest is projected:
/// another kind or revision has another shape, so projecting it as this one
/// would report a malformed artifact for one this reader does not implement.
fn select(value: &Value) -> Result<(), EnvelopeFailure> {
    let text = |member: &str| {
        value
            .get(member)
            .and_then(Value::as_str)
            .ok_or(EnvelopeFailure::Malformed)
    };
    if text("kind")? != ARTIFACT_KIND || text("schemaRevision")? != ARTIFACT_SCHEMA_REVISION {
        return Err(EnvelopeFailure::Unsupported);
    }
    Ok(())
}

/// The closed artifact's members. `members` holds strings, which the closed
/// type decides alone, so only the artifact itself can have an unknown member.
pub(crate) const MEMBERS: [&str; 6] = [
    "identity",
    "integrityDigest",
    "kind",
    "members",
    "revision",
    "schemaRevision",
];

/// Remove every member the closed artifact does not have, and say whether
/// there was one. `select` has found an object.
fn strip(value: &mut Value) -> bool {
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let before = object.len();
    object.retain(|name, _| MEMBERS.contains(&name.as_str()));
    object.len() != before
}

/// Why a formally admitted artifact is not an intact vocabulary: 017 steps 3
/// to 6, which 015 reports in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyFailure {
    /// The integrity digest is not the digest of the rest of the artifact.
    Integrity,
    /// A digest is beyond the shared encoding's capacity, so it cannot be
    /// computed, which is not the same as a digest that disagrees.
    Unframable,
    /// A member is not an `IdentityToken`.
    MemberInvalid,
    /// A member occurs more than once, adjacent or not.
    MemberDuplicate,
    /// The members are unique but not in ascending unsigned UTF-8 byte order.
    NonCanonicalOrder,
}

/// A vocabulary whose integrity holds and whose members are valid, unique,
/// and sorted, with its semantic digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntactVocabulary {
    identity: Token,
    revision: Token,
    members: Vec<Token>,
    semantic_digest: SemanticDigest,
}

impl IntactVocabulary {
    pub(crate) const fn identity(&self) -> &Token {
        &self.identity
    }

    pub(crate) const fn revision(&self) -> &Token {
        &self.revision
    }

    pub(crate) fn members(&self) -> &[Token] {
        &self.members
    }

    pub(crate) const fn semantic_digest(&self) -> &SemanticDigest {
        &self.semantic_digest
    }

    pub(crate) fn into_members(self) -> Vec<Token> {
        self.members
    }
}

impl FormalVocabulary {
    /// 017 steps 3 to 7: the integrity, each member an `IdentityToken`, no
    /// member twice, the members sorted, and then the semantic digest.
    pub(crate) fn admit(&self) -> Result<IntactVocabulary, BodyFailure> {
        let artifact = &self.0;
        let integrity = artifact.integrity().map_err(|_| BodyFailure::Unframable)?;
        if integrity != artifact.integrity_digest {
            return Err(BodyFailure::Integrity);
        }
        let members = artifact
            .members
            .iter()
            .map(|member| Token::new(member))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| BodyFailure::MemberInvalid)?;
        let mut sorted = members.clone();
        sorted.sort_unstable();
        if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(BodyFailure::MemberDuplicate);
        }
        if members != sorted {
            return Err(BodyFailure::NonCanonicalOrder);
        }
        let semantic_digest = artifact
            .semantic_digest()
            .map_err(|_| BodyFailure::Unframable)?;
        Ok(IntactVocabulary {
            identity: artifact.identity.clone(),
            revision: artifact.revision.clone(),
            members,
            semantic_digest,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    const ROOM: VocabularyLimits = VocabularyLimits {
        artifact_bytes: u64::MAX,
        depth: u64::MAX,
        members: u64::MAX,
        member_bytes: u64::MAX,
        total_member_bytes: u64::MAX,
    };

    fn sealed(members: &[&str]) -> Value {
        serde_json::to_value(
            IntentSurfaceClassVocabularyArtifact::sealed("intlify-test-vocabulary", "0", members)
                .unwrap(),
        )
        .unwrap()
    }

    fn bytes(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    fn read_value(value: &Value) -> Result<FormalVocabulary, EnvelopeFailure> {
        read(Some(&bytes(value)), &ROOM)
    }

    fn admit(value: &Value) -> Result<IntactVocabulary, BodyFailure> {
        read_value(value).unwrap().admit()
    }

    #[test]
    fn an_absent_artifact_is_missing_input() {
        assert_eq!(read(None, &ROOM), Err(EnvelopeFailure::Missing));
    }

    #[test]
    fn each_boundary_failure_has_its_015_reason() {
        let reasons = [
            (EnvelopeFailure::Missing, "missing-input"),
            (
                EnvelopeFailure::OverBound(EnvelopeLimit::ArtifactBytes),
                "over-bound-envelope",
            ),
            (
                EnvelopeFailure::OverBound(EnvelopeLimit::Depth),
                "over-bound-envelope",
            ),
            (
                EnvelopeFailure::OverBound(EnvelopeLimit::Members),
                "over-bound-envelope",
            ),
            (
                EnvelopeFailure::OverBound(EnvelopeLimit::MemberBytes),
                "over-bound-envelope",
            ),
            (
                EnvelopeFailure::OverBound(EnvelopeLimit::TotalMemberBytes),
                "over-bound-envelope",
            ),
            (EnvelopeFailure::Malformed, "malformed-envelope"),
            (EnvelopeFailure::Unsupported, "malformed-envelope"),
            (EnvelopeFailure::DuplicateMember, "duplicate-member"),
            (EnvelopeFailure::UnknownMember, "unknown-member"),
        ];
        for (failure, reason) in reasons {
            assert_eq!(failure.reason(), reason, "{failure:?}");
        }
    }

    #[test]
    fn each_limit_admits_its_exact_value_and_refuses_the_next() {
        // Counted by hand: three members of 8, 13, and 3 bytes, nested two
        // deep, written compactly.
        let artifact = sealed(&["checkout", "internal-tool", "nav"]);
        let text = bytes(&artifact);
        let exact = VocabularyLimits {
            artifact_bytes: u64::try_from(text.len()).unwrap(),
            depth: 2,
            members: 3,
            member_bytes: 13,
            total_member_bytes: 24,
        };
        assert!(read(Some(&text), &exact).is_ok());
        let over = [
            (
                VocabularyLimits {
                    artifact_bytes: exact.artifact_bytes - 1,
                    ..exact
                },
                EnvelopeLimit::ArtifactBytes,
            ),
            (VocabularyLimits { depth: 1, ..exact }, EnvelopeLimit::Depth),
            (
                VocabularyLimits {
                    members: 2,
                    ..exact
                },
                EnvelopeLimit::Members,
            ),
            (
                VocabularyLimits {
                    member_bytes: 12,
                    ..exact
                },
                EnvelopeLimit::MemberBytes,
            ),
            (
                VocabularyLimits {
                    total_member_bytes: 23,
                    ..exact
                },
                EnvelopeLimit::TotalMemberBytes,
            ),
        ];
        for (limits, limit) in over {
            assert_eq!(
                read(Some(&text), &limits),
                Err(EnvelopeFailure::OverBound(limit)),
                "{limit:?}"
            );
        }
    }

    #[test]
    fn member_limits_count_whatever_the_members_are() {
        // An escape of six characters spells two bytes, a member that is not
        // a string has none, and a `members` that is not an array has no
        // members. The counts pass, so the closed type refuses each.
        let text = br#"{"members":["\u00e9",7,["x"]]}"#;
        let limits = VocabularyLimits {
            members: 3,
            member_bytes: 2,
            total_member_bytes: 2,
            ..ROOM
        };
        assert_eq!(read(Some(text), &limits), Err(EnvelopeFailure::Malformed));
        let over = [
            (
                VocabularyLimits {
                    members: 2,
                    ..limits
                },
                EnvelopeLimit::Members,
            ),
            (
                VocabularyLimits {
                    member_bytes: 1,
                    ..limits
                },
                EnvelopeLimit::MemberBytes,
            ),
            (
                VocabularyLimits {
                    total_member_bytes: 1,
                    ..limits
                },
                EnvelopeLimit::TotalMemberBytes,
            ),
        ];
        for (limits, limit) in over {
            assert_eq!(
                read(Some(text), &limits),
                Err(EnvelopeFailure::OverBound(limit)),
                "{limit:?}"
            );
        }
        let nothing = VocabularyLimits {
            members: 0,
            member_bytes: 0,
            total_member_bytes: 0,
            ..ROOM
        };
        assert_eq!(
            read(Some(br#"{"members":"checkout"}"#), &nothing),
            Err(EnvelopeFailure::Malformed)
        );
    }

    #[test]
    fn a_member_over_its_own_limit_is_reported_before_the_total() {
        let limits = |member_bytes, total_member_bytes| VocabularyLimits {
            member_bytes,
            total_member_bytes,
            ..ROOM
        };
        let cases: [(&[&str], VocabularyLimits, EnvelopeLimit); 3] = [
            (&["aaaa", "b"], limits(3, 4), EnvelopeLimit::MemberBytes),
            (
                &["aaaa", "b"],
                limits(4, 4),
                EnvelopeLimit::TotalMemberBytes,
            ),
            // The total is past its limit before the long member is reached.
            (
                &["aaa", "bbb", "cccc"],
                limits(3, 5),
                EnvelopeLimit::MemberBytes,
            ),
        ];
        for (members, limits, limit) in cases {
            assert_eq!(
                read(Some(&bytes(&sealed(members))), &limits),
                Err(EnvelopeFailure::OverBound(limit)),
                "{members:?}"
            );
        }
    }

    #[test]
    fn limits_come_before_anything_is_decoded() {
        // Not JSON at all, yet the byte and depth limits speak first.
        let text = b"[[[ not json";
        assert_eq!(
            read(
                Some(text),
                &VocabularyLimits {
                    artifact_bytes: 10,
                    ..ROOM
                }
            ),
            Err(EnvelopeFailure::OverBound(EnvelopeLimit::ArtifactBytes))
        );
        assert_eq!(
            read(Some(text), &VocabularyLimits { depth: 2, ..ROOM }),
            Err(EnvelopeFailure::OverBound(EnvelopeLimit::Depth))
        );
        assert_eq!(read(Some(text), &ROOM), Err(EnvelopeFailure::Malformed));
        // Depth is nesting, not brackets: a bracket inside a string, even after
        // an escaped quote, is not nesting, a bracket after a string with an
        // escape is, siblings do not add up, and the deepest point counts
        // however the text continues.
        let cases: [(&[u8], u64); 5] = [
            (br#"{"members":["[[[{{{"]}"#, 2),
            (br#"["\"",[[]]]"#, 3),
            (br#"{"members":["\"[[[\\"]}"#, 2),
            (b"[[],[]]", 2),
            (b"[[[]],[]]", 3),
        ];
        for (text, deepest) in cases {
            assert_eq!(
                read(
                    Some(text),
                    &VocabularyLimits {
                        depth: deepest,
                        ..ROOM
                    }
                ),
                Err(EnvelopeFailure::Malformed)
            );
            assert_eq!(
                read(
                    Some(text),
                    &VocabularyLimits {
                        depth: deepest - 1,
                        ..ROOM
                    }
                ),
                Err(EnvelopeFailure::OverBound(EnvelopeLimit::Depth))
            );
        }
        // The member limits come before the kind is selected.
        let mut other = sealed(&["checkout", "nav"]);
        other["kind"] = json!("intent-surface-class-taxonomy");
        assert_eq!(
            read(
                Some(&bytes(&other)),
                &VocabularyLimits { members: 1, ..ROOM }
            ),
            Err(EnvelopeFailure::OverBound(EnvelopeLimit::Members))
        );
        assert_eq!(read_value(&other), Err(EnvelopeFailure::Unsupported));
    }

    #[test]
    fn the_boundary_refuses_what_is_not_the_closed_artifact() {
        let valid = sealed(&["checkout", "nav"]);
        let edited = |edit: &dyn Fn(&mut Value)| {
            let mut value = valid.clone();
            edit(&mut value);
            read_value(&value)
        };
        let malformed: [&dyn Fn(&mut Value); 9] = [
            &|value| {
                value.as_object_mut().unwrap().remove("identity");
            },
            &|value| value["identity"] = json!("Intlify"),
            &|value| value["revision"] = json!(""),
            &|value| value["members"] = json!("checkout"),
            &|value| value["members"] = json!(["checkout", 1]),
            &|value| value["members"] = json!(["checkout", null]),
            &|value| value["integrityDigest"] = json!("sha256:00"),
            &|value| value["kind"] = json!(7),
            &|value| *value = json!(["not", "an", "object"]),
        ];
        for edit in malformed {
            assert_eq!(edited(edit), Err(EnvelopeFailure::Malformed));
        }
        assert_eq!(
            edited(&|value| value["schemaRevision"] = json!("1")),
            Err(EnvelopeFailure::Unsupported)
        );
        for text in [
            &b"\xef\xbb\xbf{}"[..],
            b"{} {}",
            b"\xff",
            br#"{"members":["\ud800"]}"#,
        ] {
            assert_eq!(read(Some(text), &ROOM), Err(EnvelopeFailure::Malformed));
        }
    }

    #[test]
    fn a_repeated_member_comes_after_the_shape_and_an_unknown_one_last() {
        let valid = bytes(&sealed(&["checkout", "nav"]));
        let text = std::str::from_utf8(&valid).unwrap();
        let repeated = text.replacen('{', r#"{"revision":"9","#, 1);
        assert_eq!(
            read(Some(repeated.as_bytes()), &ROOM),
            Err(EnvelopeFailure::DuplicateMember)
        );
        let unknown = text.replacen('{', r#"{"description":"x","#, 1);
        assert_eq!(
            read(Some(unknown.as_bytes()), &ROOM),
            Err(EnvelopeFailure::UnknownMember)
        );
        // An unknown member does not hide a repeated one, and a repeated
        // member whose last value is malformed is malformed.
        let broken = text.replacen('{', r#"{"description":"x","revision":"9","#, 1);
        assert_eq!(
            read(Some(broken.as_bytes()), &ROOM),
            Err(EnvelopeFailure::DuplicateMember)
        );
        let last = text.replacen("\"revision\":\"0\"", r#""revision":"0","revision":"X""#, 1);
        assert_eq!(
            read(Some(last.as_bytes()), &ROOM),
            Err(EnvelopeFailure::Malformed)
        );
    }

    #[test]
    fn a_member_that_is_not_an_identity_token_passes_the_boundary() {
        for member in ["Nav", "", "nav ", "\u{e9}"] {
            assert!(read_value(&sealed(&[member])).is_ok(), "{member:?}");
        }
    }

    #[test]
    fn an_intact_vocabulary_keeps_its_name_and_members() {
        let intact = admit(&sealed(&["checkout", "internal-tool", "nav"])).unwrap();
        assert_eq!(intact.identity().as_str(), "intlify-test-vocabulary");
        assert_eq!(intact.revision().as_str(), "0");
        assert_eq!(
            intact
                .members()
                .iter()
                .map(Token::as_str)
                .collect::<Vec<_>>(),
            ["checkout", "internal-tool", "nav"]
        );
        let empty = admit(&sealed(&[])).unwrap();
        assert!(empty.members().is_empty());
        assert_ne!(empty.semantic_digest(), intact.semantic_digest());
    }

    #[test]
    fn the_integrity_covers_everything_but_its_own_member() {
        let valid = sealed(&["checkout", "nav"]);
        for (member, value) in [
            ("identity", json!("intlify-other-vocabulary")),
            ("revision", json!("1")),
            ("members", json!(["checkout", "navigation"])),
            ("members", json!(["checkout"])),
        ] {
            let mut edited = valid.clone();
            edited[member] = value;
            assert_eq!(admit(&edited), Err(BodyFailure::Integrity), "{member}");
        }
        // Spacing and member order are not part of the value.
        let spaced = serde_json::to_string_pretty(&valid).unwrap();
        let reordered = format!(
            "{{\"members\":{},\"integrityDigest\":{},\"revision\":\"0\",\"identity\":\"intlify-test-vocabulary\",\"schemaRevision\":\"0\",\"kind\":\"{ARTIFACT_KIND}\"}}",
            valid["members"], valid["integrityDigest"]
        );
        for text in [spaced, reordered] {
            assert!(read(Some(text.as_bytes()), &ROOM).unwrap().admit().is_ok());
        }
    }

    #[test]
    fn each_reader_step_fails_in_017s_order() {
        // Integrity before members: an invalid member under a stale digest.
        let mut stale = sealed(&["checkout", "nav"]);
        stale["members"] = json!(["Nav"]);
        assert_eq!(admit(&stale), Err(BodyFailure::Integrity));
        // An invalid member before a repeated one, a repeated one before an
        // order, which a repeat that is not adjacent also breaks.
        let cases: [(&[&str], BodyFailure); 5] = [
            (&["nav", "Nav", "nav"], BodyFailure::MemberInvalid),
            (
                &["checkout", "nav", "checkout"],
                BodyFailure::MemberDuplicate,
            ),
            (&["nav", "nav"], BodyFailure::MemberDuplicate),
            (&["nav", "checkout"], BodyFailure::NonCanonicalOrder),
            (
                &["internal-tool", "checkout", "nav"],
                BodyFailure::NonCanonicalOrder,
            ),
        ];
        for (members, failure) in cases {
            assert_eq!(admit(&sealed(members)), Err(failure), "{members:?}");
        }
    }

    #[test]
    fn the_semantic_digest_names_the_members_and_nothing_else() {
        let digest = |identity, revision, members: &[&str]| {
            let artifact =
                IntentSurfaceClassVocabularyArtifact::sealed(identity, revision, members).unwrap();
            admit(&serde_json::to_value(artifact).unwrap())
                .unwrap()
                .semantic_digest()
                .clone()
        };
        let members = ["checkout", "internal-tool", "nav"];
        let original = digest("intlify-test-vocabulary", "0", &members);
        assert_eq!(
            digest("intlify-renamed-vocabulary", "0", &members),
            original
        );
        assert_eq!(digest("intlify-test-vocabulary", "1", &members), original);
        assert_ne!(
            digest(
                "intlify-test-vocabulary",
                "0",
                &["checkout", "internal-tool", "navigation"]
            ),
            original
        );
        assert_ne!(
            digest(
                "intlify-test-vocabulary",
                "0",
                &["checkout", "internal-tool"]
            ),
            original
        );
    }

    #[test]
    fn the_preimages_are_the_artifact_without_its_digest_and_the_members_alone() {
        let artifact =
            IntentSurfaceClassVocabularyArtifact::sealed("intlify-test-vocabulary", "0", &["nav"])
                .unwrap();
        assert_eq!(
            artifact.semantic_preimage(),
            json!({
                "kind": "intent-surface-class-vocabulary",
                "schemaRevision": "0",
                "members": ["nav"],
            })
        );
        let mut preimage = serde_json::to_value(&artifact).unwrap();
        preimage.as_object_mut().unwrap().remove("integrityDigest");
        assert_eq!(
            hash(Domain::literal(INTEGRITY_DOMAIN), &preimage).map(IntegrityDigest::from_hash),
            Ok(artifact.integrity_digest.clone())
        );
        assert_ne!(INTEGRITY_DOMAIN, SEMANTIC_DOMAIN);
    }

    const COMMITTED_SCHEMA: &str =
        include_str!("../../schema/intent-surface-class-vocabulary-v0.schema.json");

    fn schema() -> Value {
        crate::schema::intent_surface_class_vocabulary_schema().unwrap()
    }

    fn sorted_strings(value: &Value) -> Vec<String> {
        let mut strings: Vec<String> = match value {
            Value::Array(items) => items
                .iter()
                .map(|item| item.as_str().unwrap().to_owned())
                .collect(),
            Value::Object(members) => members.keys().cloned().collect(),
            other => panic!("{other} lists nothing"),
        };
        strings.sort_unstable();
        strings
    }

    #[test]
    fn the_committed_schema_matches_the_current_type() {
        assert_eq!(
            schema(),
            serde_json::from_str::<Value>(COMMITTED_SCHEMA).unwrap(),
            "regenerate with: cargo run -p intlify_config \
             --example generate_vocabulary_schema -- --write"
        );
    }

    #[test]
    fn the_schema_is_closed_and_pins_one_kind_and_revision() {
        let schema = schema();
        assert_eq!(
            schema["$schema"],
            json!("http://json-schema.org/draft-07/schema#")
        );
        assert!(schema.get("$id").is_none());
        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(sorted_strings(&schema["properties"]), MEMBERS);
        assert_eq!(sorted_strings(&schema["required"]), MEMBERS);
        let definitions = &schema["definitions"];
        assert_eq!(definitions["ArtifactKind"]["enum"], json!([ARTIFACT_KIND]));
        assert_eq!(
            definitions["RevisionZero"]["enum"],
            json!([ARTIFACT_SCHEMA_REVISION])
        );
        assert_eq!(
            schema["properties"]["members"]["items"],
            json!({"$ref": "#/definitions/Token"})
        );
    }

    #[test]
    fn the_schema_refuses_the_boundary_shapes_and_an_invalid_member_but_no_repeat_or_order() {
        let schema = schema();
        let validator = jsonschema::draft7::new(&schema).unwrap();
        let vectors: Value =
            serde_json::from_str(include_str!("../../fixtures/vocabulary/vectors.json")).unwrap();
        for case in vectors["vocabularies"].as_array().unwrap() {
            assert!(validator.is_valid(&case["artifact"]), "{}", case["id"]);
        }
        let valid = sealed(&["checkout", "nav"]);
        let refused: [(&str, Value); 9] = [
            ("/kind", json!("intent-surface-class-taxonomy")),
            ("/schemaRevision", json!("1")),
            ("/identity", json!("Intlify")),
            ("/revision", json!("")),
            ("/members", json!("checkout")),
            ("/members/0", json!(1)),
            ("/integrityDigest", json!("sha256:00")),
            // The reader's step 4 refuses this one after the boundary.
            ("/members/0", json!("Checkout")),
            ("/members/1", json!("")),
        ];
        for (pointer, value) in refused {
            let mut edited = valid.clone();
            *edited.pointer_mut(pointer).unwrap() = value;
            assert!(!validator.is_valid(&edited), "{pointer}");
        }
        let mut unknown = valid.clone();
        unknown["description"] = json!("x");
        assert!(!validator.is_valid(&unknown));
        let mut missing = valid;
        missing.as_object_mut().unwrap().remove("members");
        assert!(!validator.is_valid(&missing));
        // Draft 7 cannot say "sorted", so a repeat and an order are the
        // reader's alone, as is the integrity.
        for members in [&["nav", "checkout", "nav"][..], &["nav", "checkout"]] {
            assert!(validator.is_valid(&sealed(members)), "{members:?}");
        }
    }

    #[test]
    fn the_closed_members_are_the_types_members() {
        let mut names = serde_json::to_value(
            IntentSurfaceClassVocabularyArtifact::sealed("a", "0", &[]).unwrap(),
        )
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(names, MEMBERS);
    }
}
