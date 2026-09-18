// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The closed semantic projection defined by design 017.
//!
//! These types are the exact preimage of an Intent revision. Every member is
//! part of what a revision means, so adding, removing, or reinterpreting one is
//! an explicitly versioned change to the projection specification rather than
//! an implementation detail.
//!
//! The projection deliberately carries no parser node identity, CST table, host
//! AST, source coordinate, or second selector-validation implementation. It
//! also carries nothing that belongs to a separate dependency: the persistent
//! Intent identity, surface class, policies, glossary, target, and Provider
//! revisions are not revision inputs.

use intlify_shared_json::token::VersionedIdentity;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::primitives::NonemptyText;

/// Define a closed single-value tag used as a variant discriminator.
macro_rules! tag {
    ($name:ident, $wire:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
        )]
        pub enum $name {
            #[serde(rename = $wire)]
            Value,
        }
    };
}

tag!(TextTag, "text", "Discriminator of a literal text run.");
tag!(
    ExpressionTag,
    "expression",
    "Discriminator of an expression."
);
tag!(MarkupTag, "markup", "Discriminator of a markup part.");
tag!(InputTag, "input", "Discriminator of an input declaration.");
tag!(LocalTag, "local", "Discriminator of a local declaration.");
tag!(PatternTag, "pattern", "Discriminator of a pattern body.");
tag!(MatchTag, "match", "Discriminator of a matcher body.");
tag!(LiteralTag, "literal", "Discriminator of a literal value.");
tag!(
    VariableTag,
    "variable",
    "Discriminator of a variable reference."
);
tag!(
    CatchAllTag,
    "catch-all",
    "Discriminator of a catch-all key."
);

/// One literal value, after MF2 escape decoding.
///
/// Quoted and unquoted spellings with equal decoded values are the same value.
/// A numeric-looking literal stays a string: there is no host number
/// conversion, trimming, whitespace compression, or Unicode normalization of
/// literal content anywhere in this projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiteralValue {
    pub kind: LiteralTag,
    pub value: String,
}

/// One variable reference, by its parser-owned semantic name without a sigil.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VariableValue {
    pub kind: VariableTag,
    pub name: String,
}

/// An operand or option value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Value {
    Literal(LiteralValue),
    Variable(VariableValue),
}

/// One function option, in its source order within the owning function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Opt {
    pub name: String,
    pub value: Value,
}

/// One function reference and its options.
///
/// The options are symbolic requirements. This projection encodes no second
/// runtime type system and asserts nothing about what the function computes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Function {
    pub name: String,
    pub options: Box<[Opt]>,
}

/// One attribute, in its source order within the owning production.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Attribute {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// One expression: an operand, a function, or both.
///
/// An expression with neither is not representable, matching the MF2 grammar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Expression {
    pub kind: ExpressionTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operand: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<Function>,
    pub attributes: Box<[Attribute]>,
}

/// Which side of a markup pair a part represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum MarkupForm {
    Open,
    Close,
    Standalone,
}

/// One run of literal text.
///
/// Adjacent runs are merged and empty runs are omitted, so a text part always
/// carries content and two equal messages cannot differ by how the parser
/// happened to split their text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextPart {
    pub kind: TextTag,
    pub value: String,
}

/// One markup part.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkupPart {
    pub kind: MarkupTag,
    pub form: MarkupForm,
    pub name: String,
    pub options: Box<[Opt]>,
    pub attributes: Box<[Attribute]>,
}

/// One element of a pattern, in source order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PatternPart {
    Text(TextPart),
    Expression(Expression),
    Markup(MarkupPart),
}

/// One `.input` declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputDeclaration {
    pub kind: InputTag,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<Function>,
    pub attributes: Box<[Attribute]>,
}

/// One `.local` declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalDeclaration {
    pub kind: LocalTag,
    pub name: String,
    pub expression: Expression,
}

/// One declaration, in source order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Declaration {
    Input(InputDeclaration),
    Local(LocalDeclaration),
}

/// One matcher variant key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiteralKey {
    pub kind: LiteralTag,
    pub value: String,
}

/// The catch-all key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatchAllKey {
    pub kind: CatchAllTag,
}

/// One variant key.
///
/// A literal key carries its decoded value, never the parser's normalized
/// comparison key: two keys the parser matches identically can still be
/// different message content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum VariantKey {
    Literal(LiteralKey),
    CatchAll(CatchAllKey),
}

/// One matcher variant, in source order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Variant {
    pub keys: Box<[VariantKey]>,
    pub parts: Box<[PatternPart]>,
}

/// A message with one pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PatternBody {
    pub kind: PatternTag,
    pub parts: Box<[PatternPart]>,
}

/// A message that selects among variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MatchBody {
    pub kind: MatchTag,
    pub selectors: Box<[String]>,
    pub variants: Box<[Variant]>,
}

/// The body of a message.
///
/// A simple message and a quoted pattern with equal content produce the same
/// body: the wrapper is syntax, not structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MessageBody {
    Pattern(PatternBody),
    Match(MatchBody),
}

