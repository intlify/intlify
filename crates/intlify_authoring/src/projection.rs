// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The semantic projection and the Intent revision computed from it.
//!
//! The projection is what a revision means. Everything that changes the
//! localization semantics of a message belongs here, and everything that does
//! not is deliberately absent, so ordinary refactoring cannot invalidate
//! translations while a genuine wording change always does.

pub(crate) mod build;
mod model;
mod revision;

pub use model::{
    Attribute, CatchAllKey, CatchAllTag, Declaration, Expression, ExpressionTag, Function,
    InputDeclaration, InputTag, IntentProjection, LiteralKey, LiteralTag, LiteralValue,
    LocalDeclaration, LocalTag, MarkupForm, MarkupPart, MarkupTag, MatchBody, MatchTag,
    MessageBody, MessageProjection, Opt, PatternBody, PatternPart, PatternTag, TextPart, TextTag,
    Usage, Value, VariableTag, VariableValue, Variant, VariantKey,
};
pub use revision::{
    intent_revision, projection_specification, revision_preimage, RevisionFailure,
    PROJECTION_IDENTITY, PROJECTION_REVISION,
};

use crate::primitives::{NonemptyText, PrimitiveError};

/// Assemble the complete projection from message facts and resolved context.
///
/// The message structure and external parameters come from analysis; the
/// source locale, semantic usage, and description come from context resolution.
/// Keeping them separate means a context-only change still changes the
/// revision, which is what 016 requires, without the analyzer needing to know
/// how context was resolved.
pub fn intent_projection(
    message: MessageProjection,
    parameters: Box<[String]>,
    source_locale: &str,
    usage: Option<Usage>,
    description: Option<&str>,
) -> Result<IntentProjection, PrimitiveError> {
    Ok(IntentProjection {
        mf2_specification: crate::specification::mf2_specification(),
        message,
        source_locale: NonemptyText::from_validated(source_locale)?,
        parameters,
        usage,
        description: description.map(NonemptyText::from_validated).transpose()?,
    })
}
