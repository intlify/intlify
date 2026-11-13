// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's source-evidence index: one record for each distinct Evidence
//! that a retained record refers to, so that every retained Entry Source,
//! Materialized Value, selector-origin, specification-input, artifact-input,
//! verification-input, and implementation-capability evidence reference
//! resolves (D15:1182). An Evidence view is its identity: the index holds one
//! record per distinct view, a record refers to Evidence by including the
//! same view, and resolving a reference is equality (D15:2505).
//!
//! Each index record has one Evidence record owner: the earliest unit in
//! canonical evaluation order that retains a reference to it, whatever order
//! the references arrive in (D15:1972, 2205). A Finding Occurrence refers to
//! its primary and related Evidence under the unit of its Finding; Coverage
//! Decision Evidence refers to its decision sources under the decision
//! table's unit (D15:1609). The index orders its records as Evidence orders
//! (D15:840-842, 2143), and accounting charges each record's owner the
//! record's accounting view, whose record is the Evidence view (D15:2518).

use std::collections::BTreeMap;

use serde_json::Value;

use crate::construction::registry::check::RecordKind;

use super::evaluation::Unit;
use super::evidence::Evidence;
use super::record_view::{RecordViews, ViewInvariant};

/// The source-evidence index: each distinct Evidence a retained record
/// refers to, with its Evidence record owner.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EvidenceIndex {
    records: BTreeMap<Evidence, Unit>,
}

