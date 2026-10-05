// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The operations this owner measures, and what counts as the same result.
//!
//! Each operation is the ordinary internal one. The harness prepares its input,
//! measures exactly one invocation, and afterwards observes the output. Nothing
//! inside a measured interval encodes an observation, validates a result, or
//! counts work beyond what the operation itself counts.

use std::cell::RefCell;

use intlify_authoring::test_context::TestContext;
use intlify_authoring::{ByteRange, Completeness, Diagnostic, Location, UnitOutcome};
use intlify_measurement::owner_run::observation::{Frame, Observation};
use intlify_measurement::owner_run::run::Expected;
use intlify_shared_json::quantity::Quantity;
use intlify_shared_json::token::{Token, VersionedIdentity};
use serde::{Deserialize, Serialize};

use super::owner::LABELS;
use crate::analysis::{classify, Classification, UnitAnalysis, UnitWork};
use crate::metadata::Annotation;
use crate::{
    assemble_inventory, AdmittedUnit, AssembledInventory, CheckedInventory, JsAnalysisWorkspace,
    JsAuthoringLimits, JsAuthoringProfile, JsLimitKind, ProducerFailure, UnitMember,
};

/// The two operations 016 names for this phase's Producer.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Operation {
    /// One admitted unit to its classified declarations, uses and exclusions.
    ParseAndClassify,
    /// Analyzed units to a sealed inventory and its bytes.
    InventoryAssembly,
}

impl Operation {
    /// The complete set this phase measures.
    ///
    /// Message analysis and context resolution are Phase 1's, measured by
    /// `intlify_authoring`. Identity reconciliation is a later phase. A case
    /// here never reports their work as zero; it states what it covers.
    #[cfg(test)]
    pub(super) const ALL: [Self; 2] = [Self::ParseAndClassify, Self::InventoryAssembly];

    pub(super) const fn phase(self) -> &'static str {
        match self {
            Self::ParseAndClassify => "source_discovery",
            Self::InventoryAssembly => "authoring_result",
        }
    }

    pub(super) const fn cost(self) -> &'static str {
        match self {
            Self::ParseAndClassify => "parse_and_classify",
            Self::InventoryAssembly => "inventory_assembly",
        }
    }

    pub(super) const fn boundary(self) -> &'static str {
        match self {
            Self::ParseAndClassify => "intlify-authoring-js-source-discovery",
            Self::InventoryAssembly => "intlify-authoring-js-inventory-assembly",
        }
    }
}

/// How one counted fact is known.
///
/// A count this harness does not take is said to be unavailable, and work a
/// path never does is said not to apply. Neither is written as zero, because
/// zero is a count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum Count {
    /// Counted in full.
    Exact { value: Quantity },
    /// The operation stopped past this many, so the full count is unknown.
    AtLeast { value: Quantity },
    /// This revision of the harness does not count it.
    Unavailable {},
    /// The path the operation took does no such work.
    NotApplicable {},
}

impl Count {
    const fn exact(value: u64) -> Self {
        Self::Exact {
            value: Quantity::new(value),
        }
    }
}

/// One counted fact about what an operation processed.
///
/// These are counts, never durations, and never a rate derived from one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkFact {
    name: Token,
    count: Count,
}

/// The complete logical work one case performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogicalWork {
    specification: VersionedIdentity,
    facts: Vec<WorkFact>,
}

impl LogicalWork {
    fn new(facts: impl IntoIterator<Item = (&'static str, Count)>) -> Self {
        Self {
            specification: VersionedIdentity::literal("intlify-authoring-js-logical-work", "0"),
            facts: facts
                .into_iter()
                .map(|(name, count)| WorkFact {
                    name: Token::literal(name),
                    count,
                })
                .collect(),
        }
    }

    /// Borrow one fact's count by name.
    #[cfg(test)]
    pub(super) fn count(&self, name: &str) -> Option<&Count> {
        self.facts
            .iter()
            .find(|fact| fact.name.as_str() == name)
            .map(|fact| &fact.count)
    }
}

/// What one measured invocation produced, observed after the interval closed.
pub(super) type Observed = intlify_measurement::owner_run::capture::Observed<LogicalWork>;

fn range(frame: &mut Frame, range: ByteRange) {
    frame.uint(range.start());
    frame.uint(range.end());
}

/// Frame each record's meaning, and where it points apart from that.
fn records(semantic: &mut Frame, positions: &mut Frame, records: &[Diagnostic]) {
    semantic.uint(records.len() as u64);
    for record in records {
        semantic.text(record.origin().code());
        semantic.flag(record.detail().is_some());
        if let Some(detail) = record.detail() {
            semantic.text(detail.as_str());
        }
        match record.location() {
            Location::Unit(_) => positions.uint(0),
            Location::Region(region) => {
                positions.uint(1);
                range(positions, region.range());
            }
            Location::Occurrence(occurrence) => {
                positions.uint(2);
                semantic.text(occurrence.role().as_str());
                range(positions, occurrence.range());
            }
        }
    }
}

// --- source discovery ------------------------------------------------------

pub(super) type ClassifyInput<'a> = (
    &'a TestContext,
    &'a JsAuthoringProfile,
    &'a AdmittedUnit<'a>,
    &'a JsAuthoringLimits,
    &'a RefCell<JsAnalysisWorkspace>,
);

