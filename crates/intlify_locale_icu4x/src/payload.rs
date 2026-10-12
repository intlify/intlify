// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The logical payload of each marker family: the value of ICU4X 2.2's
//! human-readable serde serialization of its data struct, which ICU4X
//! documents as the stable representation of its data. Design 017 frames
//! these values into the dataset's semantic digest, so they depend on the
//! data alone, not on the export that carried it.

use icu_locale::provider::{
    LocaleAliasesV1, LocaleLikelySubtagsExtendedV1, LocaleLikelySubtagsLanguageV1,
    LocaleLikelySubtagsScriptRegionV1,
};
use icu_provider::prelude::*;
use serde_json::Value;

use crate::markers::{Marker, MarkerBlob};

/// Serializes one singleton marker's data struct as ICU4X's human-readable
/// serde form. A macro, because the generic bounds of a data struct across
/// every lifetime are longer than the four concrete calls.
macro_rules! project {
    ($provider:expr, $marker:ty) => {{
        DataProvider::<$marker>::load(&$provider, DataRequest::default())
            .ok()
            .and_then(|response| serde_json::to_value(response.payload.get()).ok())
    }};
}

/// One marker family's logical payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalPayload {
    marker: Marker,
    value: Value,
}

impl LogicalPayload {
    /// A payload only 017's framing can carry unchanged: strings, arrays, and
    /// objects. A number, boolean, or null is refused rather than converted.
    pub fn new(marker: Marker, value: Value) -> Result<Self, PayloadFailure> {
        if !representable(&value) {
            return Err(PayloadFailure::Unrepresentable(marker));
        }
        Ok(Self { marker, value })
    }

    pub const fn marker(&self) -> Marker {
        self.marker
    }

    pub const fn value(&self) -> &Value {
        &self.value
    }
}

/// Why a marker family has no logical payload. No variant keeps the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadFailure {
    /// The payload does not decode or does not serialize.
    Undecodable(Marker),
    /// The payload holds a number, boolean, or null, which 017's framing does
    /// not carry; it is refused rather than converted.
    Unrepresentable(Marker),
}

impl MarkerBlob {
    /// The four logical payloads, in 017's order.
    pub fn payloads(&self) -> Result<Vec<LogicalPayload>, PayloadFailure> {
        Marker::ALL
            .into_iter()
            .map(|marker| self.payload(marker))
            .collect()
    }

    fn payload(&self, marker: Marker) -> Result<LogicalPayload, PayloadFailure> {
        let provider = self.provider().as_deserializing();
        let value = match marker {
            Marker::LocaleAliasesV1 => project!(provider, LocaleAliasesV1),
            Marker::LocaleLikelySubtagsExtendedV1 => {
                project!(provider, LocaleLikelySubtagsExtendedV1)
            }
            Marker::LocaleLikelySubtagsLanguageV1 => {
                project!(provider, LocaleLikelySubtagsLanguageV1)
            }
            Marker::LocaleLikelySubtagsScriptRegionV1 => {
                project!(provider, LocaleLikelySubtagsScriptRegionV1)
            }
        }
        .ok_or(PayloadFailure::Undecodable(marker))?;
        LogicalPayload::new(marker, value)
    }
}

/// Whether 017's framing carries `value` unchanged: only strings, arrays, and
/// objects, at any depth.
pub(crate) fn representable(value: &Value) -> bool {
    match value {
        Value::String(_) => true,
        Value::Array(items) => items.iter().all(representable),
        Value::Object(members) => members.values().all(representable),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

#[cfg(test)]
mod tests {
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
                let payload: DataPayload<$marker> =
                    Baked.load(DataRequest::default()).unwrap().payload;
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
}
