// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The registered 026 digest domains, narrowed to this implementation.
//!
//! Framing, the digest function, the registered domains, and the verification
//! record's single-member self-exclusion belong to `intlify_measurement`.

pub(in crate::benchmark) use intlify_measurement::encoding::{record_hash, EncodingFailure};
