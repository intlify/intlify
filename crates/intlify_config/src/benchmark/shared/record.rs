// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The closed common records, narrowed to this implementation.
//!
//! The envelope, its integrity rule, and the reference forms belong to
//! `intlify_measurement`. This module only names the tool that produced these
//! records, which is this owner rather than the common pipeline.

use intlify_measurement::identity::VersionedIdentity;

/// The exact tool that produced this owner's common records.
pub(super) fn producing_tool() -> VersionedIdentity {
    VersionedIdentity::new(
        "intlify-config-minimum-measurement",
        env!("CARGO_PKG_VERSION"),
    )
    .expect("registered producing tool")
}
