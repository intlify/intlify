// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's `LocalizationArtifact` envelope as a member of 015's Submitted
//! Profile Resolution Artifact Collection: the eight kinds the collection
//! admits, what a member declares, the exact reference it establishes, its
//! canonical submitted body, and the admission of its body.
//!
//! These are facts about one member, not checks. Whether a member is
//! unsupported, an invalid envelope, a duplicate, or in conflict is 015's to
//! decide and report. No member declares its semantic digest: the digest of a
//! tuple is computed from the submitted content.

use intlify_shared_json::encoding::{
    encode, hash, hash_with_excluded_member, Domain, EncodingFailure,
};
use intlify_shared_json::token::{valid_hex256, IntegrityDigest, Token};
use serde_json::{Map, Value};

use super::content::{self, Content};
use super::{closed, count, token, SemanticDigest};

/// The one schema revision every admitted kind has.
pub(crate) const SCHEMA_REVISION: &str = "0";

/// The one specification revision a body is admitted under. 017 registers
/// each kind under revision "0" of its governing design, and 015 revision "0"
/// supports specification revision "0" of every Policy kind and of the Target
/// Profile. Another revision still establishes a tuple, so that 015 can tell a
/// reference which revision a candidate has.
pub(crate) const SPECIFICATION_REVISION: &str = "0";

/// The digest domain 017 registers for a localization artifact's integrity.
pub(crate) const INTEGRITY_DOMAIN: &str = "localization-artifact-integrity";

/// The semantic-digest domain of the seven Policy kinds.
pub(crate) const POLICY_DOMAIN: &str = "policy-semantic-digest";

/// The semantic-digest domain of the Target Profile.
pub(crate) const TARGET_PROFILE_DOMAIN: &str = "target-profile-semantic-digest";

/// The members of a closed envelope, of its `specification`, and of its
/// `body`, each in the unsigned byte order of their names.
pub(crate) const ENVELOPE: [&str; 5] = [
    "body",
    "integrityDigest",
    "kind",
    "schemaRevision",
    "specification",
];
pub(crate) const SPECIFICATION: [&str; 2] = ["identity", "revision"];
pub(crate) const BODY: [&str; 3] = ["content", "identity", "revision"];

/// The eight kinds the collection admits: the seven Policy kinds and the
/// Target Profile. They are declared in the unsigned byte order of their
/// spellings, so the derived order is the one tuples are ordered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ArtifactKind {
    ApprovalPolicy,
    GlossarySet,
    ProviderRoutingPolicy,
    ResourceLimitPolicy,
    SelectionPolicy,
    SourceAdmissionPolicy,
    TargetProfile,
    TrustPolicy,
}

impl ArtifactKind {
    pub(crate) const ALL: [Self; 8] = [
        Self::ApprovalPolicy,
        Self::GlossarySet,
        Self::ProviderRoutingPolicy,
        Self::ResourceLimitPolicy,
        Self::SelectionPolicy,
        Self::SourceAdmissionPolicy,
        Self::TargetProfile,
        Self::TrustPolicy,
    ];

    pub(crate) const fn spelling(self) -> &'static str {
        match self {
            Self::ApprovalPolicy => "approval-policy",
            Self::GlossarySet => "glossary-set",
            Self::ProviderRoutingPolicy => "provider-routing-policy",
            Self::ResourceLimitPolicy => "resource-limit-policy",
            Self::SelectionPolicy => "selection-policy",
            Self::SourceAdmissionPolicy => "source-admission-policy",
            Self::TargetProfile => "target-profile",
            Self::TrustPolicy => "trust-policy",
        }
    }

    /// The kind `spelling` names exactly, when it names one of the eight.
    pub(crate) fn from_spelling(spelling: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.spelling() == spelling)
    }

    /// The design whose specification governs the kind's body. A member
    /// establishes a tuple only when its `specification` names this design;
    /// which of its revisions is supported is decided after.
    pub(crate) const fn governing_specification(self) -> &'static str {
        match self {
            Self::ApprovalPolicy | Self::SelectionPolicy => "intlify-design-021",
            Self::SourceAdmissionPolicy | Self::ResourceLimitPolicy | Self::TrustPolicy => {
                "intlify-design-018"
            }
            Self::ProviderRoutingPolicy | Self::GlossarySet => "intlify-design-022",
            Self::TargetProfile => "intlify-design-024",
        }
    }

    /// Whether this is one of the seven Policy kinds.
    pub(crate) const fn is_policy(self) -> bool {
        !matches!(self, Self::TargetProfile)
    }

    /// The domain the kind's semantic digest is computed under.
    pub(crate) const fn semantic_domain(self) -> &'static str {
        if self.is_policy() {
            POLICY_DOMAIN
        } else {
            TARGET_PROFILE_DOMAIN
        }
    }
}

