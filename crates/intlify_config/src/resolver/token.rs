// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Redacted subject token: the projected logical subject of a
//! unit's subject, the token record that frames it, and the token itself,
//! with the collision ordinals that keep unequal records with one digest
//! apart (D15:2179-2194, 2454-2468).
//!
//! The projected logical subject follows the key strategy the Check Registry
//! gives the subject's kind. A singleton is its subject kind ID (Decision
//! 015-178), and a logical field is its path of schema member names. A
//! subject with an admitted canonical identity is `{ "identity": value }`.
//! A subject without one is `{ "content": { "type": t } }` under
//! `redacted-structure`, the one projection of revision `"0"` (Decision
//! 015-174). A semantic position is its zero-based ordinal as a decimal string
//! (Decision 015-179). A subject the registry scopes by a parent is
//! `{ "parent": p, "subject": s }`. A compound identity takes the view 015 or
//! 017 gives it, and a collection member that establishes an exact tuple is
//! written without its semantic digest, as its conflict group is (Decisions
//! 015-175 and 015-177).
//!
//! A token is SHA-256 over the domain separator followed by the token record.
//! The record holds the Profile Specification identity and revision, the
//! phase ID, the subject kind ID, the owning scope, the key strategy, the
//! projection ID, and `C` of the projected logical subject, each one field
//! frame. Unequal records of one phase and subject kind with one digest form a
//! colliding set. Each member gets an ordinal from 0 in ascending byte order
//! of its record, and each pair counts as one comparison under
//! `maxCollisionComparisons` (Decision 015-176). The digest is a parameter, so
//! a test can make chosen records collide.
//!
//! The evaluation keys each unit by its [`logical`] subject: the raw subject
//! with the content its projection drops replaced, so raw values with one
//! projection join one unit as candidates. Nothing records a token yet.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use intlify_shared_json::encoding::{digest_bytes, encode};
use intlify_shared_json::token::{hex, Token};
use serde_json::{json, Value};

use crate::artifacts::envelope::{AdmissionKey, ExactTuple, MemberKey};
use crate::artifacts::SemanticDigest;
use crate::construction::profile::SUBJECT_TOKEN_DOMAIN;
use crate::construction::registry::check::{key_shape, kind_id};
use crate::locale::CanonicalLocale;
use crate::model::{DeploymentGroupId, TargetId};

use super::artifact_resolution::{CollectionSubject, ConflictKey};
use super::coverage::{CoverageSubject, Domain, RuleContent, SurfaceKey};
use super::evaluation::Subject;
use super::fallback::FallbackSubject;
use super::groups::GroupSubject;
use super::hydration::{EndpointField, HydrationSubject};
use super::locales::{Content, LocaleKey, LocaleSubject};
use super::negotiation::NegotiationSubject;
use super::resource_policy::{ArtifactSubject, PolicySubject, ReferenceField};
use super::subject::SubjectKind;
use super::targets::TargetSubject;

/// The Profile Specification a token belongs to: the first two fields of its
/// record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Specification<'a> {
    pub(crate) identity: &'a str,
    pub(crate) revision: &'a str,
}

