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
mod tests;
