// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reading an `authoring-inventory` artifact and deciding whether to admit it.
//!
//! The order follows design 017: bounded strict decoding, then the exact
//! kind, schema and specification tuple, then the closed body, then the
//! integrity digest, and only then the checks that need a context. Each step
//! fails with its own reason, because "this is not JSON", "this is a kind this
//! reader does not implement", "this was altered after sealing" and "this does
//! not mean what it claims under this context" call for different responses.
//!
//! Admission does not trust what an artifact says about itself. A projection is
//! rebuilt from the retained MF2 source and compared; a locale, a class and a
//! usage are checked against the context the caller supplies. A precomputed
//! answer in the record is never a substitute for recomputing it.

use std::collections::BTreeSet;

use intlify_shared_json::decode::{self, DecodeFailure};
use intlify_shared_json::token::Token;
use serde_json::Value;

use super::artifact::{
    ArtifactKind, AuthoringArtifactReference, InventoryArtifact, ARTIFACT_SCHEMA_REVISION,
    AUTHORING_SPECIFICATION_IDENTITY, AUTHORING_SPECIFICATION_REVISION,
};
use super::model::{AuthoringInventory, InventoryFailure, UnitOutcome};
use crate::context::{AuthoringContext, CanonicalLocale, ContextKind, LocaleFailure};
use crate::declaration::{
    compare_parameters, DeclarationFacts, ParameterBinding, SourceLocaleBasis,
};
use crate::limits::{AuthoringLimits, LimitKind};
use crate::message::{analyze_message, MessageFailure, MessageInput};
use crate::primitives::{ByteRange, Occurrence, SnapshotMismatch};
use crate::specification::mf2_specification;
use crate::workspace::AnalysisWorkspace;

/// The exact bytes of one unit an inventory names, supplied by the caller.
#[derive(Debug, Clone, Copy)]
pub struct SourceBytes<'a> {
    /// The unit identity, as the inventory spells it.
    pub unit: &'a str,
    /// The unit's exact bytes.
    pub bytes: &'a [u8],
}

/// Why an artifact was not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionFailure {
    /// The bytes are not one bounded value in the shared JSON encoding.
    Unreadable(DecodeFailure),
    /// The kind, schema revision or specification is not one this reader
    /// implements. A registered kind this reader does not implement yet is
    /// reported here too, rather than as a malformed body.
    Unsupported,
    /// The value is not the closed shape of this artifact.
    Shape,
    /// The stored digest is not the digest of the stored content.
    Integrity,
    /// The body breaks a structural rule every inventory satisfies.
    Inventory(InventoryFailure),
    /// The inventory's context kind is not admitted through this entry.
    ContextNotAdmitted(ContextKind),
    /// The inventory's basis pins are not the supplied context's.
    BasisMismatch,
    /// The inventory's owner is not the supplied context's.
    OwnerMismatch,
    /// A retained MF2 source does not reproduce the projection stored with it.
    InconsistentProjection,
    /// A locale, class or usage is not what the supplied context admits.
    InconsistentContext,
    /// A reference in a checked unit does not satisfy a declaration it names.
    ParameterMismatch,
    /// Supplied bytes are not the ones a unit's snapshot names.
    Source(SnapshotMismatch),
    /// Bytes were supplied for a unit the inventory does not name, or twice.
    UnknownSourceUnit,
    /// A recorded range splits a Unicode scalar of the actual source text.
    SourceBoundary,
    /// The canonicalization provider could not answer.
    LocaleProviderUnavailable,
    /// A named bound was exhausted while checking.
    Limit(LimitKind),
    /// Re-analysis of a retained message failed operationally.
    Message(MessageFailure),
}

impl From<MessageFailure> for AdmissionFailure {
    fn from(failure: MessageFailure) -> Self {
        match failure {
            MessageFailure::Limit(kind) => Self::Limit(kind),
            other => Self::Message(other),
        }
    }
}

