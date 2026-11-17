// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Coverage Decision Evidence: for each cell of an established
//! coverage decision table, the decision source its Coverage Decision Basis
//! admitted, the project default or one matched rule domain, and the Evidence
//! of where that source sits (D15:1609, 1611).
//!
//! Coverage makes one candidate for each mapping once the table is
//! established, with its Evidence as projected before the invocation's
//! evidence tokens are assigned, and the evaluation keeps the candidates under
//! the table's unit. The caller assigns those tokens once, to these
//! candidates' redacted components together with those of the Finding
//! Occurrence candidates, so that one component keeps one token in every
//! record (D15:2194). Each candidate then becomes one record, `{ cell: {
//! locale, surfaceClass }, source: "default" | RuleDomain, evidence }`
//! (D15:2485, 2487), in the candidates' order: cell order, and then, for a
//! `rule` Basis, its normalized matched-domain order (D15:1611).
//!
//! Every record's Evidence record owner is the decision table's unit
//! (D15:1609). Under that unit, the record refers to its Evidence, a record of
//! the evidence index (D15:1972), and accounting charges the unit the record's
//! accounting view (D15:2518). One rule deciding several cells gives each cell
//! its own record, all of them referring to one index record (D15:1611). A
//! record's Evidence is Materialized Value Evidence of the declaration, or the
//! Coverage Specification input when the safe default is omitted (D15:1609).

use intlify_shared_json::token::Token;
use serde_json::{json, Value};

use crate::construction::registry::check::RecordKind;
use crate::locale::CanonicalLocale;

use super::capability::EvidenceMetadataLimits;
use super::coverage::{unit, CoverageSubject, Domain};
use super::evaluation::{Check, Unit};
use super::evidence::{Evidence, EvidenceProjection};
use super::evidence_metadata::MetadataBound;
use super::evidence_token::{EvidenceTokens, Redacted};
use super::record_view::{RecordViews, ViewInvariant};
use super::token::{rule_domain, Specification};

/// One Coverage Decision Evidence mapping, before the invocation's evidence
/// tokens are assigned: a cell, the decision source its Coverage Decision
/// Basis admitted, the project default or a matched rule domain, and the
/// Evidence of that source (D15:1609, 2485).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecisionEvidence {
    pub(crate) cell: (CanonicalLocale, Token),
    /// The matched rule domain, or none for the project default.
    pub(crate) source: Option<Domain>,
    pub(crate) evidence: EvidenceProjection,
}

impl DecisionEvidence {
    /// The candidate with its Evidence as path admission leaves it under
    /// `limits` (D15:1851).
    ///
    /// # Errors
    ///
    /// Returns the first path bound, in the table's order, that its value
    /// locator or component path exceeds.
    pub(crate) fn admitted(&self, limits: &EvidenceMetadataLimits) -> Result<Self, MetadataBound> {
        Ok(Self {
            cell: self.cell.clone(),
            source: self.source.clone(),
            evidence: self.evidence.admitted(limits)?,
        })
    }
}

/// A resolver bug the records refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DecisionInvariant {
    /// The evidence tokens leave out a component of the cell's mapping.
    UnassignedToken((CanonicalLocale, Token)),
}

/// One Coverage Decision Evidence record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CoverageDecisionEvidence {
    owner: Unit,
    cell: (CanonicalLocale, Token),
    source: Option<Domain>,
    evidence: Evidence,
}

impl CoverageDecisionEvidence {
    /// The cell whose Basis the record maps.
    pub(crate) const fn cell(&self) -> &(CanonicalLocale, Token) {
        &self.cell
    }

    /// The matched rule domain, or none for the project default.
    pub(crate) const fn source(&self) -> Option<&Domain> {
        self.source.as_ref()
    }

    pub(crate) const fn evidence(&self) -> &Evidence {
        &self.evidence
    }

    /// The record's Evidence record owner: the decision table's unit
    /// (D15:1609).
    pub(crate) const fn owner(&self) -> &Unit {
        &self.owner
    }

    /// The Evidence the record refers to, under its owner.
    pub(crate) const fn reference(&self) -> (&Unit, &Evidence) {
        (&self.owner, &self.evidence)
    }

    /// `{ cell: { locale, surfaceClass }, source: "default" | RuleDomain,
    /// evidence }` (D15:2485, 2487).
    pub(crate) fn view(&self) -> Value {
        let (locale, surface) = &self.cell;
        json!({
            "cell": { "locale": locale.as_str(), "surfaceClass": surface.as_str() },
            "source": self.source.as_ref().map_or_else(|| Value::from("default"), rule_domain),
            "evidence": self.evidence.view(),
        })
    }

    /// The record's accounting view, charged to its owner (D15:2518).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an owner `views` cannot present.
    pub(crate) fn accounting(&self, views: &RecordViews<'_>) -> Result<Value, ViewInvariant> {
        views.accounting(
            &self.owner,
            RecordKind::CoverageDecisionEvidence,
            self.view(),
        )
    }
}

