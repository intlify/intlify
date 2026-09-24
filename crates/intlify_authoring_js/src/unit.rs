// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Admitting the units one invocation reads.
//!
//! Admission runs before any unit is parsed, and every failure here is
//! operational. A unit that names another owner, a grammar outside the
//! registry, or bytes it was not given is a mistake in how the caller put the
//! invocation together, and no edit to source could fix it.
//!
//! The one exception is a unit whose bytes are exactly what its snapshot names
//! but are not UTF-8. The attachment is right and the unit is not text, which
//! is the author's to fix. Such a unit is admitted, and analysis reports it as
//! failed while the other units are still read.
//!
//! Membership is declared apart from the supplied units. A unit missing from a
//! complete scope is then caught, rather than silently absent from a result
//! that still calls itself complete.

use intlify_authoring::{
    AuthoringContext, Completeness, ContextKind, PrimitiveError, SnapshotMismatch, SourceSnapshot,
    Token,
};

use crate::failure::ProducerFailure;
use crate::grammar::Grammar;
use crate::limits::{JsAuthoringLimits, JsLimitKind};
use crate::profile::JsAuthoringProfile;

/// One unit as the caller supplies it: a snapshot and the bytes it names.
#[derive(Debug, Clone)]
pub struct SourceUnit<'b> {
    snapshot: SourceSnapshot,
    bytes: &'b [u8],
}

impl<'b> SourceUnit<'b> {
    /// Pair one snapshot with the bytes the caller says it names.
    ///
    /// Nothing is checked here. Admission checks the claim, so that no range
    /// is ever addressed in bytes nobody compared with the digest.
    #[must_use]
    pub const fn new(snapshot: SourceSnapshot, bytes: &'b [u8]) -> Self {
        Self { snapshot, bytes }
    }
}

/// One unit the declared scope contains, at the revision it contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitMember {
    unit: Token,
    revision: Token,
}

impl UnitMember {
    /// Validate and retain one member.
    pub fn new(unit: &str, revision: &str) -> Result<Self, PrimitiveError> {
        Ok(Self {
            unit: Token::new(unit)?,
            revision: Token::new(revision)?,
        })
    }

    /// Borrow the member's unit identity.
    #[must_use]
    pub const fn unit(&self) -> &Token {
        &self.unit
    }

    /// Borrow the revision the scope contains.
    #[must_use]
    pub const fn revision(&self) -> &Token {
        &self.revision
    }
}

/// One unit that passed admission.
///
/// Its bytes were compared with its snapshot, so a range inside its text
/// addresses exactly what the snapshot names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedUnit<'b> {
    snapshot: SourceSnapshot,
    grammar: Grammar,
    text: Option<&'b str>,
}

impl<'b> AdmittedUnit<'b> {
    /// Borrow the unit's snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &SourceSnapshot {
        &self.snapshot
    }

    /// Return the registered grammar the snapshot names.
    #[must_use]
    pub const fn grammar(&self) -> Grammar {
        self.grammar
    }

    /// Borrow the unit's text, or `None` when its bytes are not UTF-8.
    #[must_use]
    pub const fn text(&self) -> Option<&'b str> {
        self.text
    }
}

/// Admit the units one invocation will read.
///
/// `membership` declares every unit the scope contains. Each supplied unit has
/// to be a member at the revision it supplies, and a complete scope has to
/// supply every member. The admitted units are returned in unit order, which
/// is the order an inventory records them in, whatever order they arrived in.
///
/// Checks run from the cheapest to the most expensive, and units are checked
/// in unit order, so the same inputs report the same failure however they
/// were supplied. Hashing every unit's bytes comes last.
pub fn admit_units<'b>(
    context: &dyn AuthoringContext,
    profile: &JsAuthoringProfile,
    completeness: Completeness,
    membership: &[UnitMember],
    units: &[SourceUnit<'b>],
    limits: &JsAuthoringLimits,
) -> Result<Vec<AdmittedUnit<'b>>, ProducerFailure> {
    let kind = context.basis().context_kind();
    if kind != ContextKind::TestContext {
        return Err(ProducerFailure::ProductionContextUnsupported(kind));
    }
    if context.basis().authoring_profile() != profile.identity() {
        return Err(ProducerFailure::ProfileMismatch);
    }
    // Both lists are bounded before either is sorted, so no caller-sized work
    // happens ahead of the check.
    if membership.len() as u64 > limits.units || units.len() as u64 > limits.units {
        return Err(ProducerFailure::Limit(JsLimitKind::Units));
    }

    let mut members: Vec<&UnitMember> = membership.iter().collect();
    members.sort_unstable_by(|left, right| left.unit.cmp(&right.unit));
    if let Some(pair) = members.windows(2).find(|pair| pair[0].unit == pair[1].unit) {
        return Err(ProducerFailure::DuplicateMember {
            unit: pair[0].unit.clone(),
        });
    }

    let mut supplied: Vec<&SourceUnit<'b>> = units.iter().collect();
    supplied.sort_unstable_by(|left, right| left.snapshot.unit().cmp(right.snapshot.unit()));
    if let Some(pair) = supplied
        .windows(2)
        .find(|pair| pair[0].snapshot.unit() == pair[1].snapshot.unit())
    {
        return Err(ProducerFailure::DuplicateUnit {
            unit: pair[0].snapshot.unit().clone(),
        });
    }

    let mut grammars = Vec::with_capacity(supplied.len());
    let mut total = 0_u64;
    for unit in &supplied {
        let snapshot = &unit.snapshot;
        let name = || snapshot.unit().clone();
        if snapshot.owner() != context.owner() {
            return Err(ProducerFailure::ForeignOwner { unit: name() });
        }
        let Some(grammar) = Grammar::from_identity(snapshot.grammar()) else {
            return Err(ProducerFailure::UnregisteredGrammar { unit: name() });
        };
        let member = members.binary_search_by(|member| member.unit.cmp(snapshot.unit()));
        if !member.is_ok_and(|index| members[index].revision == *snapshot.revision()) {
            return Err(ProducerFailure::NotAMember { unit: name() });
        }
        let length = unit.bytes.len() as u64;
        if length > limits.unit_bytes {
            return Err(ProducerFailure::Limit(JsLimitKind::UnitBytes));
        }
        total = total.saturating_add(length);
        if total > limits.total_bytes {
            return Err(ProducerFailure::Limit(JsLimitKind::TotalBytes));
        }
        grammars.push(grammar);
    }

    if completeness == Completeness::Complete {
        // Every supplied unit is a distinct member by now, so a complete scope
        // is missing a member exactly when some member has no supplied unit.
        if let Some(missing) = members.iter().find(|member| {
            supplied
                .binary_search_by(|unit| unit.snapshot.unit().cmp(&member.unit))
                .is_err()
        }) {
            return Err(ProducerFailure::MissingMember {
                unit: missing.unit.clone(),
            });
        }
    }

    supplied
        .into_iter()
        .zip(grammars)
        .map(|(unit, grammar)| {
            let text = match unit.snapshot.verify(unit.bytes) {
                Ok(text) => Some(text),
                Err(SnapshotMismatch::Encoding) => None,
                Err(mismatch) => {
                    return Err(ProducerFailure::Snapshot {
                        unit: unit.snapshot.unit().clone(),
                        mismatch,
                    })
                }
            };
            Ok(AdmittedUnit {
                snapshot: unit.snapshot.clone(),
                grammar,
                text,
            })
        })
        .collect()
}
