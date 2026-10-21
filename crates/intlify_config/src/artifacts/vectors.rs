// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The vectors an independent implementation checks, one document for each
//! representation this crate reads: the Intent Surface-Class Vocabulary
//! Artifact, and the envelopes of the Submitted Profile Resolution Artifact
//! Collection. Each document is written by this crate and read back by its
//! tests, and a Node checker that shares no code with it recomputes every
//! digest and decision, so a disagreement says which of the two is wrong.

pub mod envelope;
pub mod vocabulary;