impl EvidenceIndex {
    /// The index of the Evidence `references` refer to, each record owned by
    /// the earliest unit that refers to it.
    pub(crate) fn new<'a>(references: impl IntoIterator<Item = (&'a Unit, &'a Evidence)>) -> Self {
        let mut records: BTreeMap<Evidence, Unit> = BTreeMap::new();
        for (unit, evidence) in references {
            match records.get_mut(evidence) {
                Some(owner) => {
                    if unit < owner {
                        owner.clone_from(unit);
                    }
                }
                None => {
                    records.insert(evidence.clone(), unit.clone());
                }
            }
        }
        Self { records }
    }

    /// The records in Evidence order, each with its owner.
    pub(crate) fn records(&self) -> impl Iterator<Item = (&Evidence, &Unit)> {
        self.records.iter()
    }

    /// Resolves a reference to `evidence`: the owner of the equal record, or
    /// `None` when the index holds none.
    pub(crate) fn owner(&self, evidence: &Evidence) -> Option<&Unit> {
        self.records.get(evidence)
    }

    /// The accounting view of every record, in Evidence order (D15:2518).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an owner `views` cannot present.
    pub(crate) fn accounting(&self, views: &RecordViews<'_>) -> Result<Vec<Value>, ViewInvariant> {
        self.records
            .iter()
            .map(|(evidence, owner)| {
                views.accounting(owner, RecordKind::EvidenceIndex, evidence.view())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::resolver::evaluation::{Check, Evaluation, Phase, Subject, Unit};
    use crate::resolver::evidence::{Evidence, EvidenceProjection};
    use crate::resolver::evidence_component::ComponentPath;
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::origin::SafeOriginKind as O;
    use crate::resolver::record_view::{RecordViews, ViewInvariant};
    use crate::resolver::specification::Slot;
    use crate::resolver::token::{Sha256, Specification};

    use super::EvidenceIndex;

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// A unit of the vocabulary, which specification admission evaluates
    /// before the coverage table.
    fn vocabulary(check: Check) -> Unit {
        Unit {
            phase: Phase::SpecificationAdmission,
            subject: Subject::Vocabulary,
            check,
        }
    }

    /// The Evidence of a resolver input that no component of redacts.
    fn retained(path: ComponentPath) -> Evidence {
        let projection = EvidenceProjection::resolver_input(path);
        let tokens = EvidenceTokens::assign(PROFILE, projection.redacted(), &Sha256);
        projection.retained(PROFILE, &tokens).unwrap()
    }

    fn root() -> Evidence {
        retained(ComponentPath::root(O::IntentSurfaceClassVocabulary).unwrap())
    }

    fn slot() -> Evidence {
        retained(ComponentPath::specification(
            Slot::IntentSurfaceClassVocabulary,
        ))
    }

    fn data() -> Evidence {
        retained(ComponentPath::root(O::LocaleCanonicalizationData).unwrap())
    }

    /// The index holds one record for each distinct Evidence, however often
    /// it is referred to, in Evidence order: here by Safe Origin Kind rank.
    #[test]
    fn the_index_holds_each_distinct_evidence_once_in_evidence_order() {
        let intact = vocabulary(Check::IntactArtifact);
        let (root, slot, data) = (root(), slot(), data());
        let index = EvidenceIndex::new([
            (&intact, &data),
            (&intact, &root),
            (&intact, &slot),
            (&intact, &root),
        ]);
        assert_eq!(
            index
                .records()
                .map(|(evidence, _)| evidence.view()["origin"].clone())
                .collect::<Vec<_>>(),
            [
                json!("profile-resolution-specification-input"),
                json!("intent-surface-class-vocabulary"),
                json!("locale-canonicalization-data"),
            ]
        );
        assert_eq!(EvidenceIndex::new([]).records().count(), 0);
    }

    /// A record several units refer to is owned by the earliest of them,
    /// whatever order their references arrive in (D15:1972, 3300).
    #[test]
    fn the_earliest_unit_that_refers_to_a_record_owns_it_whatever_the_order() {
        let intact = vocabulary(Check::IntactArtifact);
        let asserted = vocabulary(Check::AssertedArtifact);
        assert!(intact < asserted);
        let (root, slot) = (root(), slot());
        for references in [
            [(&intact, &root), (&asserted, &root), (&asserted, &slot)],
            [(&asserted, &slot), (&asserted, &root), (&intact, &root)],
        ] {
            let index = EvidenceIndex::new(references);
            assert_eq!(index.owner(&root), Some(&intact));
            assert_eq!(index.owner(&slot), Some(&asserted));
        }
    }

    /// A reference resolves by equality: Evidence equal to a record resolves
    /// to it, and Evidence that equals none, such as another input's root,
    /// does not.
    #[test]
    fn a_reference_resolves_by_equality() {
        let intact = vocabulary(Check::IntactArtifact);
        let root = root();
        let index = EvidenceIndex::new([(&intact, &root)]);
        assert_eq!(index.owner(&self::root()), Some(&intact));
        assert_eq!(index.owner(&data()), None);
    }

    /// Each record's accounting view names its owner and keys the Evidence
    /// view by `evidence-index`, in Evidence order.
    #[test]
    fn each_records_accounting_view_names_its_owner() {
        let intact = vocabulary(Check::IntactArtifact);
        let asserted = vocabulary(Check::AssertedArtifact);
        let mut evaluation = Evaluation::default();
        for unit in [&intact, &asserted] {
            evaluation
                .check(unit.clone(), Ok(()), Ok::<(), _>)
                .unwrap()
                .unwrap();
        }
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        let (root, slot) = (root(), slot());
        let index = EvidenceIndex::new([(&asserted, &slot), (&intact, &root)]);
        assert_eq!(
            index.accounting(&views).unwrap(),
            [
                json!({
                    "owner": views.unit(&asserted).unwrap(),
                    "record": { "evidence-index": slot.view() },
                }),
                json!({
                    "owner": views.unit(&intact).unwrap(),
                    "record": { "evidence-index": root.view() },
                }),
            ]
        );
        let unrecorded = Unit {
            phase: Phase::SpecificationAdmission,
            subject: Subject::Slot(Slot::IntentSurfaceClassVocabulary),
            check: Check::SupportedAssertion,
        };
        assert_eq!(
            EvidenceIndex::new([(&unrecorded, &root)]).accounting(&views),
            Err(ViewInvariant::SubjectWithoutToken(unrecorded))
        );
    }
}