/// What a member declares about its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Declaration {
    /// The member is not an object whose `kind` is a string.
    NoKind,
    /// It declares a kind the collection does not admit, or an admitted kind
    /// with another `schemaRevision` string.
    Unsupported,
    /// It declares an admitted kind. One whose `schemaRevision` is missing or
    /// not a string still establishes no tuple.
    Admitted(ArtifactKind),
}

/// An exact reference: the kind, identity, revision, specification revision,
/// and semantic digest. Tuples order as 017 orders them: part by part, by the
/// unsigned bytes of each spelling.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ExactTuple {
    pub(crate) kind: ArtifactKind,
    pub(crate) identity: Token,
    pub(crate) revision: Token,
    pub(crate) specification_revision: Token,
    pub(crate) semantic_digest: SemanticDigest,
}

/// A collection member past 015's pre-invocation boundary: a JSON value with
/// no number in it, and its canonical submitted body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormalMember {
    value: Value,
    canonical: Vec<u8>,
}

impl FormalMember {
    /// The member of `value`, framed once. Every later digest frames a part
    /// of it, which therefore stays within the shared framing's capacity.
    pub(crate) fn new(value: Value) -> Result<Self, EncodingFailure> {
        let canonical = encode(&value)?;
        Ok(Self { value, canonical })
    }

    pub(crate) const fn value(&self) -> &Value {
        &self.value
    }

    /// `C(member)`: the canonical value framing of the complete member,
    /// `integrityDigest` included. Two members are the same submitted body
    /// exactly when these bytes are equal, whatever their transport spelling.
    /// Members that share one tuple order by them, and a member without a tuple
    /// has them as its admission key; both orders compare unsigned bytes with a
    /// shorter equal prefix first, as byte slices do.
    pub(crate) fn canonical_body(&self) -> &[u8] {
        &self.canonical
    }

    /// The canonical bytes 015's `artifactAdmission` bounds count.
    pub(crate) fn canonical_bytes(&self) -> u64 {
        count(self.canonical.len())
    }

    pub(crate) fn declaration(&self) -> Declaration {
        // A member that is not an object has no `kind` to read.
        let Some(kind) = self.value.get("kind").and_then(Value::as_str) else {
            return Declaration::NoKind;
        };
        let revision = self.value.get("schemaRevision").and_then(Value::as_str);
        match ArtifactKind::from_spelling(kind) {
            None => Declaration::Unsupported,
            Some(_) if revision.is_some_and(|revision| revision != SCHEMA_REVISION) => {
                Declaration::Unsupported
            }
            Some(kind) => Declaration::Admitted(kind),
        }
    }

    /// The envelope this member is, when it establishes its exact-reference
    /// tuple: a closed `LocalizationArtifact` of an admitted kind with schema
    /// revision "0", governed by that kind's specification, whose body is
    /// exactly `identity`, `revision`, and an object `content`. Neither the
    /// integrity digest's value nor the content is checked, so a member whose
    /// integrity or content later fails keeps its tuple.
    pub(crate) fn envelope(&self) -> Option<Envelope<'_>> {
        let Declaration::Admitted(kind) = self.declaration() else {
            return None;
        };
        let envelope = closed(&self.value, &ENVELOPE)?;
        if envelope["schemaRevision"] != SCHEMA_REVISION {
            return None;
        }
        let specification = closed(&envelope["specification"], &SPECIFICATION)?;
        let governing = token(&specification["identity"])?;
        let specification_revision = token(&specification["revision"])?;
        if governing.as_str() != kind.governing_specification() {
            return None;
        }
        let body = closed(&envelope["body"], &BODY)?;
        let identity = token(&body["identity"])?;
        let revision = token(&body["revision"])?;
        let content = body["content"].as_object()?;
        let integrity_digest = envelope["integrityDigest"]
            .as_str()
            .filter(|digest| digest.strip_prefix("sha256:").is_some_and(valid_hex256))?;
        let semantic_digest = semantic_digest(kind, &specification_revision, content);
        Some(Envelope {
            member: self,
            tuple: ExactTuple {
                kind,
                identity,
                revision,
                specification_revision,
                semantic_digest,
            },
            content,
            integrity_digest,
        })
    }
}

