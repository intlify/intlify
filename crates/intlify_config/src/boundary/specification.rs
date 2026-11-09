// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `profile-resolution-specification-input` boundary: the raw form of
//! the Profile Resolution Specification Input that 015-165 fixes, read into
//! the typed input the resolver takes or refused with one of the five reasons
//! of its row (D15:1123, 3136).
//!
//! The form is one JSON object with at most one member per specification
//! slot, named as the slot is. A slot's value is `{ identity, revision }`, and
//! the `intent-surface-class-vocabulary` slot's also has `semanticDigest`; a
//! slot without a value has no member, so `null` is no absence. The checks
//! run in 015's order: an input is present; its bytes and its depth, before
//! anything is decoded; its syntax; its closed shape, which looks only at the
//! slot members and their known fields; a repeated member or field; a member
//! or field the form does not have; and last each field's 017 grammar. The
//! first check that fails decides.
//!
//! The bounds are the Admitted Implementation Capability's
//! `bootstrapOnly.inputBoundary` bytes and depth. Whether each slot is present
//! and supported is the resolver's question in Phases 0 and 5, not the
//! boundary's.

use intlify_shared_json::json::decode_unique_json;
use intlify_shared_json::token::Token;
use serde_json::{Map, Value};

use crate::artifacts::{closed, count, depth};
use crate::input_limits::Bound;
use crate::locale::VersionedIdentity;
use crate::references::valid_semantic_digest;
use crate::resolver::specification::{Slot, SpecificationInput, VocabularyAssertion};

use super::capability::ImplementationCapability;

const IDENTITY: &str = "identity";
const REVISION: &str = "revision";
const SEMANTIC_DIGEST: &str = "semanticDigest";

/// The fields of a slot's value.
const fn fields(slot: Slot) -> &'static [&'static str] {
    if matches!(slot, Slot::IntentSurfaceClassVocabulary) {
        &[IDENTITY, REVISION, SEMANTIC_DIGEST]
    } else {
        &[IDENTITY, REVISION]
    }
}

/// The bounds on one input: the capability's, or a test's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpecificationLimits {
    pub(crate) bytes: Bound,
    pub(crate) depth: Bound,
}

impl SpecificationLimits {
    const fn of(capability: &ImplementationCapability) -> Self {
        let bounds = &capability.bootstrap_only().input_boundary;
        Self {
            bytes: bounds.max_specification_input_bytes,
            depth: bounds.max_depth,
        }
    }
}

/// Which bound an input is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpecificationBound {
    /// `inputBoundary.maxSpecificationInputBytes`, on the raw bytes.
    InputBytes,
    /// `inputBoundary.maxDepth`, on the raw bytes.
    Depth,
}

/// Where an input is malformed. All are `malformed-input`; the boundary
/// reaches them at different checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Malformation {
    /// The bytes are not one JSON value in UTF-8.
    Syntax,
    /// The value is not an object, a slot member is not an object, or a known
    /// field is missing or not a string.
    Shape,
    /// A field's string breaks its 017 grammar: `IdentityToken`,
    /// `RevisionToken`, or `SemanticDigest`.
    Grammar,
}

/// Why the boundary refuses an input: one of the five reasons of its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpecificationRefusal {
    Missing,
    OverBound(SpecificationBound),
    Malformed(Malformation),
    DuplicateMember,
    UnknownMember,
}

impl SpecificationRefusal {
    /// The reason as 015's boundary table spells it.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing-input",
            Self::OverBound(_) => "over-bound-member",
            Self::Malformed(_) => "malformed-input",
            Self::DuplicateMember => "duplicate-member",
            Self::UnknownMember => "unknown-member",
        }
    }
}

/// Admit a submitted input under the capability's bounds. An absent input is
/// `None`.
pub(crate) fn admit(
    bytes: Option<&[u8]>,
    capability: &ImplementationCapability,
) -> Result<SpecificationInput<String>, SpecificationRefusal> {
    read_input(bytes, SpecificationLimits::of(capability))
}

