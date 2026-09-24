// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's `AuthoringInventory` and the structural rules it carries.
//!
//! An inventory is the declared finite scope of one analysis: which units it
//! covered, what happened to each, and the declarations, references and
//! exclusions found in them. It is a record of what was attempted, not a
//! certificate that the attempt succeeded. A `complete` inventory may therefore
//! hold a unit that failed, because 016 describes exactly that state: "a
//! complete inventory with missing or unsuccessfully analyzed source units
//! cannot produce a complete checked result". Whether an inventory is usable as
//! complete checked input is a separate question with its own answer,
//! [`AuthoringInventory::is_complete_checked`].
//!
//! The rules checked here are the ones that hold of any well-formed record,
//! whatever its context: canonical order, no duplicates, roles that match the
//! array they sit in, every position inside a unit this inventory names, and
//! references that resolve inside the inventory. Whether a declaration's
//! projection is actually what its MF2 source means under its context is
//! admission's job, because that needs the context.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::AuthoringBasis;
use crate::declaration::{DeclarationFacts, ParameterBinding};
use crate::primitives::{
    NonemptyText, Occurrence, OccurrenceRole, OwnerIdentity, PrimitiveError, SourceSnapshot,
};
use intlify_shared_json::token::Token;

/// Whether an inventory covers the whole of its declared scope.
///
/// This describes what the caller declared it was analyzing. It is checked
/// against the caller's expected membership, and it does not change when a
/// unit inside that scope fails.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Completeness {
    /// The inventory covers the whole declared scope.
    Complete,
    /// The inventory is a smaller view, such as an editor request.
    Partial,
}

/// What happened to one source unit.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum UnitOutcome {
    /// Every occurrence in the unit was classified and resolved.
    Checked,
    /// The unit was read, and at least one of its occurrences was blocked.
    Blocked,
    /// The unit could not be read at all, so it establishes no facts.
    Failed,
}

/// One source unit in an inventory and what happened to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitResult {
    source: SourceSnapshot,
    outcome: UnitOutcome,
}

impl UnitResult {
    /// Retain one unit and its outcome.
    #[must_use]
    pub const fn new(source: SourceSnapshot, outcome: UnitOutcome) -> Self {
        Self { source, outcome }
    }

    /// Borrow the unit's exact snapshot.
    #[must_use]
    pub const fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    /// Return what happened to the unit.
    #[must_use]
    pub const fn outcome(&self) -> UnitOutcome {
        self.outcome
    }
}

/// One use site and the declarations it may use.
///
/// The declarations are a finite set: the first reference form names one or
/// more declarations in the same inventory, and a later extension may name
/// several. The parameters keep the host's evaluation order, because that is
/// the order a later lowering has to preserve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReferenceFacts {
    occurrence: Occurrence,
    declarations: Box<[Occurrence]>,
    parameters: Box<[ParameterBinding]>,
}

impl ReferenceFacts {
    /// Retain one reference.
    ///
    /// The declarations are put into canonical order when the inventory is
    /// built; their order here carries no meaning.
    #[must_use]
    pub fn new(
        occurrence: Occurrence,
        declarations: Vec<Occurrence>,
        parameters: Vec<ParameterBinding>,
    ) -> Self {
        Self {
            occurrence,
            declarations: declarations.into_boxed_slice(),
            parameters: parameters.into_boxed_slice(),
        }
    }

    /// Borrow the use site.
    #[must_use]
    pub const fn occurrence(&self) -> &Occurrence {
        &self.occurrence
    }

    /// Borrow the declarations this use site may use, in canonical order.
    #[must_use]
    pub fn declarations(&self) -> &[Occurrence] {
        &self.declarations
    }

    /// Borrow the supplied parameters, in host evaluation order.
    #[must_use]
    pub fn parameters(&self) -> &[ParameterBinding] {
        &self.parameters
    }
}

/// One value the author explicitly excluded from localization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exclusion {
    occurrence: Occurrence,
    reason: NonemptyText,
}

impl Exclusion {
    /// Retain one exclusion and the author's stated reason for it.
    pub fn new(occurrence: Occurrence, reason: &str) -> Result<Self, PrimitiveError> {
        Ok(Self {
            occurrence,
            reason: NonemptyText::from_validated(reason)?,
        })
    }

    /// Borrow the excluded occurrence.
    #[must_use]
    pub const fn occurrence(&self) -> &Occurrence {
        &self.occurrence
    }

    /// Borrow the author's reason.
    #[must_use]
    pub fn reason(&self) -> &str {
        self.reason.as_str()
    }
}

