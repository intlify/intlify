// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Redacted evidence token: the token record of an Evidence
//! component that is not disclosed, and the token itself, with the collision
//! ordinals that keep unequal records with one digest apart (D15:722,
//! 2454-2468; Decisions 015-182 and 015-183).
//!
//! A token replaces one component: a Portable Source Identity that its owner
//! does not mark non-secret, an unsafe member segment of a Logical input path
//! or of a Portable Value Locator, an unsafe Resolver Input Component Path
//! key, or a profile selector's `redacted-value`. The record names that
//! component as its evidence kind, `profile-selector` for a selector, the
//! Safe Origin Kind of the input that supplied the component, the one
//! disclosure policy that produces a token, and `C` of the projected identity.
//! The projected identity keeps only the identity-domain tag of a source
//! identity, the JSON type of a member or key, or a selector's JSON type and
//! String Length Fact, so raw values with equal records share one token.
//!
//! Every evidence record an invocation derives belongs to one colliding set,
//! whatever its kind or origin (Decision 015-176): unequal records with one
//! digest are numbered from 0 in ascending byte order, and each pair counts as
//! one comparison under `maxCollisionComparisons`. Subject records never join
//! that set. Nothing builds Evidence yet.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use intlify_shared_json::encoding::encode;
use serde_json::{json, Map, Value};

use crate::boundary::source::ExactIdentity;
use crate::construction::profile::EVIDENCE_TOKEN_DOMAIN;
use crate::structural::selection::StringLengthFact;

use super::origin::SafeOriginKind;
use super::token::{number, Digest, RedactedToken, Specification};

/// The disclosure policy every Redacted evidence token names: the only policy
/// that produces a token (Decision 015-182).
const DISCLOSURE_POLICY: &str = "redacted-structure";

/// The evidence kind a token's record names: the projection of the component
/// the token replaces, or `profile-selector` for a selector's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TokenKind {
    SourceIdentity,
    InputPathMember,
    ValuePathMember,
    ComponentKey,
    ProfileSelector,
}

impl TokenKind {
    /// 015's ID.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::SourceIdentity => "source-identity",
            Self::InputPathMember => "logical-input-path-member",
            Self::ValuePathMember => "value-path-member",
            Self::ComponentKey => "resolver-input-component-key",
            Self::ProfileSelector => "profile-selector",
        }
    }

    /// Whether an input that `origin` supplies can carry a component of this
    /// kind: configuration sources carry identities and path members, the
    /// vocabulary and the artifact collection carry the only keyed component
    /// paths (D15:832, 834), and the selector slots carry selectors.
    const fn admits(self, origin: SafeOriginKind) -> bool {
        use SafeOriginKind as O;
        match self {
            Self::SourceIdentity | Self::InputPathMember | Self::ValuePathMember => {
                matches!(origin, O::ConfigurationFile | O::ConfigurationProgrammatic)
            }
            Self::ComponentKey => matches!(
                origin,
                O::IntentSurfaceClassVocabulary | O::ProfileResolutionArtifactCollection
            ),
            Self::ProfileSelector => {
                matches!(
                    origin,
                    O::ProfileSelectorCli | O::ProfileSelectorProgrammatic
                )
            }
        }
    }
}

/// A JSON type tag (D15:2468).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum JsonType {
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}

impl JsonType {
    /// 015's tag.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Boolean => "boolean",
            Self::Number => "number",
            Self::String => "string",
            Self::Array => "array",
            Self::Object => "object",
        }
    }
}

/// What a token keeps of the component it replaces (Decision 015-183).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Projected {
    /// A source identity's identity-domain tag.
    Domain(Arc<str>),
    /// A member name, which is always a string.
    Member,
    /// The JSON type of a component key's source.
    Key(JsonType),
    /// A selector's JSON type and, for a string, its String Length Fact.
    Selector {
        kind: JsonType,
        length: Option<StringLengthFact>,
    },
}

/// A resolver bug that leaves a component without a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EvidenceTokenInvariant {
    /// An input of this origin carries no component of this kind.
    OriginOutsideKind(TokenKind, SafeOriginKind),
    /// A string selector has no String Length Fact, or another type has one.
    LengthOutsideType(JsonType),
}

