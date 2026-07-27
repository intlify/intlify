// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Versioned artifact transport codecs.
//!
//! This module owns bounded slice and synchronous-reader admission plus complete
//! canonical artifact encoding. All successful routes return immutable checked
//! artifacts or one complete owned JSON document.
//!
//! It does not own filesystem I/O, framing several artifacts, decompression,
//! async runtime behavior, publication, or linker semantics.

mod json;
mod reference;

pub use reference::{
    decode_reference_artifact, decode_reference_artifact_from_reader, encode_reference_artifact,
    ArtifactReadError,
};
