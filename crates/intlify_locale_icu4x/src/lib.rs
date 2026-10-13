// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The ICU4X 2.2.0 reference adapter for design 015's Locale
//! Canonicalization Specification revision "0".
//!
//! It reads design 017's Locale Canonicalization Data Artifact in 017's six
//! steps: the manifest at design 015's pre-invocation boundary, its
//! integrity, each payload's length and file digest, the conformance data and
//! the blob of the four ICU4X marker families that revision admits, and the
//! logical dataset rebuilt to compare its semantic digest. From the blob it
//! builds ICU4X's extended `LocaleCanonicalizer`, and around it design 015's
//! revision "0" conformance layer, which checks an identifier against the
//! dataset before ICU4X canonicalizes it and corrects it after.
//!
//! It is not yet a provider for `intlify_config`. Without the `generate`
//! feature it links no compiled data, so nothing reaches it but the blob it
//! is given.

mod canonicalize;
#[cfg(feature = "generate")]
pub mod cldr;
mod conformance;
mod dataset;
#[cfg(feature = "generate")]
pub mod generate;
mod identifier;
mod layer;
mod manifest;
mod markers;
mod payload;
mod primitives;
mod reader;
pub mod schema;

pub use canonicalize::RawFailure;
pub use conformance::{
    Collection, ConformanceFailure, ConformanceLimits, Correction, Extension, ExtensionKey,
    ExtensionType, Limited, LocaleConformanceData, TypeClass, ValidityField, ValidityGroup,
    ValidityStatus, ValueType,
};
pub use dataset::{
    DatasetFailure, LocaleCanonicalizationDataset, DATASET_DOMAIN, DATASET_SCHEMA_IDENTITY,
    DATASET_SCHEMA_REVISION,
};
pub use layer::{Component, LayerFailure, Rejection};
pub use manifest::{
    ArtifactKind, Cldr, CldrSource, DatasetDescriptor, EnvelopeFailure, EnvelopeLimit, Generation,
    LocaleCanonicalizationDataArtifact, MarkerData, Payload, RevisionZero, Role, ARTIFACT_KIND,
    ARTIFACT_SCHEMA_REVISION, INTEGRITY_DOMAIN,
};
pub use markers::{BlobFailure, Layout, Marker, MarkerBlob};
pub use payload::{LogicalPayload, PayloadFailure};
pub use primitives::{FileDigest, LogicalAddress, NonemptyText, SemanticDigest};
pub use reader::{
    AdmittedData, ArtifactLimit, DataFailure, PayloadCheck, ReadFailure, ReadLimits,
    VerifiedManifest, CONFORMANCE_FORMAT, MARKERS_FORMAT,
};
