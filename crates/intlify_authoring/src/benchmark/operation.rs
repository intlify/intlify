// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The operations this owner measures, and what counts as the same result.
//!
//! Each operation is the ordinary internal one. The harness prepares its input,
//! measures exactly one invocation, and afterwards observes the output. Nothing
//! inside a measured interval encodes an observation, validates a result, or
//! reads a counter.

use std::cell::RefCell;

use intlify_shared_json::quantity::Quantity;
use intlify_shared_json::token::{Token, VersionedIdentity};
use serde::{Deserialize, Serialize};

use super::observation::{Frame, Observation};
use crate::context::test_context::TestContext;
use crate::declaration::measured::{self, ContextFacts};
use crate::declaration::{DeclarationFacts, DeclarationInput};
use crate::limits::AuthoringLimits;
use crate::message::{analyze_message, encode, LiteralFailure, MessageAnalysis, MessageInput};
use crate::primitives::Occurrence;
use crate::workspace::AnalysisWorkspace;

/// The six-way vocabulary 016 gives this crate's measured operations.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Operation {
    LiteralEncode,
    Mf2ParseAndSemanticFacts,
    SourceLocaleAndSurfaceClass,
    DeclarationFacts,
}

impl Operation {
    /// The complete set this milestone measures.
    ///
    /// Source discovery and identity reconciliation are later phases. A case
    /// here never reports their work as zero; it states what it covers.
    #[cfg(test)]
    pub(super) const ALL: [Self; 4] = [
        Self::LiteralEncode,
        Self::Mf2ParseAndSemanticFacts,
        Self::SourceLocaleAndSurfaceClass,
        Self::DeclarationFacts,
    ];

    pub(super) const fn phase(self) -> &'static str {
        match self {
            Self::LiteralEncode | Self::Mf2ParseAndSemanticFacts => "message_analysis",
            Self::SourceLocaleAndSurfaceClass => "context_resolution",
            Self::DeclarationFacts => "authoring_result",
        }
    }

    pub(super) const fn cost(self) -> &'static str {
        match self {
            Self::LiteralEncode => "literal_encode",
            Self::Mf2ParseAndSemanticFacts => "mf2_parse_and_semantic_facts",
            Self::SourceLocaleAndSurfaceClass => "source_locale_and_surface_class",
            Self::DeclarationFacts => "declaration_facts",
        }
    }

    pub(super) const fn boundary(self) -> &'static str {
        match self {
            Self::LiteralEncode => "intlify-authoring-literal-encode",
            Self::Mf2ParseAndSemanticFacts => "intlify-authoring-mf2-semantics",
            Self::SourceLocaleAndSurfaceClass => "intlify-authoring-context-resolution",
            Self::DeclarationFacts => "intlify-authoring-declaration-facts",
        }
    }
}

/// One counted fact about what an operation actually processed.
///
/// These are counts, never durations, and never a rate derived from one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct WorkFact {
    pub(super) name: Token,
    pub(super) observation: Quantity,
}

/// The complete logical work one case performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LogicalWork {
    pub(super) specification: VersionedIdentity,
    pub(super) facts: Vec<WorkFact>,
}

impl LogicalWork {
    fn new(facts: impl IntoIterator<Item = (&'static str, u64)>) -> Self {
        Self {
            specification: VersionedIdentity::literal("intlify-authoring-logical-work", "0"),
            facts: facts
                .into_iter()
                .map(|(name, value)| WorkFact {
                    name: Token::literal(name),
                    observation: Quantity::new(value),
                })
                .collect(),
        }
    }
}

/// What one measured invocation produced, observed after the interval closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Observed {
    pub(super) observation: Observation,
    pub(super) work: LogicalWork,
    /// Whether the operation produced its complete result.
    ///
    /// A fixture declares which path it expects, and this is what that
    /// declaration is checked against.
    pub(super) complete: bool,
}

// --- literal encoding ------------------------------------------------------

pub(super) type EncodeInput<'a> = (
    &'a str,
    &'a AuthoringLimits,
    &'a RefCell<Vec<crate::message::ExtractionSegment>>,
);

pub(super) type EncodeOutput = Result<String, LiteralFailure>;

