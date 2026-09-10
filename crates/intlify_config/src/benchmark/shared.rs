// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 017 encodings used by the adopted 026 measurement records. Native owner
//! checksums keep their own algorithm and framing; this is not their codec.

pub(super) mod build;
pub(super) mod decode;
pub(super) mod encoding;
pub(super) mod environment;
pub(super) mod identity;
pub(super) mod measurement;
pub(super) mod pipeline;
pub(super) mod plan;
pub(super) mod reason;
pub(super) mod record;
