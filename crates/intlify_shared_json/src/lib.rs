// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Shared JSON primitives, canonical framing, and digests from design 017.
//!
//! This crate implements the representation subset that several Intlify owners
//! exchange: the exact token grammars, the `UInt64` decimal-string domain, the
//! `intlify.shared-json.v0` value framing, the domain-separated digest
//! function, and bounded strict decoding.
//!
//! It deliberately owns no registry. Digest domains, record kinds, identity
//! domains, and body schemas belong to the owning design and its implementing
//! crate. Encoding a value establishes its representation and integrity only;
//! admission, authorization, and semantic checks remain owner decisions.
//!
//! The crate performs no file, network, schema, or plugin retrieval, and it
//! holds no mutable global state.

pub mod decode;
#[cfg(feature = "digest")]
pub mod encoding;
pub mod json;
pub mod location;
pub mod quantity;
pub mod token;