/// A subject's own key, before a parent scopes it.
#[derive(Debug)]
enum Own {
    /// A singleton, which is its subject kind ID.
    Singleton,
    /// A logical field: its path of schema member names.
    Field(Vec<&'static str>),
    /// An admitted canonical identity, in its view.
    Identity(Value),
    /// A value without an identity, which `redacted-structure` projects to
    /// its JSON type.
    Content(&'static str),
    /// A position in a semantic sequence, counted from 0.
    Position(usize),
}

/// The projected logical subject of `subject` (D15:2468).
pub(crate) fn projected(subject: &Subject) -> Value {
    let (own, parent) = own(subject);
    let own = match own {
        Own::Singleton => Value::String(kind_id(subject.kind())),
        Own::Field(path) => Value::Array(
            path.into_iter()
                .map(|name| Value::String(name.to_owned()))
                .collect(),
        ),
        Own::Identity(value) => json!({ "identity": value }),
        Own::Content(json_type) => json!({ "content": { "type": json_type } }),
        Own::Position(ordinal) => Value::String(ordinal.to_string()),
    };
    match parent {
        Some(parent) => json!({ "parent": projected(&parent), "subject": own }),
        None => own,
    }
}

/// The logical subject of `subject`: the subject itself, with every value it
/// keys by content replaced by the one value that stands for the value's JSON
/// type, and a collection member's semantic digest dropped. Two subjects have
/// one logical subject exactly when they have one projected logical subject,
/// so a unit over the logical subject is 015's evaluation unit, which every
/// raw value with that projection joins as a candidate (D15:2203).
pub(crate) fn logical(subject: &Subject) -> Subject {
    match subject {
        Subject::Slot(_)
        | Subject::Vocabulary
        | Subject::CanonicalizationData
        | Subject::DeclarationField(_)
        | Subject::ArtifactReference(_)
        | Subject::DeliveryPlacement
        | Subject::Target(_)
        | Subject::Group(_)
        | Subject::Hydration(_)
        | Subject::SemanticDependency(_)
        | Subject::ArtifactCollection(CollectionSubject::Group(_)) => subject.clone(),
        Subject::ResourcePolicy(policy) => Subject::ResourcePolicy(match policy {
            PolicySubject::Capability(ArtifactSubject::Artifact(key)) => {
                PolicySubject::Capability(ArtifactSubject::Artifact(logical_member(key)))
            }
            PolicySubject::Recheck(ArtifactSubject::Artifact(key)) => {
                PolicySubject::Recheck(ArtifactSubject::Artifact(logical_member(key)))
            }
            other => other.clone(),
        }),
        Subject::Locale(locale) => Subject::Locale(logical_locale_subject(locale)),
        Subject::Negotiation(negotiation) => Subject::Negotiation(match negotiation {
            NegotiationSubject::Aliases => NegotiationSubject::Aliases,
            NegotiationSubject::Key(key) => NegotiationSubject::Key(logical_locale(key)),
            NegotiationSubject::Destination(key, destination) => {
                NegotiationSubject::Destination(logical_locale(key), logical_locale(destination))
            }
        }),
        Subject::Fallback(fallback) => Subject::Fallback(match fallback {
            FallbackSubject::Sources => FallbackSubject::Sources,
            FallbackSubject::Source(key) => FallbackSubject::Source(logical_locale(key)),
            FallbackSubject::Sequence(key) => FallbackSubject::Sequence(logical_locale(key)),
            FallbackSubject::Candidate(key, position) => {
                FallbackSubject::Candidate(logical_locale(key), *position)
            }
            FallbackSubject::Literal(key, literal) => {
                FallbackSubject::Literal(logical_locale(key), logical_locale(literal))
            }
            FallbackSubject::IntentSource(key) => {
                FallbackSubject::IntentSource(logical_locale(key))
            }
        }),
        Subject::Coverage(coverage) => Subject::Coverage(match coverage {
            CoverageSubject::Locale(_, key) => {
                CoverageSubject::Locale(RuleContent::projected(), logical_locale(key))
            }
            CoverageSubject::SurfaceClass(_, key) => CoverageSubject::SurfaceClass(
                RuleContent::projected(),
                match key {
                    SurfaceKey::Member(_) => key.clone(),
                    SurfaceKey::Content(_) => SurfaceKey::Content(Content::projected()),
                },
            ),
            CoverageSubject::Rule(_) => CoverageSubject::Rule(RuleContent::projected()),
            CoverageSubject::Rules
            | CoverageSubject::LocaleSelectors
            | CoverageSubject::SurfaceSelectors
            | CoverageSubject::Domain(_)
            | CoverageSubject::Cells
            | CoverageSubject::Comparisons
            | CoverageSubject::Cell(..)
            | CoverageSubject::Table => coverage.clone(),
        }),
        Subject::ArtifactCollection(CollectionSubject::Member(key)) => {
            Subject::ArtifactCollection(CollectionSubject::Member(logical_member(key)))
        }
    }
}

fn logical_locale_subject(subject: &LocaleSubject) -> LocaleSubject {
    match subject {
        LocaleSubject::RequestedLocale(key) => LocaleSubject::RequestedLocale(logical_locale(key)),
        LocaleSubject::NegotiationAliasKey(key) => {
            LocaleSubject::NegotiationAliasKey(logical_locale(key))
        }
        LocaleSubject::NegotiationAliasValue(key, value) => {
            LocaleSubject::NegotiationAliasValue(logical_locale(key), logical_locale(value))
        }
        LocaleSubject::MessageFallbackKey(key) => {
            LocaleSubject::MessageFallbackKey(logical_locale(key))
        }
        LocaleSubject::TargetRequestedLocale(target, key) => {
            LocaleSubject::TargetRequestedLocale(target.clone(), logical_locale(key))
        }
        LocaleSubject::Occurrences
        | LocaleSubject::RequestedLocales
        | LocaleSubject::DefaultSourceLocale
        | LocaleSubject::DefaultRequestedLocale
        | LocaleSubject::TargetProfiles
        | LocaleSubject::TargetId(_)
        | LocaleSubject::TargetDefaultRequestedLocale(_) => subject.clone(),
    }
}

/// A locale's canonical identity, or the one spelling that stands for every
/// spelling without one.
fn logical_locale(key: &LocaleKey) -> LocaleKey {
    match key {
        LocaleKey::Canonical(_) => key.clone(),
        LocaleKey::Content(_) => LocaleKey::Content(Content::projected()),
    }
}

/// A member's tuple without its semantic digest, or the key that stands for
/// every member of its JSON type: the key its logical subject holds.
pub(crate) fn logical_member(key: &MemberKey) -> MemberKey {
    match key {
        MemberKey::Tuple(tuple) => MemberKey::Tuple(Arc::new(ExactTuple {
            semantic_digest: SemanticDigest::from_hash([0; 32]),
            ..ExactTuple::clone(tuple)
        })),
        MemberKey::Content(content) => MemberKey::Content(content.projected()),
    }
}

/// The token record of `subject`: its eight field frames (D15:2454).
///
/// # Panics
///
/// Panics if the projected subject cannot be framed. It holds no number and
/// only identities the input bounds already limit, so that would be a defect.
pub(crate) fn record(specification: Specification<'_>, subject: &Subject) -> Vec<u8> {
    let kind = subject.kind();
    let shape = key_shape(kind);
    let projected =
        encode(&projected(subject)).expect("a projected subject is a small number-free value");
    crate::framing::Record::new()
        .field(specification.identity.as_bytes())
        .field(specification.revision.as_bytes())
        .field(kind.phase().id().as_bytes())
        .field(kind_id(kind).as_bytes())
        .field(shape.owning_scope.as_bytes())
        .field(shape.strategy.id().as_bytes())
        .field(shape.projection.id().as_bytes())
        .field(&projected)
        .finish()
}

/// The digest a token takes of its preimage: SHA-256, or in a test a digest
/// that makes chosen records collide (D15:2194).
pub(crate) trait Digest {
    fn digest(&self, preimage: &[u8]) -> [u8; 32];
}

/// SHA-256, the digest of every token outside tests.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Sha256;

impl Digest for Sha256 {
    fn digest(&self, preimage: &[u8]) -> [u8; 32] {
        digest_bytes(preimage)
    }
}

/// A Redacted token, subject or evidence: the complete digest of its record,
/// and an ordinal only when another record of its colliding set has the same
/// digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RedactedToken {
    digest: [u8; 32],
    collision: Option<u64>,
}

impl RedactedToken {
    /// The token's view, `{ token, collision? }`, the ordinal as a decimal
    /// string (D15:2452, 2468).
    pub(crate) fn view(&self) -> Value {
        let mut view = serde_json::Map::new();
        view.insert(
            "token".to_owned(),
            Value::String(format!("sha256:{}", hex(self.digest))),
        );
        if let Some(ordinal) = self.collision {
            view.insert("collision".to_owned(), Value::String(ordinal.to_string()));
        }
        Value::Object(view)
    }
}

/// The tokens of a set of subjects, and the comparisons their collisions
/// took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tokens {
    tokens: BTreeMap<Vec<u8>, RedactedToken>,
    comparisons: u64,
}

