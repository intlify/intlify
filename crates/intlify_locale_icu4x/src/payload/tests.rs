// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde_json::json;

use super::*;
use crate::markers::tests::committed;

fn payload(marker: Marker) -> Value {
    committed()
        .payloads()
        .unwrap()
        .into_iter()
        .find(|payload| payload.marker() == marker)
        .unwrap()
        .value()
        .clone()
}

/// The member names of an object payload, sorted.
fn members(value: &Value) -> Vec<String> {
    let mut names: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
    names.sort_unstable();
    names
}

#[test]
fn the_payloads_are_the_four_families_in_name_order_and_017_can_frame_them() {
    let payloads = committed().payloads().unwrap();
    assert_eq!(
        payloads
            .iter()
            .map(LogicalPayload::marker)
            .collect::<Vec<_>>(),
        Marker::ALL
    );
    for payload in &payloads {
        assert!(representable(payload.value()), "{:?}", payload.marker());
    }
}

#[test]
fn each_payload_is_the_data_struct_by_its_icu4x_field_names() {
    let aliases = members(&payload(Marker::LocaleAliasesV1));
    assert_eq!(
        aliases,
        [
            "complex_region",
            "language",
            "language_len2",
            "language_len3",
            "language_variants",
            "region_alpha",
            "region_num",
            "script",
            "sgn_region",
            "subdivision",
            "variant",
        ]
    );
    let language = payload(Marker::LocaleLikelySubtagsLanguageV1);
    let fields = members(&language);
    assert_eq!(
        fields,
        ["language", "language_region", "language_script", "und"]
    );
    // The likely subtags of `und` itself.
    assert_eq!(language["und"], json!(["en", "Latn", "US"]));
    let script_region = members(&payload(Marker::LocaleLikelySubtagsScriptRegionV1));
    assert_eq!(script_region, ["region", "script", "script_region"]);
    let extended = members(&payload(Marker::LocaleLikelySubtagsExtendedV1));
    assert_eq!(
        extended,
        [
            "language",
            "language_region",
            "language_script",
            "region",
            "script",
            "script_region",
        ]
    );
}

#[test]
fn maps_keyed_by_tuples_stay_icu4x_ordered_pairs_and_others_are_objects() {
    let language = payload(Marker::LocaleLikelySubtagsLanguageV1);
    // ICU4X writes a map whose keys are not strings as [key, value] pairs, in
    // the map's own order; nothing here re-sorts them.
    let pairs = language["language_script"].as_array().unwrap();
    assert!(!pairs.is_empty());
    for pair in pairs {
        let [key, value] = pair.as_array().unwrap().as_slice() else {
            panic!("not a pair: {pair}");
        };
        assert_eq!(key.as_array().unwrap().len(), 2);
        assert!(value.is_string());
    }
    let keys: Vec<_> = pairs.iter().map(|pair| pair[0].clone()).collect();
    assert!(keys.windows(2).all(|window| window[0] != window[1]));
    // A map keyed by single subtags stays a JSON object.
    assert!(language["language"].is_object());
}

#[test]
fn only_strings_arrays_and_objects_are_representable_at_any_depth() {
    for value in [
        json!("en"),
        json!([]),
        json!({}),
        json!({"language": [["en", "Latn"], "US"], "und": ["und", "Latn", "US"]}),
    ] {
        assert!(representable(&value), "{value}");
    }
    for value in [
        json!(null),
        json!(true),
        json!(1),
        json!(["en", 1]),
        json!({"und": ["und", null]}),
        json!({"nested": {"deeper": [false]}}),
    ] {
        assert!(!representable(&value), "{value}");
    }
}

#[test]
fn a_payload_017_cannot_frame_is_refused_by_its_family() {
    for marker in Marker::ALL {
        assert_eq!(
            LogicalPayload::new(marker, json!({"und": ["en", 1]})),
            Err(PayloadFailure::Unrepresentable(marker))
        );
        let payload = LogicalPayload::new(marker, json!({"und": ["en"]})).unwrap();
        assert_eq!(payload.marker(), marker);
        assert_eq!(payload.value(), &json!({"und": ["en"]}));
    }
}

#[cfg(feature = "generate")]
#[test]
fn the_blob_projects_what_icu4x_compiled_data_holds() {
    use icu_locale::provider::Baked;

    macro_rules! baked {
        ($marker:ty) => {{
            let payload: DataPayload<$marker> = Baked.load(DataRequest::default()).unwrap().payload;
            serde_json::to_value(payload.get()).unwrap()
        }};
    }
    let expected = [
        baked!(LocaleAliasesV1),
        baked!(LocaleLikelySubtagsExtendedV1),
        baked!(LocaleLikelySubtagsLanguageV1),
        baked!(LocaleLikelySubtagsScriptRegionV1),
    ];
    let actual: Vec<_> = committed()
        .payloads()
        .unwrap()
        .into_iter()
        .map(|payload| payload.value().clone())
        .collect();
    assert_eq!(actual, expected);
}
