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

#[cfg(test)]
mod tests {
    use intlify_authoring::test_context::TestContext;
    use intlify_authoring::{
        IntegrityDigest, OwnerIdentity, OwnerKind, SurfaceVocabulary, VersionedIdentity,
    };
    use intlify_shared_json::encoding::digest_bytes;

    use super::*;
    use crate::limits::tests::generous;
    use crate::test_support::{context, owner, snapshot, token};

    const A: &[u8] = b"const a = 1\n";
    const B: &[u8] = b"const b = 2\n";

    fn member(unit: &str) -> UnitMember {
        UnitMember::new(unit, "1").unwrap()
    }

    fn supplied<'b>(unit: &str, grammar: Grammar, bytes: &'b [u8]) -> SourceUnit<'b> {
        SourceUnit::new(snapshot(unit, grammar, bytes), bytes)
    }

    fn admit<'b>(
        completeness: Completeness,
        membership: &[UnitMember],
        units: &[SourceUnit<'b>],
        limits: &JsAuthoringLimits,
    ) -> Result<Vec<AdmittedUnit<'b>>, ProducerFailure> {
        admit_units(
            &context(),
            &JsAuthoringProfile::new(),
            completeness,
            membership,
            units,
            limits,
        )
    }

    fn complete<'b>(
        membership: &[UnitMember],
        units: &[SourceUnit<'b>],
    ) -> Result<Vec<AdmittedUnit<'b>>, ProducerFailure> {
        admit(Completeness::Complete, membership, units, &generous())
    }

    /// A snapshot of `bytes` under another owner or grammar pin.
    fn foreign(
        unit: &str,
        owner: OwnerIdentity,
        grammar: VersionedIdentity,
        bytes: &[u8],
    ) -> SourceSnapshot {
        SourceSnapshot::new(
            owner,
            unit,
            "1",
            grammar,
            bytes.len() as u64,
            IntegrityDigest::from_hash(digest_bytes(bytes)).as_str(),
        )
        .unwrap()
    }

    #[test]
    fn a_member_is_a_checked_unit_and_the_revision_the_scope_holds() {
        let member = UnitMember::new("checkout", "7").unwrap();
        assert_eq!(member.unit(), &token("checkout"));
        assert_eq!(member.revision(), &token("7"));
        assert!(UnitMember::new("", "1").is_err());
        assert!(UnitMember::new("checkout", "").is_err());
    }

    #[test]
    fn a_supplied_unit_is_only_a_claim_until_it_is_admitted() {
        // Pairing a snapshot with bytes it does not name is allowed; nothing
        // is addressed in them until admission has compared the two.
        let claimed = SourceUnit::new(snapshot("a", Grammar::JsModule, A), B);
        assert_eq!(
            complete(&[member("a")], &[claimed]),
            Err(ProducerFailure::Snapshot {
                unit: token("a"),
                mismatch: SnapshotMismatch::Utf8Digest
            })
        );
    }

    #[test]
    fn admitted_units_come_in_unit_order_with_their_grammar_and_text() {
        let admitted = complete(
            &[member("b"), member("a")],
            &[
                supplied("b", Grammar::TsScript, B),
                supplied("a", Grammar::JsModule, A),
            ],
        )
        .unwrap();
        let seen: Vec<_> = admitted
            .iter()
            .map(|unit| (unit.snapshot().unit().as_str(), unit.grammar(), unit.text()))
            .collect();
        assert_eq!(
            seen,
            [
                ("a", Grammar::JsModule, Some("const a = 1\n")),
                ("b", Grammar::TsScript, Some("const b = 2\n")),
            ]
        );
    }

    #[test]
    fn bytes_that_are_exactly_the_snapshots_but_not_text_are_admitted_without_text() {
        let bytes: &[u8] = &[0x66, 0xff, 0x0a];
        let admitted =
            complete(&[member("a")], &[supplied("a", Grammar::JsModule, bytes)]).unwrap();
        assert_eq!(admitted[0].text(), None);
        assert_eq!(admitted[0].snapshot().byte_length(), 3);
    }

    #[test]
    fn only_a_complete_scope_needs_every_member_supplied() {
        let members = [member("a"), member("b")];
        let units = [supplied("a", Grammar::JsModule, A)];
        let partial = admit(Completeness::Partial, &members, &units, &generous()).unwrap();
        assert_eq!(partial.len(), 1);
        assert_eq!(
            complete(&members, &units),
            Err(ProducerFailure::MissingMember { unit: token("b") })
        );
    }

    #[test]
    fn each_check_refuses_what_it_exists_for() {
        let other = VersionedIdentity::literal("other-authoring-profile", "0");
        let pinned_elsewhere =
            TestContext::builder(owner(), SurfaceVocabulary::new(["checkout"]).unwrap())
                .authoring_profile(other)
                .default_source_locale("en")
                .build()
                .unwrap();
        assert_eq!(
            admit_units(
                &pinned_elsewhere,
                &JsAuthoringProfile::new(),
                Completeness::Complete,
                &[member("a")],
                &[supplied("a", Grammar::JsModule, A)],
                &generous(),
            ),
            Err(ProducerFailure::ProfileMismatch)
        );
        assert_eq!(
            complete(
                &[member("a"), member("a")],
                &[supplied("a", Grammar::JsModule, A)]
            ),
            Err(ProducerFailure::DuplicateMember { unit: token("a") })
        );
        assert_eq!(
            complete(
                &[member("a")],
                &[
                    supplied("a", Grammar::JsModule, A),
                    supplied("a", Grammar::JsModule, A)
                ]
            ),
            Err(ProducerFailure::DuplicateUnit { unit: token("a") })
        );
        let stranger = OwnerIdentity::new(OwnerKind::Application, "back-office").unwrap();
        assert_eq!(
            complete(
                &[member("a")],
                &[SourceUnit::new(
                    foreign("a", stranger, Grammar::JsModule.identity(), A),
                    A
                )]
            ),
            Err(ProducerFailure::ForeignOwner { unit: token("a") })
        );
        let jsx = VersionedIdentity::literal("intlify-grammar-jsx-module", "0");
        assert_eq!(
            complete(
                &[member("a")],
                &[SourceUnit::new(foreign("a", owner(), jsx, A), A)]
            ),
            Err(ProducerFailure::UnregisteredGrammar { unit: token("a") })
        );
        assert_eq!(
            complete(
                &[UnitMember::new("a", "2").unwrap()],
                &[supplied("a", Grammar::JsModule, A)]
            ),
            Err(ProducerFailure::NotAMember { unit: token("a") })
        );
        let short = &A[..4];
        let wrong_length = SourceUnit::new(snapshot("a", Grammar::JsModule, A), short);
        assert_eq!(
            complete(&[member("a")], &[wrong_length]),
            Err(ProducerFailure::Snapshot {
                unit: token("a"),
                mismatch: SnapshotMismatch::ByteLength
            })
        );
    }

    #[test]
    fn each_bound_admits_exactly_its_value() {
        let members = [member("a"), member("b")];
        let units = [
            supplied("a", Grammar::JsModule, A),
            supplied("b", Grammar::JsModule, B),
        ];
        let mut limits = generous();
        limits.units = 2;
        limits.unit_bytes = A.len() as u64;
        limits.total_bytes = (A.len() + B.len()) as u64;
        assert!(admit(Completeness::Complete, &members, &units, &limits).is_ok());

        let mut fewer = limits;
        fewer.units = 1;
        assert_eq!(
            admit(Completeness::Complete, &members, &units, &fewer),
            Err(ProducerFailure::Limit(JsLimitKind::Units))
        );
        let mut smaller = limits;
        smaller.unit_bytes -= 1;
        assert_eq!(
            admit(Completeness::Complete, &members, &units, &smaller),
            Err(ProducerFailure::Limit(JsLimitKind::UnitBytes))
        );
        let mut less = limits;
        less.total_bytes -= 1;
        assert_eq!(
            admit(Completeness::Complete, &members, &units, &less),
            Err(ProducerFailure::Limit(JsLimitKind::TotalBytes))
        );
    }

    #[test]
    fn cheaper_checks_answer_first_and_units_are_checked_in_unit_order() {
        // The count is bounded before duplicates are looked for.
        let mut one = generous();
        one.units = 1;
        assert_eq!(
            admit(
                Completeness::Complete,
                &[member("a"), member("a")],
                &[supplied("a", Grammar::JsModule, A)],
                &one
            ),
            Err(ProducerFailure::Limit(JsLimitKind::Units))
        );
        // `a` names a grammar outside the registry and `b` another owner.
        // Whichever order they come in, `a` is checked first.
        let stranger = OwnerIdentity::new(OwnerKind::Application, "back-office").unwrap();
        let jsx = VersionedIdentity::literal("intlify-grammar-jsx-module", "0");
        let a = SourceUnit::new(foreign("a", owner(), jsx, A), A);
        let b = SourceUnit::new(foreign("b", stranger, Grammar::JsModule.identity(), B), B);
        for units in [[a.clone(), b.clone()], [b, a]] {
            assert_eq!(
                complete(&[member("a"), member("b")], &units),
                Err(ProducerFailure::UnregisteredGrammar { unit: token("a") })
            );
        }
        // Bytes are hashed last: `a`'s wrong bytes lose to `b`'s missing
        // membership, found without hashing anything.
        let wrong_bytes = SourceUnit::new(snapshot("a", Grammar::JsModule, A), B);
        assert_eq!(
            complete(
                &[member("a")],
                &[wrong_bytes, supplied("b", Grammar::JsModule, B)]
            ),
            Err(ProducerFailure::NotAMember { unit: token("b") })
        );
    }
}