/// The declared scope of one analysis and what it found.
///
/// Deserializing one reads its shape only. [`AuthoringInventory::validate`]
/// checks the structural rules, and admission checks the rest against a
/// context; neither is implied by having decoded the bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthoringInventory {
    owner: OwnerIdentity,
    scope: Token,
    basis: AuthoringBasis,
    completeness: Completeness,
    units: Box<[UnitResult]>,
    declarations: Box<[DeclarationFacts]>,
    references: Box<[ReferenceFacts]>,
    exclusions: Box<[Exclusion]>,
}

/// Why an inventory is not well formed.
///
/// Each variant names one rule, so a caller can tell a producer defect from a
/// forged or damaged record by which rule failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryFailure {
    /// Two units share one unit identity, which is at most one revision each.
    DuplicateUnit,
    /// Units are not in unit-identity order.
    UnitsUnordered,
    /// A unit or occurrence belongs to an owner other than the inventory's.
    ForeignOwner,
    /// An occurrence names a snapshot that is not exactly one of the units.
    UnknownSource,
    /// An occurrence's range runs past the end of its unit.
    RangeOutsideSource,
    /// An occurrence's role does not match the array it sits in.
    RoleMismatch,
    /// One array repeats an occurrence.
    DuplicateOccurrence,
    /// One array is not in canonical occurrence order.
    OccurrencesUnordered,
    /// A reference names no declaration at all.
    EmptyReference,
    /// A reference names a declaration this inventory does not contain.
    UnresolvedReference,
    /// A reference supplies one parameter name more than once.
    DuplicateParameter,
    /// A unit that could not be read claims facts.
    FactsFromFailedUnit,
    /// A declaration's extraction map does not describe its MF2 source.
    ExtractionMap,
}

impl AuthoringInventory {
    /// Borrow the owning application or library.
    #[must_use]
    pub const fn owner(&self) -> &OwnerIdentity {
        &self.owner
    }

    /// Borrow the name of the declared scope.
    #[must_use]
    pub const fn scope(&self) -> &Token {
        &self.scope
    }

    /// Borrow the exact input pins the analysis resolved against.
    #[must_use]
    pub const fn basis(&self) -> &AuthoringBasis {
        &self.basis
    }

    /// Return whether the inventory covers its whole declared scope.
    #[must_use]
    pub const fn completeness(&self) -> Completeness {
        self.completeness
    }

    /// Borrow the units, in unit-identity order.
    #[must_use]
    pub fn units(&self) -> &[UnitResult] {
        &self.units
    }

    /// Borrow the declarations, in canonical occurrence order.
    #[must_use]
    pub fn declarations(&self) -> &[DeclarationFacts] {
        &self.declarations
    }

    /// Borrow the references, in canonical occurrence order.
    #[must_use]
    pub fn references(&self) -> &[ReferenceFacts] {
        &self.references
    }

    /// Borrow the exclusions, in canonical occurrence order.
    #[must_use]
    pub fn exclusions(&self) -> &[Exclusion] {
        &self.exclusions
    }

    /// Return whether this inventory is complete checked authoring input.
    ///
    /// That needs both halves: the scope declared complete, and every unit in
    /// it checked. A partial inventory of checked units is checked facts for a
    /// smaller scope, and it never establishes that a declaration is absent
    /// from the project. A complete inventory with a failed unit is a faithful
    /// record of an attempt that did not succeed.
    #[must_use]
    pub fn is_complete_checked(&self) -> bool {
        self.completeness == Completeness::Complete
            && self
                .units
                .iter()
                .all(|unit| unit.outcome == UnitOutcome::Checked)
    }

