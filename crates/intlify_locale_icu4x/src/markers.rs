// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The four marker families and the check that a blob holds exactly them.
//! Reading a blob proves which markers it carries and that each decodes. It
//! is not an integrity check: ICU4X decodes damaged but well-formed data as
//! other data, and in a debug build zerovec asserts on a map whose keys are
//! out of order. The 017 reader therefore checks a payload's file digest
//! before reading it here, and the dataset's semantic digest after.

use icu_locale::provider::{
    LocaleAliasesV1, LocaleLikelySubtagsExtendedV1, LocaleLikelySubtagsLanguageV1,
    LocaleLikelySubtagsScriptRegionV1,
};
use icu_locale::LocaleCanonicalizer;
use icu_provider::prelude::*;
use icu_provider_blob::BlobDataProvider;

/// One of the four ICU4X marker families revision "0" admits, named as 017
/// records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Marker {
    LocaleAliasesV1,
    LocaleLikelySubtagsExtendedV1,
    LocaleLikelySubtagsLanguageV1,
    LocaleLikelySubtagsScriptRegionV1,
}

impl Marker {
    /// Every marker, in 017's order: by name.
    pub const ALL: [Self; 4] = [
        Self::LocaleAliasesV1,
        Self::LocaleLikelySubtagsExtendedV1,
        Self::LocaleLikelySubtagsLanguageV1,
        Self::LocaleLikelySubtagsScriptRegionV1,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::LocaleAliasesV1 => "LocaleAliasesV1",
            Self::LocaleLikelySubtagsExtendedV1 => "LocaleLikelySubtagsExtendedV1",
            Self::LocaleLikelySubtagsLanguageV1 => "LocaleLikelySubtagsLanguageV1",
            Self::LocaleLikelySubtagsScriptRegionV1 => "LocaleLikelySubtagsScriptRegionV1",
        }
    }

    pub(crate) const fn info(self) -> DataMarkerInfo {
        match self {
            Self::LocaleAliasesV1 => LocaleAliasesV1::INFO,
            Self::LocaleLikelySubtagsExtendedV1 => LocaleLikelySubtagsExtendedV1::INFO,
            Self::LocaleLikelySubtagsLanguageV1 => LocaleLikelySubtagsLanguageV1::INFO,
            Self::LocaleLikelySubtagsScriptRegionV1 => LocaleLikelySubtagsScriptRegionV1::INFO,
        }
    }
}

/// How a blob lays out its index. Both are ICU4X 2.2's blob schema; its
/// exporter chooses the wide index only when the data needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Narrow,
    Wide,
}

/// Why a blob cannot supply the four marker families. No variant keeps any
/// of the blob's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobFailure {
    /// The bytes are not exactly one ICU4X 2.2 blob.
    Malformed,
    /// The blob is in a schema from before ICU4X 2.0.
    UnsupportedSchema,
    /// A required marker family is absent.
    MissingMarker(Marker),
    /// The blob carries a marker family other than the four.
    AdditionalMarker,
    /// A required marker family is present but its payload does not decode.
    Undecodable(Marker),
}

/// A blob that holds exactly the four marker families, each of which
/// decodes, with ICU4X's extended canonicalizer built from them.
pub struct MarkerBlob {
    provider: BlobDataProvider,
    layout: Layout,
    canonicalizer: LocaleCanonicalizer,
}

impl MarkerBlob {
    /// Checks the blob's index before ICU4X reads it, then decodes every
    /// marker family, so a blob that fails here never builds a canonicalizer.
    pub fn read(bytes: Box<[u8]>) -> Result<Self, BlobFailure> {
        let (layout, hashes) = index(&bytes)?;
        check(&hashes)?;
        let provider =
            BlobDataProvider::try_new_from_blob(bytes).map_err(|_| BlobFailure::Malformed)?;
        for marker in Marker::ALL {
            decodes(&provider, marker)?;
        }
        // Every family the extended canonicalizer loads has just decoded.
        let canonicalizer = LocaleCanonicalizer::try_new_extended_with_buffer_provider(&provider)
            .map_err(|_| BlobFailure::Malformed)?;
        Ok(Self {
            provider,
            layout,
            canonicalizer,
        })
    }

