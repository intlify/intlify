// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_shared_json::decode::DecodeFailure;
use intlify_shared_json::text::{self, TextFailure};
use serde_json::{json, Value};

use super::*;

pub(crate) const VECTORS: &str = include_str!("../../fixtures/dataset/vectors.json");

pub(crate) fn vectors() -> Value {
    serde_json::from_str(VECTORS).unwrap()
}

/// The vectors' conformance value, in canonical order.
pub(crate) fn sample() -> Value {
    vectors()["conformance"].clone()
}

pub(crate) const GENEROUS: ConformanceLimits = ConformanceLimits {
    validity_groups: u64::MAX,
    codes: u64::MAX,
    extension_keys: u64::MAX,
    types: u64::MAX,
    corrections: u64::MAX,
    work: u64::MAX,
};

fn read_within(
    value: &Value,
    limits: &ConformanceLimits,
) -> Result<LocaleConformanceData, ConformanceFailure> {
    LocaleConformanceData::read(text::write(value).unwrap().as_bytes(), limits)
}

fn read(value: &Value) -> Result<LocaleConformanceData, ConformanceFailure> {
    read_within(value, &GENEROUS)
}

/// The sample with the value at `pointer` replaced.
fn edited(pointer: &str, value: Value) -> Value {
    let mut sample = sample();
    *sample.pointer_mut(pointer).unwrap() = value;
    sample
}

/// The sample with `member` added to, or removed from, the object at `pointer`.
fn member(pointer: &str, member: &str, value: Option<Value>) -> Value {
    let mut sample = sample();
    let object = sample
        .pointer_mut(pointer)
        .unwrap()
        .as_object_mut()
        .unwrap();
    match value {
        Some(value) => object.insert(member.into(), value),
        None => object.remove(member),
    };
    sample
}

#[test]
fn the_sample_reads_from_its_canonical_text_and_writes_it_back_exactly() {
    let text = text::write(&sample()).unwrap();
    assert_eq!(vectors()["conformanceText"], json!(text));
    let data = LocaleConformanceData::read(text.as_bytes(), &GENEROUS).unwrap();
    assert_eq!(data.to_text(), text);
    assert_eq!(serde_json::to_value(&data).unwrap(), sample());
}

#[test]
fn every_variant_is_spelled_as_017_names_it() {
    let spelled = |value: Value, expected: &str| assert_eq!(value, json!(expected));
    for field in [
        ValidityField::Language,
        ValidityField::Region,
        ValidityField::Script,
        ValidityField::Subdivision,
        ValidityField::Variant,
    ] {
        spelled(serde_json::to_value(field).unwrap(), field.as_str());
    }
    for status in [
        ValidityStatus::Deprecated,
        ValidityStatus::Macroregion,
        ValidityStatus::PrivateUse,
        ValidityStatus::Regular,
        ValidityStatus::Reserved,
        ValidityStatus::Special,
        ValidityStatus::Unknown,
    ] {
        spelled(serde_json::to_value(status).unwrap(), status.as_str());
    }
    for extension in [Extension::T, Extension::U] {
        spelled(serde_json::to_value(extension).unwrap(), extension.as_str());
    }
    for class in [
        TypeClass::Codepoints,
        TypeClass::PrivateUse,
        TypeClass::ReorderCode,
        TypeClass::RgKeyValue,
        TypeClass::ScriptCode,
        TypeClass::SubdivisionCode,
    ] {
        spelled(serde_json::to_value(class).unwrap(), class.as_str());
    }
    assert_eq!(
        [
            ValueType::Any,
            ValueType::Incremental,
            ValueType::Multiple,
            ValueType::Single
        ]
        .map(|value_type| serde_json::to_value(value_type).unwrap()),
        [
            json!("any"),
            json!("incremental"),
            json!("multiple"),
            json!("single")
        ]
    );
}

#[test]
fn only_the_canonical_text_of_the_value_is_read() {
    let canonical = text::write(&sample()).unwrap();
    let pretty = serde_json::to_string_pretty(&sample()).unwrap();
    // `serde_json` keeps the sample's member order, which is not byte order.
    let unsorted = serde_json::to_string(&sample()).unwrap();
    assert_ne!(unsorted, canonical);
    for text in [pretty, unsorted, format!("{canonical}\n")] {
        assert_eq!(
            LocaleConformanceData::read(text.as_bytes(), &GENEROUS),
            Err(ConformanceFailure::Text(TextFailure::NotCanonical))
        );
    }
    let mut marked = b"\xef\xbb\xbf".to_vec();
    marked.extend_from_slice(canonical.as_bytes());
    for (bytes, failure) in [
        (marked.as_slice(), DecodeFailure::Syntax),
        (
            br#"{"corrections":[],"extensionKeys":[],"validity":1}"#.as_slice(),
            DecodeFailure::Number,
        ),
    ] {
        assert_eq!(
            LocaleConformanceData::read(bytes, &GENEROUS),
            Err(ConformanceFailure::Text(TextFailure::Unreadable(failure)))
        );
    }
}

