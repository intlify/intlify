// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::fixtures::minimal_config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum Recipe {
    Minimal,
    ReversedMembers,
    PaddedBytes,
    LongString,
    TwoProfiles,
    ManyProfiles,
    ManyLocaleOccurrences,
    NestedArray,
    ManyValues,
    PortableMaximum,
    NegativeZero,
    InvalidUtf8,
    InvalidJson,
    TrailingToken,
    DuplicateKey,
    EscapedDuplicateKey,
    InvalidSurrogate,
    NonPortableNumber,
    RootNull,
    MissingVersion,
    WrongVersionType,
    UnsupportedVersion,
    EmptyProfiles,
    UnknownMember,
    MissingNullablePolicy,
    NullSourceDefault,
    InvalidSibling,
    InvalidResourceReference,
    DenseInvalid,
}

impl Recipe {
    /// No file reads, ambient config, arbitrary payloads, or network acquisition.
    /// All allocation scales are finite owner fixture constants outside intervals.
    pub(in crate::benchmark) fn source(self) -> Arc<[u8]> {
        let raw: Option<&[u8]> = match self {
            Self::PortableMaximum => Some(b"9007199254740991"),
            Self::NegativeZero => Some(b"-0.0"),
            Self::InvalidUtf8 => Some(b"\xff"),
            Self::InvalidJson => Some(b"{bad"),
            Self::TrailingToken => Some(b"null true"),
            Self::DuplicateKey => Some(br#"{"x":0,"x":1}"#),
            Self::EscapedDuplicateKey => Some(br#"{"x":0,"\u0078":1}"#),
            Self::InvalidSurrogate => Some(br#""\ud800""#),
            Self::NonPortableNumber => Some(b"9007199254740992"),
            Self::RootNull => Some(b"null"),
            _ => None,
        };
        if let Some(raw) = raw {
            return Arc::from(raw);
        }
        if self == Self::NestedArray {
            return Arc::from(format!("{}null{}", "[".repeat(128), "]".repeat(128)).into_bytes());
        }
        if self == Self::ManyValues {
            return Arc::from(serde_json::to_vec(&vec![0; 256]).expect("finite array fixture"));
        }
        let mut value = minimal_config();
        match self {
            Self::Minimal | Self::PaddedBytes => {}
            Self::ReversedMembers => reverse_objects(&mut value),
            Self::LongString => value["profiles"]["app"]["projectId"] = json!("a".repeat(4096)),
            Self::TwoProfiles | Self::ManyProfiles => {
                let profile = value["profiles"]["app"].clone();
                let count = if self == Self::TwoProfiles { 2 } else { 16 };
                for index in 1..count {
                    value["profiles"][format!("app-{index}")] = profile.clone();
                }
            }
            Self::ManyLocaleOccurrences => {
                value["profiles"]["app"]["requestedLocales"] = json!(vec!["en"; 32]);
                // Duplicates are structurally valid; this fixture never claims
                // successful semantic locale resolution.
            }
            Self::MissingVersion => {
                value.as_object_mut().unwrap().remove("schemaVersion");
            }
            Self::WrongVersionType => value["schemaVersion"] = json!(0),
            Self::UnsupportedVersion => {
                value["schemaVersion"] = json!("unsupported-fixture-version");
            }
            Self::EmptyProfiles => value["profiles"] = json!({}),
            Self::UnknownMember => value["profiles"]["app"]["unexpected"] = json!(true),
            Self::MissingNullablePolicy => {
                value["profiles"]["app"]["policies"]
                    .as_object_mut()
                    .unwrap()
                    .remove("providerRouting");
            }
            Self::NullSourceDefault => {
                value["profiles"]["app"]["defaultSourceLocale"] = Value::Null;
            }
            Self::InvalidSibling => value["profiles"]["broken"] = json!({}),
            Self::InvalidResourceReference => {
                value["profiles"]["app"]["policies"]["resourceLimits"] = json!("not-a-reference");
            }
            Self::DenseInvalid => {
                for index in 0..16 {
                    value["profiles"][format!("bad-{index}")] = json!({
                        "projectId": false, "requestedLocales": null,
                        "unexpected": [false, null], "policies": {}
                    });
                }
            }
            _ => unreachable!("raw and collection recipes handled above"),
        }
        let mut bytes = serde_json::to_vec(&value).expect("finite owner config fixture");
        if self == Self::PaddedBytes {
            bytes.extend(std::iter::repeat_n(b' ', 4096));
        }
        Arc::from(bytes)
    }
}

fn reverse_objects(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for value in object.values_mut() {
                reverse_objects(value);
            }
            *object = std::mem::take(object).into_iter().rev().collect();
        }
        Value::Array(values) => {
            for value in values {
                reverse_objects(value);
            }
        }
        _ => {}
    }
}