/// The boundary's checks under `limits`, which [`admit`] takes from the
/// capability.
pub(crate) fn read_input(
    bytes: Option<&[u8]>,
    limits: SpecificationLimits,
) -> Result<SpecificationInput<String>, SpecificationRefusal> {
    use SpecificationRefusal::{DuplicateMember, Malformed, Missing, OverBound, UnknownMember};

    let bytes = bytes.ok_or(Missing)?;
    if count(bytes.len()) > limits.bytes.get() {
        return Err(OverBound(SpecificationBound::InputBytes));
    }
    if depth(bytes) > limits.depth.get() {
        return Err(OverBound(SpecificationBound::Depth));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Malformed(Malformation::Syntax))?;
    let mut value: Value =
        serde_json::from_str(text).map_err(|_| Malformed(Malformation::Syntax))?;
    // A member or field outside the form is reported after the shape, so it is
    // taken out first and remembered.
    let outside = strip(&mut value);
    let input = shaped(&value).ok_or(Malformed(Malformation::Shape))?;
    // The text parsed above, so a repeated member is all that can fail here.
    if decode_unique_json(text).is_err() {
        return Err(DuplicateMember);
    }
    if outside {
        return Err(UnknownMember);
    }
    typed(input)
}

/// Take every member that names no slot, and every field its slot does not
/// have, out of the input, and say whether there was one. A slot member that
/// is not an object keeps its value for the shape to refuse.
fn strip(value: &mut Value) -> bool {
    let Some(input) = value.as_object_mut() else {
        return false;
    };
    let mut outside = false;
    input.retain(|name, _| {
        let known = Slot::ALL.iter().any(|slot| slot.name() == name);
        outside |= !known;
        known
    });
    for slot in Slot::ALL {
        if let Some(Value::Object(member)) = input.get_mut(slot.name()) {
            member.retain(|field, _| {
                let known = fields(slot).contains(&field.as_str());
                outside |= !known;
                known
            });
        }
    }
    outside
}

/// The slot members, when the input is an object and every slot member is an
/// object whose fields are exactly its slot's, each a string.
fn shaped(value: &Value) -> Option<&Map<String, Value>> {
    let input = value.as_object()?;
    Slot::ALL
        .iter()
        .all(|&slot| {
            input.get(slot.name()).is_none_or(|member| {
                closed(member, fields(slot))
                    .is_some_and(|member| member.values().all(Value::is_string))
            })
        })
        .then_some(input)
}

/// A field's string, when it is an `IdentityToken`, which is also the
/// `RevisionToken` grammar.
fn token(member: &Value, field: &str) -> Option<String> {
    member
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| Token::new(text).is_ok())
        .map(str::to_owned)
}

fn versioned(member: &Value) -> Option<VersionedIdentity<String>> {
    Some(VersionedIdentity {
        identity: token(member, IDENTITY)?,
        revision: token(member, REVISION)?,
    })
}

fn vocabulary(member: &Value) -> Option<VocabularyAssertion<String>> {
    let semantic_digest = member
        .get(SEMANTIC_DIGEST)
        .and_then(Value::as_str)
        .filter(|text| valid_semantic_digest(text))?;
    Some(VocabularyAssertion {
        identity: token(member, IDENTITY)?,
        revision: token(member, REVISION)?,
        semantic_digest: semantic_digest.to_owned(),
    })
}

/// The slot's value read by `read`, absent when the slot has no member, or
/// `malformed-input` when a field breaks its grammar.
fn slot<T>(
    input: &Map<String, Value>,
    slot: Slot,
    read: impl FnOnce(&Value) -> Option<T>,
) -> Result<Option<T>, SpecificationRefusal> {
    input
        .get(slot.name())
        .map(|member| read(member).ok_or(SpecificationRefusal::Malformed(Malformation::Grammar)))
        .transpose()
}