/// One component that Evidence redacts: the facts its token record holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Redacted {
    kind: TokenKind,
    origin: SafeOriginKind,
    projected: Projected,
}

impl Redacted {
    /// The admitted exact identity `identity` when its owner does not mark it
    /// non-secret. Only its identity-domain tag enters the record, never its
    /// payload.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves the component without a token.
    pub(crate) fn source_identity(
        origin: SafeOriginKind,
        identity: &ExactIdentity,
    ) -> Result<Self, EvidenceTokenInvariant> {
        Self::new(
            TokenKind::SourceIdentity,
            origin,
            Projected::Domain(Arc::from(identity.tag())),
        )
    }

    /// An unsafe member segment of a Logical input path.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves the component without a token.
    pub(crate) fn input_path_member(
        origin: SafeOriginKind,
    ) -> Result<Self, EvidenceTokenInvariant> {
        Self::new(TokenKind::InputPathMember, origin, Projected::Member)
    }

    /// An unsafe member segment of a Portable Value Locator.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves the component without a token.
    pub(crate) fn value_path_member(
        origin: SafeOriginKind,
    ) -> Result<Self, EvidenceTokenInvariant> {
        Self::new(TokenKind::ValuePathMember, origin, Projected::Member)
    }

    /// An unsafe Resolver Input Component Path key whose source has the JSON
    /// type `source`: a tuple view, or the member a content-derived admission
    /// key was derived from.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves the component without a token.
    pub(crate) fn component_key(
        origin: SafeOriginKind,
        source: JsonType,
    ) -> Result<Self, EvidenceTokenInvariant> {
        Self::new(TokenKind::ComponentKey, origin, Projected::Key(source))
    }

    /// A selector's `redacted-value` of the JSON type `kind`, with the String
    /// Length Fact a string has and no other type has.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves the component without a token.
    pub(crate) fn selector(
        origin: SafeOriginKind,
        kind: JsonType,
        length: Option<StringLengthFact>,
    ) -> Result<Self, EvidenceTokenInvariant> {
        if (kind == JsonType::String) != length.is_some() {
            return Err(EvidenceTokenInvariant::LengthOutsideType(kind));
        }
        Self::new(
            TokenKind::ProfileSelector,
            origin,
            Projected::Selector { kind, length },
        )
    }

    fn new(
        kind: TokenKind,
        origin: SafeOriginKind,
        projected: Projected,
    ) -> Result<Self, EvidenceTokenInvariant> {
        if kind.admits(origin) {
            Ok(Self {
                kind,
                origin,
                projected,
            })
        } else {
            Err(EvidenceTokenInvariant::OriginOutsideKind(kind, origin))
        }
    }

    /// The projected evidence identity: `{ "domain": tag }`,
    /// `{ "type": "string" }`, `{ "type": t }`, or `{ "type": t, "length"?: l }`
    /// (D15:2468).
    pub(crate) fn projected(&self) -> Value {
        match &self.projected {
            Projected::Domain(tag) => json!({ "domain": tag.as_ref() }),
            Projected::Member => json!({ "type": JsonType::String.id() }),
            Projected::Key(kind) => json!({ "type": kind.id() }),
            Projected::Selector { kind, length } => {
                let mut view = Map::new();
                view.insert("type".to_owned(), json!(kind.id()));
                if let Some(length) = length {
                    view.insert("length".to_owned(), length_view(*length));
                }
                Value::Object(view)
            }
        }
    }
}

/// A String Length Fact as a selector's view writes it (D15:2505).
fn length_view(length: StringLengthFact) -> Value {
    match length {
        StringLengthFact::Exact(bytes) => json!({ "exact": bytes.to_string() }),
        StringLengthFact::OverLimit => json!("over-limit"),
    }
}

