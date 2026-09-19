// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The execution state a measured case ran in.
//!
//! These are 026's independent state fields, not one summary word. Each says
//! what was actually reused or rebuilt between samples, and none of them
//! claims a warm cache, a resident dataset, or a prepared allocator unless the
//! owner observed it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Whether a measured operation writes into a caller-owned output buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
pub enum OutputBuffer {
    NotApplicable {},
    Applicable { ownership: String, reuse: String },
}

/// The complete execution state of one measured case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
// Every field ends in `_state` because 026 names them that way: each is an
// independent state, and collapsing them into one would lose which of them was
// actually observed.
pub struct Execution {
    pub process_state: String,
    pub engine_state: String,
    pub initial_preparation_state: String,
    pub cache_state: String,
    pub runtime_compilation_state: String,
    pub managed_heap_state: String,
    pub scratch_reuse_state: String,
    pub output_buffer_state: OutputBuffer,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_output_buffer_state_separates_absence_from_a_reused_buffer() {
        let absent = serde_json::to_value(OutputBuffer::NotApplicable {}).unwrap();
        assert_eq!(absent, serde_json::json!({"state": "not-applicable"}));
        let present = serde_json::to_value(OutputBuffer::Applicable {
            ownership: "caller-owned".into(),
            reuse: "fresh".into(),
        })
        .unwrap();
        assert_eq!(
            present,
            serde_json::json!({
                "state": "applicable",
                "ownership": "caller-owned",
                "reuse": "fresh"
            })
        );
        // An operation with no output buffer is not an operation with a fresh
        // one, so the two states never decode into each other.
        assert!(serde_json::from_value::<OutputBuffer>(
            serde_json::json!({"state": "not-applicable", "reuse": "fresh"})
        )
        .is_err());
    }
}