/// The typed input, when every field of every present slot keeps its grammar.
fn typed(input: &Map<String, Value>) -> Result<SpecificationInput<String>, SpecificationRefusal> {
    Ok(SpecificationInput {
        project_profile: slot(input, Slot::ProjectProfile, versioned)?,
        locale_canonicalization: slot(input, Slot::LocaleCanonicalization, versioned)?,
        locale_negotiation: slot(input, Slot::LocaleNegotiation, versioned)?,
        message_locale_fallback: slot(input, Slot::MessageLocaleFallback, versioned)?,
        coverage: slot(input, Slot::Coverage, versioned)?,
        intent_surface_class_vocabulary: slot(
            input,
            Slot::IntentSurfaceClassVocabulary,
            vocabulary,
        )?,
        delivery_graph: slot(input, Slot::DeliveryGraph, versioned)?,
        delivery_placement: slot(input, Slot::DeliveryPlacement, versioned)?,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::capability::{
        IDENTITY as CAPABILITY, REVISION as CAPABILITY_REVISION,
    };
    use crate::resolver::limits::fixture_limits;

    const DIGEST: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

    /// A complete input: every slot with the value revision `"0"` supports, and
    /// a vocabulary assertion.
    fn complete() -> Value {
        json!({
            "project-profile": { "identity": "intlify-project-profile", "revision": "0" },
            "locale-canonicalization": { "identity": "intlify-locale-canonicalization", "revision": "0" },
            "locale-negotiation": { "identity": "intlify-locale-negotiation", "revision": "0" },
            "message-locale-fallback": { "identity": "intlify-message-locale-fallback", "revision": "0" },
            "coverage": { "identity": "intlify-localization-coverage", "revision": "0" },
            "intent-surface-class-vocabulary": {
                "identity": "storefront-surfaces",
                "revision": "1",
                "semanticDigest": DIGEST,
            },
            "delivery-graph": { "identity": "intlify-delivery-graph", "revision": "0" },
            "delivery-placement": { "identity": "intlify-delivery-placement", "revision": "0" },
        })
    }

    fn tuple(identity: &str, revision: &str) -> VersionedIdentity<String> {
        VersionedIdentity {
            identity: identity.to_owned(),
            revision: revision.to_owned(),
        }
    }

    fn largest() -> Bound {
        Bound::new(u64::MAX - 1).unwrap()
    }

    fn roomy() -> SpecificationLimits {
        SpecificationLimits {
            bytes: largest(),
            depth: largest(),
        }
    }

    fn read_text(text: &str) -> Result<SpecificationInput<String>, SpecificationRefusal> {
        read_input(Some(text.as_bytes()), roomy())
    }

    fn read(value: &Value) -> Result<SpecificationInput<String>, SpecificationRefusal> {
        read_text(&value.to_string())
    }

    fn edited(edit: impl FnOnce(&mut Map<String, Value>)) -> Value {
        let mut value = complete();
        edit(value.as_object_mut().unwrap());
        value
    }

    /// The value of `slot`'s member in `object`.
    fn field<'a>(object: &'a mut Map<String, Value>, slot: &str) -> &'a mut Map<String, Value> {
        object.get_mut(slot).unwrap().as_object_mut().unwrap()
    }

    const SHAPE: SpecificationRefusal = SpecificationRefusal::Malformed(Malformation::Shape);
    const GRAMMAR: SpecificationRefusal = SpecificationRefusal::Malformed(Malformation::Grammar);
    const SYNTAX: SpecificationRefusal = SpecificationRefusal::Malformed(Malformation::Syntax);

    #[test]
    fn a_complete_input_is_admitted_slot_by_slot() {
        let input = read(&complete()).unwrap();
        assert_eq!(
            input,
            SpecificationInput {
                project_profile: Some(tuple("intlify-project-profile", "0")),
                locale_canonicalization: Some(tuple("intlify-locale-canonicalization", "0")),
                locale_negotiation: Some(tuple("intlify-locale-negotiation", "0")),
                message_locale_fallback: Some(tuple("intlify-message-locale-fallback", "0")),
                coverage: Some(tuple("intlify-localization-coverage", "0")),
                intent_surface_class_vocabulary: Some(VocabularyAssertion {
                    identity: "storefront-surfaces".to_owned(),
                    revision: "1".to_owned(),
                    semantic_digest: DIGEST.to_owned(),
                }),
                delivery_graph: Some(tuple("intlify-delivery-graph", "0")),
                delivery_placement: Some(tuple("intlify-delivery-placement", "0")),
            }
        );
    }

    /// A slot without a member is absent, and the empty object is an input
    /// with every slot absent: whether a slot must be present is the
    /// resolver's question, not the boundary's.
    #[test]
    fn an_omitted_member_is_an_absent_slot() {
        let input = read(&edited(|object| {
            object.remove("coverage");
            object.remove("intent-surface-class-vocabulary");
        }))
        .unwrap();
        assert_eq!(input.coverage, None);
        assert_eq!(input.intent_surface_class_vocabulary, None);
        assert_eq!(
            input.project_profile,
            Some(tuple("intlify-project-profile", "0"))
        );
        let empty = read_text("{}").unwrap();
        assert_eq!(
            empty,
            SpecificationInput {
                project_profile: None,
                locale_canonicalization: None,
                locale_negotiation: None,
                message_locale_fallback: None,
                coverage: None,
                intent_surface_class_vocabulary: None,
                delivery_graph: None,
                delivery_placement: None,
            }
        );
        // Member order carries no meaning.
        let reordered =
            r#"{"coverage":{"revision":"0","identity":"intlify-localization-coverage"}}"#;
        assert_eq!(
            read_text(reordered).unwrap().coverage,
            Some(tuple("intlify-localization-coverage", "0"))
        );
    }

    #[test]
    fn an_absent_input_is_missing() {
        assert_eq!(
            read_input(None, roomy()),
            Err(SpecificationRefusal::Missing)
        );
    }

    /// The bytes and the depth are each bounded before anything is decoded,
    /// so an input over a bound is refused even when it would not parse.
    #[test]
    fn the_bytes_and_the_depth_are_bounded_before_decoding() {
        let text = complete().to_string();
        let size = count(text.len());
        let levels = depth(text.as_bytes());
        assert_eq!(levels, 2);
        let under = |bytes: u64, levels: u64| {
            read_input(
                Some(text.as_bytes()),
                SpecificationLimits {
                    bytes: Bound::new(bytes).unwrap(),
                    depth: Bound::new(levels).unwrap(),
                },
            )
        };
        assert!(under(size, levels).is_ok());
        assert_eq!(
            under(size - 1, levels),
            Err(SpecificationRefusal::OverBound(
                SpecificationBound::InputBytes
            ))
        );
        assert_eq!(
            under(size, levels - 1),
            Err(SpecificationRefusal::OverBound(SpecificationBound::Depth))
        );
        // The bytes come first, and neither waits on the syntax.
        assert_eq!(
            under(size - 1, levels - 1),
            Err(SpecificationRefusal::OverBound(
                SpecificationBound::InputBytes
            ))
        );
        let broken = b"[[[";
        assert_eq!(
            read_input(
                Some(broken),
                SpecificationLimits {
                    bytes: largest(),
                    depth: Bound::new(2).unwrap(),
                },
            ),
            Err(SpecificationRefusal::OverBound(SpecificationBound::Depth))
        );
    }

    #[test]
    fn bytes_that_are_not_one_json_value_are_malformed() {
        assert_eq!(read_text("{"), Err(SYNTAX));
        assert_eq!(read_text("{} {}"), Err(SYNTAX));
        assert_eq!(read_text(""), Err(SYNTAX));
        assert_eq!(read_input(Some(&[b'{', 0xff, b'}']), roomy()), Err(SYNTAX));
    }

    #[test]
    fn a_value_outside_the_closed_shape_is_malformed() {
        for text in ["[]", "null", "\"coverage\"", "1"] {
            assert_eq!(read_text(text), Err(SHAPE), "{text}");
        }
        // `null` is not absence, and a slot member is an object.
        for member in [
            json!(null),
            json!("intlify-localization-coverage"),
            json!([]),
            json!(0),
        ] {
            let value = edited(|object| {
                object.insert("coverage".to_owned(), member.clone());
            });
            assert_eq!(read(&value), Err(SHAPE), "{member}");
        }
        // Each field is present and a string.
        let missing = edited(|object| {
            field(object, "coverage").remove(REVISION);
        });
        assert_eq!(read(&missing), Err(SHAPE));
        let number = edited(|object| {
            field(object, "coverage").insert(REVISION.to_owned(), json!(0));
        });
        assert_eq!(read(&number), Err(SHAPE));
        let digestless = edited(|object| {
            field(object, "intent-surface-class-vocabulary").remove(SEMANTIC_DIGEST);
        });
        assert_eq!(read(&digestless), Err(SHAPE));
    }

    #[test]
    fn a_repeated_member_or_field_is_a_duplicate() {
        // The complete input again with one more `coverage` member in front.
        let complete = complete().to_string();
        let member = format!(
            "{{\"coverage\":{{\"identity\":\"x\",\"revision\":\"0\"}},{}",
            &complete[1..]
        );
        assert_eq!(
            read_text(&member),
            Err(SpecificationRefusal::DuplicateMember)
        );
        let field = r#"{"coverage":{"identity":"a","identity":"b","revision":"0"}}"#;
        assert_eq!(read_text(field), Err(SpecificationRefusal::DuplicateMember));
    }

    #[test]
    fn a_member_or_field_outside_the_form_is_unknown() {
        let member = edited(|object| {
            object.insert("schemaRevision".to_owned(), json!("0"));
        });
        assert_eq!(read(&member), Err(SpecificationRefusal::UnknownMember));
        let field = edited(|object| {
            field(object, "coverage").insert("semanticDigest".to_owned(), json!(DIGEST));
        });
        assert_eq!(read(&field), Err(SpecificationRefusal::UnknownMember));
        // A member outside the form is unknown whatever its value, since the
        // shape looks only at slot members.
        assert_eq!(
            read_text(r#"{"foo":1}"#),
            Err(SpecificationRefusal::UnknownMember)
        );
        assert_eq!(
            read_text(r#"{"coverage":{"identity":"a","revision":"0","extra":[1]}}"#),
            Err(SpecificationRefusal::UnknownMember)
        );
    }

    #[test]
    fn a_field_outside_its_grammar_is_malformed() {
        for (slot, name, spelling) in [
            ("coverage", IDENTITY, "Intlify-Localization-Coverage"),
            ("coverage", IDENTITY, ""),
            ("coverage", REVISION, "0 "),
            ("project-profile", REVISION, "-0"),
            (
                "intent-surface-class-vocabulary",
                IDENTITY,
                "storefront_surfaces!",
            ),
            (
                "intent-surface-class-vocabulary",
                SEMANTIC_DIGEST,
                "sha256:11",
            ),
            (
                "intent-surface-class-vocabulary",
                SEMANTIC_DIGEST,
                "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            ),
        ] {
            let value = edited(|object| {
                field(object, slot).insert(name.to_owned(), json!(spelling));
            });
            assert_eq!(read(&value), Err(GRAMMAR), "{slot}.{name} = {spelling:?}");
        }
    }

    /// The first check that fails decides: the shape before a repeated member,
    /// a repeated member before an unknown one, and an unknown one before a
    /// field's grammar.
    #[test]
    fn the_first_failing_check_decides() {
        let shape_and_duplicate = r#"{"coverage":null,"coverage":null}"#;
        assert_eq!(read_text(shape_and_duplicate), Err(SHAPE));
        let duplicate_and_unknown =
            r#"{"foo":1,"coverage":{"identity":"a","identity":"a","revision":"0"}}"#;
        assert_eq!(
            read_text(duplicate_and_unknown),
            Err(SpecificationRefusal::DuplicateMember)
        );
        let unknown_and_grammar = r#"{"foo":1,"coverage":{"identity":"A","revision":"0"}}"#;
        assert_eq!(
            read_text(unknown_and_grammar),
            Err(SpecificationRefusal::UnknownMember)
        );
        let shape_and_unknown = r#"{"foo":1,"coverage":{"identity":"a"}}"#;
        assert_eq!(read_text(shape_and_unknown), Err(SHAPE));
    }

    /// The capability whose `inputBoundary` sets these two bounds, and every
    /// other bound at its minimum, as the capability boundary admits it.
    fn capability(bytes: u64, depth: u64) -> ImplementationCapability {
        let mut value = ImplementationCapability::fixture_owned(fixture_limits(1)).normalized();
        let bounds = &mut value["bootstrapOnly"]["inputBoundary"];
        bounds["maxSpecificationInputBytes"] = json!(bytes.to_string());
        bounds["maxDepth"] = json!(depth.to_string());
        let constructed = VersionedIdentity {
            identity: CAPABILITY,
            revision: CAPABILITY_REVISION,
        };
        crate::boundary::capability::admit(Some(&value), &constructed).unwrap()
    }

    /// The boundary bounds the input by the capability's `inputBoundary`, each
    /// bound by its own.
    #[test]
    fn the_capability_bounds_the_input() {
        let raw = complete().to_string().into_bytes();
        let (size, levels) = (count(raw.len()), depth(&raw));
        let under = |bytes, depth| admit(Some(&raw), &capability(bytes, depth));
        assert_eq!(under(size, levels), read(&complete()));
        assert_eq!(
            under(size - 1, levels),
            Err(SpecificationRefusal::OverBound(
                SpecificationBound::InputBytes
            ))
        );
        assert_eq!(
            under(size, levels - 1),
            Err(SpecificationRefusal::OverBound(SpecificationBound::Depth))
        );
        let least = ImplementationCapability::fixture_owned(fixture_limits(1));
        assert_eq!(
            admit(Some(&raw), &least),
            Err(SpecificationRefusal::OverBound(
                SpecificationBound::InputBytes
            ))
        );
    }

    /// Each refusal spells one of the five reasons of 015's boundary row.
    #[test]
    fn each_refusal_has_its_reason() {
        use SpecificationRefusal::{DuplicateMember, Malformed, Missing, OverBound, UnknownMember};
        let reasons = [
            (Missing, "missing-input"),
            (
                OverBound(SpecificationBound::InputBytes),
                "over-bound-member",
            ),
            (OverBound(SpecificationBound::Depth), "over-bound-member"),
            (Malformed(Malformation::Syntax), "malformed-input"),
            (Malformed(Malformation::Shape), "malformed-input"),
            (Malformed(Malformation::Grammar), "malformed-input"),
            (DuplicateMember, "duplicate-member"),
            (UnknownMember, "unknown-member"),
        ];
        for (refusal, reason) in reasons {
            assert_eq!(refusal.reason(), reason);
        }
    }
}