/// `H(the kind's domain, { kind, specificationRevision, content })` over the
/// submitted content: the name and revision are left out, so the digest pins
/// the content rather than what it is called.
fn semantic_digest(
    kind: ArtifactKind,
    specification_revision: &Token,
    content: &Map<String, Value>,
) -> SemanticDigest {
    let mut preimage = Map::new();
    preimage.insert("kind".into(), kind.spelling().into());
    preimage.insert(
        "specificationRevision".into(),
        specification_revision.as_str().into(),
    );
    preimage.insert("content".into(), Value::Object(content.clone()));
    hash(
        Domain::literal(kind.semantic_domain()),
        &Value::Object(preimage),
    )
    .map(SemanticDigest::from_hash)
    .expect("the boundary framed the whole member, so a part of it frames")
}

/// Why the body of a member that establishes a tuple is not admitted, in the
/// order body admission meets them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyFailure {
    /// The integrity digest is not the digest of the rest of the member.
    IntegrityInvalid,
    /// The body is under a revision of its governing specification other than
    /// the one supported, for which no content is defined.
    SpecificationRevisionUnsupported,
    /// The content is not the kind's closed content.
    ContentInvalid,
}

/// A member that establishes its exact-reference tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Envelope<'a> {
    member: &'a FormalMember,
    tuple: ExactTuple,
    content: &'a Map<String, Value>,
    integrity_digest: &'a str,
}

impl<'a> Envelope<'a> {
    pub(crate) const fn tuple(&self) -> &ExactTuple {
        &self.tuple
    }

    pub(crate) const fn member(&self) -> &FormalMember {
        self.member
    }

    /// The submitted content, which the owning design admits only after the
    /// integrity holds.
    pub(crate) const fn content(&self) -> &Map<String, Value> {
        self.content
    }

    /// Whether the integrity digest is `H("localization-artifact-integrity",
    /// member)` with only the member's own top-level `integrityDigest` left
    /// out.
    pub(crate) fn integrity_holds(&self) -> bool {
        hash_with_excluded_member(
            Domain::literal(INTEGRITY_DOMAIN),
            &self.member.value,
            &["integrityDigest"],
        )
        .map(IntegrityDigest::from_hash)
        .expect("the boundary framed the whole member, so a part of it frames")
        .as_str()
            == self.integrity_digest
    }

    /// Body admission: the integrity first, then the specification revision,
    /// and only then the content, which is read as the kind's closed content
    /// where 017 defines one. The content of a Resource Limit Policy, a Trust
    /// Policy, or a Glossary Set is 018's or 022's to admit and passes on as
    /// the object it is.
    pub(crate) fn admit(&self) -> Result<Content<'a>, BodyFailure> {
        if !self.integrity_holds() {
            return Err(BodyFailure::IntegrityInvalid);
        }
        if self.tuple.specification_revision.as_str() != SPECIFICATION_REVISION {
            return Err(BodyFailure::SpecificationRevisionUnsupported);
        }
        content::read(self.tuple.kind, self.content).ok_or(BodyFailure::ContentInvalid)
    }
}

/// The member with its integrity digest computed over the rest, or as it is
/// when it has no top-level `integrityDigest` to fill in.
#[cfg(any(test, feature = "vectors", feature = "benchmark"))]
pub(crate) fn sealed(mut member: Value) -> Value {
    if member.get("integrityDigest").is_some() {
        let digest = hash_with_excluded_member(
            Domain::literal(INTEGRITY_DOMAIN),
            &member,
            &["integrityDigest"],
        )
        .map(IntegrityDigest::from_hash)
        .expect("a fixture member frames");
        member["integrityDigest"] = Value::from(digest.as_str());
    }
    member
}