impl Tokens {
    /// The token of every distinct record among `subjects`. Subjects with one
    /// record share one token. The unequal records of one subject kind, which
    /// belongs to one phase, that share a digest are numbered from 0 in
    /// ascending byte order, before any token is handed out, and each pair of
    /// them counts as one comparison.
    pub(crate) fn assign<'a>(
        specification: Specification<'_>,
        subjects: impl IntoIterator<Item = &'a Subject>,
        digest: &impl Digest,
    ) -> Self {
        let mut sets: BTreeMap<(SubjectKind, [u8; 32]), BTreeSet<Vec<u8>>> = BTreeMap::new();
        for subject in subjects {
            let record = record(specification, subject);
            let mut preimage = SUBJECT_TOKEN_DOMAIN.as_bytes().to_vec();
            preimage.extend_from_slice(&record);
            sets.entry((subject.kind(), digest.digest(&preimage)))
                .or_default()
                .insert(record);
        }
        let (tokens, comparisons) = number(sets);
        Self {
            tokens,
            comparisons,
        }
    }

    /// The token of `subject`, if it was among the assigned subjects.
    pub(crate) fn token(
        &self,
        specification: Specification<'_>,
        subject: &Subject,
    ) -> Option<RedactedToken> {
        self.tokens.get(&record(specification, subject)).copied()
    }

    /// The comparisons that every colliding set took, one per pair.
    pub(crate) const fn comparisons(&self) -> u64 {
        self.comparisons
    }
}

/// The token of every record of `sets`, each set the records of one key that
/// share one digest, and the comparisons the sets took. A set of unequal
/// records is numbered from 0 in ascending byte order before any token is
/// handed out, and each pair in it counts as one comparison (Decision
/// 015-176). A set of one record takes no ordinal and no comparison.
pub(crate) fn number<K: Ord>(
    sets: BTreeMap<(K, [u8; 32]), BTreeSet<Vec<u8>>>,
) -> (BTreeMap<Vec<u8>, RedactedToken>, u64) {
    let mut tokens = BTreeMap::new();
    let mut comparisons = 0_u64;
    for ((_, digest), records) in sets {
        let members = u64::try_from(records.len()).expect("an in-memory count fits u64");
        comparisons = comparisons.saturating_add(members.saturating_mul(members - 1) / 2);
        let collides = members > 1;
        for (ordinal, record) in (0_u64..).zip(records) {
            tokens.insert(
                record,
                RedactedToken {
                    digest,
                    collision: collides.then_some(ordinal),
                },
            );
        }
    }
    (tokens, comparisons)
}

/// The own key of `subject` and the subject that scopes it.
fn own(subject: &Subject) -> (Own, Option<Subject>) {
    let alone = |own| (own, None);
    match subject {
        Subject::Slot(_)
        | Subject::Vocabulary
        | Subject::CanonicalizationData
        | Subject::SemanticDependency(_) => alone(Own::Singleton),
        Subject::ResourcePolicy(policy) => alone(match policy {
            PolicySubject::Capability(artifact) | PolicySubject::Recheck(artifact) => {
                match artifact {
                    ArtifactSubject::Artifacts
                    | ArtifactSubject::PolicyArtifacts
                    | ArtifactSubject::TargetProfileArtifacts
                    | ArtifactSubject::References
                    | ArtifactSubject::ArtifactBytes
                    | ArtifactSubject::ReferenceBytes => Own::Singleton,
                    ArtifactSubject::Artifact(key) => member(key),
                    ArtifactSubject::Reference(field) => reference(field),
                }
            }
            PolicySubject::ProfileId(profile) => identity(profile.as_str()),
            PolicySubject::ResourceLimits
            | PolicySubject::Nodes
            | PolicySubject::Depth
            | PolicySubject::CollectionEntries
            | PolicySubject::StringBytes
            | PolicySubject::LongestString
            | PolicySubject::Profiles
            | PolicySubject::Selector
            | PolicySubject::StructuralAnalysis => Own::Singleton,
        }),
        Subject::DeclarationField(field) => alone(Own::Field(vec![field.name()])),
        Subject::Locale(locale) => locale_subject(locale),
        Subject::Negotiation(negotiation) => match negotiation {
            NegotiationSubject::Aliases => alone(Own::Field(vec!["localeNegotiation", "aliases"])),
            NegotiationSubject::Key(key) => alone(locale_key(key)),
            NegotiationSubject::Destination(key, destination) => (
                locale_key(destination),
                Some(Subject::Negotiation(NegotiationSubject::Key(key.clone()))),
            ),
        },
        Subject::Fallback(fallback) => fallback_subject(fallback),
        Subject::Coverage(coverage) => coverage_subject(coverage),
        Subject::ArtifactCollection(collection) => alone(match collection {
            CollectionSubject::Member(key) => member(key),
            CollectionSubject::Group(group) => Own::Identity(conflict(group)),
        }),
        Subject::ArtifactReference(field) => alone(reference(field)),
        Subject::DeliveryPlacement => alone(Own::Field(vec!["delivery", "placement"])),
        Subject::Target(TargetSubject::Target(target)) => alone(identity(target.as_str())),
        Subject::Group(group) => group_subject(group),
        Subject::Hydration(hydration) => hydration_subject(hydration),
    }
}

fn locale_subject(subject: &LocaleSubject) -> (Own, Option<Subject>) {
    let under_target =
        |target: &TargetId| Some(Subject::Locale(LocaleSubject::TargetId(target.clone())));
    match subject {
        LocaleSubject::Occurrences => (Own::Singleton, None),
        LocaleSubject::RequestedLocale(key)
        | LocaleSubject::NegotiationAliasKey(key)
        | LocaleSubject::MessageFallbackKey(key) => (locale_key(key), None),
        LocaleSubject::RequestedLocales => (Own::Field(vec!["requestedLocales"]), None),
        LocaleSubject::DefaultSourceLocale => (Own::Field(vec!["defaultSourceLocale"]), None),
        LocaleSubject::DefaultRequestedLocale => (Own::Field(vec!["defaultRequestedLocale"]), None),
        LocaleSubject::NegotiationAliasValue(key, value) => (
            locale_key(value),
            Some(Subject::Locale(LocaleSubject::NegotiationAliasKey(
                key.clone(),
            ))),
        ),
        LocaleSubject::TargetProfiles => (Own::Field(vec!["targetProfiles"]), None),
        LocaleSubject::TargetId(target) => (identity(target.as_str()), None),
        LocaleSubject::TargetRequestedLocale(target, key) => {
            (locale_key(key), under_target(target))
        }
        LocaleSubject::TargetDefaultRequestedLocale(target) => (
            Own::Field(vec!["defaultRequestedLocale"]),
            under_target(target),
        ),
    }
}