/// Every redacted component of `candidates`' Evidence, for the invocation's
/// evidence tokens.
pub(crate) fn redacted(candidates: &[DecisionEvidence]) -> impl Iterator<Item = &Redacted> {
    candidates
        .iter()
        .flat_map(|candidate| candidate.evidence.redacted())
}

/// The records of `candidates`, in their order, under the invocation's
/// evidence tokens `tokens`, each owned by the decision table's unit.
///
/// # Errors
///
/// Returns the resolver bug of a component `tokens` leaves out.
pub(crate) fn records(
    candidates: &[DecisionEvidence],
    specification: Specification<'_>,
    tokens: &EvidenceTokens,
) -> Result<Vec<CoverageDecisionEvidence>, DecisionInvariant> {
    let owner = unit(CoverageSubject::Table, Check::CompleteTable);
    candidates
        .iter()
        .map(|candidate| {
            let evidence = candidate
                .evidence
                .retained(specification, tokens)
                .ok_or_else(|| DecisionInvariant::UnassignedToken(candidate.cell.clone()))?;
            #[cfg(test)]
            carried(&evidence, specification, tokens);
            Ok(CoverageDecisionEvidence {
                owner: owner.clone(),
                cell: candidate.cell.clone(),
                source: candidate.source.clone(),
                evidence,
            })
        })
        .collect()
}

/// Asserts that `evidence` is what Coverage Decision Evidence carries:
/// Materialized Value Evidence of the declaration, or the Coverage
/// Specification input (D15:1609).
#[cfg(test)]
fn carried(evidence: &Evidence, specification: Specification<'_>, tokens: &EvidenceTokens) {
    use super::evidence_component::ComponentPath;
    use super::origin::EvidenceKind;
    use super::specification::Slot;
    let input = EvidenceProjection::resolver_input(ComponentPath::specification(Slot::Coverage))
        .retained(specification, tokens);
    assert!(
        evidence.kind() == EvidenceKind::ConfigurationValue || input.as_ref() == Some(evidence),
        "Coverage Decision Evidence carries {evidence:?}"
    );
}

#[cfg(test)]
mod tests {
    use intlify_shared_json::token::Token;
    use serde_json::json;

    use crate::boundary::source::{file_source_revision, read_pair, ExactIdentity, SourceLimits};
    use crate::input_limits::Bound;
    use crate::locale::CanonicalLocale;
    use crate::resolver::coverage::{unit, CoverageSubject, Domain};
    use crate::resolver::evaluation::{Blocking, Check, Evaluation, Unit};
    use crate::resolver::evidence::{EvidenceProjection, Source};
    use crate::resolver::evidence_component::ComponentPath;
    use crate::resolver::evidence_index::EvidenceIndex;
    use crate::resolver::evidence_path::Step;
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::locales::fixtures::canonical;
    use crate::resolver::origin::SafeOriginKind as O;
    use crate::resolver::record_view::{RecordViews, ViewInvariant};
    use crate::resolver::specification::Slot;
    use crate::resolver::token::{Sha256, Specification};