pub(super) type ClassifyOutput = Result<Classification, ProducerFailure>;

pub(super) fn invoke_classify(input: ClassifyInput<'_>) -> ClassifyOutput {
    let (context, profile, unit, limits, workspace) = input;
    classify(
        context,
        profile,
        unit,
        limits,
        &mut workspace.borrow_mut(),
        &|| false,
    )
}

/// The counts a parsed unit has, and what is said of them on other paths.
fn parsed(work: UnitWork) -> [(&'static str, Count); 4] {
    [
        ("ast_nodes", Count::exact(work.ast_nodes)),
        ("scopes", Count::exact(work.scopes)),
        ("symbols", Count::exact(work.symbols)),
        ("identifier_references", Count::exact(work.references)),
    ]
}

/// Counts this revision of the harness does not take, on any path.
const UNCOUNTED: [&str; 5] = [
    "comments",
    "intrinsic_call_candidates",
    "tracked_origins",
    "alias_links",
    "effects",
];

/// Counts of classified candidates, which only a read unit has.
const CANDIDATES: [&str; 8] = [
    "declaration_candidates",
    "use_candidates",
    "exclusions",
    "parameter_bindings",
    "input_map_segments",
    "annotations",
    "host_diagnostics",
    "outside_profile_sinks",
];

fn discovery_work(
    unit: &AdmittedUnit<'_>,
    head: impl IntoIterator<Item = (&'static str, Count)>,
    candidates: impl IntoIterator<Item = (&'static str, Count)>,
) -> LogicalWork {
    let mut facts = vec![("source_bytes", Count::exact(unit.snapshot().byte_length()))];
    facts.extend(head);
    // The receiver proof walks the syntax tree. It builds no control-flow
    // graph, so there are no blocks to count on any path.
    facts.push(("cfg_blocks", Count::NotApplicable {}));
    facts.extend(UNCOUNTED.map(|name| (name, Count::Unavailable {})));
    facts.extend(candidates);
    LogicalWork::new(facts)
}

// The capture measures an operation through `fn(I) -> O` and observes it
// through `fn(&O, I)`, so the observer takes whatever the operation returned.
pub(super) fn observe_classify(output: &ClassifyOutput, input: ClassifyInput<'_>) -> Observed {
    let (_, profile, unit, limits, _) = input;
    let mut semantic = LABELS.framing.frame("parse-and-classify");
    let mut positions = LABELS.framing.frame("parse-and-classify-positions");
    match output {
        Err(failure) => {
            semantic.uint(0);
            semantic.text(&format!("{failure:?}"));
            // Only a refused tree says how far parsing got: past the bound.
            let head = match failure {
                ProducerFailure::Limit(JsLimitKind::AstNodes) => vec![
                    ("parse_attempts", Count::exact(1)),
                    (
                        "ast_nodes",
                        Count::AtLeast {
                            value: Quantity::new(limits.ast_nodes.saturating_add(1)),
                        },
                    ),
                ],
                _ => vec![
                    ("parse_attempts", Count::Unavailable {}),
                    ("ast_nodes", Count::Unavailable {}),
                ],
            };
            let rest = ["scopes", "symbols", "identifier_references", "proof_steps"]
                .map(|name| (name, Count::Unavailable {}));
            Observed {
                observation: Observation {
                    semantic: semantic.finish(),
                    positions: None,
                },
                work: discovery_work(
                    unit,
                    head.into_iter().chain(rest),
                    CANDIDATES.map(|name| (name, Count::Unavailable {})),
                ),
                path: Expected::OperationalFailure,
            }
        }
        Ok(Classification::Failed(analysis)) => {
            semantic.uint(1);
            records(&mut semantic, &mut positions, analysis.diagnostics());
            let work = analysis.work();
            // A unit that is not text is never parsed; one the host rejected
            // was parsed once, but left no tree to count.
            let tree = if work.parse_attempts == 0 {
                Count::NotApplicable {}
            } else {
                Count::Unavailable {}
            };
            let head = [
                ("parse_attempts", Count::exact(work.parse_attempts)),
                ("ast_nodes", tree.clone()),
                ("scopes", tree.clone()),
                ("symbols", tree.clone()),
                ("identifier_references", tree),
                ("proof_steps", Count::NotApplicable {}),
            ];
            let candidates = CANDIDATES.map(|name| match name {
                "host_diagnostics" => (name, Count::exact(analysis.diagnostics().len() as u64)),
                _ => (name, Count::NotApplicable {}),
            });
            Observed {
                observation: Observation {
                    semantic: semantic.finish(),
                    positions: Some(positions.finish()),
                },
                work: discovery_work(unit, head, candidates),
                path: Expected::Blocked,
            }
        }
        Ok(Classification::Read(read)) => {
            semantic.uint(2);
            let recognized = &read.recognized;
            let mut bindings = 0;
            let mut segments = 0;
            let mut annotations = 0;
            semantic.uint(recognized.declarations.len() as u64);
            for declared in &recognized.declarations {
                semantic.text(declared.occurrence.role().as_str());
                semantic.flag(matches!(declared.source, crate::explicit::Source::Authored));
                semantic.text(&declared.text);
                range(&mut positions, declared.occurrence.range());
                segments += declared.input_map.len() as u64;
                for segment in &declared.input_map {
                    range(&mut positions, segment.input());
                    range(&mut positions, segment.source());
                }
                // A declaration's own use site carries the same bindings as
                // the use recorded for it, so they are counted once, there.
                semantic.flag(declared.parameters.is_some());
                for binding in declared.parameters.iter().flatten() {
                    semantic.text(binding.name());
                    range(&mut positions, binding.expression().range());
                }
                match &declared.annotation {
                    Annotation::Absent => semantic.uint(0),
                    Annotation::Valid(values) => {
                        annotations += 1;
                        semantic.uint(1);
                        for value in [
                            &values.source_locale,
                            &values.surface_class,
                            &values.description,
                        ] {
                            semantic.flag(value.is_some());
                            if let Some(value) = value {
                                semantic.text(value);
                            }
                        }
                    }
                    Annotation::Rejected => {
                        annotations += 1;
                        semantic.uint(2);
                    }
                }
            }
            semantic.uint(recognized.uses.len() as u64);
            for used in &recognized.uses {
                semantic.uint(used.declaration as u64);
                semantic.flag(used.shared);
                range(&mut positions, used.occurrence.range());
                for binding in &used.parameters {
                    bindings += 1;
                    semantic.text(binding.name());
                    range(&mut positions, binding.expression().range());
                }
                semantic.uint(used.parameters.len() as u64);
            }
            semantic.uint(recognized.exclusions.len() as u64);
            for exclusion in &recognized.exclusions {
                semantic.text(exclusion.reason());
                range(&mut positions, exclusion.occurrence().range());
            }
            let diagnostics = read.reporter.diagnostics();
            records(&mut semantic, &mut positions, diagnostics);
            semantic.uint(read.outside_profile);
            let work = read.work;
            let head = [("parse_attempts", Count::exact(work.parse_attempts))]
                .into_iter()
                .chain(parsed(work))
                .chain([(
                    "proof_steps",
                    // Nothing is proven without a DOM global to prove from.
                    if profile.admits_document() {
                        Count::exact(work.proof_steps)
                    } else {
                        Count::NotApplicable {}
                    },
                )]);
            let candidates = [
                (
                    "declaration_candidates",
                    recognized.declarations.len() as u64,
                ),
                ("use_candidates", recognized.uses.len() as u64),
                ("exclusions", recognized.exclusions.len() as u64),
                ("parameter_bindings", bindings),
                ("input_map_segments", segments),
                ("annotations", annotations),
                ("host_diagnostics", diagnostics.len() as u64),
                ("outside_profile_sinks", read.outside_profile),
            ]
            .map(|(name, value)| (name, Count::exact(value)));
            Observed {
                observation: Observation {
                    semantic: semantic.finish(),
                    positions: Some(positions.finish()),
                },
                work: discovery_work(unit, head, candidates),
                // A unit with a record that blocks it reports why it cannot be
                // checked; it is not refused.
                path: if read.reporter.blocks() {
                    Expected::Blocked
                } else {
                    Expected::Complete
                },
            }
        }
    }
}

// --- inventory assembly ----------------------------------------------------

pub(super) type AssembleInput<'a> = (
    &'a TestContext,
    &'a JsAuthoringProfile,
    &'a str,
    Completeness,
    &'a [UnitMember],
    &'a [UnitAnalysis],
    &'a JsAuthoringLimits,
);

pub(super) type AssembleOutput = Result<(AssembledInventory, Option<Vec<u8>>), ProducerFailure>;

pub(super) fn invoke_assemble(input: AssembleInput<'_>) -> AssembleOutput {
    let (context, profile, scope, completeness, membership, analyses, limits) = input;
    let assembled = assemble_inventory(
        context,
        profile,
        scope,
        completeness,
        membership,
        analyses,
        limits,
    )?;
    // The bytes are what a consumer is handed, so producing them is part of
    // the operation measured.
    let bytes = serde_json::to_vec(assembled.inspection_inventory()).ok();
    Ok((assembled, bytes))
}

pub(super) fn observe_assemble(output: &AssembleOutput, input: AssembleInput<'_>) -> Observed {
    let (_, _, _, _, membership, analyses, _) = input;
    let mut semantic = LABELS.framing.frame("inventory-assembly");
    let supplied = [
        ("units", Count::exact(analyses.len() as u64)),
        ("members", Count::exact(membership.len() as u64)),
    ];
    let assembled = match output {
        Err(failure) => {
            semantic.uint(0);
            semantic.text(&format!("{failure:?}"));
            return Observed {
                observation: Observation {
                    semantic: semantic.finish(),
                    positions: None,
                },
                work: LogicalWork::new(
                    supplied
                        .into_iter()
                        .chain(ASSEMBLED.map(|name| (name, Count::Unavailable {}))),
                ),
                path: Expected::OperationalFailure,
            };
        }
        Ok(assembled) => assembled,
    };
    let (inventory, bytes) = assembled;
    semantic.uint(1);
    semantic.uint(match inventory.checked_inventory() {
        None => 0,
        Some(CheckedInventory::Complete(_)) => 1,
        Some(CheckedInventory::Partial(_)) => 2,
    });
    semantic.flag(bytes.is_some());
    if let Some(bytes) = bytes {
        semantic.bytes(bytes);
    }
    let mut ignored = LABELS.framing.frame("inventory-assembly-positions");
    records(&mut semantic, &mut ignored, inventory.diagnostics());
    let body = inventory.inspection_inventory().body();
    let outcome = |wanted| {
        body.units()
            .iter()
            .filter(|unit| unit.outcome() == wanted)
            .count() as u64
    };
    let counted = [
        ("declarations", body.declarations().len() as u64),
        ("references", body.references().len() as u64),
        ("exclusions", body.exclusions().len() as u64),
        ("checked_units", outcome(UnitOutcome::Checked)),
        ("blocked_units", outcome(UnitOutcome::Blocked)),
        ("failed_units", outcome(UnitOutcome::Failed)),
        ("diagnostics", inventory.diagnostics().len() as u64),
    ]
    .map(|(name, value)| (name, Count::exact(value)));
    let artifact = (
        "artifact_bytes",
        bytes.as_ref().map_or(Count::Unavailable {}, |bytes| {
            Count::exact(bytes.len() as u64)
        }),
    );
    Observed {
        // Every position is inside the artifact bytes, which the semantic
        // observation already covers.
        observation: Observation {
            semantic: semantic.finish(),
            positions: None,
        },
        work: LogicalWork::new(supplied.into_iter().chain(counted).chain([artifact])),
        path: if inventory.checked_inventory().is_some() {
            Expected::Complete
        } else {
            Expected::Blocked
        },
    }
}

/// Counts only an assembled inventory has.
const ASSEMBLED: [&str; 8] = [
    "declarations",
    "references",
    "exclusions",
    "checked_units",
    "blocked_units",
    "failed_units",
    "diagnostics",
    "artifact_bytes",
];