/// Values outside the closed structure, each with what makes it so. The
/// committed schema refuses every one of them too.
pub(crate) fn shapes() -> Vec<(&'static str, Value)> {
    vec![
        ("not an object", json!([])),
        ("unknown member", member("", "extra", Some(json!([])))),
        ("missing member", member("", "corrections", None)),
        (
            "group member",
            member("/validity/0", "note", Some(json!("x"))),
        ),
        ("group codes", member("/validity/0", "codes", None)),
        ("field", edited("/validity/0/field", json!("territory"))),
        (
            "CLDR spelling",
            edited("/validity/3/status", json!("private_use")),
        ),
        ("empty code", edited("/validity/0/codes/0", json!(""))),
        ("code type", edited("/validity/0/codes/0", json!(["en"]))),
        (
            "legacy key name",
            member("/extensionKeys/1", "alias", Some(json!("calendar"))),
        ),
        ("value type", member("/extensionKeys/0", "valueType", None)),
        (
            "unknown value type",
            edited("/extensionKeys/0/valueType", json!("several")),
        ),
        (
            "deprecated",
            edited("/extensionKeys/0/deprecated", json!("false")),
        ),
        (
            "extension",
            edited("/extensionKeys/0/extension", json!("x")),
        ),
        (
            "kind",
            edited("/extensionKeys/0/types/0/kind", json!("pattern")),
        ),
        (
            "class",
            edited("/extensionKeys/0/types/0/class", json!("region-code")),
        ),
        (
            "class member",
            member("/extensionKeys/0/types/0", "deprecated", Some(json!(false))),
        ),
        (
            "literal member",
            member("/extensionKeys/1/types/0", "deprecated", None),
        ),
        (
            "literal with a class",
            member(
                "/extensionKeys/1/types/0",
                "class",
                Some(json!("codepoints")),
            ),
        ),
        (
            "empty literal",
            edited("/extensionKeys/1/types/0/type", json!("")),
        ),
        (
            "correction member",
            member("/corrections/0", "preferred", Some(json!("x"))),
        ),
        ("empty correction", edited("/corrections/0/to", json!(""))),
    ]
}

#[test]
fn a_value_outside_the_closed_structure_is_refused_as_a_shape() {
    for (case, value) in shapes() {
        assert_eq!(read(&value), Err(ConformanceFailure::Shape), "{case}");
    }
}

#[test]
fn a_collection_out_of_order_is_unsorted_and_a_repeat_is_repeated() {
    let swapped = |pointer: &str, a: usize, b: usize| {
        let mut sample = sample();
        sample
            .pointer_mut(pointer)
            .unwrap()
            .as_array_mut()
            .unwrap()
            .swap(a, b);
        sample
    };
    for (value, failure) in [
        (
            swapped("/validity", 0, 1),
            ConformanceFailure::Unsorted(Collection::ValidityGroups),
        ),
        // Statuses order by spelling: `macroregion` before `private-use`.
        (
            swapped("/validity", 2, 3),
            ConformanceFailure::Unsorted(Collection::ValidityGroups),
        ),
        (
            edited("/validity/1/status", json!("regular")),
            ConformanceFailure::Repeated(Collection::ValidityGroups),
        ),
        (
            swapped("/validity/0/codes", 0, 1),
            ConformanceFailure::Unsorted(Collection::Codes),
        ),
        (
            edited("/validity/0/codes/1", json!("en")),
            ConformanceFailure::Repeated(Collection::Codes),
        ),
        // Bytes, not case: `hant` is after `Latn`.
        (
            edited("/validity/8/codes", json!(["hant", "Latn"])),
            ConformanceFailure::Unsorted(Collection::Codes),
        ),
        (
            swapped("/extensionKeys", 0, 1),
            ConformanceFailure::Unsorted(Collection::ExtensionKeys),
        ),
        (
            edited("/extensionKeys/2/key", json!("ca")),
            ConformanceFailure::Repeated(Collection::ExtensionKeys),
        ),
        // A class comes before every literal.
        (
            swapped("/extensionKeys/2/types", 0, 1),
            ConformanceFailure::Unsorted(Collection::Types),
        ),
        (
            swapped("/extensionKeys/1/types", 1, 2),
            ConformanceFailure::Unsorted(Collection::Types),
        ),
        // A type listed twice is repeated whatever its deprecation says.
        (
            edited(
                "/extensionKeys/1/types/1",
                json!({"kind": "literal", "type": "ethioaa", "deprecated": true}),
            ),
            ConformanceFailure::Repeated(Collection::Types),
        ),
        (
            swapped("/corrections", 0, 1),
            ConformanceFailure::Unsorted(Collection::Corrections),
        ),
        (
            edited(
                "/corrections/1",
                json!({"extension": "u", "key": "ca", "from": "ethiopic-amete-alem", "to": "gregory"}),
            ),
            ConformanceFailure::Repeated(Collection::Corrections),
        ),
    ] {
        assert_eq!(read(&value), Err(failure), "{value}");
    }
    // Byte order puts digits before capitals and capitals before lowercase.
    assert!(read(&edited("/validity/8/codes", json!(["Latn", "hant"]))).is_ok());
    assert!(read(&edited("/validity/2/codes", json!(["001", "419", "EU"]))).is_ok());
}

