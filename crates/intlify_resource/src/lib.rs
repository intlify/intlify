// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Consumer-neutral host resource extraction and validated write-back support.
//!
//! This crate owns host-document identity, spans, resource limits, offset maps,
//! and adapter artifacts. It intentionally does not depend on the MF2 parser,
//! formatter, linter, or CLI: consumers compose extracted message entries with
//! those message-level cores.

mod error;
mod identity;
mod limits;
mod offset_map;
mod span;

pub use error::{
    DeclaredFormat, DocumentUnsupportedFeature, EntryUnsupportedReason, FormatClassificationSource,
    InternalResourceErrorReason, ResourceError, ResourceErrorCode, ResourceErrorDetails,
    ResourceErrorSite, ResourcePhase,
};
pub use identity::{CatalogKey, CatalogKeyDomain, EntryHandle, EntryKey, StructuralPathKey};
pub use limits::{
    preflight_host_bytes, ResourceLimit, MAX_ENTRIES, MAX_HOST_BYTES, MAX_IDENTITY_BYTES,
    MAX_MESSAGE_BYTES, MAX_NESTING_DEPTH, MAX_OFFSET_MAP_SEGMENTS, MAX_TOTAL_MESSAGE_BYTES,
};
pub use offset_map::{MessageOffsetMap, OffsetMapError};
pub use span::Utf8ByteSpan;

#[cfg(test)]
mod tests {
    use super::{
        CatalogKey, EntryHandle, EntryKey, MessageOffsetMap, ResourceError, ResourceErrorSite,
        StructuralPathKey, Utf8ByteSpan,
    };

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn public_core_values_are_send_and_sync() {
        assert_send_sync::<Utf8ByteSpan>();
        assert_send_sync::<StructuralPathKey>();
        assert_send_sync::<CatalogKey>();
        assert_send_sync::<EntryKey>();
        assert_send_sync::<EntryHandle>();
        assert_send_sync::<MessageOffsetMap>();
        assert_send_sync::<ResourceErrorSite>();
        assert_send_sync::<ResourceError>();
    }
}