pub(super) fn invoke_encode(input: EncodeInput<'_>) -> EncodeOutput {
    let (text, limits, segments) = input;
    encode(text, limits, &mut segments.borrow_mut())
}

pub(super) fn observe_encode(output: &EncodeOutput, input: EncodeInput<'_>) -> Observed {
    let (text, _, segments) = input;
    let mut semantic = Frame::new("literal-encode");
    let segments = segments.borrow();
    let Ok(encoded) = output else {
        semantic.uint(0);
        return Observed {
            observation: Observation {
                semantic: semantic.finish(),
                positions: None,
            },
            work: LogicalWork::new([("input_bytes", text.len() as u64), ("emitted_bytes", 0)]),
            complete: false,
        };
    };
    semantic.uint(1);
    semantic.text(encoded);
    let mut positions = Frame::new("literal-encode-positions");
    for segment in segments.iter() {
        positions.uint(segment.extracted().start());
        positions.uint(segment.extracted().end());
        positions.uint(segment.source().start());
        positions.uint(segment.source().end());
    }
    Observed {
        observation: Observation {
            semantic: semantic.finish(),
            positions: Some(positions.finish()),
        },
        work: LogicalWork::new([
            ("input_bytes", text.len() as u64),
            ("emitted_bytes", encoded.len() as u64),
            ("extraction_segments", segments.len() as u64),
        ]),
        complete: true,
    }
}

// --- MF2 parse and semantic facts -----------------------------------------

pub(super) type Mf2Input<'a> = (
    &'a str,
    &'a Occurrence,
    &'a AuthoringLimits,
    &'a RefCell<AnalysisWorkspace>,
);

pub(super) fn invoke_mf2(input: Mf2Input<'_>) -> Option<MessageAnalysis> {
    let (source, occurrence, limits, workspace) = input;
    analyze_message(
        MessageInput::Mf2(source),
        occurrence,
        limits,
        &mut workspace.borrow_mut(),
    )
    .ok()
}

// The capture measures an operation through `fn(I) -> O` and observes it
// through `fn(&O, I)`, so the observer takes whatever the operation returned.
// An `Option<&T>` here would not be that type.
#[allow(
    clippy::ref_option,
    reason = "the capture contract fixes this signature"
)]
pub(super) fn observe_mf2(output: &Option<MessageAnalysis>, _: Mf2Input<'_>) -> Observed {
    let mut semantic = Frame::new("mf2-semantics");
    let mut positions = Frame::new("mf2-semantics-positions");
    let Some(analysis) = output else {
        semantic.uint(0);
        return Observed {
            observation: Observation {
                semantic: semantic.finish(),
                positions: None,
            },
            work: LogicalWork::new([("operational_failure", 1)]),
            complete: false,
        };
    };
    semantic.uint(1);
    semantic.text(analysis.mf2_source());
    semantic.flag(analysis.facts().is_some());
    let parameters = analysis.facts().map_or(0, |facts| {
        for name in facts.parameters() {
            semantic.text(name);
        }
        semantic
            .json(&serde_json::to_value(facts.message()).expect("the projection model serializes"));
        facts.parameters().len() as u64
    });
    for diagnostic in analysis.diagnostics() {
        semantic.text(diagnostic.origin().code());
    }
    for segment in analysis.extraction_map() {
        positions.uint(segment.extracted().start());
        positions.uint(segment.extracted().end());
        positions.uint(segment.source().start());
        positions.uint(segment.source().end());
    }
    Observed {
        observation: Observation {
            semantic: semantic.finish(),
            positions: Some(positions.finish()),
        },
        work: LogicalWork::new([
            ("source_bytes", analysis.mf2_source().len() as u64),
            ("external_parameters", parameters),
            ("diagnostics", analysis.diagnostics().len() as u64),
        ]),
        // Returning an analysis is not producing facts: a message the parser
        // reported on was understood well enough to be rejected, not to be used.
        complete: analysis.facts().is_some(),
    }
}

// --- context resolution ----------------------------------------------------

pub(super) type ContextInput<'a> = (
    &'a TestContext,
    &'a DeclarationInput<'a>,
    &'a AuthoringLimits,
);

pub(super) fn invoke_context(input: ContextInput<'_>) -> Option<Result<ContextFacts, usize>> {
    let (context, declaration, limits) = input;
    measured::context_facts(context, declaration, limits).ok()
}