fn fallback_subject(subject: &FallbackSubject) -> (Own, Option<Subject>) {
    let under =
        |source: &LocaleKey| Some(Subject::Fallback(FallbackSubject::Source(source.clone())));
    match subject {
        FallbackSubject::Sources => (Own::Field(vec!["messageFallback"]), None),
        FallbackSubject::Source(key) => (locale_key(key), None),
        // The sequence is the value of its mapping entry: the empty path
        // under that entry (D15:2184).
        FallbackSubject::Sequence(source) => (Own::Field(Vec::new()), under(source)),
        FallbackSubject::Candidate(source, position) => (Own::Position(*position), under(source)),
        FallbackSubject::Literal(source, locale) => (locale_key(locale), under(source)),
        FallbackSubject::IntentSource(source) => (
            Own::Identity(json!({ "kind": "intent-source-locale" })),
            under(source),
        ),
    }
}

fn coverage_subject(subject: &CoverageSubject) -> (Own, Option<Subject>) {
    let under = |rule: &RuleContent| Some(Subject::Coverage(CoverageSubject::Rule(rule.clone())));
    match subject {
        CoverageSubject::Rules => (Own::Field(vec!["coverage", "rules"]), None),
        CoverageSubject::LocaleSelectors
        | CoverageSubject::SurfaceSelectors
        | CoverageSubject::Cells
        | CoverageSubject::Comparisons
        | CoverageSubject::Table => (Own::Singleton, None),
        CoverageSubject::Locale(rule, key) => (locale_key(key), under(rule)),
        CoverageSubject::SurfaceClass(rule, key) => (
            match key {
                SurfaceKey::Member(surface) => identity(surface.as_str()),
                SurfaceKey::Content(_) => Own::Content("string"),
            },
            under(rule),
        ),
        // A rule has no identity before normalization, and a rule is an
        // object.
        CoverageSubject::Rule(_) => (Own::Content("object"), None),
        CoverageSubject::Domain(domain) => (Own::Identity(rule_domain(domain)), None),
        CoverageSubject::Cell(locale, surface) => (
            Own::Identity(json!({ "locale": locale.as_str(), "surfaceClass": surface.as_str() })),
            None,
        ),
    }
}

fn group_subject(subject: &GroupSubject) -> (Own, Option<Subject>) {
    let under =
        |group: &DeploymentGroupId| Some(Subject::Group(GroupSubject::Group(group.clone())));
    match subject {
        GroupSubject::Groups => (Own::Field(vec!["deploymentGroups"]), None),
        GroupSubject::Memberships => (Own::Singleton, None),
        GroupSubject::Group(group) => (identity(group.as_str()), None),
        GroupSubject::Members(group) => (Own::Field(vec!["members"]), under(group)),
        GroupSubject::Member(group, target) => (identity(target.as_str()), under(group)),
        GroupSubject::Assignment(target) => (identity(target.as_str()), None),
    }
}

fn hydration_subject(subject: &HydrationSubject) -> (Own, Option<Subject>) {
    let under =
        |group: &DeploymentGroupId| Some(Subject::Group(GroupSubject::Group(group.clone())));
    match subject {
        HydrationSubject::Relations => (Own::Singleton, None),
        HydrationSubject::Endpoint(group, server, client, field) => (
            Own::Field(vec![match field {
                EndpointField::Client => "client",
                EndpointField::Server => "server",
            }]),
            Some(Subject::Hydration(HydrationSubject::Relation(
                group.clone(),
                server.clone(),
                client.clone(),
            ))),
        ),
        HydrationSubject::Relation(group, server, client) => (
            Own::Identity(json!({ "server": server.as_str(), "client": client.as_str() })),
            under(group),
        ),
        HydrationSubject::Roles(group, target) => (identity(target.as_str()), under(group)),
    }
}

/// A single identity, which is its string.
fn identity(value: &str) -> Own {
    Own::Identity(Value::String(value.to_owned()))
}

/// A locale's canonical identity, or a spelling without one.
fn locale_key(key: &LocaleKey) -> Own {
    match key {
        LocaleKey::Canonical(locale) => identity(locale.as_str()),
        LocaleKey::Content(_) => Own::Content("string"),
    }
}

/// A collection member: the conflict-group view of its tuple, without the
/// semantic digest, or the JSON type of a member without one.
fn member(key: &MemberKey) -> Own {
    match key {
        MemberKey::Tuple(tuple) => Own::Identity(conflict(&ConflictKey::of(tuple))),
        MemberKey::Content(content) => Own::Content(json_type(content)),
    }
}

/// A reference: a `policies` slot's field path, or a target's Target ID.
fn reference(field: &ReferenceField) -> Own {
    match field {
        ReferenceField::Policy(slot) => Own::Field(vec!["policies", slot.name()]),
        ReferenceField::TargetProfile(target) => identity(target.as_str()),
    }
}

/// 017's reference object without `semanticDigest`.
fn conflict(group: &ConflictKey) -> Value {
    json!({
        "kind": group.kind.spelling(),
        "identity": group.identity.as_str(),
        "revision": group.revision.as_str(),
        "specificationRevision": group.specification_revision.as_str(),
    })
}

/// A normalized rule domain, `{ locales, surfaceClasses }`.
fn rule_domain(domain: &Domain) -> Value {
    json!({
        "locales": domain.locales().iter().map(CanonicalLocale::as_str).collect::<Vec<_>>(),
        "surfaceClasses": domain.surfaces().iter().map(Token::as_str).collect::<Vec<_>>(),
    })
}