/// The token record of `redacted`: its six field frames (D15:2454-2466).
///
/// # Panics
///
/// Panics if the projected identity cannot be framed. It holds no number and
/// only a domain tag the boundary already bounds, so that would be a defect.
pub(crate) fn record(specification: Specification<'_>, redacted: &Redacted) -> Vec<u8> {
    let projected = encode(&redacted.projected())
        .expect("a projected evidence identity is a small number-free value");
    crate::framing::Record::new()
        .field(specification.identity.as_bytes())
        .field(specification.revision.as_bytes())
        .field(redacted.kind.id().as_bytes())
        .field(redacted.origin.id().as_bytes())
        .field(DISCLOSURE_POLICY.as_bytes())
        .field(&projected)
        .finish()
}

/// The tokens of one invocation's redacted components, and the comparisons
/// their collisions took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvidenceTokens {
    tokens: BTreeMap<Vec<u8>, RedactedToken>,
    comparisons: u64,
}

impl EvidenceTokens {
    /// The token of every distinct record among `components`. Components with
    /// one record share one token. All unequal records that share a digest
    /// form one colliding set, whatever their kinds and origins, numbered from
    /// 0 in ascending byte order before any token is handed out, and each pair
    /// of them counts as one comparison.
    pub(crate) fn assign<'a>(
        specification: Specification<'_>,
        components: impl IntoIterator<Item = &'a Redacted>,
        digest: &impl Digest,
    ) -> Self {
        let mut sets: BTreeMap<((), [u8; 32]), BTreeSet<Vec<u8>>> = BTreeMap::new();
        for component in components {
            let record = record(specification, component);
            let mut preimage = EVIDENCE_TOKEN_DOMAIN.as_bytes().to_vec();
            preimage.extend_from_slice(&record);
            sets.entry(((), digest.digest(&preimage)))
                .or_default()
                .insert(record);
        }
        let (tokens, comparisons) = number(sets);
        Self {
            tokens,
            comparisons,
        }
    }

    /// The token of `component`, if it was among the assigned components.
    pub(crate) fn token(
        &self,
        specification: Specification<'_>,
        component: &Redacted,
    ) -> Option<RedactedToken> {
        self.tokens.get(&record(specification, component)).copied()
    }

    /// The comparisons that the colliding sets took, one per pair.
    pub(crate) const fn comparisons(&self) -> u64 {
        self.comparisons
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::boundary::source::{file_source_revision, read_pair, ExactIdentity, SourceLimits};
    use crate::input_limits::Bound;
    use crate::resolver::evaluation::Subject;
    use crate::resolver::origin::SafeOriginKind as O;
    use crate::resolver::token::{Digest, Sha256, Specification, Tokens};
    use crate::structural::selection::StringLengthFact;

    use super::{EvidenceTokenInvariant, EvidenceTokens, JsonType, Redacted, TokenKind};

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

    fn source() -> Redacted {
        let identity = exact("intlify-file-source.v0:storefront/intlify.config.json");
        Redacted::source_identity(O::ConfigurationFile, &identity).unwrap()
    }

    fn selector_exact() -> Redacted {
        Redacted::selector(
            O::ProfileSelectorCli,
            JsonType::String,
            Some(StringLengthFact::Exact(12)),
        )
        .unwrap()
    }

    /// One component of every kind, under origins that can supply it.
    fn every_kind() -> Vec<(Redacted, &'static str)> {
        vec![
            (
                source(),
                "sha256:0b14760705ef603246b0f47ea23829ebfcccc8a47da7bc7fc19fc4b0d291612f",
            ),
            (
                Redacted::input_path_member(O::ConfigurationProgrammatic).unwrap(),
                "sha256:0856f1e9b4249ebb1eca3f368db3d3e9ecbc208aac95ec20f079981da86dc0a8",
            ),
            (
                Redacted::value_path_member(O::ConfigurationFile).unwrap(),
                "sha256:663eb6254e94a3289fce71ce1f181e96c1aa4152eaa2a82784966b55d5ce8b43",
            ),
            (
                Redacted::component_key(O::ProfileResolutionArtifactCollection, JsonType::Object)
                    .unwrap(),
                "sha256:e8b3e31460ec2c3bea9c5e56a1fd062e0699ed3daa04e758b76bdb44f6460c6a",
            ),
            (
                selector_exact(),
                "sha256:6bab6f0d58d5a25b8824c38ebda6c4767bd9ee9d2b07aede68066b2bbdb6a3da",
            ),
            (
                Redacted::selector(
                    O::ProfileSelectorProgrammatic,
                    JsonType::String,
                    Some(StringLengthFact::OverLimit),
                )
                .unwrap(),
                "sha256:bd0cf2a8e189cc9d44491f753464d5676936b57ecef39b168e0c9043d86d4adc",
            ),
            (
                Redacted::selector(O::ProfileSelectorCli, JsonType::Null, None).unwrap(),
                "sha256:f1b67b4a6fd933b358e4f99e5599be4141a1ab7184a9649ff597f96298e70966",
            ),
        ]
    }

    /// Pinned against SHA-256, the record framing, and `C` computed apart
    /// from this crate.
    #[test]
    fn tokens_match_vectors_computed_independently() {
        let cases = every_kind();
        let tokens = EvidenceTokens::assign(
            PROFILE,
            cases.iter().map(|(component, _)| component),
            &Sha256,
        );
        for (component, expected) in &cases {
            assert_eq!(
                tokens.token(PROFILE, component).unwrap().view(),
                json!({ "token": expected }),
                "{component:?}"
            );
        }
        assert_eq!(tokens.comparisons(), 0);
    }

    #[test]
    fn each_kind_keeps_only_its_projection() {
        let projected: Vec<_> = every_kind()
            .iter()
            .map(|(component, _)| (component.kind.id(), component.projected()))
            .collect();
        assert_eq!(
            projected,
            [
                (
                    "source-identity",
                    json!({ "domain": "intlify-file-source.v0" })
                ),
                ("logical-input-path-member", json!({ "type": "string" })),
                ("value-path-member", json!({ "type": "string" })),
                ("resolver-input-component-key", json!({ "type": "object" })),
                (
                    "profile-selector",
                    json!({ "type": "string", "length": { "exact": "12" } })
                ),
                (
                    "profile-selector",
                    json!({ "type": "string", "length": "over-limit" })
                ),
                ("profile-selector", json!({ "type": "null" })),
            ]
        );
        assert_eq!(
            [
                JsonType::Null,
                JsonType::Boolean,
                JsonType::Number,
                JsonType::String,
                JsonType::Array,
                JsonType::Object
            ]
            .map(JsonType::id),
            ["null", "boolean", "number", "string", "array", "object"]
        );
    }

    #[test]
    fn equal_records_share_a_token_and_origins_keep_them_apart() {
        // Two exact sources of one domain are grouped under one token: their
        // payloads never enter the record.
        let first = source();
        let second =
            Redacted::source_identity(O::ConfigurationFile, &exact("intlify-file-source.v0:other"))
                .unwrap();
        assert_eq!(first, second);
        let programmatic = Redacted::source_identity(
            O::ConfigurationProgrammatic,
            &exact("intlify-file-source.v0:storefront/intlify.config.json"),
        )
        .unwrap();
        let tokens = EvidenceTokens::assign(PROFILE, [&first, &second, &programmatic], &Sha256);
        assert_eq!(
            tokens.token(PROFILE, &first),
            tokens.token(PROFILE, &second)
        );
        // Equal projections under different origins are different tokens
        // (D15:3362), as are the two path-member kinds.
        assert_ne!(
            tokens.token(PROFILE, &first),
            tokens.token(PROFILE, &programmatic)
        );
        let input = Redacted::input_path_member(O::ConfigurationFile).unwrap();
        let value = Redacted::value_path_member(O::ConfigurationFile).unwrap();
        let members = EvidenceTokens::assign(PROFILE, [&input, &value], &Sha256);
        assert_ne!(
            members.token(PROFILE, &input),
            members.token(PROFILE, &value)
        );
        // A component outside the assigned set has no token.
        assert_eq!(tokens.token(PROFILE, &input), None);
    }

    #[test]
    fn every_evidence_record_of_an_invocation_forms_one_colliding_set() {
        let cases = every_kind();
        let components: Vec<_> = cases.iter().map(|(component, _)| component).collect();
        let tokens = EvidenceTokens::assign(PROFILE, components.iter().copied(), &Constant);
        let constant = format!("sha256:{}", "07".repeat(32));
        // Ordinals follow the ascending bytes of the records, across kinds and
        // origins.
        let mut records: Vec<(Vec<u8>, usize)> = components
            .iter()
            .enumerate()
            .map(|(index, component)| (super::record(PROFILE, component), index))
            .collect();
        records.sort();
        for (ordinal, (_, index)) in records.iter().enumerate() {
            assert_eq!(
                tokens.token(PROFILE, components[*index]).unwrap().view(),
                json!({ "token": constant, "collision": ordinal.to_string() })
            );
        }
        // Seven records take one comparison per pair.
        assert_eq!(tokens.comparisons(), 21);
    }

    /// D15:3362: a subject record and an evidence record with one digest form
    /// no colliding set.
    #[test]
    fn subject_and_evidence_records_never_share_a_set() {
        let subject = Subject::Vocabulary;
        let subjects = Tokens::assign(PROFILE, [&subject], &Constant);
        let evidence = EvidenceTokens::assign(PROFILE, [&source()], &Constant);
        let constant = format!("sha256:{}", "07".repeat(32));
        assert_eq!(
            subjects.token(PROFILE, &subject).unwrap().view(),
            json!({ "token": constant })
        );
        assert_eq!(
            evidence.token(PROFILE, &source()).unwrap().view(),
            json!({ "token": constant })
        );
        assert_eq!(subjects.comparisons() + evidence.comparisons(), 0);
    }

    #[test]
    fn a_component_an_origin_cannot_supply_or_a_misplaced_length_has_no_token() {
        for (kind, origin, built) in [
            (
                TokenKind::SourceIdentity,
                O::ProfileSelectorCli,
                Redacted::source_identity(
                    O::ProfileSelectorCli,
                    &exact("intlify-file-source.v0:x"),
                ),
            ),
            (
                TokenKind::InputPathMember,
                O::IntentSurfaceClassVocabulary,
                Redacted::input_path_member(O::IntentSurfaceClassVocabulary),
            ),
            (
                TokenKind::ValuePathMember,
                O::ImplementationCapability,
                Redacted::value_path_member(O::ImplementationCapability),
            ),
            (
                TokenKind::ComponentKey,
                O::LocaleCanonicalizationData,
                Redacted::component_key(O::LocaleCanonicalizationData, JsonType::Object),
            ),
            (
                TokenKind::ProfileSelector,
                O::ConfigurationFile,
                Redacted::selector(O::ConfigurationFile, JsonType::Null, None),
            ),
        ] {
            assert_eq!(
                built,
                Err(EvidenceTokenInvariant::OriginOutsideKind(kind, origin))
            );
        }
        assert_eq!(
            Redacted::selector(O::ProfileSelectorCli, JsonType::String, None),
            Err(EvidenceTokenInvariant::LengthOutsideType(JsonType::String))
        );
        assert_eq!(
            Redacted::selector(
                O::ProfileSelectorCli,
                JsonType::Number,
                Some(StringLengthFact::Exact(1))
            ),
            Err(EvidenceTokenInvariant::LengthOutsideType(JsonType::Number))
        );
        // The vocabulary also carries keys, and every configuration origin
        // carries identities and members.
        assert!(Redacted::component_key(O::IntentSurfaceClassVocabulary, JsonType::String).is_ok());
        assert!(Redacted::value_path_member(O::ConfigurationProgrammatic).is_ok());
    }

    #[test]
    fn kinds_keep_015s_ids() {
        assert_eq!(
            [
                TokenKind::SourceIdentity,
                TokenKind::InputPathMember,
                TokenKind::ValuePathMember,
                TokenKind::ComponentKey,
                TokenKind::ProfileSelector,
            ]
            .map(TokenKind::id),
            [
                "source-identity",
                "logical-input-path-member",
                "value-path-member",
                "resolver-input-component-key",
                "profile-selector",
            ]
        );
    }
}
