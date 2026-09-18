// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 017 authoring value types: owner-qualified identity, exact input pins,
//! source snapshots, and occurrence evidence.
//!
//! These types validate structure only. A well-formed occurrence is not proof
//! that the named bytes exist, that the digest matches them, or that the caller
//! is authorized to analyze them. Checking evidence against an actual immutable
//! snapshot belongs to the Producer that supplies the bytes.

use intlify_shared_json::token::{IdentityFailure, IntegrityDigest, Token, VersionedIdentity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A semantic digest and an integrity digest share the `sha256:` presentation
/// but never the same meaning. The alias keeps the reading site explicit.
pub type SemanticDigest = IntegrityDigest;

/// Complete failure of an authoring primitive construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveError {
    /// A token, digest, or opaque value is outside its exact grammar.
    InvalidToken,
    /// Text was required to be nonempty.
    EmptyText,
    /// A half-open range ends before it starts.
    ReversedRange,
    /// A range extends past the byte length it addresses.
    RangeOutsideSource,
}

impl From<IdentityFailure> for PrimitiveError {
    fn from(_: IdentityFailure) -> Self {
        Self::InvalidToken
    }
}

fn valid_opaque128(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

intlify_shared_json::shared_string_type!(
    pub Opaque128,
    valid_opaque128,
    "^[0-9a-f]{32}$",
    "invalid 128-bit opaque value"
);

intlify_shared_json::shared_string_type!(
    pub NonemptyText,
    str_is_nonempty,
    "^(?s).+$",
    "value must be a nonempty string"
);

fn str_is_nonempty(value: &str) -> bool {
    !value.is_empty()
}

/// Which owner domain an identity belongs to.
///
/// The same identity token under the two kinds denotes different owners.
/// Spelling is not proof of publisher identity or authorization.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum OwnerKind {
    Application,
    Library,
}

// An application's owner identity is its checked 015 `projectId`, not a
// configuration-scoped Profile ID, package name, Selection Scope, or path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerIdentity {
    kind: OwnerKind,
    identity: Token,
}

impl OwnerIdentity {
    /// Validate and retain one owner-qualified identity.
    pub fn new(kind: OwnerKind, identity: &str) -> Result<Self, PrimitiveError> {
        Ok(Self {
            kind,
            identity: Token::new(identity)?,
        })
    }

    /// Return the owner domain.
    #[must_use]
    pub const fn kind(&self) -> OwnerKind {
        self.kind
    }

    /// Borrow the owner-local identity token.
    #[must_use]
    pub const fn identity(&self) -> &Token {
        &self.identity
    }
}

// Identity equality compares the complete pair. Equal owner-local values under
// different owners identify different Intents, so a consumer must never merge
// them by comparing the local value alone.
//
// Phase 1 validates this spelling but never mints one: allocation, registry
// admission, and reconciliation are Phase 3 operations.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageIntentId {
    owner: OwnerIdentity,
    value: Opaque128,
}

impl MessageIntentId {
    /// Retain one already allocated identity for comparison or replay.
    ///
    /// This is not an allocation. A new owner-local value comes from an
    /// authorized registry-update host using operating-system cryptographic
    /// randomness, outside read-only authoring.
    pub fn retained(owner: OwnerIdentity, value: &str) -> Result<Self, PrimitiveError> {
        Ok(Self {
            owner,
            value: Opaque128::from_validated(value)?,
        })
    }

    /// Borrow the owning application or library.
    #[must_use]
    pub const fn owner(&self) -> &OwnerIdentity {
        &self.owner
    }
}

// An exact dependency pin: identity, revision, and the semantic digest of the
// checked input. A pin is not the input body, and it cannot replace a missing
// checked input at admission time.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExactInputBinding {
    identity: Token,
    revision: Token,
    semantic_digest: SemanticDigest,
}

impl ExactInputBinding {
    /// Validate and retain one exact input pin.
    pub fn new(
        identity: &str,
        revision: &str,
        semantic_digest: &str,
    ) -> Result<Self, PrimitiveError> {
        Ok(Self {
            identity: Token::new(identity)?,
            revision: Token::new(revision)?,
            semantic_digest: serde_json::from_value(serde_json::Value::String(
                semantic_digest.to_owned(),
            ))
            .map_err(|_| PrimitiveError::InvalidToken)?,
        })
    }

    /// Borrow the pinned identity.
    #[must_use]
    pub const fn identity(&self) -> &Token {
        &self.identity
    }

    /// Borrow the pinned revision.
    #[must_use]
    pub const fn revision(&self) -> &Token {
        &self.revision
    }
}

/// One half-open UTF-8 byte range.
///
/// Empty insertion ranges and end-of-input positions are representable. A range
/// addresses bytes, never Unicode scalar values or UTF-16 code units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ByteRange {
    #[schemars(with = "String")]
    #[serde(serialize_with = "serialize_offset")]
    start: u64,
    #[schemars(with = "String")]
    #[serde(serialize_with = "serialize_offset")]
    end: u64,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde requires the reference form