// The capture measures an operation through `fn(I) -> O` and observes it
// through `fn(&O, I)`, so the observer takes whatever the operation returned.
// An `Option<&T>` here would not be that type.
#[allow(
    clippy::ref_option,
    reason = "the capture contract fixes this signature"
)]
pub(super) fn observe_context(
    output: &Option<Result<ContextFacts, usize>>,
    _: ContextInput<'_>,
) -> Observed {
    let mut semantic = Frame::new("context-resolution");
    match output {
        None => {
            semantic.uint(0);
            Observed {
                observation: Observation {
                    semantic: semantic.finish(),
                    positions: None,
                },
                work: LogicalWork::new([("operational_failure", 1)]),
                complete: false,
            }
        }
        Some(Err(reported)) => {
            semantic.uint(1);
            semantic.uint(*reported as u64);
            Observed {
                observation: Observation {
                    semantic: semantic.finish(),
                    positions: None,
                },
                work: LogicalWork::new([("diagnostics", *reported as u64)]),
                complete: false,
            }
        }
        Some(Ok(facts)) => {
            semantic.uint(2);
            semantic.text(facts.locale.as_str());
            semantic.text(facts.basis.as_str());
            semantic.text(&facts.surface_class);
            semantic.flag(facts.usage.is_some());
            semantic.flag(facts.description.is_some());
            if let Some(description) = &facts.description {
                semantic.text(description);
            }
            Observed {
                observation: Observation {
                    semantic: semantic.finish(),
                    positions: None,
                },
                work: LogicalWork::new([("resolved_facts", 4), ("diagnostics", 0)]),
                complete: true,
            }
        }
    }
}

// --- declaration facts -----------------------------------------------------

pub(super) type FactsInput<'a> = (
    &'a DeclarationInput<'a>,
    &'a MessageAnalysis,
    &'a ContextFacts,
    &'a AuthoringLimits,
);

pub(super) type FactsOutput = Option<
    Result<
        (
            DeclarationFacts,
            intlify_shared_json::token::IntegrityDigest,
        ),
        usize,
    >,
>;

pub(super) fn invoke_facts(input: FactsInput<'_>) -> FactsOutput {
    let (declaration, analysis, context, limits) = input;
    measured::declaration_facts(declaration, analysis, context, limits).ok()
}

// The capture measures an operation through `fn(I) -> O` and observes it
// through `fn(&O, I)`, so the observer takes whatever the operation returned.
// An `Option<&T>` here would not be that type.
#[allow(
    clippy::ref_option,
    reason = "the capture contract fixes this signature"
)]
pub(super) fn observe_facts(output: &FactsOutput, _: FactsInput<'_>) -> Observed {
    let mut semantic = Frame::new("declaration-facts");
    match output {
        None => {
            semantic.uint(0);
            Observed {
                observation: Observation {
                    semantic: semantic.finish(),
                    positions: None,
                },
                work: LogicalWork::new([("operational_failure", 1)]),
                complete: false,
            }
        }
        Some(Err(reported)) => {
            semantic.uint(1);
            semantic.uint(*reported as u64);
            Observed {
                observation: Observation {
                    semantic: semantic.finish(),
                    positions: None,
                },
                work: LogicalWork::new([("diagnostics", *reported as u64)]),
                complete: false,
            }
        }
        Some(Ok((facts, revision))) => {
            semantic.uint(2);
            semantic.text(facts.mf2_source());
            semantic.text(facts.surface_class());
            semantic.text(facts.source_locale_basis().as_str());
            // The revision is the point of the operation, so it is observed
            // directly rather than through the projection that produced it.
            semantic.text(revision.as_str());
            let mut positions = Frame::new("declaration-facts-positions");
            for segment in facts.extraction_map() {
                positions.uint(segment.extracted().start());
                positions.uint(segment.extracted().end());
                positions.uint(segment.source().start());
                positions.uint(segment.source().end());
            }
            Observed {
                observation: Observation {
                    semantic: semantic.finish(),
                    positions: Some(positions.finish()),
                },
                work: LogicalWork::new([
                    ("mf2_bytes", facts.mf2_source().len() as u64),
                    ("extraction_segments", facts.extraction_map().len() as u64),
                ]),
                complete: true,
            }
        }
    }
}