    /// Check every structural rule a well-formed inventory satisfies.
    pub fn validate(&self) -> Result<(), InventoryFailure> {
        self.validate_units()?;
        let lookup = UnitLookup::new(&self.units);
        let failed: BTreeSet<&Token> = self
            .units
            .iter()
            .filter(|unit| unit.outcome == UnitOutcome::Failed)
            .map(|unit| unit.source.unit())
            .collect();
        // Facts address positions in a unit, so a unit that could not be read
        // cannot have produced any: claiming one would assert a declaration's
        // presence in source nobody parsed.
        let admit = |occurrence: &Occurrence, role: RoleCheck| {
            lookup.admit(occurrence, &self.owner)?;
            if !role.accepts(occurrence.role()) {
                return Err(InventoryFailure::RoleMismatch);
            }
            if failed.contains(occurrence.source().unit()) {
                return Err(InventoryFailure::FactsFromFailedUnit);
            }
            Ok(())
        };

        for facts in &*self.declarations {
            admit(facts.occurrence(), RoleCheck::Declaration)?;
            validate_extraction(facts)?;
        }
        canonical(self.declarations.iter().map(DeclarationFacts::occurrence))?;

        let declared: Vec<&Occurrence> = self
            .declarations
            .iter()
            .map(DeclarationFacts::occurrence)
            .collect();
        for reference in &*self.references {
            admit(&reference.occurrence, RoleCheck::Reference)?;
            if reference.declarations.is_empty() {
                return Err(InventoryFailure::EmptyReference);
            }
            canonical(reference.declarations.iter())?;
            for target in &*reference.declarations {
                // The canonical order leaves out a snapshot's declared length,
                // so finding a neighbour at the same position is not enough. A
                // target is never checked against its unit on its own, and a
                // declaration always is, so requiring the two to be equal is
                // what carries that check over to the target.
                match declared.binary_search_by(|candidate| candidate.canonical_cmp(target)) {
                    Ok(index) if declared[index] == target => {}
                    _ => return Err(InventoryFailure::UnresolvedReference),
                }
            }
            let mut names = BTreeSet::new();
            for binding in &*reference.parameters {
                admit(binding.expression(), RoleCheck::ParameterExpression)?;
                if !names.insert(binding.name()) {
                    return Err(InventoryFailure::DuplicateParameter);
                }
            }
        }
        canonical(self.references.iter().map(ReferenceFacts::occurrence))?;

        for exclusion in &*self.exclusions {
            admit(&exclusion.occurrence, RoleCheck::Exclusion)?;
        }
        canonical(self.exclusions.iter().map(Exclusion::occurrence))?;
        Ok(())
    }

    fn validate_units(&self) -> Result<(), InventoryFailure> {
        for unit in &*self.units {
            if unit.source.owner() != &self.owner {
                return Err(InventoryFailure::ForeignOwner);
            }
        }
        for pair in self.units.windows(2) {
            match pair[0].source.unit().cmp(pair[1].source.unit()) {
                Ordering::Less => {}
                Ordering::Equal => return Err(InventoryFailure::DuplicateUnit),
                Ordering::Greater => return Err(InventoryFailure::UnitsUnordered),
            }
        }
        Ok(())
    }
}

/// Which roles one array admits.
#[derive(Clone, Copy)]
enum RoleCheck {
    Declaration,
    Reference,
    ParameterExpression,
    Exclusion,
}

impl RoleCheck {
    const fn accepts(self, role: OccurrenceRole) -> bool {
        match self {
            Self::Declaration => role.is_declaration(),
            Self::Reference => matches!(role, OccurrenceRole::Reference),
            Self::ParameterExpression => matches!(role, OccurrenceRole::ParameterExpression),
            Self::Exclusion => matches!(role, OccurrenceRole::Exclusion),
        }
    }
}

/// Resolve an occurrence to the unit it claims, by unit identity.
struct UnitLookup<'a> {
    units: &'a [UnitResult],
}

impl<'a> UnitLookup<'a> {
    const fn new(units: &'a [UnitResult]) -> Self {
        Self { units }
    }

    fn admit(
        &self,
        occurrence: &Occurrence,
        owner: &OwnerIdentity,
    ) -> Result<(), InventoryFailure> {
        let source = occurrence.source();
        if source.owner() != owner {
            return Err(InventoryFailure::ForeignOwner);
        }
        // Units are validated as ordered and duplicate-free before this runs,
        // so the unit identity finds at most one candidate.
        let unit = self
            .units
            .binary_search_by(|candidate| candidate.source.unit().cmp(source.unit()))
            .map(|index| &self.units[index])
            .map_err(|_| InventoryFailure::UnknownSource)?;
        // The whole snapshot has to match, not only the identity: the same unit
        // at a different revision, digest, length or grammar is a different
        // snapshot, and equal identities with inconsistent bytes are a
        // conflict rather than a neighbour.
        if &unit.source != source {
            return Err(InventoryFailure::UnknownSource);
        }
        // Decoding does not run `Occurrence::new`, so a decoded range has not
        // been checked against its unit yet.
        if occurrence.range().end() > source.byte_length() {
            return Err(InventoryFailure::RangeOutsideSource);
        }
        Ok(())
    }
}

