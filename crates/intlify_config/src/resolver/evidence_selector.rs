// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Profile Selector Evidence Projection: what profile-selector
//! Evidence keeps of the selector the invocation boundary admitted, once the
//! final projection authority is fixed (D15:805-813, 2501-2502; Decision
//! 015-183).
//!
//! `absent` means no selector was supplied and keeps nothing else.
//! `safe-profile-id` keeps the exact ASCII bytes of a declared Profile ID that
//! the selector, a complete string within the final bound, exactly matched;
//! the bytes come from the admitted declaration, never from the selector.
//! Every other selector is a `redacted-value`: its JSON type, its String
//! Length Fact when it is a string, and the Redacted evidence token of only
//! those facts, so the selectors of one origin with equal facts share one
//! token and an over-limit string keeps no length.
//!
//! Projections rank `absent`, `safe-profile-id`, then `redacted-value`. IDs
//! compare by their ASCII bytes, and redacted values by JSON type rank, then
//! String Length Fact (absence, `exact(n)` in ascending length, `over-limit`),
//! then the complete token bytes and the collision ordinal, absence first.
//! Evidence compares the Safe Origin Kind before them. The derived order here
//! is that comparison. Nothing builds Evidence yet.

use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::input_limits::Bound;
use crate::model::ProfileId;
use crate::structural::selection::{
    InvalidSelectorType, SelectionInvariant, SelectorInput, SelectorShape, StringLengthFact,
};

use super::evidence_token::{
    length_view, EvidenceTokenInvariant, EvidenceTokens, JsonType, Redacted, TokenKind,
};
use super::origin::SafeOriginKind;
use super::token::{RedactedToken, Specification};

/// A resolver bug that leaves a selector without a projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SelectorEvidenceInvariant {
    /// The origin supplies no selector, or the token's facts disagree.
    Token(EvidenceTokenInvariant),
    /// The final authority is larger than the bound an over-limit string was
    /// observed under, so its String Length Fact is unknown.
    Selection(SelectionInvariant),
}

/// A selector as Evidence projects it, before the invocation's evidence
/// tokens are assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SelectorProjection {
    Absent,
    SafeProfileId(Arc<str>),
    RedactedValue {
        kind: JsonType,
        length: Option<StringLengthFact>,
        component: Redacted,
    },
}

impl SelectorProjection {
    /// Project `input`, which an input of `origin` supplied, under the final
    /// projection authority `authority` and the independently admitted
    /// declared Profile IDs `declared` (D15:807-809, 820).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an origin that supplies no selector, or of
    /// an authority that an over-limit string was not observed under.
    pub(crate) fn project<'d>(
        origin: SafeOriginKind,
        input: &SelectorInput,
        authority: Bound,
        declared: impl IntoIterator<Item = &'d ProfileId>,
    ) -> Result<Self, SelectorEvidenceInvariant> {
        if !TokenKind::ProfileSelector.admits(origin) {
            return Err(SelectorEvidenceInvariant::Token(
                EvidenceTokenInvariant::OriginOutsideKind(TokenKind::ProfileSelector, origin),
            ));
        }
        let kind = match input.shape() {
            SelectorShape::Absent => return Ok(Self::Absent),
            SelectorShape::String => JsonType::String,
            SelectorShape::Invalid(kind) => json_type(kind),
        };
        if let Some(id) = input.declared(authority, declared) {
            return Ok(Self::SafeProfileId(Arc::from(id.as_str())));
        }
        let length = input
            .length_fact(authority)
            .map_err(SelectorEvidenceInvariant::Selection)?;
        let component =
            Redacted::selector(origin, kind, length).map_err(SelectorEvidenceInvariant::Token)?;
        Ok(Self::RedactedValue {
            kind,
            length,
            component,
        })
    }

    /// The component a token replaces, when the selector is redacted.
    pub(crate) const fn redacted(&self) -> Option<&Redacted> {
        match self {
            Self::Absent | Self::SafeProfileId(_) => None,
            Self::RedactedValue { component, .. } => Some(component),
        }
    }

    /// The projection Evidence retains, `None` when the selector is redacted
    /// and `tokens` was not assigned its component.
    pub(crate) fn retained(
        &self,
        specification: Specification<'_>,
        tokens: &EvidenceTokens,
    ) -> Option<SelectorEvidence> {
        Some(match self {
            Self::Absent => SelectorEvidence::Absent,
            Self::SafeProfileId(id) => SelectorEvidence::SafeProfileId(Arc::clone(id)),
            Self::RedactedValue {
                kind,
                length,
                component,
            } => SelectorEvidence::RedactedValue {
                kind: *kind,
                length: *length,
                token: tokens.token(specification, component)?,
            },
        })
    }
}