/// A sealed envelope of `kind` under its governing specification's revision
/// "0".
#[cfg(any(test, feature = "vectors", feature = "benchmark"))]
pub(crate) fn envelope_of(
    kind: ArtifactKind,
    identity: &str,
    revision: &str,
    content: Value,
) -> Value {
    let mut member = serde_json::json!({
        "kind": kind.spelling(),
        "schemaRevision": SCHEMA_REVISION,
        "specification": {"identity": kind.governing_specification(), "revision": "0"},
        "body": {"identity": identity, "revision": revision},
        "integrityDigest": IntegrityDigest::from_hash([0; 32]).as_str(),
    });
    member["body"]["content"] = content;
    sealed(member)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::artifacts::content::LocalizedApproval;
    use crate::references::{PolicyReference, TargetProfileReference};

    fn member(value: Value) -> FormalMember {
        FormalMember::new(value).unwrap()
    }

    /// Body admission of `value`, which establishes a tuple, without the
    /// content it reads.
    fn admission(value: Value) -> Result<(), BodyFailure> {
        member(value).envelope().unwrap().admit().map(|_| ())
    }

    fn approval() -> Value {
        envelope_of(
            ArtifactKind::ApprovalPolicy,
            "storefront-approval",
            "1",
            json!({"localizedApproval": "required"}),
        )
    }

    fn tuple_of(value: Value) -> Option<ExactTuple> {
        member(value)
            .envelope()
            .map(|envelope| envelope.tuple().clone())
    }

    fn digest_of(value: Value) -> SemanticDigest {
        tuple_of(value).unwrap().semantic_digest
    }

    /// `value` with the member at `pointer` replaced, or removed for `None`.
    /// The integrity digest is left as it was.
    fn edited(value: &Value, pointer: &str, replacement: Option<Value>) -> Value {
        let mut edited = value.clone();
        let (parent, name) = pointer.rsplit_once('/').unwrap();
        let object = edited.pointer_mut(parent).unwrap().as_object_mut().unwrap();
        match replacement {
            Some(replacement) => object.insert(name.into(), replacement),
            None => object.remove(name),
        };
        edited
    }

    #[test]
    fn kinds_are_spelled_governed_and_ordered_as_017_registers_them() {
        let spellings = ArtifactKind::ALL.map(ArtifactKind::spelling);
        assert_eq!(
            spellings,
            [
                "approval-policy",
                "glossary-set",
                "provider-routing-policy",
                "resource-limit-policy",
                "selection-policy",
                "source-admission-policy",
                "target-profile",
                "trust-policy",
            ]
        );
        // The derived order is the order of the spellings' bytes.
        assert!(ArtifactKind::ALL.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(spellings
            .windows(2)
            .all(|pair| pair[0].as_bytes() < pair[1].as_bytes()));
        for kind in ArtifactKind::ALL {
            assert_eq!(ArtifactKind::from_spelling(kind.spelling()), Some(kind));
        }
        for other in [
            "Approval-Policy",
            "approval-policy ",
            "localized-message",
            "",
        ] {
            assert_eq!(ArtifactKind::from_spelling(other), None);
        }
        assert_eq!(
            ArtifactKind::ALL.map(ArtifactKind::governing_specification),
            [
                "intlify-design-021",
                "intlify-design-022",
                "intlify-design-022",
                "intlify-design-018",
                "intlify-design-021",
                "intlify-design-018",
                "intlify-design-024",
                "intlify-design-018",
            ]
        );
        assert_eq!(
            ArtifactKind::ALL.map(ArtifactKind::semantic_domain),
            [
                POLICY_DOMAIN,
                POLICY_DOMAIN,
                POLICY_DOMAIN,
                POLICY_DOMAIN,
                POLICY_DOMAIN,
                POLICY_DOMAIN,
                TARGET_PROFILE_DOMAIN,
                POLICY_DOMAIN,
            ]
        );
        assert_ne!(POLICY_DOMAIN, TARGET_PROFILE_DOMAIN);
        assert_ne!(POLICY_DOMAIN, INTEGRITY_DOMAIN);
    }

    #[test]
    fn a_kind_is_declared_only_by_an_object_with_a_string_kind() {
        for value in [
            json!(["approval-policy"]),
            json!("approval-policy"),
            json!(null),
            json!({"schemaRevision": "0"}),
            json!({"kind": null, "schemaRevision": "0"}),
            json!({"kind": ["approval-policy"], "schemaRevision": "0"}),
        ] {
            let member = member(value);
            assert_eq!(
                member.declaration(),
                Declaration::NoKind,
                "{:?}",
                member.value()
            );
            assert!(member.envelope().is_none());
        }
    }

    #[test]
    fn another_kind_or_schema_revision_string_is_unsupported() {
        let unsupported = [
            edited(&approval(), "/kind", Some(json!("localized-message"))),
            edited(
                &approval(),
                "/kind",
                Some(json!("intent-surface-class-vocabulary")),
            ),
            edited(&approval(), "/kind", Some(json!("Approval-Policy"))),
            edited(&approval(), "/schemaRevision", Some(json!("1"))),
            edited(&approval(), "/schemaRevision", Some(json!(""))),
        ];
        for value in unsupported {
            let member = member(value);
            assert_eq!(
                member.declaration(),
                Declaration::Unsupported,
                "{:?}",
                member.value()
            );
            assert!(member.envelope().is_none());
        }
        // An admitted kind without a schema revision string is still declared,
        // but it is no envelope.
        for revision in [None, Some(json!(null)), Some(json!(["0"]))] {
            let member = member(edited(&approval(), "/schemaRevision", revision));
            assert_eq!(
                member.declaration(),
                Declaration::Admitted(ArtifactKind::ApprovalPolicy)
            );
            assert!(member.envelope().is_none());
        }
    }

    #[test]
    fn every_admitted_kind_establishes_its_tuple() {
        for kind in ArtifactKind::ALL {
            let value = envelope_of(kind, "storefront", "7", json!({"note": kind.spelling()}));
            let member = member(value);
            assert_eq!(member.declaration(), Declaration::Admitted(kind));
            let envelope = member.envelope().unwrap();
            let tuple = envelope.tuple();
            assert_eq!(tuple.kind, kind);
            assert_eq!(tuple.identity.as_str(), "storefront");
            assert_eq!(tuple.revision.as_str(), "7");
            assert_eq!(tuple.specification_revision.as_str(), "0");
            assert_eq!(
                envelope.content(),
                json!({"note": kind.spelling()}).as_object().unwrap()
            );
            assert!(envelope.integrity_holds());
        }
    }

    #[test]
    fn a_missing_or_malformed_envelope_member_establishes_no_tuple() {
        let valid = approval();
        assert!(tuple_of(valid.clone()).is_some());
        let mut cases: Vec<(&str, Option<Value>)> = ENVELOPE
            .iter()
            .filter(|member| **member != "kind")
            .map(|member| (*member, None))
            .collect();
        cases.extend([
            ("/description", Some(json!("x"))),
            ("/specification/identity", None),
            ("/specification/revision", None),
            ("/specification/note", Some(json!("x"))),
            ("/specification/identity", Some(json!("intlify-design-018"))),
            ("/specification/identity", Some(json!("Intlify-Design-021"))),
            ("/specification/revision", Some(json!(""))),
            ("/specification", Some(json!("intlify-design-021"))),
            ("/body/identity", None),
            ("/body/revision", None),
            ("/body/content", None),
            ("/body/note", Some(json!("x"))),
            ("/body/identity", Some(json!("Storefront"))),
            ("/body/revision", Some(json!(["1"]))),
            ("/body/content", Some(json!(["required"]))),
            ("/body/content", Some(json!("required"))),
            ("/body/content", Some(json!(null))),
            ("/body", Some(json!([]))),
            ("/integrityDigest", Some(json!("sha256:00"))),
            ("/integrityDigest", Some(json!(null))),
        ]);
        for (pointer, replacement) in cases {
            let pointer = if pointer.starts_with('/') {
                pointer.to_owned()
            } else {
                format!("/{pointer}")
            };
            let value = edited(&valid, &pointer, replacement.clone());
            assert_eq!(tuple_of(value), None, "{pointer} {replacement:?}");
        }
    }

    #[test]
    fn a_failing_integrity_or_content_keeps_the_tuple() {
        let valid = approval();
        let tuple = tuple_of(valid.clone()).unwrap();
        // Changing the content after sealing leaves a stale integrity digest.
        let mut stale = valid.clone();
        stale["body"]["content"]["localizedApproval"] = json!("not-required");
        let stale = member(stale);
        let envelope = stale.envelope().unwrap();
        assert!(!envelope.integrity_holds());
        assert_eq!(envelope.tuple().identity, tuple.identity);
        assert_eq!(
            envelope.admit().map(|_| ()),
            Err(BodyFailure::IntegrityInvalid)
        );
        // A content no approval policy has fails only at body admission.
        let invalid = member(sealed(edited(
            &valid,
            "/body/content",
            Some(json!({"localizedApproval": "sometimes"})),
        )));
        let envelope = invalid.envelope().unwrap();
        assert!(envelope.integrity_holds());
        assert_eq!(envelope.tuple().kind, ArtifactKind::ApprovalPolicy);
        assert_eq!(
            envelope.admit().map(|_| ()),
            Err(BodyFailure::ContentInvalid)
        );
        // A revision of the governing specification other than "0" is kept.
        let revised = tuple_of(edited(&valid, "/specification/revision", Some(json!("1"))));
        assert_eq!(revised.unwrap().specification_revision.as_str(), "1");
    }

    #[test]
    fn every_admitted_kind_admits_its_closed_content() {
        for kind in ArtifactKind::ALL {
            let value = envelope_of(kind, "storefront", "1", content::sample(kind));
            assert_eq!(admission(value), Ok(()), "{kind:?}");
        }
        let approval = member(approval());
        assert_eq!(
            approval.envelope().unwrap().admit(),
            Ok(Content::Approval(LocalizedApproval::Required))
        );
        // A Glossary Set's content, which 022 admits, passes on as the
        // submitted object; a Trust Policy's is 018's closed body.
        let glossary = member(envelope_of(
            ArtifactKind::GlossarySet,
            "glossary",
            "1",
            json!({"roots": ["a"]}),
        ));
        let envelope = glossary.envelope().unwrap();
        assert_eq!(envelope.admit(), Ok(Content::External(envelope.content())));
        let trust = member(envelope_of(
            ArtifactKind::TrustPolicy,
            "trust",
            "1",
            json!({"roots": ["a"]}),
        ));
        assert_eq!(
            trust.envelope().unwrap().admit(),
            Err(BodyFailure::ContentInvalid)
        );
    }

    #[test]
    fn body_admission_checks_the_integrity_then_the_specification_revision_then_the_content() {
        let valid = approval();
        let invalid_content = Some(json!({"localizedApproval": "sometimes"}));
        let revised = Some(json!("1"));
        assert_eq!(admission(valid.clone()), Ok(()));
        assert_eq!(
            admission(sealed(edited(
                &valid,
                "/body/content",
                invalid_content.clone()
            ))),
            Err(BodyFailure::ContentInvalid)
        );
        assert_eq!(
            admission(sealed(edited(
                &valid,
                "/specification/revision",
                revised.clone()
            ))),
            Err(BodyFailure::SpecificationRevisionUnsupported)
        );
        assert_eq!(
            admission(edited(&valid, "/body/revision", Some(json!("2")))),
            Err(BodyFailure::IntegrityInvalid)
        );
        // No content is read under a revision that defines none.
        let both = sealed(edited(
            &edited(&valid, "/specification/revision", revised),
            "/body/content",
            invalid_content,
        ));
        assert_eq!(
            admission(both.clone()),
            Err(BodyFailure::SpecificationRevisionUnsupported)
        );
        // And nothing is read past an integrity that fails.
        let mut stale = both;
        stale["body"]["identity"] = json!("renamed-approval");
        assert_eq!(admission(stale), Err(BodyFailure::IntegrityInvalid));
    }

    #[test]
    fn only_specification_revision_zero_is_admitted_for_every_kind() {
        for kind in ArtifactKind::ALL {
            let base = envelope_of(kind, "storefront", "1", content::sample(kind));
            for revision in ["1", "00", "0.0", "0-rc"] {
                let value = sealed(edited(
                    &base,
                    "/specification/revision",
                    Some(json!(revision)),
                ));
                // The tuple keeps the revision, so 015 can tell a reference
                // which revision a candidate is under.
                let tuple = tuple_of(value.clone()).unwrap();
                assert_eq!(tuple.specification_revision.as_str(), revision);
                assert_eq!(
                    admission(value),
                    Err(BodyFailure::SpecificationRevisionUnsupported),
                    "{kind:?} {revision}"
                );
            }
        }
    }

    #[test]
    fn the_integrity_covers_the_member_but_its_own_digest() {
        let valid = approval();
        assert!(member(valid.clone()).envelope().unwrap().integrity_holds());
        for (pointer, value) in [
            ("/body/identity", json!("other-approval")),
            ("/body/revision", json!("2")),
            ("/specification/revision", json!("1")),
            ("/body/content/localizedApproval", json!("not-required")),
        ] {
            let mut edited = valid.clone();
            *edited.pointer_mut(pointer).unwrap() = value;
            assert!(
                !member(edited).envelope().unwrap().integrity_holds(),
                "{pointer}"
            );
        }
        // A nested `integrityDigest` is content like any other member.
        let nested = envelope_of(
            ArtifactKind::TrustPolicy,
            "trust",
            "1",
            json!({"integrityDigest": "sha256:not-mine"}),
        );
        assert!(member(nested.clone()).envelope().unwrap().integrity_holds());
        let mut changed = nested;
        changed["body"]["content"]["integrityDigest"] = json!("sha256:changed");
        assert!(!member(changed).envelope().unwrap().integrity_holds());
    }

    #[test]
    fn the_semantic_digest_pins_kind_specification_revision_and_content_only() {
        let content = json!({"note": "shared"});
        let base = envelope_of(ArtifactKind::TrustPolicy, "trust", "1", content.clone());
        let digest = digest_of(base.clone());
        // The name, the revision, and the integrity are not part of it.
        for (pointer, value) in [
            ("/body/identity", json!("renamed-trust")),
            ("/body/revision", json!("2")),
        ] {
            assert_eq!(
                digest_of(edited(&base, pointer, Some(value))),
                digest,
                "{pointer}"
            );
        }
        let mut stale = base.clone();
        stale["integrityDigest"] = json!(IntegrityDigest::from_hash([7; 32]).as_str());
        assert_eq!(digest_of(stale), digest);
        // The kind, the specification revision, and the content are.
        assert_ne!(
            digest_of(envelope_of(
                ArtifactKind::GlossarySet,
                "trust",
                "1",
                content.clone()
            )),
            digest
        );
        assert_ne!(
            digest_of(edited(&base, "/specification/revision", Some(json!("1")))),
            digest
        );
        assert_ne!(
            digest_of(edited(
                &base,
                "/body/content",
                Some(json!({"note": "other"}))
            )),
            digest
        );
        // The Target Profile's digest is in its own domain.
        let target = envelope_of(ArtifactKind::TargetProfile, "web", "1", content.clone());
        let preimage =
            json!({"kind": "target-profile", "specificationRevision": "0", "content": content});
        assert_eq!(
            digest_of(target),
            SemanticDigest::from_hash(
                hash(Domain::literal(TARGET_PROFILE_DOMAIN), &preimage).unwrap()
            )
        );
    }

    #[test]
    fn the_canonical_body_ignores_transport_spelling_and_keeps_the_integrity_digest() {
        let valid = approval();
        let compact = serde_json::to_string(&valid).unwrap();
        let spaced = serde_json::to_string_pretty(&valid).unwrap();
        // The same members in another order, and with one character escaped.
        let escaped = compact.replacen(
            "\"revision\":\"1\"",
            &format!("\"revision\":\"{}u0031\"", '\\'),
            1,
        );
        let reordered = format!(
            "{{\"integrityDigest\":{},\"body\":{},\"specification\":{},\"schemaRevision\":\"0\",\"kind\":\"approval-policy\"}}",
            valid["integrityDigest"], valid["body"], valid["specification"]
        );
        let canonical = |text: &str| {
            let value = intlify_shared_json::decode::value(text.as_bytes(), true).unwrap();
            member(value).canonical_body().to_vec()
        };
        let expected = canonical(&compact);
        for text in [&spaced, &escaped, &reordered] {
            assert_eq!(canonical(text), expected, "{text}");
        }
        assert_eq!(expected, encode(&valid).unwrap());
        // The digest and the content are both in it.
        let mut other_digest = valid.clone();
        other_digest["integrityDigest"] = json!(IntegrityDigest::from_hash([7; 32]).as_str());
        let other_content = edited(
            &valid,
            "/body/content",
            Some(json!({"localizedApproval": "not-required"})),
        );
        for other in [other_digest, other_content] {
            assert_ne!(member(other).canonical_body(), expected.as_slice());
        }
        let framed = member(valid);
        assert_eq!(framed.canonical_bytes(), count(expected.len()));
    }

    #[test]
    fn canonical_bodies_order_by_their_unsigned_bytes() {
        let bodies = [json!("b"), json!("a"), json!("ab"), json!(["a"]), json!({})]
            .map(|value| member(value).canonical_body().to_vec());
        let mut sorted = bodies.to_vec();
        sorted.sort();
        // A string frame (tag 4) comes before an array (5) and an object (6).
        // Among strings, the length in the frame's header puts a shorter one
        // first, then the bytes decide.
        assert_eq!(
            sorted,
            vec![
                bodies[1].clone(),
                bodies[0].clone(),
                bodies[2].clone(),
                bodies[3].clone(),
                bodies[4].clone(),
            ]
        );
    }

    #[test]
    fn tuples_order_by_each_part_in_turn() {
        let tuple =
            |kind, identity: &str, revision: &str, specification: &str, digest: u8| ExactTuple {
                kind,
                identity: Token::new(identity).unwrap(),
                revision: Token::new(revision).unwrap(),
                specification_revision: Token::new(specification).unwrap(),
                semantic_digest: SemanticDigest::from_hash([digest; 32]),
            };
        let ordered = [
            tuple(ArtifactKind::ApprovalPolicy, "z", "9", "9", 9),
            tuple(ArtifactKind::GlossarySet, "a", "1", "0", 1),
            tuple(ArtifactKind::GlossarySet, "b", "1", "0", 1),
            tuple(ArtifactKind::GlossarySet, "b", "10", "0", 1),
            tuple(ArtifactKind::GlossarySet, "b", "2", "0", 1),
            tuple(ArtifactKind::GlossarySet, "b", "2", "1", 0),
            tuple(ArtifactKind::GlossarySet, "b", "2", "1", 1),
        ];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
    }

    fn reference(kind: &str, identity: &str, digest: &SemanticDigest) -> Value {
        json!({
            "kind": kind,
            "identity": identity,
            "revision": "1",
            "specificationRevision": "0",
            "semanticDigest": digest.as_str(),
        })
    }

    #[test]
    fn a_reference_names_the_tuple_an_envelope_establishes() {
        let tuple = tuple_of(approval()).unwrap();
        let named: PolicyReference = serde_json::from_value(reference(
            "approval-policy",
            "storefront-approval",
            &tuple.semantic_digest,
        ))
        .unwrap();
        assert_eq!(named.tuple(), tuple);
        let renamed: PolicyReference = serde_json::from_value(reference(
            "approval-policy",
            "other-approval",
            &tuple.semantic_digest,
        ))
        .unwrap();
        assert_ne!(renamed.tuple(), tuple);
        // Each Policy kind names its own artifact kind, and a Target Profile
        // reference names the Target Profile.
        for kind in ArtifactKind::ALL
            .into_iter()
            .filter(|kind| kind.is_policy())
        {
            let named: PolicyReference = serde_json::from_value(reference(
                kind.spelling(),
                "policy",
                &tuple.semantic_digest,
            ))
            .unwrap();
            assert_eq!(named.tuple().kind, kind);
        }
        let target: TargetProfileReference =
            serde_json::from_value(reference("target-profile", "web", &tuple.semantic_digest))
                .unwrap();
        assert_eq!(target.tuple().kind, ArtifactKind::TargetProfile);
        assert_eq!(target.tuple().identity.as_str(), "web");
    }

    #[test]
    fn a_reference_has_the_canonical_bytes_of_its_value_whatever_its_spelling() {
        let value = reference(
            "selection-policy",
            "selection",
            &SemanticDigest::from_hash([3; 32]),
        );
        let expected = count(encode(&value).unwrap().len());
        for text in [
            serde_json::to_string(&value).unwrap(),
            serde_json::to_string_pretty(&value).unwrap(),
        ] {
            let named: PolicyReference = serde_json::from_str(&text).unwrap();
            assert_eq!(named.canonical_bytes(), expected);
        }
        let target: TargetProfileReference = serde_json::from_value(reference(
            "target-profile",
            "web",
            &SemanticDigest::from_hash([3; 32]),
        ))
        .unwrap();
        assert_eq!(
            target.canonical_bytes(),
            count(
                encode(&serde_json::to_value(&target).unwrap())
                    .unwrap()
                    .len()
            )
        );
    }
}