/// Check that occurrences are strictly increasing in canonical order.
fn canonical<'a>(
    occurrences: impl Iterator<Item = &'a Occurrence>,
) -> Result<(), InventoryFailure> {
    let mut previous: Option<&Occurrence> = None;
    for occurrence in occurrences {
        if let Some(previous) = previous {
            match previous.canonical_cmp(occurrence) {
                Ordering::Less => {}
                Ordering::Equal => return Err(InventoryFailure::DuplicateOccurrence),
                Ordering::Greater => return Err(InventoryFailure::OccurrencesUnordered),
            }
        }
        previous = Some(occurrence);
    }
    Ok(())
}

/// Check that a declaration's extraction map describes its MF2 source.
///
/// The emitted side has to cover every byte of the MF2 source, in order and
/// without gaps, on scalar boundaries. The source side has to stay inside the
/// declaration's own occurrence, which is where the text it describes came
/// from.
fn validate_extraction(facts: &DeclarationFacts) -> Result<(), InventoryFailure> {
    let source = facts.mf2_source();
    let range = facts.occurrence().range();
    let mut covered = 0_u64;
    for segment in facts.extraction_map() {
        let extracted = segment.extracted();
        if extracted.start() != covered
            || extracted.end() > source.len() as u64
            || !source.is_char_boundary(extracted.start() as usize)
            || !source.is_char_boundary(extracted.end() as usize)
        {
            return Err(InventoryFailure::ExtractionMap);
        }
        covered = extracted.end();
        let origin = segment.source();
        if origin.start() < range.start() || origin.end() > range.end() {
            return Err(InventoryFailure::ExtractionMap);
        }
    }
    if covered != source.len() as u64 {
        return Err(InventoryFailure::ExtractionMap);
    }
    Ok(())
}

/// Assemble an inventory from facts in any order.
///
/// The builder puts every array into canonical order and then validates the
/// result, so what it returns is the canonical form of what it was given. It
/// never removes a duplicate: a repeated occurrence is an error in whatever
/// produced it, and dropping one would hide that.
#[derive(Debug, Clone)]
pub struct InventoryBuilder {
    owner: OwnerIdentity,
    scope: Token,
    basis: AuthoringBasis,
    completeness: Completeness,
    units: Vec<UnitResult>,
    declarations: Vec<DeclarationFacts>,
    references: Vec<ReferenceFacts>,
    exclusions: Vec<Exclusion>,
}

impl InventoryBuilder {
    /// Start an inventory for one owner, scope and basis.
    pub fn new(
        owner: OwnerIdentity,
        scope: &str,
        basis: AuthoringBasis,
        completeness: Completeness,
    ) -> Result<Self, PrimitiveError> {
        Ok(Self {
            owner,
            scope: Token::new(scope)?,
            basis,
            completeness,
            units: Vec::new(),
            declarations: Vec::new(),
            references: Vec::new(),
            exclusions: Vec::new(),
        })
    }

    /// Record one unit and what happened to it.
    pub fn unit(&mut self, unit: UnitResult) -> &mut Self {
        self.units.push(unit);
        self
    }

    /// Record declarations.
    pub fn declarations(&mut self, facts: impl IntoIterator<Item = DeclarationFacts>) -> &mut Self {
        self.declarations.extend(facts);
        self
    }

    /// Record one reference.
    pub fn reference(&mut self, reference: ReferenceFacts) -> &mut Self {
        self.references.push(reference);
        self
    }

    /// Record one exclusion.
    pub fn exclusion(&mut self, exclusion: Exclusion) -> &mut Self {
        self.exclusions.push(exclusion);
        self
    }

    /// Put everything into canonical order and validate the result.
    pub fn finish(self) -> Result<AuthoringInventory, InventoryFailure> {
        let Self {
            owner,
            scope,
            basis,
            completeness,
            mut units,
            mut declarations,
            mut references,
            mut exclusions,
        } = self;
        units.sort_by(|left, right| left.source.unit().cmp(right.source.unit()));
        declarations.sort_by(|left, right| left.occurrence().canonical_cmp(right.occurrence()));
        for reference in &mut references {
            reference.declarations.sort_by(Occurrence::canonical_cmp);
        }
        references.sort_by(|left, right| left.occurrence.canonical_cmp(&right.occurrence));
        exclusions.sort_by(|left, right| left.occurrence.canonical_cmp(&right.occurrence));
        let inventory = AuthoringInventory {
            owner,
            scope,
            basis,
            completeness,
            units: units.into_boxed_slice(),
            declarations: declarations.into_boxed_slice(),
            references: references.into_boxed_slice(),
            exclusions: exclusions.into_boxed_slice(),
        };
        inventory.validate()?;
        Ok(inventory)
    }
}
