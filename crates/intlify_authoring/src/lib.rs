// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Language-neutral authoring semantics for design 016, Phase 1.
//!
//! This crate implements the shared meaning that every host Producer reuses:
//! the literal-versus-MF2 distinction, the MF2 parser handoff, external
//! parameter requirements, source-locale and context resolution, and 017's
//! semantic projection and Intent revision.
//!
//! It knows no host language. Callers supply already decoded message text,
//! occurrence evidence, parameter names, and metadata values; recognizing host
//! syntax, intrinsic bindings, UI surfaces, annotations, and exclusion markers
//! belongs to a Producer built on top of this crate.
//!
//! It also assigns no persistent identity. Allocating a `MessageIntentId`,
//! reading or publishing an identity registry, and reconciling declaration
//! history are Phase 3 operations that are deliberately absent here. A checked
//! result from this crate is therefore authoring evidence, not an
//! identity-resolved authoring result that a downstream consumer may treat as
//! complete input.
//!
//! No file, network, registry, or plugin is retrieved, no host code is
//! evaluated, and no mutable global state is held.

mod diagnostic;
mod limits;
mod message;
mod primitives;
mod projection;
pub mod schema;
mod specification;
mod workspace;

// Shared 017 primitives that appear in this crate's own signatures, so a
// caller does not need a direct dependency on the shared crate to use them.
pub use intlify_shared_json::token::{IntegrityDigest, Token, VersionedIdentity};

pub use diagnostic::{Diagnostic, DiagnosticOrigin, ReasonFamily, Severity, Stage};
pub use limits::{AuthoringLimits, LimitKind, LimitsError};
pub use message::{
    analyze_message, ExtractionSegment, MessageAnalysis, MessageFacts, MessageFailure, MessageInput,
};
pub use primitives::{
    ByteRange, ExactInputBinding, MessageIntentId, NonemptyText, Opaque128, OwnerIdentity,
    OwnerKind, PrimitiveError, SemanticDigest, SourceSnapshot,
};
pub use primitives::{Occurrence, OccurrenceRole};
pub use projection::{
    intent_projection, intent_revision, projection_specification, revision_preimage, Attribute,
    CatchAllKey, CatchAllTag, Declaration, Expression, ExpressionTag, Function, InputDeclaration,
    InputTag, IntentProjection, LiteralKey, LiteralTag, LiteralValue, LocalDeclaration, LocalTag,
    MarkupForm, MarkupPart, MarkupTag, MatchBody, MatchTag, MessageBody, MessageProjection, Opt,
    PatternBody, PatternPart, PatternTag, RevisionFailure, TextPart, TextTag, Usage, Value,
    VariableTag, VariableValue, Variant, VariantKey, PROJECTION_IDENTITY, PROJECTION_REVISION,
};
pub use specification::{mf2_specification, MF2_SEMANTICS_IDENTITY, MF2_SEMANTICS_REVISION};
pub use workspace::AnalysisWorkspace;
