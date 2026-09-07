// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fixed owner-fixture input binding. This covers preparation as well as the
//! actual immutable operation input; it is not a common Measurement Case ID or
//! shared-artifact digest. No acquisition, clock, run, or physical ID enters it.

use crate::benchmark::observation::{document, Digest, Frame};
use crate::benchmark::operation::Prepared;
use crate::input_limits::InputLimits;
use crate::structural::StructuralLimits;

use super::prepare::{selector_input, Candidate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) enum ContextFailure {
    Encoding,
    OperationMismatch,
    SelectorMismatch,
}

pub(super) fn observe(candidate: &Candidate) -> Result<Digest, ContextFailure> {
    if candidate.prepared.operation() != candidate.declaration.operation {
        return Err(ContextFailure::OperationMismatch);
    }
    let mut frame = Frame::new("fixture-input-context");
    frame
        .json(&serde_json::to_value(&candidate.declaration).map_err(|_| ContextFailure::Encoding)?);
    // Pin the limits used before the measured operation too. These describe
    // fixture preparation, not additional work inside a component interval.
    input_limits(&mut frame, candidate.input_limits);
    frame.text(candidate.prepared.operation().boundary());
    match &candidate.prepared {
        Prepared::Entry { source, limits } => {
            frame.bytes(source);
            input_limits(&mut frame, *limits);
        }
        Prepared::Structural {
            schema,
            doc,
            limits,
        } => {
            frame.digest(document(doc).identity());
            frame.json(schema.schema_body());
            structural_limits(&mut frame, *limits);
        }
        Prepared::Authoring(analysis) => {
            // Complete prepared input, including source, schema, bounds, and
            // admitted fragments; not just a claim that preparation succeeded.
            frame.digest(analysis.benchmark_observation().identity());
        }
        Prepared::Select { analysis, selector } => {
            let expected = selector_input(
                candidate.declaration.selector,
                analysis.benchmark_profile_id_bound(),
            );
            if !selector.benchmark_matches_fixture(&expected) {
                return Err(ContextFailure::SelectorMismatch);
            }
            // The declaration already names the fixed non-secret selector
            // recipe. Never hash or disclose an arbitrary submitted string.
            frame.digest(analysis.benchmark_observation().identity());
        }
    }
    Ok(frame.finish())
}

fn input_limits(frame: &mut Frame, limits: InputLimits) {
    for bound in [
        limits.raw.max_file_bytes,
        limits.raw.max_parser_tokens,
        limits.value.max_nodes,
        limits.value.max_depth,
        limits.value.max_collection_entries,
        limits.value.max_total_string_bytes,
        limits.value.max_single_string_bytes,
    ] {
        frame.uint(bound.get());
    }
}

fn structural_limits(frame: &mut Frame, limits: StructuralLimits) {
    for bound in [
        limits.max_profiles,
        limits.max_profile_id_bytes,
        limits.max_structural_analysis_units,
    ] {
        frame.uint(bound.get());
    }
}