/// One inventory artifact that passed admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedInventory {
    artifact: InventoryArtifact,
    unverified: Box<[Token]>,
}

impl AdmittedInventory {
    /// Borrow the admitted artifact.
    #[must_use]
    pub const fn artifact(&self) -> &InventoryArtifact {
        &self.artifact
    }

    /// Borrow the admitted inventory.
    #[must_use]
    pub const fn inventory(&self) -> &AuthoringInventory {
        self.artifact.body()
    }

    /// Return the reference another artifact uses to name this one.
    #[must_use]
    pub fn reference(&self) -> AuthoringArtifactReference {
        self.artifact.reference()
    }

    /// Return the units whose bytes were not supplied, in unit order.
    ///
    /// Their snapshots and the ranges inside them were checked for shape but
    /// not against real bytes. That is a different, weaker result, and it is
    /// kept visible rather than folded into a single "admitted".
    #[must_use]
    pub fn unverified_units(&self) -> &[Token] {
        &self.unverified
    }
}

/// Admit an inventory artifact under a checked context.
///
/// This entry does not admit a test context: an ordinary build can never hold
/// one, and an inventory claiming one cannot become production evidence by
/// being read here. Production kinds are not admitted in this phase either,
/// because production admission needs the checked 015 inputs and the 017/018
/// work later phases own. The test-owned entry is
/// `test_context::admit_inventory`.
pub fn admit_inventory(
    bytes: &[u8],
    context: &dyn AuthoringContext,
    sources: &[SourceBytes<'_>],
    limits: &AuthoringLimits,
    workspace: &mut AnalysisWorkspace,
) -> Result<AdmittedInventory, AdmissionFailure> {
    admit(bytes, context, sources, limits, workspace, Entry::Ordinary)
}

/// Which entry an admission came through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Entry {
    Ordinary,
    #[cfg(feature = "test-context")]
    TestOwned,
}

