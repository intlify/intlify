// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reads the pinned CLDR 48.2.0 records into design 017's conformance data,
//! by the rules of design 015's revision "0" conformance layer: the validity
//! groups from `common/validity`, every `u` and `t` key from `common/bcp47`,
//! and the corrections derived from those keys' types. Only the generator
//! uses this, under the `generate` feature.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

use roxmltree::{Document, ParsingOptions};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::conformance::{
    ConformanceFailure, Correction, ExtensionKey, ExtensionType, LocaleConformanceData, TypeClass,
    ValidityGroup,
};
use crate::primitives::NonemptyText;

/// The CLDR revision these records are from.
pub const CLDR_REVISION: &str = "48.2.0";

/// Every CLDR file the generator reads, relative to the CLDR distribution
/// root, sorted by address.
pub const SOURCES: [&str; 20] = [
    "common/bcp47/calendar.xml",
    "common/bcp47/collation.xml",
    "common/bcp47/currency.xml",
    "common/bcp47/measure.xml",
    "common/bcp47/number.xml",
    "common/bcp47/segmentation.xml",
    "common/bcp47/timezone.xml",
    "common/bcp47/transform-destination.xml",
    "common/bcp47/transform.xml",
    "common/bcp47/transform_hybrid.xml",
    "common/bcp47/transform_ime.xml",
    "common/bcp47/transform_keyboard.xml",
    "common/bcp47/transform_mt.xml",
    "common/bcp47/transform_private_use.xml",
    "common/bcp47/variant.xml",
    "common/validity/language.xml",
    "common/validity/region.xml",
    "common/validity/script.xml",
    "common/validity/subdivision.xml",
    "common/validity/variant.xml",
];

/// The files of [`SOURCES`], read from the copy vendored with this crate.
pub fn vendored() -> std::io::Result<Vec<(&'static str, Vec<u8>)>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("cldr");
    SOURCES
        .into_iter()
        .map(|address| Ok((address, std::fs::read(root.join(address))?)))
        .collect()
}

/// Why CLDR's records do not give revision "0"'s conformance data. It keeps
/// none of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CldrFailure {
    /// A file is not UTF-8 XML.
    Unreadable,
    /// A validity group names a field or status 017 does not.
    UnknownValidity,
    /// A range is not a code, `~`, and one character from the code's last on,
    /// both digits or both letters of one case.
    MalformedRange,
    /// A code is listed twice for one field.
    RepeatedCode,
    /// A key names an extension other than `u` and `t`, or a `valueType` 017
    /// does not.
    UnknownKey,
    /// A special type name is not one of the six classes.
    UnknownClass,
    /// A key, or a type of one key, is listed twice.
    RepeatedKey,
    /// Two mappings give one source different values.
    NonDeterministic,
    /// A chain of mappings comes back to a value it passed.
    Cycle,
    /// A chain ends in something other than a listed type that is not
    /// deprecated.
    Unresolved,
    /// ICU4X 2.2.0 already realizes a mapping, so the dataset would record no
    /// admitted spelling for its source.
    Realized,
    /// The records do not make conformance data in 017's order.
    Conformance(ConformanceFailure),
}

/// Whether ICU4X 2.2.0 already turns `from` into `to` for one key: given the
/// extension, the key, `from`, and `to`.
pub type Realizes<'a> = &'a dyn Fn(&str, &str, &str, &str) -> bool;

/// Revision "0"'s conformance data from the CLDR files, given by address.
pub fn conformance_data(
    files: &[(&str, &[u8])],
    realizes: Realizes<'_>,
) -> Result<LocaleConformanceData, CldrFailure> {
    let under = |prefix: &str| -> Vec<&[u8]> {
        files
            .iter()
            .filter(|(address, _)| address.starts_with(prefix))
            .map(|(_, bytes)| *bytes)
            .collect()
    };
    let validity = validity(&under("common/validity/"))?;
    let keys = keys(&under("common/bcp47/"))?;
    let extension_keys = keys
        .iter()
        .map(extension_key)
        .collect::<Result<Vec<_>, _>>()?;
    let corrections = corrections(&keys, realizes)?;
    LocaleConformanceData::new(validity, extension_keys, corrections)
        .map_err(CldrFailure::Conformance)
}

