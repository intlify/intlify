// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The ICU4X 2.2.0 reference adapter for design 015's Locale
//! Canonicalization Specification revision "0".
//!
//! It reads the four ICU4X marker families that revision admits from a blob
//! the caller supplies, checks that the blob holds exactly them, projects each
//! into the logical payload that design 017 frames into the dataset's
//! semantic digest, and builds ICU4X's extended `LocaleCanonicalizer` from
//! them.
//!
//! It is not yet the Intlify conformance layer (validity data and
//! corrections), the reader of 017's Locale Canonicalization Data Artifact
//! manifest, or a provider for `intlify_config`. Its canonicalization is
//! ICU4X's raw behavior, which accepts identifiers revision "0" rejects.
//! Without the `generate` feature it links no compiled data, so nothing
//! reaches it but the blob it is given.

mod canonicalize;
#[cfg(feature = "generate")]
pub mod generate;
mod markers;
mod payload;

pub use canonicalize::RawFailure;
pub use markers::{BlobFailure, Layout, Marker, MarkerBlob};
pub use payload::{LogicalPayload, PayloadFailure};