pub(crate) fn admit(
    bytes: &[u8],
    context: &dyn AuthoringContext,
    sources: &[SourceBytes<'_>],
    limits: &AuthoringLimits,
    workspace: &mut AnalysisWorkspace,
    entry: Entry,
) -> Result<AdmittedInventory, AdmissionFailure> {
    let value = decode::value(bytes, true).map_err(AdmissionFailure::Unreadable)?;
    select(&value)?;
    let artifact: InventoryArtifact = decode::typed(value).map_err(|_| AdmissionFailure::Shape)?;
    if !artifact
        .verify_integrity()
        .map_err(|_| AdmissionFailure::Shape)?
    {
        return Err(AdmissionFailure::Integrity);
    }
    let inventory = artifact.body();
    inventory.validate().map_err(AdmissionFailure::Inventory)?;

    admit_context(inventory, context, entry)?;
    if inventory.declarations().len() as u64 > limits.declarations {
        return Err(AdmissionFailure::Limit(LimitKind::Declarations));
    }
    for facts in inventory.declarations() {
        check_declaration(facts, context, limits, workspace)?;
    }
    check_references(inventory)?;
    let unverified = check_sources(inventory, sources)?;
    Ok(AdmittedInventory {
        artifact,
        unverified: unverified.into_boxed_slice(),
    })
}

/// Select the exact tuple before decoding the body.
///
/// A different kind or revision has a different body shape, so decoding it as
/// an inventory would report a shape failure for what is really a version this
/// reader does not implement.
fn select(value: &Value) -> Result<(), AdmissionFailure> {
    let object = value.as_object().ok_or(AdmissionFailure::Shape)?;
    let text = |member: &str| {
        object
            .get(member)
            .and_then(Value::as_str)
            .ok_or(AdmissionFailure::Shape)
    };
    if ArtifactKind::from_wire(text("kind")?) != Some(ArtifactKind::AuthoringInventory) {
        return Err(AdmissionFailure::Unsupported);
    }
    if text("schemaRevision")? != ARTIFACT_SCHEMA_REVISION {
        return Err(AdmissionFailure::Unsupported);
    }
    let specification = object
        .get("authoringSpecification")
        .and_then(Value::as_object)
        .ok_or(AdmissionFailure::Shape)?;
    let part = |member: &str| {
        specification
            .get(member)
            .and_then(Value::as_str)
            .ok_or(AdmissionFailure::Shape)
    };
    if part("identity")? != AUTHORING_SPECIFICATION_IDENTITY
        || part("revision")? != AUTHORING_SPECIFICATION_REVISION
    {
        return Err(AdmissionFailure::Unsupported);
    }
    Ok(())
}

fn admit_context(
    inventory: &AuthoringInventory,
    context: &dyn AuthoringContext,
    entry: Entry,
) -> Result<(), AdmissionFailure> {
    let kind = inventory.basis().context_kind();
    match kind {
        // A test context is admitted only through the test-owned entry, which
        // exists only in a build that can construct one.
        ContextKind::TestContext if entry == Entry::Ordinary => {
            return Err(AdmissionFailure::ContextNotAdmitted(kind));
        }
        ContextKind::TestContext => {}
        ContextKind::ApplicationProfile | ContextKind::LibraryContext => {
            return Err(AdmissionFailure::ContextNotAdmitted(kind));
        }
    }
    if context.basis() != inventory.basis() {
        return Err(AdmissionFailure::BasisMismatch);
    }
    if context.owner() != inventory.owner() {
        return Err(AdmissionFailure::OwnerMismatch);
    }
    Ok(())
}

/// Rebuild one declaration's meaning and compare it with what is stored.
fn check_declaration(
    facts: &DeclarationFacts,
    context: &dyn AuthoringContext,
    limits: &AuthoringLimits,
    workspace: &mut AnalysisWorkspace,
) -> Result<(), AdmissionFailure> {
    let projection = facts.projection();
    // Displayed text is stored as the MF2 its encoding produced, and that MF2
    // parses to the same message, so every declaration is re-read as MF2.
    let analysis = analyze_message(
        MessageInput::Mf2(facts.mf2_source()),
        facts.occurrence(),
        limits,
        workspace,
    )?;
    let Some(message) = analysis.facts() else {
        return Err(AdmissionFailure::InconsistentProjection);
    };
    if projection.mf2_specification != mf2_specification()
        || message.message() != &projection.message
        || message.parameters() != &*projection.parameters
    {
        return Err(AdmissionFailure::InconsistentProjection);
    }

    let locale = projection.source_locale.as_str();
    match facts.source_locale_basis() {
        SourceLocaleBasis::ContextDefault => {
            if context.default_source_locale().map(CanonicalLocale::as_str) != Some(locale) {
                return Err(AdmissionFailure::InconsistentContext);
            }
        }
        // An explicit locale was canonicalized when it was resolved, so the
        // stored value has to be a fixed point of the same canonicalization.
        SourceLocaleBasis::Explicit => match context.canonicalize(locale) {
            Ok(canonical) if canonical.as_str() == locale => {}
            Err(LocaleFailure::Unavailable) => {
                return Err(AdmissionFailure::LocaleProviderUnavailable)
            }
            Ok(_) | Err(LocaleFailure::InvalidIdentifier | LocaleFailure::UnsupportedInput) => {
                return Err(AdmissionFailure::InconsistentContext)
            }
        },
    }
    if !context.surface_vocabulary().admits(facts.surface_class()) {
        return Err(AdmissionFailure::InconsistentContext);
    }
    if let Some(usage) = &projection.usage {
        if context.usage_profile() != Some(&usage.profile) {
            return Err(AdmissionFailure::InconsistentContext);
        }
        if usage.value.as_str().len() as u64 > limits.metadata_value_bytes {
            return Err(AdmissionFailure::Limit(LimitKind::MetadataValueBytes));
        }
    }
    if let Some(description) = &projection.description {
        if description.as_str().len() as u64 > limits.metadata_value_bytes {
            return Err(AdmissionFailure::Limit(LimitKind::MetadataValueBytes));
        }
    }
    Ok(())
}

/// Check that every reference in a checked unit satisfies what it names.
///
/// A checked unit claims every occurrence in it resolved. A reference whose
/// parameters do not match a declaration it may use did not resolve, so the
/// claim and the record disagree. Blocked units make no such claim.
fn check_references(inventory: &AuthoringInventory) -> Result<(), AdmissionFailure> {
    let checked: BTreeSet<&Token> = inventory
        .units()
        .iter()
        .filter(|unit| unit.outcome() == UnitOutcome::Checked)
        .map(|unit| unit.source().unit())
        .collect();
    for reference in inventory.references() {
        if !checked.contains(reference.occurrence().source().unit()) {
            continue;
        }
        for target in reference.declarations() {
            let declaration = find_declaration(inventory, target).ok_or(
                AdmissionFailure::Inventory(InventoryFailure::UnresolvedReference),
            )?;
            let matched = compare_parameters(
                &declaration.projection().parameters,
                reference.parameters(),
                reference.occurrence(),
                &mut |_| {},
            );
            if !matched {
                return Err(AdmissionFailure::ParameterMismatch);
            }
        }
    }
    Ok(())
}

fn find_declaration<'a>(
    inventory: &'a AuthoringInventory,
    target: &Occurrence,
) -> Option<&'a DeclarationFacts> {
    let declarations = inventory.declarations();
    declarations
        .binary_search_by(|facts| facts.occurrence().canonical_cmp(target))
        .ok()
        .map(|index| &declarations[index])
}