/// The JSON type tag of a selector of an invalid type.
const fn json_type(kind: InvalidSelectorType) -> JsonType {
    match kind {
        InvalidSelectorType::Null => JsonType::Null,
        InvalidSelectorType::Boolean => JsonType::Boolean,
        InvalidSelectorType::Number => JsonType::Number,
        InvalidSelectorType::Array => JsonType::Array,
        InvalidSelectorType::Object => JsonType::Object,
    }
}

/// A retained Profile Selector Evidence Projection, in 015's order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SelectorEvidence {
    Absent,
    SafeProfileId(Arc<str>),
    RedactedValue {
        kind: JsonType,
        length: Option<StringLengthFact>,
        token: RedactedToken,
    },
}

impl SelectorEvidence {
    /// `{ "absent": null }`, `{ "safe-profile-id": text }`, or
    /// `{ "redacted-value": { type, length?, token } }` (D15:2501-2502, 2506).
    pub(crate) fn view(&self) -> Value {
        match self {
            Self::Absent => json!({ "absent": null }),
            Self::SafeProfileId(id) => json!({ "safe-profile-id": id.as_ref() }),
            Self::RedactedValue {
                kind,
                length,
                token,
            } => {
                let mut value = Map::new();
                value.insert("type".to_owned(), json!(kind.id()));
                if let Some(length) = length {
                    value.insert("length".to_owned(), length_view(*length));
                }
                value.insert("token".to_owned(), token.view());
                json!({ "redacted-value": value })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::input_limits::Bound;
    use crate::model::ProfileId;
    use crate::resolver::evidence_token::{
        EvidenceTokenInvariant, EvidenceTokens, JsonType, TokenKind,
    };
    use crate::resolver::origin::SafeOriginKind as O;
    use crate::resolver::token::fixtures::LastByte;
    use crate::resolver::token::{RedactedToken, Sha256, Specification};
    use crate::structural::selection::{
        InvalidSelectorType, SelectionInvariant, SelectorInput, StringLengthFact,
    };

    use super::{SelectorEvidence, SelectorEvidenceInvariant, SelectorProjection};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };
    /// PR 5d-1's selector vectors, computed apart from this crate: a string of
    /// 12 bytes and a null under `profile-selector-cli`, and an over-limit
    /// string under `profile-selector-programmatic`.
    const EXACT_12: &str =
        "sha256:6bab6f0d58d5a25b8824c38ebda6c4767bd9ee9d2b07aede68066b2bbdb6a3da";
    const NULL: &str = "sha256:f1b67b4a6fd933b358e4f99e5599be4141a1ab7184a9649ff597f96298e70966";
    const OVER_LIMIT: &str =
        "sha256:bd0cf2a8e189cc9d44491f753464d5676936b57ecef39b168e0c9043d86d4adc";

    fn bound(value: u64) -> Bound {
        Bound::new(value).unwrap()
    }

    fn ids(names: &[&str]) -> Vec<ProfileId> {
        names
            .iter()
            .map(|name| serde_json::from_value(json!(name)).unwrap())
            .collect()
    }

    fn project(origin: O, input: &SelectorInput, authority: u64) -> SelectorProjection {
        SelectorProjection::project(
            origin,
            input,
            bound(authority),
            &ids(&["app", "storefront"]),
        )
        .unwrap()
    }

    /// The retained projections of `projected`, their tokens assigned
    /// together.
    fn retain(projected: &[SelectorProjection]) -> Vec<SelectorEvidence> {
        let tokens = EvidenceTokens::assign(
            PROFILE,
            projected.iter().filter_map(SelectorProjection::redacted),
            &Sha256,
        );
        projected
            .iter()
            .map(|projection| projection.retained(PROFILE, &tokens).unwrap())
            .collect()
    }

    fn views(projected: &[SelectorProjection]) -> Vec<serde_json::Value> {
        retain(projected)
            .iter()
            .map(SelectorEvidence::view)
            .collect()
    }

    #[test]
    fn each_variant_keeps_only_its_facts() {
        let projected = [
            project(O::ProfileSelectorCli, &SelectorInput::absent(bound(64)), 64),
            project(
                O::ProfileSelectorCli,
                &SelectorInput::string("app", bound(64)),
                64,
            ),
            project(
                O::ProfileSelectorCli,
                &SelectorInput::string("unknown-prof", bound(64)),
                64,
            ),
            project(
                O::ProfileSelectorProgrammatic,
                &SelectorInput::over_limit(bound(8)),
                8,
            ),
            project(
                O::ProfileSelectorCli,
                &SelectorInput::invalid_type(InvalidSelectorType::Null, bound(64)),
                64,
            ),
        ];
        assert_eq!(
            views(&projected),
            [
                json!({ "absent": null }),
                json!({ "safe-profile-id": "app" }),
                json!({ "redacted-value": {
                    "type": "string", "length": { "exact": "12" }, "token": { "token": EXACT_12 },
                } }),
                json!({ "redacted-value": {
                    "type": "string", "length": "over-limit", "token": { "token": OVER_LIMIT },
                } }),
                json!({ "redacted-value": { "type": "null", "token": { "token": NULL } } }),
            ]
        );
    }

    /// A declared ID is kept only as a complete string within the final
    /// authority; past it, or outside the syntax, or undeclared, the string is
    /// redacted with its length.
    #[test]
    fn only_a_declared_id_within_the_final_authority_is_kept() {
        let string = |value: &str, authority| {
            project(
                O::ProfileSelectorCli,
                &SelectorInput::string(value, bound(64)),
                authority,
            )
        };
        let retained = retain(&[
            string("storefront", 10),
            string("storefront", 9),
            string("App", 64),
            string("apps", 64),
        ]);
        assert_eq!(
            retained[0],
            SelectorEvidence::SafeProfileId("storefront".into())
        );
        let lengths: Vec<_> = retained[1..]
            .iter()
            .map(|evidence| evidence.view()["redacted-value"]["length"].clone())
            .collect();
        assert_eq!(
            lengths,
            [
                json!("over-limit"),
                json!({ "exact": "3" }),
                json!({ "exact": "4" })
            ]
        );
    }

    #[test]
    fn every_invalid_type_keeps_only_its_tag() {
        use InvalidSelectorType::{Array, Boolean, Null, Number, Object};
        let projected: Vec<_> = [Null, Boolean, Number, Array, Object]
            .into_iter()
            .map(|kind| {
                project(
                    O::ProfileSelectorCli,
                    &SelectorInput::invalid_type(kind, bound(64)),
                    64,
                )
            })
            .collect();
        let tags: Vec<_> = views(&projected)
            .iter()
            .map(|view| {
                assert!(view["redacted-value"].get("length").is_none(), "{view}");
                view["redacted-value"]["type"].clone()
            })
            .collect();
        assert_eq!(
            tags,
            [
                json!("null"),
                json!("boolean"),
                json!("number"),
                json!("array"),
                json!("object")
            ]
        );
    }

    /// The origin is checked even when the selector is absent, and an
    /// over-limit string has no fact under an authority larger than the
    /// bound it was observed under.
    #[test]
    fn a_wrong_origin_or_an_unknown_length_is_a_resolver_bug() {
        for input in [
            SelectorInput::absent(bound(64)),
            SelectorInput::string("app", bound(64)),
        ] {
            assert_eq!(
                SelectorProjection::project(O::ConfigurationFile, &input, bound(64), &ids(&[])),
                Err(SelectorEvidenceInvariant::Token(
                    EvidenceTokenInvariant::OriginOutsideKind(
                        TokenKind::ProfileSelector,
                        O::ConfigurationFile
                    )
                ))
            );
        }
        assert_eq!(
            SelectorProjection::project(
                O::ProfileSelectorCli,
                &SelectorInput::over_limit(bound(8)),
                bound(9),
                &ids(&[]),
            ),
            Err(SelectorEvidenceInvariant::Selection(
                SelectionInvariant::BootstrapBoundMismatch
            ))
        );
    }

    #[test]
    fn a_redacted_selector_without_its_token_is_not_retained() {
        let tokens = EvidenceTokens::assign(PROFILE, [], &Sha256);
        let redacted = project(
            O::ProfileSelectorCli,
            &SelectorInput::string("apps", bound(64)),
            64,
        );
        assert_eq!(redacted.retained(PROFILE, &tokens), None);
        let absent = project(O::ProfileSelectorCli, &SelectorInput::absent(bound(64)), 64);
        assert_eq!(
            absent.retained(PROFILE, &tokens),
            Some(SelectorEvidence::Absent)
        );
    }

    /// Safe IDs compare by their exact unsigned ASCII bytes: not by length,
    /// which would put `b` first, and with `-`, `.`, digits, and `_` where
    /// their bytes put them (D15:287, 813, 3499).
    #[test]
    fn safe_ids_compare_by_their_exact_ascii_bytes() {
        let names = ["b", "aa", "a_b", "a0", "a.b", "a-b"];
        let declared = ids(&names);
        let none = EvidenceTokens::assign(PROFILE, [], &Sha256);
        let mut retained: Vec<_> = names
            .iter()
            .map(|name| {
                SelectorProjection::project(
                    O::ProfileSelectorCli,
                    &SelectorInput::string(name, bound(64)),
                    bound(64),
                    &declared,
                )
                .unwrap()
                .retained(PROFILE, &none)
                .unwrap()
            })
            .collect();
        retained.sort();
        let sorted: Vec<_> = retained.iter().map(SelectorEvidence::view).collect();
        assert_eq!(
            sorted,
            ["a-b", "a.b", "a0", "a_b", "aa", "b"].map(|id| json!({ "safe-profile-id": id }))
        );
    }

    /// Redacted values rank by all six JSON types, a string's facts by
    /// ascending exact length, numerically, and then `over-limit`, and a value
    /// without a fact before any with one (D15:811, 813, 3500).
    #[test]
    fn redacted_values_rank_by_type_then_string_length_fact() {
        use InvalidSelectorType::{Array, Boolean, Null, Number, Object};
        let cli = |input: SelectorInput| project(O::ProfileSelectorCli, &input, 64);
        let invalid = |kind| cli(SelectorInput::invalid_type(kind, bound(64)));
        let mut retained = retain(&[
            invalid(Object),
            invalid(Array),
            cli(SelectorInput::over_limit(bound(64))),
            cli(SelectorInput::string("abcdefghij", bound(64))),
            cli(SelectorInput::string("ab", bound(64))),
            invalid(Number),
            invalid(Boolean),
            invalid(Null),
        ]);
        retained.sort();
        let facts: Vec<_> = retained
            .iter()
            .map(|evidence| {
                let view = evidence.view();
                json!([
                    view["redacted-value"]["type"],
                    view["redacted-value"]["length"]
                ])
            })
            .collect();
        assert_eq!(
            facts,
            [
                json!(["null", null]),
                json!(["boolean", null]),
                json!(["number", null]),
                json!(["string", { "exact": "2" }]),
                json!(["string", { "exact": "10" }]),
                json!(["string", "over-limit"]),
                json!(["array", null]),
                json!(["object", null]),
            ]
        );
        // No admitted string lacks a fact, so absence is compared directly.
        let with = |length| SelectorEvidence::RedactedValue {
            kind: JsonType::String,
            length,
            token: RedactedToken::measuring(),
        };
        assert!(with(None) < with(Some(StringLengthFact::Exact(0))));
        assert!(
            with(Some(StringLengthFact::Exact(u64::MAX))) < with(Some(StringLengthFact::OverLimit))
        );
    }

    /// Selectors with equal facts share one projection and token, an
    /// over-limit one whatever its final length; across origins they compare
    /// by every digest byte and then by the ordinal (D15:809, 811, 813, 3500).
    #[test]
    fn equal_facts_group_and_then_order_by_digest_and_ordinal() {
        let string = |value: &str, authority| {
            project(
                O::ProfileSelectorCli,
                &SelectorInput::string(value, bound(1024)),
                authority,
            )
        };
        assert_eq!(string("unknown-aaaa", 64), string("unknown-bbbb", 64));
        let long = "x".repeat(1000);
        assert_eq!(string("0123456789", 9), string(&long, 9));
        let view = views(&[string(&long, 9)]).remove(0);
        assert_eq!(view["redacted-value"]["length"], json!("over-limit"));
        assert!(!view.to_string().contains("1000"), "{view}");
        let of = |origin| {
            project(
                origin,
                &SelectorInput::string("unknown-aaaa", bound(64)),
                64,
            )
        };
        let (cli, programmatic) = (
            of(O::ProfileSelectorCli),
            of(O::ProfileSelectorProgrammatic),
        );
        let under = |digest: &LastByte<'_>| {
            let tokens = EvidenceTokens::assign(
                PROFILE,
                [&cli, &programmatic].map(|p| p.redacted().unwrap()),
                digest,
            );
            (
                cli.retained(PROFILE, &tokens).unwrap(),
                programmatic.retained(PROFILE, &tokens).unwrap(),
            )
        };
        // The digest decides, against the order of the records.
        let (first, second) = under(&LastByte(&[
            ("profile-selector-programmatic", 1),
            ("profile-selector-cli", 2),
        ]));
        assert!(second < first);
        // The digest decides before the ordinal: the second ordinal of the
        // lower digest precedes the higher digest, which has none.
        let null = project(
            O::ProfileSelectorCli,
            &SelectorInput::invalid_type(InvalidSelectorType::Null, bound(64)),
            64,
        );
        let tokens = EvidenceTokens::assign(
            PROFILE,
            [&null, &cli, &programmatic].map(|p| p.redacted().unwrap()),
            &LastByte(&[
                ("profile-selector-programmatic", 1),
                ("null", 1),
                ("profile-selector-cli", 2),
            ]),
        );
        let lower = programmatic.retained(PROFILE, &tokens).unwrap();
        let higher = cli.retained(PROFILE, &tokens).unwrap();
        assert_eq!(
            lower.view()["redacted-value"]["token"]["collision"],
            json!("1")
        );
        assert!(lower < higher);
        // One digest: the ordinals, in the order of the records, and a token
        // without an ordinal before them.
        let (first, second) = under(&LastByte(&[]));
        assert!(first < second);
        let tokens = EvidenceTokens::assign(PROFILE, cli.redacted(), &LastByte(&[]));
        assert!(cli.retained(PROFILE, &tokens).unwrap() < first);
        assert_eq!(
            first.view()["redacted-value"]["token"]["collision"],
            json!("0")
        );
        assert_eq!(
            second.view()["redacted-value"]["token"]["collision"],
            json!("1")
        );
    }

    /// Variant rank, then ID bytes, then JSON type rank, String Length Fact,
    /// and token.
    #[test]
    fn projections_compare_by_variant_then_facts() {
        let cli =
            |input: SelectorInput, authority| project(O::ProfileSelectorCli, &input, authority);
        let mut retained = retain(&[
            cli(
                SelectorInput::invalid_type(InvalidSelectorType::Array, bound(64)),
                64,
            ),
            cli(SelectorInput::string("a-longer-one", bound(64)), 64),
            cli(SelectorInput::over_limit(bound(8)), 8),
            cli(SelectorInput::string("storefront", bound(64)), 64),
            cli(SelectorInput::string("x", bound(64)), 64),
            cli(SelectorInput::absent(bound(64)), 64),
            cli(
                SelectorInput::invalid_type(InvalidSelectorType::Null, bound(64)),
                64,
            ),
            cli(SelectorInput::string("app", bound(64)), 64),
        ]);
        retained.sort();
        let views: Vec<_> = retained
            .iter()
            .map(|evidence| {
                let view = evidence.view();
                view.get("redacted-value").map_or(view.clone(), |value| {
                    json!([value["type"], value.get("length")])
                })
            })
            .collect();
        assert_eq!(
            views,
            [
                json!({ "absent": null }),
                json!({ "safe-profile-id": "app" }),
                json!({ "safe-profile-id": "storefront" }),
                json!(["null", null]),
                json!(["string", { "exact": "1" }]),
                json!(["string", { "exact": "12" }]),
                json!(["string", "over-limit"]),
                json!(["array", null]),
            ]
        );
    }
}