fn serialize_offset<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    intlify_shared_json::quantity::Quantity::new(*value).serialize(serializer)
}

impl<'de> Deserialize<'de> for ByteRange {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            start: intlify_shared_json::quantity::Quantity,
            end: intlify_shared_json::quantity::Quantity,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.start.get(), wire.end.get())
            .map_err(|_| serde::de::Error::custom("range ends before it starts"))
    }
}

impl ByteRange {
    /// Validate and retain one half-open range.
    pub fn new(start: u64, end: u64) -> Result<Self, PrimitiveError> {
        if end < start {
            return Err(PrimitiveError::ReversedRange);
        }
        Ok(Self { start, end })
    }

    /// Return the inclusive start offset.
    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }

    /// Return the exclusive end offset.
    #[must_use]
    pub const fn end(self) -> u64 {
        self.end
    }

    /// Return the addressed byte count.
    #[must_use]
    pub const fn len(self) -> u64 {
        self.end - self.start
    }

    /// Return whether the range addresses no bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// The syntax role an occurrence plays in its source unit.
///
/// Declaration facts admit only the three declaration roles. The remaining
/// roles describe use sites, parameter expressions, and explicit exclusions.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum OccurrenceRole {
    UiLiteral,
    IntentLiteral,
    Mf2Declaration,
    Reference,
    ParameterExpression,
    Exclusion,
}

impl OccurrenceRole {
    /// Return whether this role establishes a message's source semantics.
    #[must_use]
    pub const fn is_declaration(self) -> bool {
        matches!(
            self,
            Self::UiLiteral | Self::IntentLiteral | Self::Mf2Declaration
        )
    }

    /// Return the exact wire spelling, used for deterministic ordering.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UiLiteral => "ui-literal",
            Self::IntentLiteral => "intent-literal",
            Self::Mf2Declaration => "mf2-declaration",
            Self::Reference => "reference",
            Self::ParameterExpression => "parameter-expression",
            Self::Exclusion => "exclusion",
        }
    }
}

// One immutable host source snapshot. The unit token is a caller-supplied
// identity within the owner; it is not a file path or a persistent Intent ID,
// and a move may change it without changing an established Intent identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceSnapshot {
    owner: OwnerIdentity,
    unit: Token,
    revision: Token,
    grammar: VersionedIdentity,
    #[schemars(with = "String")]
    #[serde(with = "offset")]
    byte_length: u64,
    utf8_digest: IntegrityDigest,
}

mod offset {
    use intlify_shared_json::quantity::Quantity;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[allow(clippy::trivially_copy_pass_by_ref)] // serde requires the reference form
    pub(super) fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        Quantity::new(*value).serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        Quantity::deserialize(deserializer).map(Quantity::get)
    }
}

impl SourceSnapshot {
    /// Validate and retain one source snapshot reference.
    ///
    /// `utf8_digest` is the complete digest of the exact source bytes. This
    /// constructor records the claim; verifying it against actual bytes is the
    /// supplying Producer's obligation.
    pub fn new(
        owner: OwnerIdentity,
        unit: &str,
        revision: &str,
        grammar: VersionedIdentity,
        byte_length: u64,
        utf8_digest: &str,
    ) -> Result<Self, PrimitiveError> {
        Ok(Self {
            owner,
            unit: Token::new(unit)?,
            revision: Token::new(revision)?,
            grammar,
            byte_length,
            utf8_digest: serde_json::from_value(serde_json::Value::String(utf8_digest.to_owned()))
                .map_err(|_| PrimitiveError::InvalidToken)?,
        })
    }

    /// Borrow the owning application or library.
    #[must_use]
    pub const fn owner(&self) -> &OwnerIdentity {
        &self.owner
    }

    /// Borrow the owner-local source unit token.
    #[must_use]
    pub const fn unit(&self) -> &Token {
        &self.unit
    }

    /// Return the exact source byte length.
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }
}

// Where a declaration, reference, parameter expression, or exclusion appears in
// one exact snapshot. An arbitrary supplied locator is not proof of origin.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Occurrence {
    source: SourceSnapshot,
    range: ByteRange,
    role: OccurrenceRole,
}

impl Occurrence {
    /// Validate and retain one occurrence against its snapshot's byte length.
    pub fn new(
        source: SourceSnapshot,
        range: ByteRange,
        role: OccurrenceRole,
    ) -> Result<Self, PrimitiveError> {
        if range.end() > source.byte_length() {
            return Err(PrimitiveError::RangeOutsideSource);
        }
        Ok(Self {
            source,
            range,
            role,
        })
    }

    /// Borrow the snapshot this occurrence addresses.
    #[must_use]
    pub const fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    /// Return the addressed half-open range.
    #[must_use]
    pub const fn range(&self) -> ByteRange {
        self.range
    }

    /// Return the syntax role.
    #[must_use]
    pub const fn role(&self) -> OccurrenceRole {
        self.role
    }

