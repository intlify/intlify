// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's Locale Canonicalization Dataset: the logical payloads of the
//! four marker families and the conformance data, under one dataset schema.
//!
//! Its semantic digest identifies the data, not the export that carried it: a
//! baked, blob, or other export of the same data has the same digest and a
//! different file digest. Where the data came from and how a payload encodes
//! it are not part of the preimage.

use intlify_shared_json::encoding::{hash, Domain, EncodingFailure};
use intlify_shared_json::token::VersionedIdentity;
use serde::Serialize;
use serde_json::Value;

use crate::conformance::{ConformanceFailure, LocaleConformanceData};
use crate::markers::Marker;
use crate::payload::LogicalPayload;
use crate::primitives::SemanticDigest;

/// The identity of the one dataset schema this crate implements. It fixes the
/// closed structure and the marker set.
pub const DATASET_SCHEMA_IDENTITY: &str = "intlify-locale-canonicalization-dataset";

/// The revision of that dataset schema.
pub const DATASET_SCHEMA_REVISION: &str = "0";

/// The digest domain 017 registers for a dataset's semantic digest.
pub const DATASET_DOMAIN: &str = "locale-canonicalization-dataset";

/// One complete dataset under dataset schema revision "0".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleCanonicalizationDataset {
    markers: Vec<LogicalPayload>,
    conformance: LocaleConformanceData,
}

/// Why a dataset cannot be built. No variant keeps any of the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetFailure {
    /// The payloads are not exactly the four marker families in 017's order.
    Markers,
    /// The conformance data is not in its canonical order.
    Conformance(ConformanceFailure),
}

impl LocaleCanonicalizationDataset {
    /// Put four logical payloads and the conformance data under the dataset
    /// schema. Nothing is sorted here: a dataset in any order but 017's would
    /// have a second digest for the same data, so it is refused instead.
    pub fn new(
        markers: Vec<LogicalPayload>,
        conformance: LocaleConformanceData,
    ) -> Result<Self, DatasetFailure> {
        if !markers.iter().map(LogicalPayload::marker).eq(Marker::ALL) {
            return Err(DatasetFailure::Markers);
        }
        conformance.check().map_err(DatasetFailure::Conformance)?;
        Ok(Self {
            markers,
            conformance,
        })
    }

    /// `H("locale-canonicalization-dataset", dataset)`.
    pub fn semantic_digest(&self) -> Result<SemanticDigest, EncodingFailure> {
        hash(Domain::literal(DATASET_DOMAIN), &self.preimage()?).map(SemanticDigest::from_hash)
    }

    /// The complete dataset value that the digest frames.
    pub(crate) fn preimage(&self) -> Result<Value, EncodingFailure> {
        serde_json::to_value(Preimage {
            dataset_schema: VersionedIdentity::literal(
                DATASET_SCHEMA_IDENTITY,
                DATASET_SCHEMA_REVISION,
            ),
            markers: self
                .markers
                .iter()
                .map(|payload| MarkerEntry {
                    marker: payload.marker().name(),
                    payload: payload.value(),
                })
                .collect(),
            conformance: &self.conformance,
        })
        .map_err(|_| EncodingFailure::Serialization)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Preimage<'a> {
    dataset_schema: VersionedIdentity,
    markers: Vec<MarkerEntry<'a>>,
    conformance: &'a LocaleConformanceData,
}

#[derive(Serialize)]
struct MarkerEntry<'a> {
    marker: &'static str,
    payload: &'a Value,
}

#[cfg(test)]
mod tests;