/// Check supplied bytes, and every range inside the units they belong to.
///
/// Returns the units no bytes were supplied for.
fn check_sources(
    inventory: &AuthoringInventory,
    sources: &[SourceBytes<'_>],
) -> Result<Vec<Token>, AdmissionFailure> {
    let units = inventory.units();
    let mut verified = BTreeSet::new();
    for supplied in sources {
        let index = units
            .binary_search_by(|unit| unit.source().unit().as_str().cmp(supplied.unit))
            .map_err(|_| AdmissionFailure::UnknownSourceUnit)?;
        if !verified.insert(index) {
            return Err(AdmissionFailure::UnknownSourceUnit);
        }
        let snapshot = units[index].source();
        let text = snapshot
            .verify(supplied.bytes)
            .map_err(AdmissionFailure::Source)?;
        for range in ranges_in(inventory, snapshot.unit()) {
            if !text.is_char_boundary(range.start() as usize)
                || !text.is_char_boundary(range.end() as usize)
            {
                return Err(AdmissionFailure::SourceBoundary);
            }
        }
    }
    Ok(units
        .iter()
        .enumerate()
        .filter(|(index, _)| !verified.contains(index))
        .map(|(_, unit)| unit.source().unit().clone())
        .collect())
}

/// Every range an inventory records inside one unit.
fn ranges_in<'a>(
    inventory: &'a AuthoringInventory,
    unit: &'a Token,
) -> impl Iterator<Item = ByteRange> + 'a {
    let inside = move |occurrence: &&Occurrence| occurrence.source().unit() == unit;
    let declarations = inventory.declarations().iter().flat_map(move |facts| {
        std::iter::once(facts.occurrence())
            .filter(inside)
            .map(Occurrence::range)
            .chain(
                facts
                    .extraction_map()
                    .iter()
                    .filter(move |_| inside(&facts.occurrence()))
                    .map(|segment| segment.source()),
            )
    });
    let references = inventory.references().iter().flat_map(move |reference| {
        std::iter::once(reference.occurrence())
            .chain(
                reference
                    .parameters()
                    .iter()
                    .map(ParameterBinding::expression),
            )
            .filter(inside)
            .map(Occurrence::range)
    });
    let exclusions = inventory
        .exclusions()
        .iter()
        .map(super::model::Exclusion::occurrence)
        .filter(inside)
        .map(Occurrence::range);
    declarations.chain(references).chain(exclusions)
}