/// The JSON type of a member's canonical submitted body, from the tag that
/// starts it: a member past the boundary holds no number.
fn json_type(content: &AdmissionKey) -> &'static str {
    match content.as_bytes().first() {
        Some(0x01) => "null",
        Some(0x02 | 0x03) => "boolean",
        Some(0x04) => "string",
        Some(0x05) => "array",
        Some(0x06) => "object",
        _ => unreachable!("a canonical submitted body is one framed value without a number"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use intlify_shared_json::token::Token;
    use serde_json::{json, Value};

    use crate::artifacts::envelope::{ArtifactKind, ExactTuple, FormalMember, MemberKey};
    use crate::artifacts::SemanticDigest;
    use crate::construction::registry::check::{key_shape, KeyStrategy};
    use crate::model::{CoverageRule, DeploymentGroupId, ProfileId};
    use crate::resolver::artifact_resolution::{CollectionSubject, ConflictKey};
    use crate::resolver::coverage::{CoverageSubject, Domain, RuleContent, SurfaceKey};
    use crate::resolver::dependency::SemanticGroup;
    use crate::resolver::evaluation::Subject;
    use crate::resolver::fallback::FallbackSubject;
    use crate::resolver::groups::GroupSubject;
    use crate::resolver::hydration::{EndpointField, HydrationSubject};
    use crate::resolver::identity::DeclarationField;
    use crate::resolver::locales::fixtures::{canonical, content, key, target};
    use crate::resolver::locales::{Content, LocaleSubject};
    use crate::resolver::negotiation::NegotiationSubject;
    use crate::resolver::resource_policy::{
        ArtifactSubject, PolicySlot, PolicySubject, ReferenceField,
    };
    use crate::resolver::specification::Slot;
    use crate::resolver::subject::SubjectKind;
    use crate::resolver::targets::TargetSubject;

    use super::{logical, own, projected, record, Digest, Own, Sha256, Specification, Tokens};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// A digest under which every record collides.
    struct Constant;

    impl Digest for Constant {
        fn digest(&self, _: &[u8]) -> [u8; 32] {
            [7; 32]
        }
    }

    fn token(value: &str) -> Token {
        Token::new(value).unwrap()
    }

    fn group(id: &str) -> DeploymentGroupId {
        serde_json::from_value(json!(id)).unwrap()
    }

    fn rule() -> RuleContent {
        let rule: CoverageRule = serde_json::from_value(
            json!({ "mode": "direct-required", "requestedLocales": ["en"] }),
        )
        .unwrap();
        RuleContent::of(&rule)
    }

    fn tuple(digest: u8) -> MemberKey {
        MemberKey::Tuple(Arc::new(ExactTuple {
            kind: ArtifactKind::TrustPolicy,
            identity: token("trust"),
            revision: token("1"),
            specification_revision: token("0"),
            semantic_digest: SemanticDigest::from_hash([digest; 32]),
        }))
    }

    fn conflict() -> ConflictKey {
        let MemberKey::Tuple(tuple) = tuple(1) else {
            unreachable!("a tuple")
        };
        ConflictKey::of(&tuple)
    }

    fn untupled(value: Value) -> MemberKey {
        FormalMember::new(value).unwrap().key()
    }

    fn requested(key: crate::resolver::locales::LocaleKey) -> Subject {
        Subject::Locale(LocaleSubject::RequestedLocale(key))
    }

    /// The token view of `subject` when it is assigned alone under SHA-256.
    fn alone(subject: &Subject) -> Value {
        Tokens::assign(PROFILE, [subject], &Sha256)
            .token(PROFILE, subject)
            .unwrap()
            .view()
    }

    /// One subject of every kind the resolver records units for.
    #[expect(clippy::too_many_lines, reason = "one subject of each shape")]
    fn every_shape() -> Vec<Subject> {
        let artifact = |member| {
            [
                ArtifactSubject::Artifacts,
                ArtifactSubject::PolicyArtifacts,
                ArtifactSubject::TargetProfileArtifacts,
                ArtifactSubject::References,
                ArtifactSubject::Artifact(member),
                ArtifactSubject::ArtifactBytes,
                ArtifactSubject::Reference(ReferenceField::Policy(PolicySlot::Trust)),
                ArtifactSubject::Reference(ReferenceField::TargetProfile(target("web"))),
                ArtifactSubject::ReferenceBytes,
            ]
        };
        let mut subjects: Vec<Subject> = Slot::ALL.into_iter().map(Subject::Slot).collect();
        subjects.extend([Subject::Vocabulary, Subject::CanonicalizationData]);
        subjects.extend(
            artifact(tuple(1))
                .into_iter()
                .map(|subject| Subject::ResourcePolicy(PolicySubject::Capability(subject))),
        );
        subjects.extend(
            artifact(untupled(json!("x")))
                .into_iter()
                .map(|subject| Subject::ResourcePolicy(PolicySubject::Recheck(subject))),
        );
        subjects.extend(
            [
                PolicySubject::ResourceLimits,
                PolicySubject::Nodes,
                PolicySubject::Depth,
                PolicySubject::CollectionEntries,
                PolicySubject::StringBytes,
                PolicySubject::LongestString,
                PolicySubject::Profiles,
                PolicySubject::ProfileId(
                    serde_json::from_value::<ProfileId>(json!("app")).unwrap(),
                ),
                PolicySubject::Selector,
                PolicySubject::StructuralAnalysis,
            ]
            .map(Subject::ResourcePolicy),
        );
        subjects.extend(
            [
                DeclarationField::ProjectId,
                DeclarationField::SelectionScope,
            ]
            .map(Subject::DeclarationField),
        );
        subjects.extend(
            [
                LocaleSubject::Occurrences,
                LocaleSubject::RequestedLocale(key("en")),
                LocaleSubject::RequestedLocales,
                LocaleSubject::DefaultSourceLocale,
                LocaleSubject::DefaultRequestedLocale,
                LocaleSubject::NegotiationAliasKey(content("x!")),
                LocaleSubject::NegotiationAliasValue(content("x!"), key("en")),
                LocaleSubject::MessageFallbackKey(key("ja")),
                LocaleSubject::TargetProfiles,
                LocaleSubject::TargetId(target("web")),
                LocaleSubject::TargetRequestedLocale(target("web"), key("en")),
                LocaleSubject::TargetDefaultRequestedLocale(target("web")),
            ]
            .map(Subject::Locale),
        );
        subjects.extend(
            [
                NegotiationSubject::Aliases,
                NegotiationSubject::Key(key("en")),
                NegotiationSubject::Destination(key("en"), key("fr")),
            ]
            .map(Subject::Negotiation),
        );
        subjects.extend(
            [
                FallbackSubject::Sources,
                FallbackSubject::Source(key("ja")),
                FallbackSubject::Sequence(key("ja")),
                FallbackSubject::Candidate(key("ja"), 0),
                FallbackSubject::Literal(key("ja"), key("en")),
                FallbackSubject::IntentSource(key("ja")),
            ]
            .map(Subject::Fallback),
        );
        subjects.extend(
            [
                CoverageSubject::Rules,
                CoverageSubject::LocaleSelectors,
                CoverageSubject::SurfaceSelectors,
                CoverageSubject::Locale(rule(), key("en")),
                CoverageSubject::SurfaceClass(rule(), SurfaceKey::Member(token("nav"))),
                CoverageSubject::Rule(rule()),
                CoverageSubject::Domain(Domain::of(vec![canonical("en")], vec![token("nav")])),
                CoverageSubject::Cells,
                CoverageSubject::Comparisons,
                CoverageSubject::Cell(canonical("en"), token("nav")),
                CoverageSubject::Table,
            ]
            .map(Subject::Coverage),
        );
        subjects.extend(
            [
                CollectionSubject::Member(tuple(1)),
                CollectionSubject::Group(conflict()),
            ]
            .map(Subject::ArtifactCollection),
        );
        subjects.extend(
            [
                ReferenceField::Policy(PolicySlot::ResourceLimits),
                ReferenceField::TargetProfile(target("web")),
            ]
            .map(Subject::ArtifactReference),
        );
        subjects.extend([
            Subject::DeliveryPlacement,
            Subject::Target(TargetSubject::Target(target("web"))),
        ]);
        subjects.extend(
            [
                GroupSubject::Groups,
                GroupSubject::Memberships,
                GroupSubject::Group(group("site")),
                GroupSubject::Members(group("site")),
                GroupSubject::Member(group("site"), target("web")),
                GroupSubject::Assignment(target("web")),
            ]
            .map(Subject::Group),
        );
        subjects.extend(
            [
                HydrationSubject::Relations,
                HydrationSubject::Endpoint(
                    group("site"),
                    target("ssr"),
                    target("web"),
                    EndpointField::Server,
                ),
                HydrationSubject::Relation(group("site"), target("ssr"), target("web")),
                HydrationSubject::Roles(group("site"), target("web")),
            ]
            .map(Subject::Hydration),
        );
        subjects.extend(SemanticGroup::ALL.map(Subject::SemanticDependency));
        subjects
    }

    #[test]
    fn every_subject_takes_the_key_shape_of_its_kind_in_the_registry() {
        let mut kinds = BTreeSet::new();
        for subject in every_shape() {
            let kind = subject.kind();
            kinds.insert(kind);
            let shape = key_shape(kind);
            let (own, parent) = own(&subject);
            assert_eq!(parent.as_ref().map(Subject::kind), shape.parent, "{kind:?}");
            let strategy = match own {
                Own::Singleton => KeyStrategy::Singleton,
                Own::Field(_) => KeyStrategy::LogicalField,
                Own::Identity(_) | Own::Content(_) => KeyStrategy::CanonicalIdentityOrContent,
                Own::Position(_) => KeyStrategy::SemanticPosition,
            };
            assert_eq!(strategy, shape.strategy, "{kind:?}");
        }
        let recorded: BTreeSet<SubjectKind> = SubjectKind::ALL
            .into_iter()
            .filter(|kind| !kind.declared_only())
            .collect();
        assert_eq!(kinds, recorded);
    }

    /// Every arm of the projection, each with a value of its own.
    #[test]
    #[expect(clippy::too_many_lines, reason = "one row for each arm")]
    fn each_shape_of_subject_projects_as_015_writes_it() {
        let parent =
            |parent: Value, subject: Value| json!({ "parent": parent, "subject": subject });
        let web = json!({ "identity": "web" });
        let ja = json!({ "identity": "ja" });
        let site = json!({ "identity": "site" });
        let rule_content = json!({ "content": { "type": "object" } });
        let string = json!({ "content": { "type": "string" } });
        let relation = json!({ "identity": { "server": "ssr", "client": "web" } });
        let trust = json!({
            "identity": {
                "kind": "trust-policy",
                "identity": "trust",
                "revision": "1",
                "specificationRevision": "0",
            },
        });
        let capability = |subject| Subject::ResourcePolicy(PolicySubject::Capability(subject));
        let cases = [
            (
                Subject::Slot(Slot::ProjectProfile),
                json!("project-profile-slot"),
            ),
            (Subject::Vocabulary, json!("vocabulary")),
            (
                Subject::CanonicalizationData,
                json!("canonicalization-data"),
            ),
            (
                Subject::SemanticDependency(SemanticGroup::Coverage),
                json!("coverage-dependency"),
            ),
            (
                capability(ArtifactSubject::Artifacts),
                json!("capability-artifacts"),
            ),
            (
                capability(ArtifactSubject::Artifact(tuple(1))),
                trust.clone(),
            ),
            (
                capability(ArtifactSubject::Reference(ReferenceField::Policy(
                    PolicySlot::Trust,
                ))),
                json!(["policies", "trust"]),
            ),
            (
                capability(ArtifactSubject::Reference(ReferenceField::TargetProfile(
                    target("web"),
                ))),
                web.clone(),
            ),
            (
                Subject::ResourcePolicy(PolicySubject::Recheck(ArtifactSubject::Artifact(
                    untupled(json!("x")),
                ))),
                string.clone(),
            ),
            (
                Subject::ResourcePolicy(PolicySubject::ProfileId(
                    serde_json::from_value::<ProfileId>(json!("app")).unwrap(),
                )),
                json!({ "identity": "app" }),
            ),
            (
                Subject::ResourcePolicy(PolicySubject::Nodes),
                json!("root-nodes"),
            ),
            (
                Subject::DeclarationField(DeclarationField::SelectionScope),
                json!(["selectionScope"]),
            ),
            (
                Subject::Locale(LocaleSubject::Occurrences),
                json!("locale-occurrences"),
            ),
            (requested(key("en")), json!({ "identity": "en" })),
            (requested(content("x!")), string.clone()),
            (
                Subject::Locale(LocaleSubject::RequestedLocales),
                json!(["requestedLocales"]),
            ),
            (
                Subject::Locale(LocaleSubject::DefaultSourceLocale),
                json!(["defaultSourceLocale"]),
            ),
            (
                Subject::Locale(LocaleSubject::DefaultRequestedLocale),
                json!(["defaultRequestedLocale"]),
            ),
            (
                Subject::Locale(LocaleSubject::NegotiationAliasKey(content("x!"))),
                string.clone(),
            ),
            (
                Subject::Locale(LocaleSubject::NegotiationAliasValue(
                    content("x!"),
                    key("en"),
                )),
                parent(string.clone(), json!({ "identity": "en" })),
            ),
            (
                Subject::Locale(LocaleSubject::MessageFallbackKey(key("ja"))),
                ja.clone(),
            ),
            (
                Subject::Locale(LocaleSubject::TargetProfiles),
                json!(["targetProfiles"]),
            ),
            (
                Subject::Locale(LocaleSubject::TargetId(target("web"))),
                web.clone(),
            ),
            (
                Subject::Locale(LocaleSubject::TargetRequestedLocale(
                    target("web"),
                    key("en"),
                )),
                parent(web.clone(), json!({ "identity": "en" })),
            ),
            (
                Subject::Locale(LocaleSubject::TargetDefaultRequestedLocale(target("web"))),
                parent(web.clone(), json!(["defaultRequestedLocale"])),
            ),
            (
                Subject::Negotiation(NegotiationSubject::Aliases),
                json!(["localeNegotiation", "aliases"]),
            ),
            (
                Subject::Negotiation(NegotiationSubject::Key(key("en"))),
                json!({ "identity": "en" }),
            ),
            (
                Subject::Negotiation(NegotiationSubject::Destination(key("en"), key("fr"))),
                parent(json!({ "identity": "en" }), json!({ "identity": "fr" })),
            ),
            (
                Subject::Fallback(FallbackSubject::Sources),
                json!(["messageFallback"]),
            ),
            (
                Subject::Fallback(FallbackSubject::Source(key("ja"))),
                ja.clone(),
            ),
            // A sequence is the empty path under its mapping entry.
            (
                Subject::Fallback(FallbackSubject::Sequence(key("ja"))),
                parent(ja.clone(), json!([])),
            ),
            // A position counts from 0.
            (
                Subject::Fallback(FallbackSubject::Candidate(key("ja"), 2)),
                parent(ja.clone(), json!("2")),
            ),
            (
                Subject::Fallback(FallbackSubject::Literal(key("ja"), key("en"))),
                parent(ja.clone(), json!({ "identity": "en" })),
            ),
            (
                Subject::Fallback(FallbackSubject::IntentSource(key("ja"))),
                parent(
                    ja.clone(),
                    json!({ "identity": { "kind": "intent-source-locale" } }),
                ),
            ),
            (
                Subject::Coverage(CoverageSubject::Rules),
                json!(["coverage", "rules"]),
            ),
            (
                Subject::Coverage(CoverageSubject::LocaleSelectors),
                json!("coverage-locale-selectors"),
            ),
            (
                Subject::Coverage(CoverageSubject::Locale(rule(), key("en"))),
                parent(rule_content.clone(), json!({ "identity": "en" })),
            ),
            (
                Subject::Coverage(CoverageSubject::SurfaceClass(
                    rule(),
                    SurfaceKey::Member(token("nav")),
                )),
                parent(rule_content.clone(), json!({ "identity": "nav" })),
            ),
            (
                Subject::Coverage(CoverageSubject::SurfaceClass(
                    rule(),
                    SurfaceKey::Content(Content::new("not a member")),
                )),
                parent(rule_content.clone(), string.clone()),
            ),
            (
                Subject::Coverage(CoverageSubject::Rule(rule())),
                rule_content,
            ),
            (
                Subject::Coverage(CoverageSubject::Domain(Domain::of(
                    vec![canonical("en"), canonical("fr")],
                    vec![token("nav")],
                ))),
                json!({ "identity": { "locales": ["en", "fr"], "surfaceClasses": ["nav"] } }),
            ),
            (
                Subject::Coverage(CoverageSubject::Cell(canonical("en"), token("nav"))),
                json!({ "identity": { "locale": "en", "surfaceClass": "nav" } }),
            ),
            (
                Subject::Coverage(CoverageSubject::Table),
                json!("coverage-table"),
            ),
            (
                Subject::ArtifactCollection(CollectionSubject::Member(untupled(json!(["x"])))),
                json!({ "content": { "type": "array" } }),
            ),
            (
                Subject::ArtifactCollection(CollectionSubject::Group(conflict())),
                trust,
            ),
            (
                Subject::ArtifactReference(ReferenceField::Policy(PolicySlot::SourceAdmission)),
                json!(["policies", "sourceAdmission"]),
            ),
            (
                Subject::ArtifactReference(ReferenceField::TargetProfile(target("web"))),
                web.clone(),
            ),
            (Subject::DeliveryPlacement, json!(["delivery", "placement"])),
            (
                Subject::Target(TargetSubject::Target(target("web"))),
                web.clone(),
            ),
            (
                Subject::Group(GroupSubject::Groups),
                json!(["deploymentGroups"]),
            ),
            (
                Subject::Group(GroupSubject::Memberships),
                json!("memberships"),
            ),
            (
                Subject::Group(GroupSubject::Group(group("site"))),
                site.clone(),
            ),
            (
                Subject::Group(GroupSubject::Members(group("site"))),
                parent(site.clone(), json!(["members"])),
            ),
            (
                Subject::Group(GroupSubject::Member(group("site"), target("web"))),
                parent(site.clone(), web.clone()),
            ),
            (
                Subject::Group(GroupSubject::Assignment(target("web"))),
                web.clone(),
            ),
            (
                Subject::Hydration(HydrationSubject::Relations),
                json!("relations"),
            ),
            // A parent that has its own parent nests.
            (
                Subject::Hydration(HydrationSubject::Endpoint(
                    group("site"),
                    target("ssr"),
                    target("web"),
                    EndpointField::Server,
                )),
                parent(parent(site.clone(), relation.clone()), json!(["server"])),
            ),
            (
                Subject::Hydration(HydrationSubject::Endpoint(
                    group("site"),
                    target("ssr"),
                    target("web"),
                    EndpointField::Client,
                )),
                parent(parent(site.clone(), relation.clone()), json!(["client"])),
            ),
            (
                Subject::Hydration(HydrationSubject::Relation(
                    group("site"),
                    target("ssr"),
                    target("web"),
                )),
                parent(site.clone(), relation),
            ),
            (
                Subject::Hydration(HydrationSubject::Roles(group("site"), target("web"))),
                parent(site, web),
            ),
        ];
        for (subject, expected) in cases {
            assert_eq!(projected(&subject), expected, "{subject:?}");
        }
    }

    /// Pinned against SHA-256 and the framings computed apart from this crate.
    #[test]
    fn tokens_match_vectors_computed_independently() {
        let vectors = [
            (
                Subject::Vocabulary,
                "33db40b6ca29583cfa4f0593235495fbe2246d5e162654e5b18e7f792706c1c5",
            ),
            (
                Subject::DeclarationField(DeclarationField::ProjectId),
                "84ca38c73c8a7a23874e0b9ea1bac0939ad6703113e36ccad00aa51500619cb4",
            ),
            (
                requested(key("en")),
                "e35e4a12d32518f6b6381c1e067c9a72413635878e1d45cea653102676711227",
            ),
            (
                requested(content("x!")),
                "ac7304d853d1a86b4cb42389ca5415bbe64ea2c7d2fbbc1422f84d0b38d06b4c",
            ),
            (
                Subject::Fallback(FallbackSubject::Candidate(key("ja"), 0)),
                "ed7326bc60a7b7d3407804f65f89a44c2402f89ab04d869fb28b57085aa92bd2",
            ),
            (
                Subject::ArtifactCollection(CollectionSubject::Group(conflict())),
                "f1b044b7fb363e23ba9dbb6ff6bc10b958b89a3ac8b3051336ee9fd0e5230cc5",
            ),
        ];
        for (subject, digest) in vectors {
            assert_eq!(
                alone(&subject),
                json!({ "token": format!("sha256:{digest}") }),
                "{subject:?}"
            );
        }
    }

    #[test]
    fn values_with_one_projection_share_one_token() {
        let first = requested(content("x!"));
        let second = requested(content("not a locale"));
        assert_eq!(record(PROFILE, &first), record(PROFILE, &second));
        let tokens = Tokens::assign(PROFILE, [&first, &second], &Sha256);
        assert_eq!(
            tokens.token(PROFILE, &first),
            tokens.token(PROFILE, &second)
        );
        assert_eq!(tokens.comparisons(), 0);
        assert!(!alone(&first).as_object().unwrap().contains_key("collision"));
    }

    #[test]
    fn a_colliding_set_is_numbered_from_zero_in_record_byte_order() {
        let subjects = [
            requested(key("fr")),
            requested(content("x!")),
            requested(key("en")),
        ];
        let tokens = Tokens::assign(PROFILE, &subjects, &Constant);
        let mut records: Vec<Vec<u8>> = subjects
            .iter()
            .map(|subject| record(PROFILE, subject))
            .collect();
        records.sort();
        for subject in &subjects {
            let ordinal = records
                .iter()
                .position(|own| *own == record(PROFILE, subject))
                .unwrap();
            let view = tokens.token(PROFILE, subject).unwrap().view();
            assert_eq!(view["collision"], json!(ordinal.to_string()), "{subject:?}");
            assert_eq!(view["token"], json!(format!("sha256:{}", "07".repeat(32))));
        }
        // Three members, so three pairs.
        assert_eq!(tokens.comparisons(), 3);
    }

    #[test]
    fn equal_digests_in_other_phases_or_kinds_form_no_colliding_set() {
        let subjects = [
            // One phase, two kinds.
            requested(key("en")),
            Subject::Locale(LocaleSubject::NegotiationAliasKey(key("en"))),
            // Another phase.
            Subject::Negotiation(NegotiationSubject::Key(key("en"))),
        ];
        let tokens = Tokens::assign(PROFILE, &subjects, &Constant);
        for subject in &subjects {
            let view = tokens.token(PROFILE, subject).unwrap().view();
            assert_eq!(view.as_object().unwrap().len(), 1, "{subject:?}");
        }
        assert_eq!(tokens.comparisons(), 0);
    }

    #[test]
    fn a_view_holds_the_complete_digest_in_lowercase_hex() {
        let view = alone(&Subject::Vocabulary);
        let token = view["token"].as_str().unwrap();
        let digits = token.strip_prefix("sha256:").unwrap();
        assert_eq!(digits.len(), 64);
        assert!(digits
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    }

    #[test]
    fn a_collection_member_is_its_conflict_group_without_the_digest() {
        let member = |digest| Subject::ArtifactCollection(CollectionSubject::Member(tuple(digest)));
        let view = json!({
            "identity": {
                "kind": "trust-policy",
                "identity": "trust",
                "revision": "1",
                "specificationRevision": "0",
            },
        });
        // Bodies with different digests share the member subject.
        assert_eq!(projected(&member(1)), view);
        assert_eq!(record(PROFILE, &member(1)), record(PROFILE, &member(2)));
        // The conflict group has the same view in its own kind and scope.
        let group = Subject::ArtifactCollection(CollectionSubject::Group(conflict()));
        assert_eq!(projected(&group), view);
        assert_ne!(alone(&member(1)), alone(&group));
    }

    #[test]
    fn a_member_without_a_tuple_projects_its_json_type() {
        let cases = [
            (json!(null), "null"),
            (json!(true), "boolean"),
            (json!("x"), "string"),
            (json!(["x"]), "array"),
            (
                json!({ "kind": "approval-policy", "schemaRevision": "0" }),
                "object",
            ),
        ];
        for (value, json_type) in cases {
            let subject = Subject::ArtifactCollection(CollectionSubject::Member(untupled(value)));
            assert_eq!(
                projected(&subject),
                json!({ "content": { "type": json_type } })
            );
        }
    }

    /// Every subject of every shape, and raw values that a projection joins
    /// with them or keeps apart.
    fn raw_variants() -> Vec<Subject> {
        let other: CoverageRule = serde_json::from_value(
            json!({ "mode": "fallback-allowed", "requestedLocales": ["fr"] }),
        )
        .unwrap();
        let other = RuleContent::of(&other);
        let member = |key| Subject::ArtifactCollection(CollectionSubject::Member(key));
        let mut subjects = every_shape();
        subjects.extend([
            requested(content("x!")),
            requested(content("not a locale")),
            requested(key("fr")),
            member(tuple(2)),
            member(untupled(json!("y"))),
            member(untupled(json!(false))),
            member(untupled(json!(true))),
            Subject::Coverage(CoverageSubject::Rule(other.clone())),
            Subject::Coverage(CoverageSubject::Locale(rule(), content("a"))),
            Subject::Coverage(CoverageSubject::Locale(other.clone(), content("b"))),
            Subject::Coverage(CoverageSubject::SurfaceClass(
                other,
                SurfaceKey::Content(Content::new("a")),
            )),
            Subject::Coverage(CoverageSubject::SurfaceClass(
                rule(),
                SurfaceKey::Content(Content::new("b")),
            )),
            Subject::Fallback(FallbackSubject::Candidate(content("zz"), 0)),
            Subject::Fallback(FallbackSubject::Candidate(content("yy"), 0)),
            Subject::Locale(LocaleSubject::NegotiationAliasValue(
                content("y?"),
                key("en"),
            )),
            Subject::Locale(LocaleSubject::NegotiationAliasValue(
                content("y?"),
                content("z?"),
            )),
            Subject::Negotiation(NegotiationSubject::Destination(content("q"), key("fr"))),
        ]);
        subjects
    }

    #[test]
    fn logical_subjects_are_equal_exactly_when_their_token_records_are() {
        let subjects = raw_variants();
        for first in &subjects {
            let own = logical(first);
            assert_eq!(logical(&own), own, "{first:?}");
            assert_eq!(projected(&own), projected(first), "{first:?}");
            for second in &subjects {
                assert_eq!(
                    own == logical(second),
                    record(PROFILE, first) == record(PROFILE, second),
                    "{first:?} {second:?}"
                );
            }
        }
    }
}