    pub const fn layout(&self) -> Layout {
        self.layout
    }

    pub(crate) const fn provider(&self) -> &BlobDataProvider {
        &self.provider
    }

    pub(crate) const fn canonicalizer(&self) -> &LocaleCanonicalizer {
        &self.canonicalizer
    }
}

/// The serialized layout of an ICU4X blob, mirrored because ICU4X keeps its
/// schema types private: the schema variant, then the marker hashes, the
/// locale index, and the payload buffers. The variants from before ICU4X 2.0
/// are listed only so the variant numbers line up; none is read.
#[derive(serde::Deserialize)]
enum Schema<'data> {
    V001(Unread),
    V002(Unread),
    V002Bigger(Unread),
    #[serde(borrow)]
    V003(Index<'data>),
    #[serde(borrow)]
    V003Bigger(Index<'data>),
}

#[derive(serde::Deserialize)]
struct Unread;

#[derive(serde::Deserialize)]
struct Index<'data> {
    markers: &'data [u8],
    _locales: &'data [u8],
    _buffers: &'data [u8],
}

const HASH_BYTES: usize = 4;

fn index(bytes: &[u8]) -> Result<(Layout, Vec<[u8; HASH_BYTES]>), BlobFailure> {
    let (schema, rest) =
        postcard::take_from_bytes::<Schema<'_>>(bytes).map_err(|_| BlobFailure::Malformed)?;
    let (layout, index) = match schema {
        Schema::V001(_) | Schema::V002(_) | Schema::V002Bigger(_) => {
            return Err(BlobFailure::UnsupportedSchema);
        }
        Schema::V003(index) => (Layout::Narrow, index),
        Schema::V003Bigger(index) => (Layout::Wide, index),
    };
    if !rest.is_empty() || index.markers.len() % HASH_BYTES != 0 {
        return Err(BlobFailure::Malformed);
    }
    // The remainder is empty: the length was just checked.
    let (hashes, _) = index.markers.as_chunks::<HASH_BYTES>();
    Ok((layout, hashes.to_vec()))
}

fn check(hashes: &[[u8; HASH_BYTES]]) -> Result<(), BlobFailure> {
    // ICU4X writes the index in ascending order without repeats and searches
    // it that way, so any other index is not one it wrote.
    if hashes.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(BlobFailure::Malformed);
    }
    for marker in Marker::ALL {
        if !hashes.contains(&marker.info().id.hashed().to_bytes()) {
            return Err(BlobFailure::MissingMarker(marker));
        }
    }
    if hashes.len() > Marker::ALL.len() {
        return Err(BlobFailure::AdditionalMarker);
    }
    Ok(())
}

fn decodes(provider: &BlobDataProvider, marker: Marker) -> Result<(), BlobFailure> {
    let provider = provider.as_deserializing();
    let request = DataRequest::default();
    let loaded = match marker {
        Marker::LocaleAliasesV1 => {
            DataProvider::<LocaleAliasesV1>::load(&provider, request).map(drop)
        }
        Marker::LocaleLikelySubtagsExtendedV1 => {
            DataProvider::<LocaleLikelySubtagsExtendedV1>::load(&provider, request).map(drop)
        }
        Marker::LocaleLikelySubtagsLanguageV1 => {
            DataProvider::<LocaleLikelySubtagsLanguageV1>::load(&provider, request).map(drop)
        }
        Marker::LocaleLikelySubtagsScriptRegionV1 => {
            DataProvider::<LocaleLikelySubtagsScriptRegionV1>::load(&provider, request).map(drop)
        }
    };
    loaded.map_err(|_| BlobFailure::Undecodable(marker))
}

#[cfg(test)]
pub(crate) mod tests;
