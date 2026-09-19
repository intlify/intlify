// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The record-integrity preimage, narrowed to this implementation's tests.
//!
//! Framing, the digest function, the registered domains, and the verification
//! record's single-member self-exclusion belong to `intlify_measurement`.

#[cfg(test)]
pub(super) use intlify_measurement::encoding::record_hash;