fn document(bytes: &[u8]) -> Result<Document<'_>, CldrFailure> {
    let text = std::str::from_utf8(bytes).map_err(|_| CldrFailure::Unreadable)?;
    // Each file names its DTD by a relative path, which is never fetched.
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    Document::parse_with_options(text, options).map_err(|_| CldrFailure::Unreadable)
}

/// The validity groups, one per field and status, each with its codes
/// expanded and sorted, in 017's order.
fn validity(files: &[&[u8]]) -> Result<Vec<ValidityGroup>, CldrFailure> {
    // Keyed by 017's spellings, so the map's order is 017's order.
    let mut groups: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for bytes in files {
        let document = document(bytes)?;
        for id in document
            .descendants()
            .filter(|node| node.has_tag_name("id"))
        {
            let (Some(field), Some(status)) = (id.attribute("type"), id.attribute("idStatus"))
            else {
                return Err(CldrFailure::UnknownValidity);
            };
            let codes = groups
                .entry((field.to_owned(), status.replace('_', "-")))
                .or_default();
            // A comment such as `<!-- 257 items -->` splits the codes into
            // several text nodes, so each one is read.
            let tokens = id
                .children()
                .filter(roxmltree::Node::is_text)
                .flat_map(|node| node.text().unwrap_or_default().split_whitespace());
            for token in tokens {
                for code in expand(token)? {
                    if !codes.insert(code) {
                        return Err(CldrFailure::RepeatedCode);
                    }
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    for ((field, _), codes) in &groups {
        for code in codes {
            if !seen.insert((field, code)) {
                return Err(CldrFailure::RepeatedCode);
            }
        }
    }
    groups
        .iter()
        .map(|((field, status), codes)| {
            Ok(ValidityGroup::new(
                spelled(field).ok_or(CldrFailure::UnknownValidity)?,
                spelled(status).ok_or(CldrFailure::UnknownValidity)?,
                codes
                    .iter()
                    .map(|code| text(code))
                    .collect::<Result<_, _>>()?,
            ))
        })
        .collect()
}

/// CLDR's range notation: `XA~C` is `XA`, `XB`, and `XC`, the code's last
/// character running through the one after `~`, within the digits, the
/// uppercase letters, or the lowercase letters.
fn expand(token: &str) -> Result<Vec<String>, CldrFailure> {
    let Some((start, end)) = token.split_once('~') else {
        return Ok(vec![token.to_owned()]);
    };
    let (Some(first), &[last]) = (start.bytes().last(), end.as_bytes()) else {
        return Err(CldrFailure::MalformedRange);
    };
    let within = |run: RangeInclusive<u8>| run.contains(&first) && run.contains(&last);
    if !start.is_ascii()
        || first > last
        || ![b'0'..=b'9', b'A'..=b'Z', b'a'..=b'z']
            .into_iter()
            .any(within)
    {
        return Err(CldrFailure::MalformedRange);
    }
    // `start` is ASCII, so its last byte is its last character.
    let head = &start[..start.len() - 1];
    Ok((first..=last)
        .map(|byte| format!("{head}{}", char::from(byte)))
        .collect())
}

/// One key as CLDR lists it, with what the corrections need of its types.
struct RawKey {
    extension: String,
    name: String,
    deprecated: bool,
    value_type: String,
    types: Vec<RawType>,
}

struct RawType {
    name: String,
    deprecated: bool,
    preferred: Option<String>,
    aliases: Vec<String>,
}

/// Every key, sorted by extension and then name.
fn keys(files: &[&[u8]]) -> Result<Vec<RawKey>, CldrFailure> {
    let mut keys = BTreeMap::new();
    for bytes in files {
        let document = document(bytes)?;
        for key in document
            .descendants()
            .filter(|node| node.has_tag_name("key"))
        {
            let extension = key.attribute("extension").unwrap_or("u");
            let value_type = key.attribute("valueType").unwrap_or("single");
            let name = key.attribute("name").ok_or(CldrFailure::UnknownKey)?;
            if !matches!(extension, "u" | "t")
                || !matches!(value_type, "single" | "multiple" | "incremental" | "any")
            {
                return Err(CldrFailure::UnknownKey);
            }
            let mut names = BTreeSet::new();
            let mut types = Vec::new();
            for node in key.children().filter(|node| node.has_tag_name("type")) {
                let name = node.attribute("name").ok_or(CldrFailure::UnknownKey)?;
                if !names.insert(name) {
                    return Err(CldrFailure::RepeatedKey);
                }
                types.push(RawType {
                    name: name.to_owned(),
                    deprecated: node.attribute("deprecated") == Some("true"),
                    preferred: node.attribute("preferred").map(str::to_owned),
                    aliases: node
                        .attribute("alias")
                        .unwrap_or_default()
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect(),
                });
            }
            let raw = RawKey {
                extension: extension.to_owned(),
                name: name.to_owned(),
                deprecated: key.attribute("deprecated") == Some("true"),
                value_type: value_type.to_owned(),
                types,
            };
            if keys
                .insert((extension.to_owned(), name.to_owned()), raw)
                .is_some()
            {
                return Err(CldrFailure::RepeatedKey);
            }
        }
    }
    Ok(keys.into_values().collect())
}

/// A literal type name has a lowercase letter; a special one has none.
fn literal(name: &str) -> bool {
    name.bytes().any(|byte| byte.is_ascii_lowercase())
}

fn class(name: &str) -> Result<TypeClass, CldrFailure> {
    Ok(match name {
        "CODEPOINTS" => TypeClass::Codepoints,
        "PRIVATE_USE" => TypeClass::PrivateUse,
        "REORDER_CODE" => TypeClass::ReorderCode,
        "RG_KEY_VALUE" => TypeClass::RgKeyValue,
        "SCRIPT_CODE" => TypeClass::ScriptCode,
        "SUBDIVISION_CODE" => TypeClass::SubdivisionCode,
        _ => return Err(CldrFailure::UnknownClass),
    })
}

/// One key as 017 records it, its types sorted by kind and then by class
/// or type.
fn extension_key(key: &RawKey) -> Result<ExtensionKey, CldrFailure> {
    let mut types = BTreeMap::new();
    for raw in &key.types {
        let (order, entry) = if literal(&raw.name) {
            (
                ("literal", raw.name.clone()),
                ExtensionType::Literal {
                    name: text(&raw.name)?,
                    deprecated: raw.deprecated,
                },
            )
        } else {
            let class = class(&raw.name)?;
            (
                ("class", class.as_str().to_owned()),
                ExtensionType::Class { class },
            )
        };
        // `keys` has refused a type listed twice, and each special name is
        // one class, so no entry replaces another.
        types.insert(order, entry);
    }
    Ok(ExtensionKey::new(
        spelled(&key.extension).ok_or(CldrFailure::UnknownKey)?,
        text(&key.name)?,
        key.deprecated,
        spelled(&key.value_type).ok_or(CldrFailure::UnknownKey)?,
        types.into_values().collect(),
    ))
}

/// The corrections of every key that is not deprecated: each source a
/// direct-input value, each mapped to its final value, in 017's order.
fn corrections(keys: &[RawKey], realizes: Realizes<'_>) -> Result<Vec<Correction>, CldrFailure> {
    let mut corrections = Vec::new();
    for key in keys.iter().filter(|key| !key.deprecated) {
        let listed: BTreeMap<&str, &RawType> = key
            .types
            .iter()
            .filter(|raw| literal(&raw.name))
            .map(|raw| (raw.name.as_str(), raw))
            .collect();
        let mappings = mappings(&listed)?;
        for (from, to) in &mappings {
            let last = final_value(&mappings, to)?;
            let target = listed
                .get(last)
                .filter(|raw| !raw.deprecated)
                .ok_or(CldrFailure::Unresolved)?;
            if realizes(&key.extension, &key.name, from, &target.name) {
                return Err(CldrFailure::Realized);
            }
            corrections.push(Correction::new(
                spelled(&key.extension).ok_or(CldrFailure::UnknownKey)?,
                text(&key.name)?,
                text(from)?,
                text(&target.name)?,
            ));
        }
    }
    Ok(corrections)
}

/// One key's mappings before flattening: each deprecated type to its
/// preferred value, and each alias to its type, when the source is a
/// well-formed type and an alias does not name another listed type.
fn mappings(listed: &BTreeMap<&str, &RawType>) -> Result<BTreeMap<String, String>, CldrFailure> {
    let mut mappings = BTreeMap::new();
    let mut add = |from: String, to: &str| {
        if !well_formed(&from) {
            return Ok(());
        }
        match mappings.insert(from, to.to_owned()) {
            Some(previous) if previous != to => Err(CldrFailure::NonDeterministic),
            _ => Ok(()),
        }
    };
    for raw in listed.values() {
        if let (true, Some(preferred)) = (raw.deprecated, &raw.preferred) {
            add(raw.name.clone(), preferred)?;
        }
        for alias in &raw.aliases {
            let alias = alias.to_ascii_lowercase();
            if !listed.contains_key(alias.as_str()) {
                add(alias, &raw.name)?;
            }
        }
    }
    Ok(mappings)
}

/// Follow a chain of mappings from `value` to the value no mapping changes.
/// A chain that comes back to a value it passed is a cycle, wherever it
/// started.
fn final_value<'a>(
    mappings: &'a BTreeMap<String, String>,
    mut value: &'a str,
) -> Result<&'a str, CldrFailure> {
    let mut passed = BTreeSet::new();
    while let Some(next) = mappings.get(value) {
        if !passed.insert(value) {
            return Err(CldrFailure::Cycle);
        }
        value = next;
    }
    Ok(value)
}

/// A type value a direct input can spell: subtags of three to eight
/// lowercase ASCII letters and digits.
fn well_formed(value: &str) -> bool {
    value.split('-').all(|subtag| {
        (3..=8).contains(&subtag.len())
            && subtag
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    })
}

/// A value of an enum 017 spells, from that spelling.
fn spelled<T: DeserializeOwned>(spelling: &str) -> Option<T> {
    serde_json::from_value(Value::String(spelling.to_owned())).ok()
}

fn text(value: &str) -> Result<NonemptyText, CldrFailure> {
    NonemptyText::from_validated(value).map_err(|_| CldrFailure::UnknownKey)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const NEVER: Realizes<'static> = &|_, _, _, _| false;

    fn validity_of(xml: &str) -> Result<Value, CldrFailure> {
        let groups = validity(&[xml.as_bytes()])?;
        Ok(serde_json::to_value(groups).unwrap())
    }

    /// A `common/bcp47` file with the given keys.
    fn bcp47(keys: &str) -> String {
        format!(
            "<?xml version='1.0' encoding='UTF-8' ?>\n<!DOCTYPE ldmlBCP47 SYSTEM '../../common/dtd/ldmlBCP47.dtd'>\n<ldmlBCP47><keyword>{keys}</keyword></ldmlBCP47>"
        )
    }

    fn corrections_of(keys: &str, realizes: Realizes<'_>) -> Result<Value, CldrFailure> {
        let file = bcp47(keys);
        let raw = super::keys(&[file.as_bytes()])?;
        Ok(serde_json::to_value(corrections(&raw, realizes)?).unwrap())
    }

    #[test]
    fn a_range_runs_the_last_character_through_the_one_after_the_tilde() {
        assert_eq!(
            expand("XA~C"),
            Ok(vec!["XA".into(), "XB".into(), "XC".into()])
        );
        assert_eq!(
            expand("001~3"),
            Ok(vec!["001".into(), "002".into(), "003".into()])
        );
        assert_eq!(
            expand("aaa~c"),
            Ok(vec!["aaa".into(), "aab".into(), "aac".into()])
        );
        assert_eq!(expand("en"), Ok(vec!["en".into()]));
        assert_eq!(expand("XB~B"), Ok(vec!["XB".into()]));
        let malformed = [
            "XC~A", "XA~BC", "~B", "XA~", "XA~-", "X!~/", "X9~A", "XZ~a", "é~f", "éa~b",
        ];
        for token in malformed {
            assert_eq!(expand(token), Err(CldrFailure::MalformedRange), "{token}");
        }
    }

    #[test]
    fn only_the_files_under_validity_and_bcp47_are_read() {
        let validity = "<supplementalData><idValidity><id type='region' idStatus='regular'>AC</id></idValidity></supplementalData>";
        let keys = bcp47("<key name='ca'><type name='gregory'/></key>");
        let files: [(&str, &[u8]); 3] = [
            ("common/bcp47/calendar.xml", keys.as_bytes()),
            (
                "common/supplemental/supplementalMetadata.xml",
                b"<unclosed>",
            ),
            ("common/validity/region.xml", validity.as_bytes()),
        ];
        assert_eq!(
            serde_json::to_value(conformance_data(&files, NEVER).unwrap()).unwrap(),
            json!({
                "validity": [{"field": "region", "status": "regular", "codes": ["AC"]}],
                "extensionKeys": [{"extension": "u", "key": "ca", "deprecated": false, "valueType": "single", "types": [
                    {"kind": "literal", "type": "gregory", "deprecated": false},
                ]}],
                "corrections": [],
            })
        );
    }

    #[test]
    fn validity_groups_codes_by_field_and_status_across_comments_and_files() {
        let xml = "<?xml version='1.0' encoding='UTF-8' ?>\n<!DOCTYPE supplementalData SYSTEM '../../common/dtd/ldmlSupplemental.dtd'>\n<supplementalData><idValidity>\
            <id type='region' idStatus='regular'>\t<!-- 3 items -->\n AD~E <!-- more --> AC </id>\
            <id type='region' idStatus='private_use'> XC </id>\
            <id type='language' idStatus='regular'> en </id>\
            </idValidity></supplementalData>";
        assert_eq!(
            validity_of(xml),
            Ok(json!([
                {"field": "language", "status": "regular", "codes": ["en"]},
                {"field": "region", "status": "private-use", "codes": ["XC"]},
                {"field": "region", "status": "regular", "codes": ["AC", "AD", "AE"]},
            ]))
        );
    }

    #[test]
    fn a_validity_group_017_does_not_name_or_a_repeated_code_fails() {
        let file = |ids: &str| {
            format!("<supplementalData><idValidity>{ids}</idValidity></supplementalData>")
        };
        for (ids, failure) in [
            ("<id type='territory' idStatus='regular'>US</id>", CldrFailure::UnknownValidity),
            ("<id type='region' idStatus='retired'>US</id>", CldrFailure::UnknownValidity),
            ("<id idStatus='regular'>US</id>", CldrFailure::UnknownValidity),
            ("<id type='region' idStatus='regular'>US US</id>", CldrFailure::RepeatedCode),
            (
                "<id type='region' idStatus='regular'>US</id><id type='region' idStatus='deprecated'>US</id>",
                CldrFailure::RepeatedCode,
            ),
            ("<id type='region' idStatus='regular'>XC~A</id>", CldrFailure::MalformedRange),
        ] {
            assert_eq!(validity_of(&file(ids)), Err(failure), "{ids}");
        }
        // The same code in two fields is two codes.
        let both = file("<id type='region' idStatus='regular'>AC</id><id type='script' idStatus='regular'>AC</id>");
        assert!(validity_of(&both).is_ok());
        assert_eq!(
            validity(&[b"\xff".as_slice()]),
            Err(CldrFailure::Unreadable)
        );
        assert_eq!(
            validity(&[b"<unclosed>".as_slice()]),
            Err(CldrFailure::Unreadable)
        );
    }

    #[test]
    fn keys_take_their_defaults_and_their_types_017s_order() {
        let file = bcp47(
            "<key name='kr' valueType='multiple' description='x' deprecated='false'>\
               <!-- only types --><description>x</description><type name='space'/><type name='REORDER_CODE'/><type name='digit' deprecated='true'/>\
               <type name='punct' deprecated='false'/>\
             </key>\
             <key extension='t' name='x0' valueType='any'><type name='PRIVATE_USE'/></key>\
             <key name='ca' deprecated='true'><type name='gregory' alias='gregorian'/></key>",
        );
        let raw = keys(&[file.as_bytes()]).unwrap();
        let keys: Vec<_> = raw.iter().map(|key| extension_key(key).unwrap()).collect();
        assert_eq!(
            serde_json::to_value(keys).unwrap(),
            json!([
                {"extension": "t", "key": "x0", "deprecated": false, "valueType": "any", "types": [{"kind": "class", "class": "private-use"}]},
                {"extension": "u", "key": "ca", "deprecated": true, "valueType": "single", "types": [{"kind": "literal", "type": "gregory", "deprecated": false}]},
                {"extension": "u", "key": "kr", "deprecated": false, "valueType": "multiple", "types": [
                    {"kind": "class", "class": "reorder-code"},
                    {"kind": "literal", "type": "digit", "deprecated": true},
                    {"kind": "literal", "type": "punct", "deprecated": false},
                    {"kind": "literal", "type": "space", "deprecated": false},
                ]},
            ])
        );
    }

    #[test]
    fn each_special_type_is_one_of_the_six_classes() {
        for (name, class) in [
            ("CODEPOINTS", TypeClass::Codepoints),
            ("PRIVATE_USE", TypeClass::PrivateUse),
            ("REORDER_CODE", TypeClass::ReorderCode),
            ("RG_KEY_VALUE", TypeClass::RgKeyValue),
            ("SCRIPT_CODE", TypeClass::ScriptCode),
            ("SUBDIVISION_CODE", TypeClass::SubdivisionCode),
        ] {
            assert_eq!(super::class(name), Ok(class));
        }
        assert_eq!(super::class("REGION_CODE"), Err(CldrFailure::UnknownClass));
        assert!(literal("101key") && literal("islamic-civil") && !literal("RG_KEY_VALUE"));
    }

    #[test]
    fn a_key_017_cannot_record_fails() {
        for (keys, failure) in [
            (
                "<key name='ca' extension='x'><type name='a'/></key>",
                CldrFailure::UnknownKey,
            ),
            (
                "<key name='ca' valueType='several'><type name='a'/></key>",
                CldrFailure::UnknownKey,
            ),
            ("<key><type name='a'/></key>", CldrFailure::UnknownKey),
            ("<key name='ca'><type/></key>", CldrFailure::UnknownKey),
            (
                "<key name='ca'><type name='abc'/><type name='abc'/></key>",
                CldrFailure::RepeatedKey,
            ),
            (
                "<key name='ca'><type name='abc'/></key><key name='ca'><type name='def'/></key>",
                CldrFailure::RepeatedKey,
            ),
        ] {
            let file = bcp47(keys);
            assert_eq!(keys_failure(&file), failure, "{keys}");
        }
        let file = bcp47("<key name='kr'><type name='REGION_CODE'/></key>");
        let raw = super::keys(&[file.as_bytes()]).unwrap();
        assert_eq!(
            extension_key(&raw[0]).err(),
            Some(CldrFailure::UnknownClass)
        );
        let file = bcp47("<key name=''><type name='abc'/></key>");
        let raw = super::keys(&[file.as_bytes()]).unwrap();
        assert_eq!(extension_key(&raw[0]).err(), Some(CldrFailure::UnknownKey));
        // The same name in two extensions is two keys.
        let both = bcp47("<key name='ca'><type name='abc'/></key><key extension='t' name='ca'><type name='abc'/></key>");
        assert_eq!(
            super::keys(&[both.as_bytes()]).map(|keys| keys.len()),
            Ok(2)
        );
    }

    fn keys_failure(file: &str) -> CldrFailure {
        match super::keys(&[file.as_bytes()]) {
            Err(failure) => failure,
            Ok(_) => panic!("{file} was read"),
        }
    }

    #[test]
    fn corrections_come_from_preferred_values_and_well_formed_aliases() {
        let keys = "<key name='ca' valueType='incremental'>\
              <type name='islamic-civil'/>\
              <type name='islamicc' deprecated='true' preferred='islamic-civil' alias='islamic-civil'/>\
              <type name='ethioaa' alias='ethiopic-amete-alem'/>\
              <type name='gregory' alias='gregorian'/>\
            </key>\
            <key name='tz'><type name='jptyo' alias='Asia/Tokyo Japan JAPAN'/><type name='utc' alias='UTC Zulu'/></key>\
            <key name='rg'><type name='RG_KEY_VALUE' alias='abcz'/><type name='usx' preferred='usy'/><type name='usy'/></key>\
            <key name='kh' deprecated='true'><type name='true' alias='yes'/></key>\
            <key name='kn'><type name='true' alias='yes'/><type name='false' alias='no'/></key>";
        assert_eq!(
            corrections_of(keys, NEVER),
            Ok(json!([
                {"extension": "u", "key": "ca", "from": "ethiopic-amete-alem", "to": "ethioaa"},
                {"extension": "u", "key": "ca", "from": "islamicc", "to": "islamic-civil"},
                {"extension": "u", "key": "kn", "from": "yes", "to": "true"},
                {"extension": "u", "key": "tz", "from": "japan", "to": "jptyo"},
                {"extension": "u", "key": "tz", "from": "zulu", "to": "utc"},
            ]))
        );
    }

    #[test]
    fn a_chain_is_flattened_to_its_final_value() {
        let keys = "<key name='ms'>\
              <type name='aaa' deprecated='true' preferred='bbb'/>\
              <type name='bbb' deprecated='true' preferred='ccc'/>\
              <type name='ccc' alias='zzz'/>\
            </key>";
        assert_eq!(
            corrections_of(keys, NEVER),
            Ok(json!([
                {"extension": "u", "key": "ms", "from": "aaa", "to": "ccc"},
                {"extension": "u", "key": "ms", "from": "bbb", "to": "ccc"},
                {"extension": "u", "key": "ms", "from": "zzz", "to": "ccc"},
            ]))
        );
    }

    #[test]
    fn a_mapping_that_is_not_deterministic_or_final_fails_generation() {
        for (keys, failure) in [
            (
                "<key name='ms'><type name='aaa' alias='xyz'/><type name='bbb' alias='xyz'/></key>",
                CldrFailure::NonDeterministic,
            ),
            (
                "<key name='ms'><type name='aaa' deprecated='true' preferred='bbb'/><type name='bbb' deprecated='true' preferred='aaa'/></key>",
                CldrFailure::Cycle,
            ),
            (
                "<key name='ms'><type name='aaa' deprecated='true' preferred='aaa'/></key>",
                CldrFailure::Cycle,
            ),
            (
                "<key name='ms'><type name='aaa' deprecated='true' preferred='missing'/></key>",
                CldrFailure::Unresolved,
            ),
            (
                "<key name='ms'><type name='aaa' deprecated='true' preferred='bbb'/><type name='bbb' deprecated='true'/></key>",
                CldrFailure::Unresolved,
            ),
        ] {
            assert_eq!(corrections_of(keys, NEVER), Err(failure), "{keys}");
        }
    }

    #[test]
    fn a_mapping_icu4x_realizes_fails_generation() {
        let keys = "<key name='ms'><type name='uksystem' alias='imperial'/></key>";
        let asked = std::cell::RefCell::new(Vec::new());
        let realizes = |extension: &str, key: &str, from: &str, to: &str| {
            asked
                .borrow_mut()
                .push(format!("{extension}-{key}-{from}-{to}"));
            true
        };
        assert_eq!(corrections_of(keys, &realizes), Err(CldrFailure::Realized));
        assert_eq!(asked.into_inner(), ["u-ms-imperial-uksystem"]);
    }

    #[test]
    fn only_lowercase_alphanumeric_subtags_of_three_to_eight_are_well_formed() {
        for value in ["abc", "abcdefgh", "101key", "beta-metsehaf", "a1b"] {
            assert!(well_formed(value), "{value}");
        }
        for value in [
            "ab",
            "abcdefghi",
            "Abc",
            "a/b",
            "abc-",
            "-abc",
            "abc--def",
            "",
            "ab_cd",
        ] {
            assert!(!well_formed(value), "{value}");
        }
    }

    #[test]
    fn the_vendored_records_give_revision_0s_conformance_data() {
        let sources = vendored().unwrap();
        assert_eq!(
            sources
                .iter()
                .map(|(address, _)| *address)
                .collect::<Vec<_>>(),
            SOURCES
        );
        let mut sorted = SOURCES;
        sorted.sort_unstable();
        assert_eq!(
            sorted, SOURCES,
            "addresses are sorted by unsigned UTF-8 bytes"
        );
        let files: Vec<(&str, &[u8])> = sources
            .iter()
            .map(|(address, bytes)| (*address, bytes.as_slice()))
            .collect();
        let data = serde_json::to_value(conformance_data(&files, NEVER).unwrap()).unwrap();
        let groups = data["validity"].as_array().unwrap();
        let codes: usize = groups
            .iter()
            .map(|group| group["codes"].as_array().unwrap().len())
            .sum();
        // The counts CLDR's own comments give, summed.
        assert_eq!((groups.len(), codes), (24, 15_418));
        let keys = data["extensionKeys"].as_array().unwrap();
        let types: usize = keys
            .iter()
            .map(|key| key["types"].as_array().unwrap().len())
            .sum();
        assert_eq!((keys.len(), types), (37, 1_080));
        let corrections = data["corrections"].as_array().unwrap();
        assert_eq!(corrections.len(), 64);
        let mapped = |key: &str, from: &str| {
            corrections
                .iter()
                .find(|correction| correction["key"] == key && correction["from"] == from)
                .map(|correction| correction["to"].clone())
        };
        assert_eq!(mapped("ca", "islamicc"), Some(json!("islamic-civil")));
        assert_eq!(mapped("ca", "islamic-civil"), None);
        assert_eq!(
            mapped("kh", "yes"),
            None,
            "a deprecated key has no corrections"
        );
        assert_eq!(mapped("tz", "est5edt"), Some(json!("usnyc")));
        assert_eq!(
            mapped("tz", "utc"),
            None,
            "an alias naming its own type is no mapping"
        );
    }
}