    use super::{records, redacted, CoverageDecisionEvidence, DecisionEvidence, DecisionInvariant};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };
    const FILE: &str = "intlify-file-source.v0:storefront/intlify.config.json";

    fn exact(text: &str) -> ExactIdentity {
        let largest = Bound::new(u64::MAX - 1).unwrap();
        let limits = SourceLimits {
            identity_bytes: largest,
            revision_bytes: largest,
        };
        read_pair(Some(text), Some(&file_source_revision(b"")), b"", limits)
            .unwrap()
            .identity()
            .clone()
    }

    /// Materialized Value Evidence of `steps` in the declaration's file.
    fn value(steps: &[Step<'_>]) -> EvidenceProjection {
        let identity = exact(FILE);
        EvidenceProjection::configuration_value(
            O::ConfigurationFile,
            Source {
                identity: &identity,
                marked: true,
                locator: None,
            },
            steps,
            None,
        )
        .unwrap()
    }

    /// The rule at `index` of the declaration `profile` names.
    fn rule(profile: Step<'_>, index: u64) -> EvidenceProjection {
        value(&[
            Step::Safe("profiles"),
            profile,
            Step::Safe("coverage"),
            Step::Safe("rules"),
            Step::Index(index),
        ])
    }

    fn specification(slot: Slot) -> EvidenceProjection {
        EvidenceProjection::resolver_input(ComponentPath::specification(slot))
    }

    fn cell(locale: &str, surface: &str) -> (CanonicalLocale, Token) {
        (canonical(locale), Token::new(surface).unwrap())
    }

    fn fr() -> Domain {
        Domain::of(
            vec![canonical("fr")],
            vec![Token::new("checkout").unwrap(), Token::new("nav").unwrap()],
        )
    }

    fn table() -> Unit {
        unit(CoverageSubject::Table, Check::CompleteTable)
    }

    fn mapping(
        (locale, surface): (&str, &str),
        source: Option<Domain>,
        evidence: EvidenceProjection,
    ) -> DecisionEvidence {
        DecisionEvidence {
            cell: cell(locale, surface),
            source,
            evidence,
        }
    }

    /// A record presents its cell, its source, `default` or the matched rule
    /// domain, and its Evidence, and the records keep the candidates' order
    /// (D15:1611, 2485, 2487).
    #[test]
    fn a_record_presents_its_cell_source_and_evidence() {
        let candidates = [
            mapping(("en", "nav"), None, specification(Slot::Coverage)),
            mapping(("fr", "checkout"), Some(fr()), rule(Step::Safe("web"), 1)),
        ];
        let tokens = EvidenceTokens::assign(PROFILE, redacted(&candidates), &Sha256);
        let retained = |candidate: &DecisionEvidence| {
            candidate
                .evidence
                .retained(PROFILE, &tokens)
                .unwrap()
                .view()
        };
        let views: Vec<_> = records(&candidates, PROFILE, &tokens)
            .unwrap()
            .iter()
            .map(CoverageDecisionEvidence::view)
            .collect();
        assert_eq!(
            views,
            [
                json!({
                    "cell": { "locale": "en", "surfaceClass": "nav" },
                    "source": "default",
                    "evidence": retained(&candidates[0]),
                }),
                json!({
                    "cell": { "locale": "fr", "surfaceClass": "checkout" },
                    "source": { "locales": ["fr"], "surfaceClasses": ["checkout", "nav"] },
                    "evidence": retained(&candidates[1]),
                }),
            ]
        );
        assert_eq!(views[0]["evidence"]["kind"], "specification-input");
        assert_eq!(views[1]["evidence"]["kind"], "configuration-value");
    }

    /// The decision table's unit owns every record and is charged each
    /// record's accounting view; one rule deciding two cells gives two
    /// records that refer to one index record, which the table owns too
    /// (D15:1609, 1611, 2518).
    #[test]
    fn the_decision_table_owns_each_record_and_one_rule_is_one_index_record() {
        let candidates = [
            mapping(("fr", "checkout"), Some(fr()), rule(Step::Safe("web"), 0)),
            mapping(("fr", "nav"), Some(fr()), rule(Step::Safe("web"), 0)),
        ];
        let tokens = EvidenceTokens::assign(PROFILE, redacted(&candidates), &Sha256);
        let records = records(&candidates, PROFILE, &tokens).unwrap();
        let mut evaluation = Evaluation::default();
        evaluation
            .check(table(), Ok(()), Ok::<(), Blocking>)
            .unwrap()
            .unwrap();
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        for record in &records {
            assert_eq!(record.owner(), &table());
            assert_eq!(
                record.accounting(&views).unwrap(),
                json!({
                    "owner": views.unit(&table()).unwrap(),
                    "record": { "coverage-decision-evidence": record.view() },
                })
            );
        }
        assert_ne!(records[0].cell(), records[1].cell());
        assert_eq!(records[0].evidence(), records[1].evidence());
        let index = EvidenceIndex::new(records.iter().map(CoverageDecisionEvidence::reference));
        let held: Vec<_> = index.records().collect();
        assert_eq!(held, [(records[0].evidence(), &table())]);
        // Without the table's subject token, no view names the owner.
        let unrecorded = RecordViews::assign(PROFILE, &Evaluation::default(), &Sha256);
        assert_eq!(
            records[0].accounting(&unrecorded),
            Err(ViewInvariant::SubjectWithoutToken(table()))
        );
    }

    /// A member the declaration's path names unsafely is one of the
    /// candidates' redacted components; tokens assigned without it leave the
    /// cell's record a resolver bug (D15:2194).
    #[test]
    fn a_redacted_member_needs_the_invocations_token() {
        let candidates = [mapping(("fr", "nav"), Some(fr()), rule(Step::Unsafe, 0))];
        assert_eq!(redacted(&candidates).count(), 1);
        let without = EvidenceTokens::assign(PROFILE, [], &Sha256);
        assert_eq!(
            records(&candidates, PROFILE, &without),
            Err(DecisionInvariant::UnassignedToken(cell("fr", "nav")))
        );
        let tokens = EvidenceTokens::assign(PROFILE, redacted(&candidates), &Sha256);
        let records = records(&candidates, PROFILE, &tokens).unwrap();
        assert_eq!(
            Some(records[0].evidence()),
            candidates[0].evidence.retained(PROFILE, &tokens).as_ref()
        );
    }

    /// A test whose record carries Evidence Coverage Decision Evidence never
    /// carries, such as another slot of the specification, fails.
    #[test]
    #[should_panic(expected = "Coverage Decision Evidence carries")]
    fn evidence_of_another_slot_fails_the_test() {
        let candidates = [mapping(
            ("en", "nav"),
            None,
            specification(Slot::LocaleNegotiation),
        )];
        let tokens = EvidenceTokens::assign(PROFILE, redacted(&candidates), &Sha256);
        let _ = records(&candidates, PROFILE, &tokens);
    }
}