/// The structured message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageProjection {
    pub declarations: Box<[Declaration]>,
    pub body: MessageBody,
}

/// The bounded semantic usage a declaration was proven to have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Usage {
    pub profile: VersionedIdentity,
    pub value: NonemptyText,
}

/// The complete localization-relevant projection of one declaration.
///
/// Equal projections mean equal localization semantics. Host source
/// coordinates, quote spelling, non-semantic trivia, reference counts, and
/// parameter-expression implementation are deliberately absent, so formatting
/// that leaves both the parsed structure and literal content unchanged cannot
/// change a revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentProjection {
    pub mf2_specification: VersionedIdentity,
    pub message: MessageProjection,
    pub source_locale: NonemptyText,
    pub parameters: Box<[String]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<NonemptyText>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text(value: &str) -> PatternPart {
        PatternPart::Text(TextPart {
            kind: TextTag::Value,
            value: value.to_owned(),
        })
    }

    fn projection(body: MessageBody) -> IntentProjection {
        IntentProjection {
            mf2_specification: crate::specification::mf2_specification(),
            message: MessageProjection {
                declarations: Box::new([]),
                body,
            },
            source_locale: NonemptyText::from_validated("en").unwrap(),
            parameters: Box::new([]),
            usage: None,
            description: None,
        }
    }

    #[test]
    fn absent_optional_members_are_omitted_rather_than_null() {
        let value = serde_json::to_value(projection(MessageBody::Pattern(PatternBody {
            kind: PatternTag::Value,
            parts: Box::new([text("Pay now")]),
        })))
        .unwrap();
        let object = value.as_object().unwrap();
        assert!(!object.contains_key("usage"));
        assert!(!object.contains_key("description"));
        assert_eq!(
            value["message"]["body"],
            json!({"kind": "pattern", "parts": [{"kind": "text", "value": "Pay now"}]})
        );
        // An expression without an operand or a function omits both members.
        let bare = serde_json::to_value(Expression {
            kind: ExpressionTag::Value,
            operand: None,
            function: None,
            attributes: Box::new([]),
        })
        .unwrap();
        assert_eq!(bare, json!({"kind": "expression", "attributes": []}));
    }

    #[test]
    fn each_variant_is_selected_by_its_own_discriminator() {
        for (value, expected) in [
            (
                json!({"kind": "text", "value": "a"}),
                PatternPart::Text(TextPart {
                    kind: TextTag::Value,
                    value: "a".into(),
                }),
            ),
            (
                json!({"kind": "expression", "attributes": []}),
                PatternPart::Expression(Expression {
                    kind: ExpressionTag::Value,
                    operand: None,
                    function: None,
                    attributes: Box::new([]),
                }),
            ),
            (
                json!({"kind": "markup", "form": "open", "name": "b", "options": [], "attributes": []}),
                PatternPart::Markup(MarkupPart {
                    kind: MarkupTag::Value,
                    form: MarkupForm::Open,
                    name: "b".into(),
                    options: Box::new([]),
                    attributes: Box::new([]),
                }),
            ),
        ] {
            assert_eq!(
                serde_json::from_value::<PatternPart>(value.clone()).unwrap(),
                expected
            );
            assert_eq!(serde_json::to_value(&expected).unwrap(), value);
        }
    }

    #[test]
    fn closed_bodies_reject_unknown_members_and_wrong_discriminators() {
        for invalid in [
            json!({"kind": "text", "value": "a", "extra": true}),
            json!({"kind": "text"}),
            json!({"kind": "unknown", "value": "a"}),
            json!({"value": "a"}),
            json!({"kind": "expression", "attributes": [], "operand": {"kind": "unknown"}}),
            json!("text"),
        ] {
            assert!(
                serde_json::from_value::<PatternPart>(invalid.clone()).is_err(),
                "{invalid} was admitted"
            );
        }
        // A literal and a variable are distinguished by their own tag.
        assert_eq!(
            serde_json::from_value::<Value>(json!({"kind": "literal", "value": "1"})).unwrap(),
            Value::Literal(LiteralValue {
                kind: LiteralTag::Value,
                value: "1".into()
            })
        );
        assert!(serde_json::from_value::<Value>(json!({"kind": "literal", "name": "x"})).is_err());
    }

    #[test]
    fn a_source_locale_and_usage_value_cannot_be_empty() {
        let mut value = serde_json::to_value(projection(MessageBody::Match(MatchBody {
            kind: MatchTag::Value,
            selectors: Box::new(["count".into()]),
            variants: Box::new([Variant {
                keys: Box::new([VariantKey::CatchAll(CatchAllKey {
                    kind: CatchAllTag::Value,
                })]),
                parts: Box::new([text("many")]),
            }]),
        })))
        .unwrap();
        assert!(serde_json::from_value::<IntentProjection>(value.clone()).is_ok());
        value["sourceLocale"] = json!("");
        assert!(serde_json::from_value::<IntentProjection>(value).is_err());
    }
}
