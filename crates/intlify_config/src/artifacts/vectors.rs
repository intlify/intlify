// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The vectors an independent implementation checks, one document for each
//! representation this crate reads: the Intent Surface-Class Vocabulary
//! Artifact, the envelopes of the Submitted Profile Resolution Artifact
//! Collection, 015's record framing with the text of a Resolver Construction
//! Identity, 015's bound values with the capability bounds, the regular
//! expressions of Draft 7's `pattern` under revision `"0"`, and the numbers,
//! JSON values, URI references, and JSON Pointers its Draft 7 validator reads
//! exactly. Each document is written by this crate and read back by its
//! tests, and a Node checker that shares no code with it recomputes every
//! digest and decision, so a disagreement says which of the two is wrong.

pub mod bounds;
pub mod envelope;
pub mod framing;
pub mod pattern;
pub mod validator_values;
pub mod vocabulary;