#[test]
fn a_code_may_be_in_one_group_of_its_field_only() {
    for value in [
        edited("/validity/1/codes", json!(["en", "qaa", "qfz"])),
        // The two groups of one field need not be next to each other.
        edited("/validity/6/codes", json!(["001", "ZZ"])),
    ] {
        assert_eq!(
            read(&value),
            Err(ConformanceFailure::CodeInTwoGroups),
            "{value}"
        );
    }
    // A code's field is part of what it is.
    assert!(read(&edited("/validity/8/codes", json!(["Hant", "Latn", "ZZ"]))).is_ok());
}

#[test]
fn a_correction_maps_straight_to_a_value_nothing_corrects_further() {
    let corrections = |value: Value| edited("/corrections", value);
    let chained = corrections(json!([
        {"extension": "u", "key": "ca", "from": "ethioaa", "to": "ethiopic"},
        {"extension": "u", "key": "ca", "from": "ethiopic-amete-alem", "to": "ethioaa"},
    ]));
    let to_itself = corrections(json!([
        {"extension": "u", "key": "ca", "from": "islamicc", "to": "islamicc"},
    ]));
    for value in [chained, to_itself] {
        assert_eq!(
            read(&value),
            Err(ConformanceFailure::Unflattened),
            "{value}"
        );
    }
    // Another key's correction, or another extension's, is not a chain.
    let other_keys = corrections(json!([
        {"extension": "t", "key": "ca", "from": "islamic-civil", "to": "islamic"},
        {"extension": "u", "key": "ca", "from": "islamicc", "to": "islamic-civil"},
        {"extension": "u", "key": "kr", "from": "islamic-civil", "to": "digit"},
    ]));
    assert!(read(&other_keys).is_ok());
}

#[test]
fn each_limit_admits_its_exact_count_and_refuses_the_next() {
    let exact = ConformanceLimits {
        validity_groups: 9,
        codes: 20,
        extension_keys: 4,
        types: 9,
        corrections: 2,
        work: 44,
    };
    assert!(read_within(&sample(), &exact).is_ok());
    for (limits, limited) in [
        (
            ConformanceLimits {
                validity_groups: 8,
                ..exact
            },
            Limited::ValidityGroups,
        ),
        (ConformanceLimits { codes: 19, ..exact }, Limited::Codes),
        (
            ConformanceLimits {
                extension_keys: 3,
                ..exact
            },
            Limited::ExtensionKeys,
        ),
        (ConformanceLimits { types: 8, ..exact }, Limited::Types),
        (
            ConformanceLimits {
                corrections: 1,
                ..exact
            },
            Limited::Corrections,
        ),
        (ConformanceLimits { work: 43, ..exact }, Limited::Work),
    ] {
        assert_eq!(
            read_within(&sample(), &limits),
            Err(ConformanceFailure::Limit(limited))
        );
    }
}

#[test]
fn limits_are_checked_before_the_value_is_projected() {
    let none = ConformanceLimits {
        validity_groups: 0,
        codes: 0,
        extension_keys: 0,
        types: 0,
        corrections: 0,
        work: 0,
    };
    // Two groups that are not groups are still two groups to count.
    let malformed = json!({"corrections": [], "extensionKeys": [], "validity": ["x", "y"]});
    assert_eq!(
        read_within(
            &malformed,
            &ConformanceLimits {
                validity_groups: 1,
                ..GENEROUS
            }
        ),
        Err(ConformanceFailure::Limit(Limited::ValidityGroups))
    );
    // A member that is not a list counts nothing, and fails projection.
    for value in [
        json!({"corrections": "x", "extensionKeys": "x", "validity": "x"}),
        json!({"corrections": [], "extensionKeys": [{"types": "x"}], "validity": []}),
        json!({"corrections": [], "extensionKeys": [], "validity": [{"codes": {}}]}),
    ] {
        assert_eq!(
            read_within(
                &value,
                &ConformanceLimits {
                    extension_keys: 1,
                    validity_groups: 1,
                    work: 1,
                    ..none
                }
            ),
            Err(ConformanceFailure::Shape),
            "{value}"
        );
    }
}