    /// Compare two occurrences in 017's canonical order.
    ///
    /// The order is owner kind and identity, unit, revision, grammar, source
    /// digest, numeric start and end, then role spelling. Numeric offsets are
    /// compared as integers, never as their decimal strings.
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.source
            .owner
            .cmp(&other.source.owner)
            .then_with(|| self.source.unit.cmp(&other.source.unit))
            .then_with(|| self.source.revision.cmp(&other.source.revision))
            .then_with(|| self.source.grammar.cmp(&other.source.grammar))
            .then_with(|| self.source.utf8_digest.cmp(&other.source.utf8_digest))
            .then_with(|| self.range.start.cmp(&other.range.start))
            .then_with(|| self.range.end.cmp(&other.range.end))
            .then_with(|| {
                self.role
                    .as_str()
                    .as_bytes()
                    .cmp(other.role.as_str().as_bytes())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn owner() -> OwnerIdentity {
        OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap()
    }

    fn snapshot(byte_length: u64) -> SourceSnapshot {
        SourceSnapshot::new(
            owner(),
            "checkout",
            "1",
            VersionedIdentity::literal("intlify-js-grammar", "0"),
            byte_length,
            &format!("sha256:{}", "0".repeat(64)),
        )
        .unwrap()
    }

    #[test]
    fn owner_equality_compares_the_complete_kind_and_identity_pair() {
        let application = OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap();
        let library = OwnerIdentity::new(OwnerKind::Library, "storefront").unwrap();
        assert_ne!(application, library);
        assert_eq!(
            serde_json::to_value(&application).unwrap(),
            json!({"kind": "application", "identity": "storefront"})
        );
        assert_eq!(
            OwnerIdentity::new(OwnerKind::Application, "Storefront"),
            Err(PrimitiveError::InvalidToken)
        );
    }

    #[test]
    fn equal_local_intent_values_under_different_owners_stay_distinct() {
        let value = "0".repeat(32);
        let application = MessageIntentId::retained(
            OwnerIdentity::new(OwnerKind::Application, "a").unwrap(),
            &value,
        )
        .unwrap();
        let library =
            MessageIntentId::retained(OwnerIdentity::new(OwnerKind::Library, "a").unwrap(), &value)
                .unwrap();
        assert_ne!(application, library);
        assert_eq!(application.owner().kind(), OwnerKind::Application);
        for invalid in [
            "0".repeat(31),
            "0".repeat(33),
            "A".repeat(32),
            "g".repeat(32),
        ] {
            assert_eq!(
                MessageIntentId::retained(owner(), &invalid),
                Err(PrimitiveError::InvalidToken)
            );
        }
    }

    #[test]
    fn ranges_are_half_open_allow_zero_width_and_reject_reversal() {
        let empty = ByteRange::new(7, 7).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert_eq!(ByteRange::new(8, 7), Err(PrimitiveError::ReversedRange));
        assert_eq!(
            serde_json::to_value(ByteRange::new(0, 9_007_199_254_740_993).unwrap()).unwrap(),
            json!({"start": "0", "end": "9007199254740993"})
        );
        // Offsets are exact decimal strings; a JSON number is not admitted.
        assert!(serde_json::from_value::<ByteRange>(json!({"start": 0, "end": 1})).is_err());
        assert!(serde_json::from_value::<ByteRange>(json!({"start": "1", "end": "0"})).is_err());
        assert_eq!(
            serde_json::from_value::<ByteRange>(json!({"start": "1", "end": "2"})).unwrap(),
            ByteRange::new(1, 2).unwrap()
        );
    }

    #[test]
    fn occurrences_must_address_bytes_inside_their_snapshot() {
        let source = snapshot(10);
        assert!(Occurrence::new(
            source.clone(),
            ByteRange::new(10, 10).unwrap(),
            OccurrenceRole::UiLiteral
        )
        .is_ok());
        assert_eq!(
            Occurrence::new(
                source,
                ByteRange::new(0, 11).unwrap(),
                OccurrenceRole::UiLiteral
            ),
            Err(PrimitiveError::RangeOutsideSource)
        );
    }

    #[test]
    fn canonical_order_compares_offsets_numerically_not_as_decimal_text() {
        let source = snapshot(200);
        let make = |start: u64, end: u64, role| {
            Occurrence::new(source.clone(), ByteRange::new(start, end).unwrap(), role).unwrap()
        };
        let ninth = make(9, 10, OccurrenceRole::UiLiteral);
        let tenth = make(10, 11, OccurrenceRole::UiLiteral);
        assert_eq!(ninth.canonical_cmp(&tenth), std::cmp::Ordering::Less);
        // Role breaks a tie by exact spelling, after both offsets.
        let intent = make(9, 10, OccurrenceRole::IntentLiteral);
        assert_eq!(intent.canonical_cmp(&ninth), std::cmp::Ordering::Less);
        assert!(OccurrenceRole::Mf2Declaration.is_declaration());
        assert!(!OccurrenceRole::Reference.is_declaration());
    }
}
