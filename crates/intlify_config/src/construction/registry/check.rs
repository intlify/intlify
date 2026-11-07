// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Check Registry and its schema, as far as each row's identity, subject
//! kind, prerequisites, Finding codes, Evidence records, and dependencies go.
//!
//! Design 015's Check Registry has one row for each `(phase, subject kind,
//! check)` the resolver evaluates (D15:2167). Revision `"0"` writes the rows
//! from the resolver's own kinds and checks: every check of every kind in
//! [`SubjectKind::ALL`], the kinds declared only included, in phase, kind-rank,
//! and check-rank order. A row names its check ID, phase, rule ID, subject
//! kind, key strategy, content projection, owning scope, parent subject, its
//! two ranks, its prerequisites, the Finding codes it may report, the ordinary
//! Evidence record kinds its units may own, and the dependency records they
//! own. Its payload strategy comes with its own stage.
//!
//! 015 leaves the IDs of kinds, scopes, and checks to the registry (D15:2167,
//! 2169). A kind's ID is its name in kebab case. A singleton's owning scope is
//! its own kind; every other kind names the field, map, or collection its
//! keys belong to. A check ID reads `<area>.<subject>.<check>` like 015's
//! examples, with the phase's area first, and a rule ID is the check ID after
//! `015.`. A kind keyed by an identity or content uses `redacted-structure`,
//! because classifying invalid content as non-secret would expose arbitrary
//! values (Decision 015-157); every other kind uses `none`.
//!
//! A row's prerequisites are the checks its units wait on (D15:2143, 2160): a
//! unit waits on a check whose result it needs, and is not evaluated without
//! it, or whose result alone names its subjects, and is then not recorded. A
//! row lists, in row order, the checks it waits on that none of the others it
//! lists already waits on. A check under the admitted Resource Limit Policy
//! waits on every Policy recheck (D15:1675). The kinds declared only list no
//! prerequisite until their units exist, and no row lists a check of
//! structural admission or profile selection yet: the resolver starts from
//! their complete results.
//!
//! A row's Finding codes are those its units may report, a notice included,
//! in 015's table order: what the resolver's checks report, and for the kinds
//! declared only, what 015 gives their checks. Every evaluation a test runs
//! asserts that each code it records is one its unit's row lists. Whether a
//! code names a Finding Registry row comes with that registry.
//!
//! A row's record kinds follow 015's rules for who owns which ordinary
//! Evidence record, since the records themselves come with Resolution
//! Evidence: every unit owns its evaluation status, a check that reports
//! codes owns Findings with their occurrences and evidence index, the
//! decision table owns both coverage decision records, and the first kinds
//! of entry and structural admission and each semantic group's owner own the
//! dependency records (D15:2510).
//!
//! Construction reads the registry it admitted with the registry
//! cross-references (row 41), and refuses one this type cannot read or whose
//! rows do not relate as 015 requires: a check ID names one row, under its own
//! rule ID; the rows of one kind in one phase agree on its key shape, parent,
//! and rank (D15:2178); kinds rank uniquely within a phase and checks within a
//! kind (D15:2193, 2195); only a kind keyed by an identity or content names a
//! projection (D15:2182); a singleton is unscoped; a parent is a kind with
//! rows in the same or an earlier phase (D15:2169), from which following
//! parents ends at an unscoped kind; a prerequisite names a row once, never
//! one of a later phase or of a kind its phase ranks later, and following
//! prerequisites never comes back to a row; and every dependency record is
//! owned by exactly one row, which lists its record kind (D15:2269). A row of
//! the same kind may be an earlier subject's check (D15:2143), so its rank is
//! not compared. Ranks are compared as the shortest decimal strings the
//! schema spells them as. Whether the rows are this resolver's own kinds and
//! checks is not part of the graph: a closed registry that is not the one
//! compiled in fails the implemented-authority row (43) by its file digest.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use intlify_shared_json::decode;

use crate::resolver::dependency::{EntrySlot, MaterializedSlot, SemanticGroup};
use crate::resolver::evaluation::{Code, Phase, Subject};
use crate::resolver::subject::SubjectKind;

use super::RANK_PATTERN;

/// The registry's identity.
pub(crate) const IDENTITY: &str = "intlify-project-profile-check-registry";

/// The registry schema's identity.
pub(crate) const SCHEMA_IDENTITY: &str = "intlify-project-profile-check-registry-schema";

/// The revision of the registry and of its schema.
pub(crate) const REVISION: &str = "0";

/// The registry's path below the conformance suite's root.
pub(crate) const PATH: &str = "registries/check-v0.json";

/// The registry schema's path below the conformance suite's root.
pub(crate) const SCHEMA_PATH: &str = "registries/check-v0.schema.json";

/// The registry's file digest, which the resolver is compiled from.
pub(crate) const DIGEST: &str =
    "sha256:7d9214e104b268947ec0d997f99b80074db9df844c24d121aa317f7e70a95c13";

/// The registry schema's file digest, which the resolver is compiled from.
pub(crate) const SCHEMA_DIGEST: &str =
    "sha256:c105ce7fa39c9f7dbe0107b540b047df6a5201a177a59979b4b57b01e5addd93";

/// A check ID: three kebab-case segments joined by dots.
pub(crate) const CHECK_ID_PATTERN: &str =
    "^[a-z][a-z0-9]*(-[a-z0-9]+)*\\.[a-z][a-z0-9]*(-[a-z0-9]+)*\\.[a-z][a-z0-9]*(-[a-z0-9]+)*$";

/// A rule ID: a check ID after `015.`.
pub(crate) const RULE_ID_PATTERN: &str = "^015\\.[a-z][a-z0-9]*(-[a-z0-9]+)*\\.[a-z][a-z0-9]*(-[a-z0-9]+)*\\.[a-z][a-z0-9]*(-[a-z0-9]+)*$";

/// An owning scope's ID: one kebab-case segment.
pub(crate) const SCOPE_PATTERN: &str = "^[a-z][a-z0-9]*(-[a-z0-9]+)*$";

/// 015's four subject-key strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum KeyStrategy {
    Singleton,
    LogicalField,
    CanonicalIdentityOrContent,
    SemanticPosition,
}

/// 015's Subject content projections: `none` for a kind keyed by no identity
/// or content, and one closed projection otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ContentProjection {
    None,
    NonSecretContent,
    RedactedStructure,
}

/// The kind whose subjects scope a row's subjects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ParentSubject {
    pub(crate) phase_id: Phase,
    pub(crate) subject_kind: SubjectKind,
}

/// 015's twelve ordinary Evidence record kinds, the Entry domain's first, in
/// 015's order.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RecordKind {
    EntryFinding,
    EntryEvaluationStatus,
    FindingOccurrence,
    CoverageDecisionEvidence,
    EvidenceIndex,
    EntryDependency,
    SharedUnavailabilityCause,
    SharedFinding,
    SharedEvaluationStatus,
    CoverageDecisionBasis,
    SemanticDependency,
    MaterializedDependency,
}

/// A dependency record a row's units own, as its record kind and ID.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(tag = "recordKind", content = "id", deny_unknown_fields)]
pub(crate) enum OwnedDependency {
    #[serde(rename = "semantic-dependency")]
    Semantic(SemanticGroup),
    #[serde(rename = "materialized-dependency")]
    Materialized(MaterializedSlot),
    #[serde(rename = "entry-dependency")]
    Entry(EntrySlot),
}

impl OwnedDependency {
    /// The record kind of the dependency record.
    pub(crate) const fn record_kind(self) -> RecordKind {
        match self {
            Self::Semantic(_) => RecordKind::SemanticDependency,
            Self::Materialized(_) => RecordKind::MaterializedDependency,
            Self::Entry(_) => RecordKind::EntryDependency,
        }
    }
}

/// One row: one check of one subject kind in one phase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CheckRow {
    #[schemars(pattern(CHECK_ID_PATTERN))]
    pub(crate) check_id: String,
    pub(crate) phase_id: Phase,
    #[schemars(pattern(RULE_ID_PATTERN))]
    pub(crate) rule_id: String,
    pub(crate) subject_kind: SubjectKind,
    pub(crate) subject_key_strategy: KeyStrategy,
    pub(crate) subject_content_projection: ContentProjection,
    #[schemars(pattern(SCOPE_PATTERN))]
    pub(crate) owning_scope: String,
    /// Absent for an unscoped subject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "ParentSubject")]
    pub(crate) parent_subject: Option<ParentSubject>,
    #[schemars(pattern(RANK_PATTERN))]
    pub(crate) subject_kind_rank: String,
    #[schemars(pattern(RANK_PATTERN))]
    pub(crate) check_rank: String,
    /// The checks the row waits on, except those another listed check waits on.
    #[schemars(extend("uniqueItems" = true), inner(pattern(CHECK_ID_PATTERN)))]
    pub(crate) prerequisite_check_ids: Vec<String>,
    /// The Finding codes the row's units may report, in 015's table order.
    #[schemars(extend("uniqueItems" = true))]
    pub(crate) possible_finding_codes: Vec<Code>,
    /// The ordinary Evidence record kinds the row's units may own.
    #[schemars(extend("uniqueItems" = true))]
    pub(crate) possible_evidence_record_kinds: Vec<RecordKind>,
    /// The dependency records the row's units own.
    #[schemars(extend("uniqueItems" = true))]
    pub(crate) owned_dependencies: Vec<OwnedDependency>,
}

/// The registry's body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckRegistry {
    #[schemars(length(min = 1))]
    pub(crate) rows: Vec<CheckRow>,
}

impl CheckRegistry {
    /// The revision-`"0"` registry: every check of every kind, in phase,
    /// kind-rank, and check-rank order.
    pub(crate) fn revision_zero() -> Self {
        let mut rows = Vec::new();
        for kind in SubjectKind::ALL {
            let shape = shape(kind);
            let ids = check_ids(kind);
            let prerequisites = prerequisite_ids(kind);
            let codes = finding_codes(kind);
            let owned = owned_dependencies(kind);
            debug_assert_eq!(ids.len(), kind.checks().len(), "{kind:?}");
            debug_assert_eq!(prerequisites.len(), ids.len(), "{kind:?}");
            debug_assert_eq!(codes.len(), ids.len(), "{kind:?}");
            for (rank, ((id, prerequisites), codes)) in
                ids.iter().zip(prerequisites).zip(codes).enumerate()
            {
                rows.push(CheckRow {
                    check_id: (*id).to_owned(),
                    phase_id: kind.phase(),
                    rule_id: format!("015.{id}"),
                    subject_kind: kind,
                    subject_key_strategy: shape.strategy,
                    subject_content_projection: shape.projection(),
                    owning_scope: shape.scope.map_or_else(|| kind_id(kind), str::to_owned),
                    parent_subject: shape.parent.map(|parent| ParentSubject {
                        phase_id: parent.phase(),
                        subject_kind: parent,
                    }),
                    subject_kind_rank: kind.rank().to_string(),
                    check_rank: rank.to_string(),
                    prerequisite_check_ids: prerequisites
                        .iter()
                        .map(|&prerequisite| prerequisite.to_owned())
                        .collect(),
                    possible_finding_codes: codes.to_vec(),
                    possible_evidence_record_kinds: record_kinds(kind, codes, &owned),
                    owned_dependencies: owned.clone(),
                });
            }
        }
        Self { rows }
    }

    /// Read a registry its admitted schema accepted, as the closed graph needs
    /// it: a registry this type cannot read, or one whose rows do not relate
    /// as 015 requires, is not part of one.
    pub(crate) fn read(value: &Value) -> Option<Self> {
        let registry = decode::typed::<Self>(value.clone()).ok()?;
        registry.is_closed().then_some(registry)
    }

    /// Whether the rows relate as 015 requires: each check ID is one row's,
    /// under its own rule ID; every row's key shape is sound and agrees with
    /// the other rows of its kind; kinds rank uniquely within a phase and
    /// checks within a kind; every chain of parents ends; so does every chain
    /// of prerequisites; and every dependency record has one owner.
    fn is_closed(&self) -> bool {
        let mut domains = BTreeMap::new();
        for row in &self.rows {
            let domain = Domain::of(row);
            if row.rule_id.strip_prefix("015.") != Some(row.check_id.as_str())
                || !domain.is_sound(row.phase_id)
            {
                return false;
            }
            if *domains
                .entry((row.phase_id, row.subject_kind))
                .or_insert(domain)
                != domain
            {
                return false;
            }
        }
        let check_ids: BTreeSet<&str> = self.rows.iter().map(|row| row.check_id.as_str()).collect();
        let check_ranks: BTreeSet<(Phase, SubjectKind, &str)> = self
            .rows
            .iter()
            .map(|row| (row.phase_id, row.subject_kind, row.check_rank.as_str()))
            .collect();
        let kind_ranks: BTreeSet<(Phase, &str)> = domains
            .iter()
            .map(|(&(phase, _), domain)| (phase, domain.rank))
            .collect();
        check_ids.len() == self.rows.len()
            && check_ranks.len() == self.rows.len()
            && kind_ranks.len() == domains.len()
            && parents_end(&domains)
            && prerequisites_end(&self.rows)
            && dependencies_owned_once(&self.rows)
    }
}

/// What the rows of one kind in one phase agree on (D15:2178): the kind's key
/// shape, its parent, and its rank among the phase's kinds (D15:2193).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Domain<'a> {
    strategy: KeyStrategy,
    projection: ContentProjection,
    scope: &'a str,
    parent: Option<&'a ParentSubject>,
    rank: &'a str,
}

impl<'a> Domain<'a> {
    fn of(row: &'a CheckRow) -> Self {
        Self {
            strategy: row.subject_key_strategy,
            projection: row.subject_content_projection,
            scope: &row.owning_scope,
            parent: row.parent_subject.as_ref(),
            rank: &row.subject_kind_rank,
        }
    }

    /// Whether 015 allows this key shape in `phase`: a kind keyed by an
    /// identity or content names one closed projection and every other kind
    /// none (D15:2182), a singleton's phase-global subject is unscoped
    /// (D15:2173), and a parent comes from the same or an earlier phase
    /// (D15:2169).
    fn is_sound(self, phase: Phase) -> bool {
        let keyed = self.strategy == KeyStrategy::CanonicalIdentityOrContent;
        keyed == (self.projection != ContentProjection::None)
            && self.parent.is_none_or(|parent| {
                self.strategy != KeyStrategy::Singleton && parent.phase_id <= phase
            })
    }
}

/// Whether every kind's parent is a kind with rows (D15:2169) and following
/// parents from any kind ends at an unscoped one, so that no kind scopes
/// itself.
fn parents_end(domains: &BTreeMap<(Phase, SubjectKind), Domain<'_>>) -> bool {
    domains.keys().all(|&start| {
        let mut kind = start;
        // A chain that visits more domains than there are repeats one.
        for _ in 0..domains.len() {
            match domains.get(&kind).map(|domain| domain.parent) {
                None => return false,
                Some(None) => return true,
                Some(Some(parent)) => kind = (parent.phase_id, parent.subject_kind),
            }
        }
        false
    })
}

/// Whether every prerequisite names a row once, never one whose units follow
/// all of its dependent's in canonical unit order, and whether following
/// prerequisites from any row ends, so that no row waits on itself
/// (D15:2269).
fn prerequisites_end(rows: &[CheckRow]) -> bool {
    let named: BTreeMap<&str, &CheckRow> = rows
        .iter()
        .map(|row| (row.check_id.as_str(), row))
        .collect();
    // How many of each row's prerequisites have not ended yet, and which rows
    // wait on each row.
    let mut pending = BTreeMap::new();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for row in rows {
        let mut listed = BTreeSet::new();
        for id in &row.prerequisite_check_ids {
            match named.get(id.as_str()) {
                Some(prerequisite) if listed.insert(id.as_str()) && !follows(prerequisite, row) => {
                    dependents
                        .entry(id.as_str())
                        .or_default()
                        .push(row.check_id.as_str());
                }
                _ => return false,
            }
        }
        pending.insert(row.check_id.as_str(), listed.len());
    }
    let mut ended: Vec<&str> = pending
        .iter()
        .filter(|&(_, &left)| left == 0)
        .map(|(&id, _)| id)
        .collect();
    let mut count = 0;
    while let Some(id) = ended.pop() {
        count += 1;
        for &dependent in dependents.get(id).into_iter().flatten() {
            if let Some(left) = pending.get_mut(dependent) {
                *left -= 1;
                if *left == 0 {
                    ended.push(dependent);
                }
            }
        }
    }
    count == pending.len()
}

/// Whether every dependency record is owned by exactly one row, which lists
/// its record kind among those it may own (D15:2269).
fn dependencies_owned_once(rows: &[CheckRow]) -> bool {
    let mut owned = BTreeSet::new();
    for row in rows {
        for dependency in &row.owned_dependencies {
            if !owned.insert(*dependency)
                || !row
                    .possible_evidence_record_kinds
                    .contains(&dependency.record_kind())
            {
                return false;
            }
        }
    }
    owned.len() == SemanticGroup::ALL.len() + MaterializedSlot::ALL.len() + EntrySlot::ALL.len()
}

/// Whether `prerequisite`'s units follow all of `dependent`'s in canonical
/// unit order: it is of a later phase, or of a kind its phase ranks later
/// (D15:2143). A row of the same kind may be an earlier subject's check.
fn follows(prerequisite: &CheckRow, dependent: &CheckRow) -> bool {
    place(prerequisite) > place(dependent)
}

/// A row's phase and kind rank, the rank compared as a number.
fn place(row: &CheckRow) -> (Phase, usize, &str) {
    let rank = row.subject_kind_rank.as_str();
    (row.phase_id, rank.len(), rank)
}

/// The registry's schema.
///
/// # Errors
///
/// Returns the serialization failure of the generated schema, which the types
/// rule out.
pub(crate) fn schema() -> Result<Value, serde_json::Error> {
    crate::schema::draft7_schema::<CheckRegistry>()
}

/// A kind's registry ID: its name in kebab case.
fn kind_id(kind: SubjectKind) -> String {
    match serde_json::to_value(kind) {
        Ok(Value::String(id)) => id,
        _ => unreachable!("a kind serializes as its ID"),
    }
}

/// The shape of a kind's keys: its strategy, its owning scope (`None` for a
/// singleton, whose scope is its own kind), and the kind whose subjects scope
/// its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    strategy: KeyStrategy,
    scope: Option<&'static str>,
    parent: Option<SubjectKind>,
}

impl Shape {
    const fn projection(self) -> ContentProjection {
        match self.strategy {
            KeyStrategy::CanonicalIdentityOrContent => ContentProjection::RedactedStructure,
            KeyStrategy::Singleton | KeyStrategy::LogicalField | KeyStrategy::SemanticPosition => {
                ContentProjection::None
            }
        }
    }
}

/// The shape of each kind's keys, from the key the resolver gives its
/// subjects: none for a singleton, a field path in the owning scope for a
/// single-valued field (Decision 015-155), an identity or a content projection
/// in a map or collection, and a position in a semantic sequence. A parent's
/// own key comes first in a parent-scoped key.
#[expect(
    clippy::too_many_lines,
    reason = "one arm for each shape of 015's kinds"
)]
const fn shape(kind: SubjectKind) -> Shape {
    use KeyStrategy::{
        CanonicalIdentityOrContent as Identity, LogicalField as Field, SemanticPosition, Singleton,
    };
    use SubjectKind as K;
    let (strategy, scope, parent) = match kind {
        K::ProjectProfileSlot
        | K::EntryDependencies
        | K::CompleteInput
        | K::MaterializedDependencies
        | K::ConfigurationSchemaSelection
        | K::CapabilityStructuralAnalysis
        | K::ProfileSpecificationDependency
        | K::CapabilityProfiles
        | K::ProfileSelector
        | K::CapabilityArtifacts
        | K::CapabilityPolicyArtifacts
        | K::CapabilityTargetProfileArtifacts
        | K::CapabilityReferences
        | K::CapabilityArtifactBytes
        | K::CapabilityReferenceBytes
        | K::ResourceLimits
        | K::RootNodes
        | K::RootDepth
        | K::RootCollectionEntries
        | K::RootStringBytes
        | K::RootLongestString
        | K::Profiles
        | K::Selector
        | K::StructuralAnalysis
        | K::Artifacts
        | K::PolicyArtifacts
        | K::TargetProfileArtifacts
        | K::References
        | K::ArtifactBytes
        | K::ReferenceBytes
        | K::LocaleCanonicalizationSlot
        | K::LocaleNegotiationSlot
        | K::MessageLocaleFallbackSlot
        | K::CoverageSlot
        | K::IntentSurfaceClassVocabularySlot
        | K::DeliveryGraphSlot
        | K::DeliveryPlacementSlot
        | K::Vocabulary
        | K::CanonicalizationData
        | K::LocaleCanonicalizationDependency
        | K::ProjectIdentityDependency
        | K::LocaleOccurrences
        | K::ProjectLocalesDependency
        | K::CoverageLocaleSelectors
        | K::CoverageSurfaceSelectors
        | K::CoverageCells
        | K::CoverageComparisons
        | K::CoverageTable
        | K::LocaleNegotiationDependency
        | K::MessageFallbackDependency
        | K::CoverageDependency
        | K::RequiredPoliciesDependency
        | K::OptionalPoliciesDependency
        | K::DeliveryDependency
        | K::TargetsDependency
        | K::Memberships
        | K::Relations
        | K::DeploymentGroupsDependency
        | K::DiagnosticDomain => (Singleton, None, None),
        // Structural admission: each scope level's fields and each identity
        // map's keys (Decision 015-155).
        K::RootField => (Field, Some("root"), None),
        K::ProfileKey | K::CapabilityProfileId | K::ProfileId => (Identity, Some("profiles"), None),
        K::ProfileField => (Field, Some("profile"), Some(K::ProfileKey)),
        K::TargetProfileKey => (Identity, Some("target-profiles"), Some(K::ProfileKey)),
        K::TargetField => (Field, Some("target"), Some(K::TargetProfileKey)),
        K::DeploymentGroupKey => (Identity, Some("deployment-groups"), Some(K::ProfileKey)),
        K::GroupField => (Field, Some("group"), Some(K::DeploymentGroupKey)),
        // The selected declaration's single-valued fields, unscoped by its
        // Profile ID after profile selection.
        K::CapabilityPolicyReference
        | K::PolicyReference
        | K::DeclarationField
        | K::RequestedLocales
        | K::DefaultSourceLocale
        | K::DefaultRequestedLocale
        | K::TargetProfiles
        | K::NegotiationAliases
        | K::FallbackSources
        | K::CoverageRules
        | K::ResolvedPolicyReference
        | K::DeliveryPlacement
        | K::Groups => (Field, Some("declaration"), None),
        // Target IDs and Group IDs; the Profile IDs share the `profiles` key
        // kind's arm above.
        K::TargetId | K::Target | K::Assignment => (Identity, Some("target-profiles"), None),
        K::Group => (Identity, Some("deployment-groups"), None),
        // The artifact collection, its conflict groups, and the Target
        // Profile reference of each Target ID.
        K::CapabilityArtifact | K::Artifact | K::CollectionMember => {
            (Identity, Some("artifact-collection"), None)
        }
        K::CollectionGroup => (Identity, Some("artifact-conflict-groups"), None),
        K::CapabilityTargetProfileReference
        | K::TargetProfileReference
        | K::ResolvedTargetProfileReference => (Identity, Some("target-profile-references"), None),
        // Locales, and the locales under a target or an alias key.
        K::RequestedLocale => (Identity, Some("requested-locales"), None),
        K::TargetRequestedLocale => (
            Identity,
            Some("target-requested-locales"),
            Some(K::TargetId),
        ),
        K::TargetDefaultRequestedLocale => (Field, Some("target"), Some(K::TargetId)),
        K::NegotiationAliasKey | K::NegotiationKey => (Identity, Some("negotiation-aliases"), None),
        K::NegotiationAliasValue => (
            Identity,
            Some("negotiation-destinations"),
            Some(K::NegotiationAliasKey),
        ),
        K::NegotiationDestination => (
            Identity,
            Some("negotiation-destinations"),
            Some(K::NegotiationKey),
        ),
        // The fallback mapping's keys and, under each, its sequence and its
        // candidates (Decision 015-158).
        K::MessageFallbackKey | K::FallbackSource => (Identity, Some("message-fallback"), None),
        K::FallbackSequence => (Field, Some("fallback-entry"), Some(K::FallbackSource)),
        K::FallbackCandidate => (
            SemanticPosition,
            Some("fallback-candidates"),
            Some(K::FallbackSource),
        ),
        K::FallbackLiteral => (Identity, Some("fallback-literals"), Some(K::FallbackSource)),
        K::FallbackIntentSource => (
            Identity,
            Some("fallback-intent-sources"),
            Some(K::FallbackSource),
        ),
        // Coverage rules, which scope their selector items by their content,
        // their normalized domains, and the decision table's cells.
        K::CoverageRule => (Identity, Some("coverage-rules"), None),
        K::CoverageLocale => (Identity, Some("coverage-locales"), Some(K::CoverageRule)),
        K::CoverageSurfaceClass => (
            Identity,
            Some("coverage-surface-classes"),
            Some(K::CoverageRule),
        ),
        K::CoverageDomain => (Identity, Some("coverage-domains"), None),
        K::CoverageCell => (Identity, Some("coverage-cells"), None),
        // A group's fields and members, and its relations, their endpoints,
        // and the roles its Target IDs take.
        K::GroupMembers => (Field, Some("group"), Some(K::Group)),
        K::GroupMember => (Identity, Some("group-members"), Some(K::Group)),
        K::Relation => (Identity, Some("hydration-relations"), Some(K::Group)),
        K::RelationEndpoint => (Field, Some("relation"), Some(K::Relation)),
        K::Roles => (Identity, Some("hydration-roles"), Some(K::Group)),
    };
    Shape {
        strategy,
        scope,
        parent,
    }
}

/// Each kind's check IDs, in the order of [`SubjectKind::checks`]. A check
/// that only states a bound reads `within-bound` under the admitted policy and
/// `within-capability` before it; 015's five examples are kept as written.
#[expect(clippy::too_many_lines, reason = "one arm for each of 015's kinds")]
const fn check_ids(kind: SubjectKind) -> &'static [&'static str] {
    use SubjectKind as K;
    match kind {
        K::ProjectProfileSlot => &["specification.project-profile.supported"],
        K::EntryDependencies => &["dependency.entry.established-slots"],
        K::CompleteInput => &[
            "entry.input.within-file-bytes",
            "entry.input.within-parser-tokens",
            "entry.input.well-formed-json",
            "entry.input.unique-members",
            "entry.input.json-compatible",
            "entry.input.within-nodes",
            "entry.input.within-depth",
            "entry.input.within-collection-entries",
            "entry.input.within-total-string-bytes",
            "entry.input.within-single-string-bytes",
        ],
        K::MaterializedDependencies => &["dependency.materialized.established-slots"],
        K::ConfigurationSchemaSelection => &[
            "structural.schema-selection.object-root",
            "structural.schema-selection.present-schema-version",
            "structural.schema-selection.string-schema-version",
            "structural.schema-selection.supported-schema-version",
        ],
        K::CapabilityStructuralAnalysis => &["structural.analysis.within-capability"],
        K::RootField => &[
            "structural.root.required-field",
            "structural.root.field-type",
            "structural.root.field-value",
            "structural.root.known-field",
        ],
        K::ProfileKey => &["structural.profile-key.field-value"],
        K::ProfileField => &[
            "structural.profile.required-field",
            "structural.profile.field-type",
            "structural.profile.field-value",
            "structural.profile.known-field",
        ],
        K::TargetProfileKey => &["structural.target-key.field-value"],
        K::TargetField => &[
            "structural.target.required-field",
            "structural.target.field-type",
            "structural.target.field-value",
            "structural.target.known-field",
        ],
        K::DeploymentGroupKey => &["structural.group-key.field-value"],
        K::GroupField => &[
            "structural.group.required-field",
            "structural.group.field-type",
            "structural.group.field-value",
            "structural.group.known-field",
        ],
        K::ProfileSpecificationDependency => &["dependency.profile-specification.established"],
        K::CapabilityProfiles => &["selection.profiles.within-capability"],
        K::CapabilityProfileId => &["selection.profile-id.within-capability"],
        K::ProfileSelector => &[
            "selection.selector.within-capability",
            "selection.selector.required",
            "selection.selector.valid",
            "selection.selector.declared",
        ],
        K::CapabilityArtifacts => &["resource-policy.artifacts.within-capability"],
        K::CapabilityPolicyArtifacts => &["resource-policy.policy-artifacts.within-capability"],
        K::CapabilityTargetProfileArtifacts => {
            &["resource-policy.target-profile-artifacts.within-capability"]
        }
        K::CapabilityReferences => &["resource-policy.references.within-capability"],
        K::CapabilityArtifact => &["resource-policy.artifact.within-capability"],
        K::CapabilityArtifactBytes => &["resource-policy.artifact-bytes.within-capability"],
        K::CapabilityPolicyReference => &["resource-policy.policy-reference.within-capability"],
        K::CapabilityTargetProfileReference => {
            &["resource-policy.target-profile-reference.within-capability"]
        }
        K::CapabilityReferenceBytes => &["resource-policy.reference-bytes.within-capability"],
        K::ResourceLimits => &[
            "resource-policy.resource-limits.bootstrap-candidate",
            "resource-policy.resource-limits.compatible",
        ],
        K::RootNodes => &["resource-policy.root-nodes.within-bound"],
        K::RootDepth => &["resource-policy.root-depth.within-bound"],
        K::RootCollectionEntries => &["resource-policy.root-collection-entries.within-bound"],
        K::RootStringBytes => &["resource-policy.root-string-bytes.within-bound"],
        K::RootLongestString => &["resource-policy.root-longest-string.within-bound"],
        K::Profiles => &["resource-policy.profiles.within-bound"],
        K::ProfileId => &["resource-policy.profile-id.within-bound"],
        K::Selector => &["resource-policy.selector.within-bound"],
        K::StructuralAnalysis => &["resource-policy.structural-analysis.within-bound"],
        K::Artifacts => &["resource-policy.artifacts.within-bound"],
        K::PolicyArtifacts => &["resource-policy.policy-artifacts.within-bound"],
        K::TargetProfileArtifacts => &["resource-policy.target-profile-artifacts.within-bound"],
        K::References => &["resource-policy.references.within-bound"],
        K::Artifact => &["resource-policy.artifact.within-bound"],
        K::ArtifactBytes => &["resource-policy.artifact-bytes.within-bound"],
        K::PolicyReference => &["resource-policy.policy-reference.within-bound"],
        K::TargetProfileReference => &["resource-policy.target-profile-reference.within-bound"],
        K::ReferenceBytes => &["resource-policy.reference-bytes.within-bound"],
        K::LocaleCanonicalizationSlot => &["specification.locale-canonicalization.supported"],
        K::LocaleNegotiationSlot => &["specification.locale-negotiation.supported"],
        K::MessageLocaleFallbackSlot => &["specification.message-locale-fallback.supported"],
        K::CoverageSlot => &["specification.coverage.supported"],
        K::IntentSurfaceClassVocabularySlot => {
            &["specification.intent-surface-class-vocabulary.supported"]
        }
        K::DeliveryGraphSlot => &["specification.delivery-graph.supported"],
        K::DeliveryPlacementSlot => &["specification.delivery-placement.supported"],
        K::Vocabulary => &[
            "specification.vocabulary.intact",
            "specification.vocabulary.asserted",
        ],
        K::CanonicalizationData => &["specification.locale-data.matching"],
        K::LocaleCanonicalizationDependency => &["dependency.locale-canonicalization.established"],
        K::DeclarationField => &["identity.declaration-field.opaque"],
        K::ProjectIdentityDependency => &["dependency.project-identity.established"],
        K::LocaleOccurrences => &["locale.occurrences.within-bound"],
        K::RequestedLocale => &["locale.requested.canonical", "locale.requested.unique"],
        K::RequestedLocales => &["locale.requested-set.within-bound"],
        K::DefaultSourceLocale => &["locale.default-source.canonical"],
        K::DefaultRequestedLocale => &[
            "locale.default-requested.canonical",
            "locale.default-requested.member",
        ],
        K::NegotiationAliasKey => &["locale.alias-key.canonical"],
        K::NegotiationAliasValue => &["locale.alias-destination.canonical"],
        K::MessageFallbackKey => &["locale.fallback-key.canonical"],
        K::TargetProfiles => &["locale.target-profiles.within-bound"],
        K::TargetId => &["locale.target-id.within-bound"],
        K::TargetRequestedLocale => &[
            "locale.target-requested.canonical",
            "locale.target-requested.unique",
        ],
        K::TargetDefaultRequestedLocale => &["locale.target-default-requested.canonical"],
        K::ProjectLocalesDependency => &["dependency.project-locales.established"],
        K::NegotiationAliases => &["negotiation.aliases.within-bound"],
        K::NegotiationKey => &["negotiation.alias-key.unique"],
        K::NegotiationDestination => &["negotiation.destination.member"],
        K::FallbackSources => &["fallback.sources.within-bound"],
        K::FallbackSource => &["fallback.source.unique", "fallback.source.member"],
        K::FallbackSequence => &["fallback.sequence.within-bound"],
        K::FallbackCandidate => &[
            "fallback.candidate.canonical",
            "fallback.candidate.distinct-from-source",
        ],
        K::FallbackLiteral => &["fallback.literal.unique"],
        K::FallbackIntentSource => &["fallback.intent-source.unique"],
        K::CoverageRules => &["coverage.rules.within-bound"],
        K::CoverageLocaleSelectors => &["coverage.locale-selectors.within-bound"],
        K::CoverageSurfaceSelectors => &["coverage.surface-selectors.within-bound"],
        K::CoverageLocale => &[
            "coverage.locale.canonical",
            "coverage.locale.unique",
            "coverage.locale.member",
        ],
        K::CoverageSurfaceClass => &[
            "coverage.surface-class.vocabulary-member",
            "coverage.surface-class.unique",
        ],
        K::CoverageRule => &["coverage.rule.normalized-domain"],
        K::CoverageDomain => &["coverage.domain.unique"],
        K::CoverageCells => &["coverage.cells.within-bound"],
        K::CoverageComparisons => &["coverage.comparisons.within-bound"],
        K::CoverageCell => &["coverage.cell.effective-mode"],
        K::CoverageTable => &["coverage.table.complete"],
        K::LocaleNegotiationDependency => &["dependency.locale-negotiation.established"],
        K::MessageFallbackDependency => &["dependency.message-fallback.established"],
        K::CoverageDependency => &["dependency.coverage.established"],
        K::CollectionMember => &[
            "artifact.member.admissible",
            "artifact.member.single-occurrence",
        ],
        K::CollectionGroup => &["artifact.conflict-group.consistent-content"],
        K::ResolvedPolicyReference => &["artifact.policy-reference.resolved"],
        K::ResolvedTargetProfileReference => &["artifact.target-profile-reference.resolved"],
        K::RequiredPoliciesDependency => &["dependency.required-policies.established"],
        K::OptionalPoliciesDependency => &["dependency.optional-policies.established"],
        K::DeliveryPlacement => &["delivery.placement.effective-mode"],
        K::DeliveryDependency => &["dependency.delivery.established"],
        K::Target => &[
            "target.requested-locales.project-subset",
            "target.effective-default.supported",
            "target.requested-locales.directed",
        ],
        K::TargetsDependency => &["dependency.targets.established"],
        K::Groups => &["group.groups.within-bound"],
        K::Memberships => &["group.memberships.within-bound"],
        K::Group => &["group.group-id.within-bound"],
        K::GroupMembers => &["group.members.within-bound"],
        K::GroupMember => &[
            "group.member.within-bound",
            "group.member.known",
            "group.member.unique",
        ],
        K::Assignment => &["group.target.single-group"],
        K::Relations => &["hydration.relations.within-bound"],
        K::RelationEndpoint => &[
            "hydration.endpoint.within-bound",
            "hydration.endpoint.known",
            "hydration.endpoint.same-group",
            "hydration.endpoint.role",
        ],
        K::Relation => &[
            "hydration.relation.distinct-endpoints",
            "hydration.relation.unique",
            "hydration.relation.same-requested-locales",
            "hydration.relation.same-effective-default",
            "hydration.relation.static-compatibility",
        ],
        K::Roles => &["hydration.target.single-role"],
        K::DeploymentGroupsDependency => &["dependency.deployment-groups.established"],
        K::DiagnosticDomain => &["reporting.diagnostic-domain.complete"],
    }
}

/// No prerequisite.
const NONE: &[&str] = &[];

/// The prerequisites of a check of a kind declared only: none listed until
/// its units exist.
const NOT_YET: &[&str] = &[];

/// What a check under the admitted Resource Limit Policy waits on: every
/// Policy recheck, in row order (D15:1675).
const CONFIRMED_POLICY: &[&str] = &[
    "resource-policy.root-nodes.within-bound",
    "resource-policy.root-depth.within-bound",
    "resource-policy.root-collection-entries.within-bound",
    "resource-policy.root-string-bytes.within-bound",
    "resource-policy.root-longest-string.within-bound",
    "resource-policy.profiles.within-bound",
    "resource-policy.profile-id.within-bound",
    "resource-policy.selector.within-bound",
    "resource-policy.structural-analysis.within-bound",
    "resource-policy.artifacts.within-bound",
    "resource-policy.policy-artifacts.within-bound",
    "resource-policy.target-profile-artifacts.within-bound",
    "resource-policy.references.within-bound",
    "resource-policy.artifact.within-bound",
    "resource-policy.artifact-bytes.within-bound",
    "resource-policy.policy-reference.within-bound",
    "resource-policy.target-profile-reference.within-bound",
    "resource-policy.reference-bytes.within-bound",
];

/// Each kind's prerequisites, one list for each of its checks in the order
/// of [`SubjectKind::checks`]: the checks its units wait on that none of the
/// others listed already waits on, in row order. The resolver's tests make
/// each unit of complete invocations block, and these are the waits the
/// units show. The kinds declared only wait on nothing until their units
/// exist.
#[expect(clippy::too_many_lines, reason = "one list for each of 015's checks")]
const fn prerequisite_ids(kind: SubjectKind) -> &'static [&'static [&'static str]] {
    use SubjectKind as K;
    match kind {
        K::ProjectProfileSlot
        | K::CapabilityArtifacts
        | K::CapabilityPolicyArtifacts
        | K::CapabilityTargetProfileArtifacts
        | K::CapabilityReferences
        | K::CapabilityArtifact
        | K::CapabilityArtifactBytes
        | K::CapabilityPolicyReference
        | K::CapabilityTargetProfileReference
        | K::CapabilityReferenceBytes => &[NONE],
        K::ProfileSpecificationDependency => &[&["specification.project-profile.supported"]],
        K::ResourceLimits => &[
            &[
                "resource-policy.artifacts.within-capability",
                "resource-policy.policy-artifacts.within-capability",
                "resource-policy.target-profile-artifacts.within-capability",
                "resource-policy.artifact.within-capability",
                "resource-policy.artifact-bytes.within-capability",
                "resource-policy.policy-reference.within-capability",
            ],
            &["resource-policy.resource-limits.bootstrap-candidate"],
        ],
        K::RootNodes
        | K::RootDepth
        | K::RootCollectionEntries
        | K::RootStringBytes
        | K::RootLongestString
        | K::Profiles
        | K::ProfileId
        | K::Selector
        | K::StructuralAnalysis
        | K::Artifacts
        | K::PolicyArtifacts
        | K::TargetProfileArtifacts
        | K::References
        | K::Artifact
        | K::ArtifactBytes
        | K::PolicyReference
        | K::TargetProfileReference
        | K::ReferenceBytes => &[&["resource-policy.resource-limits.compatible"]],
        K::LocaleCanonicalizationSlot
        | K::LocaleNegotiationSlot
        | K::MessageLocaleFallbackSlot
        | K::CoverageSlot
        | K::IntentSurfaceClassVocabularySlot
        | K::DeliveryGraphSlot
        | K::DeliveryPlacementSlot
        | K::DeclarationField
        | K::LocaleOccurrences
        | K::TargetProfiles
        | K::CollectionGroup
        | K::Groups
        | K::Memberships
        | K::Relations => &[CONFIRMED_POLICY],
        K::Vocabulary => &[
            CONFIRMED_POLICY,
            &[
                "specification.intent-surface-class-vocabulary.supported",
                "specification.vocabulary.intact",
            ],
        ],
        K::CanonicalizationData => &[&["specification.locale-canonicalization.supported"]],
        K::LocaleCanonicalizationDependency => &[&["specification.locale-data.matching"]],
        K::ProjectIdentityDependency => &[&["identity.declaration-field.opaque"]],
        K::RequestedLocale => &[
            &[
                "specification.locale-data.matching",
                "locale.occurrences.within-bound",
            ],
            &["locale.requested.canonical"],
        ],
        K::RequestedLocales => &[&["locale.requested.canonical"]],
        K::DefaultSourceLocale
        | K::NegotiationAliasKey
        | K::NegotiationAliasValue
        | K::MessageFallbackKey => &[&[
            "specification.locale-data.matching",
            "locale.occurrences.within-bound",
        ]],
        K::DefaultRequestedLocale => &[
            &[
                "specification.locale-data.matching",
                "locale.occurrences.within-bound",
            ],
            &[
                "locale.requested.canonical",
                "locale.default-requested.canonical",
            ],
        ],
        K::TargetId => &[&["locale.target-profiles.within-bound"]],
        K::TargetRequestedLocale => &[
            &[
                "specification.locale-data.matching",
                "locale.occurrences.within-bound",
                "locale.target-id.within-bound",
            ],
            &["locale.target-requested.canonical"],
        ],
        K::TargetDefaultRequestedLocale => &[&[
            "specification.locale-data.matching",
            "locale.occurrences.within-bound",
            "locale.target-id.within-bound",
        ]],
        K::ProjectLocalesDependency => &[&[
            "locale.requested.unique",
            "locale.requested-set.within-bound",
            "locale.default-source.canonical",
            "locale.default-requested.member",
        ]],
        K::NegotiationAliases => &[&["specification.locale-negotiation.supported"]],
        K::NegotiationKey => &[&[
            "locale.alias-key.canonical",
            "negotiation.aliases.within-bound",
        ]],
        K::NegotiationDestination => &[&[
            "locale.requested.canonical",
            "locale.alias-destination.canonical",
            "negotiation.aliases.within-bound",
        ]],
        K::FallbackSources => &[&["specification.message-locale-fallback.supported"]],
        K::FallbackSource => &[
            &[
                "locale.fallback-key.canonical",
                "fallback.sources.within-bound",
            ],
            &[
                "locale.requested.canonical",
                "locale.fallback-key.canonical",
                "fallback.sources.within-bound",
            ],
        ],
        K::FallbackSequence => &[&["fallback.source.unique"]],
        K::FallbackCandidate => &[
            &["fallback.sequence.within-bound"],
            &["fallback.candidate.canonical"],
        ],
        K::FallbackLiteral => &[&["fallback.candidate.canonical"]],
        K::FallbackIntentSource => &[&["fallback.sequence.within-bound"]],
        K::CoverageRules | K::CoverageLocaleSelectors | K::CoverageSurfaceSelectors => {
            &[&["specification.coverage.supported"]]
        }
        K::CoverageLocale => &[
            &[
                "specification.locale-data.matching",
                "locale.occurrences.within-bound",
                "coverage.rules.within-bound",
                "coverage.locale-selectors.within-bound",
            ],
            &["coverage.locale.canonical"],
            &["locale.requested.canonical", "coverage.locale.canonical"],
        ],
        K::CoverageSurfaceClass => &[
            &[
                "specification.vocabulary.asserted",
                "coverage.rules.within-bound",
                "coverage.surface-selectors.within-bound",
            ],
            &["coverage.surface-class.vocabulary-member"],
        ],
        K::CoverageRule => &[&[
            "coverage.locale.unique",
            "coverage.locale.member",
            "coverage.surface-class.unique",
        ]],
        K::CoverageDomain => &[&["coverage.rule.normalized-domain"]],
        K::CoverageCells | K::CoverageComparisons => &[&[
            "specification.coverage.supported",
            "specification.vocabulary.asserted",
            "locale.requested.canonical",
        ]],
        K::CoverageCell => &[&[
            "coverage.domain.unique",
            "coverage.cells.within-bound",
            "coverage.comparisons.within-bound",
        ]],
        K::CoverageTable => &[&["coverage.cell.effective-mode"]],
        K::LocaleNegotiationDependency => &[&[
            "negotiation.alias-key.unique",
            "negotiation.destination.member",
        ]],
        K::MessageFallbackDependency => &[&[
            "fallback.source.member",
            "fallback.candidate.distinct-from-source",
            "fallback.literal.unique",
            "fallback.intent-source.unique",
        ]],
        K::CoverageDependency => &[&["coverage.table.complete"]],
        K::CollectionMember => &[CONFIRMED_POLICY, CONFIRMED_POLICY],
        K::ResolvedPolicyReference => &[&[
            "artifact.member.admissible",
            "artifact.member.single-occurrence",
            "artifact.conflict-group.consistent-content",
        ]],
        K::ResolvedTargetProfileReference => &[&[
            "locale.target-id.within-bound",
            "artifact.member.admissible",
            "artifact.member.single-occurrence",
            "artifact.conflict-group.consistent-content",
        ]],
        K::RequiredPoliciesDependency | K::OptionalPoliciesDependency => {
            &[&["artifact.policy-reference.resolved"]]
        }
        K::DeliveryPlacement => &[&[
            "specification.delivery-graph.supported",
            "specification.delivery-placement.supported",
        ]],
        K::DeliveryDependency => &[&["delivery.placement.effective-mode"]],
        K::Target => &[
            &[
                "locale.requested.canonical",
                "locale.target-requested.canonical",
            ],
            &[
                "locale.default-requested.canonical",
                "locale.target-requested.canonical",
                "locale.target-default-requested.canonical",
            ],
            &[
                "locale.target-requested.canonical",
                "artifact.target-profile-reference.resolved",
            ],
        ],
        K::TargetsDependency => &[&[
            "locale.target-requested.unique",
            "target.requested-locales.project-subset",
            "target.effective-default.supported",
            "target.requested-locales.directed",
        ]],
        K::Group => &[&["group.groups.within-bound"]],
        K::GroupMembers => &[&["group.group-id.within-bound"]],
        K::GroupMember => &[
            &[
                "group.memberships.within-bound",
                "group.members.within-bound",
            ],
            &[
                "locale.target-profiles.within-bound",
                "group.member.within-bound",
            ],
            &["group.member.within-bound"],
        ],
        K::Assignment => &[&[
            "locale.target-id.within-bound",
            "group.memberships.within-bound",
            "group.members.within-bound",
        ]],
        K::RelationEndpoint => &[
            &[
                "group.group-id.within-bound",
                "hydration.relations.within-bound",
            ],
            &[
                "locale.target-profiles.within-bound",
                "hydration.endpoint.within-bound",
            ],
            &[
                "group.members.within-bound",
                "hydration.endpoint.within-bound",
            ],
            &[
                "artifact.target-profile-reference.resolved",
                "hydration.endpoint.known",
            ],
        ],
        K::Relation => &[
            &["hydration.endpoint.within-bound"],
            &["hydration.endpoint.within-bound"],
            &[
                "target.requested-locales.project-subset",
                "hydration.endpoint.known",
            ],
            &[
                "target.effective-default.supported",
                "hydration.endpoint.known",
            ],
            &[
                "artifact.target-profile-reference.resolved",
                "hydration.endpoint.known",
            ],
        ],
        K::Roles => &[&["hydration.endpoint.within-bound"]],
        K::DeploymentGroupsDependency => &[&[
            "group.member.known",
            "group.member.unique",
            "group.target.single-group",
            "hydration.endpoint.same-group",
            "hydration.endpoint.role",
            "hydration.relation.distinct-endpoints",
            "hydration.relation.unique",
            "hydration.relation.same-requested-locales",
            "hydration.relation.same-effective-default",
            "hydration.relation.static-compatibility",
            "hydration.target.single-role",
        ]],
        // Declared only: their units come with their phases' Findings.
        K::EntryDependencies
        | K::MaterializedDependencies
        | K::CapabilityStructuralAnalysis
        | K::ProfileKey
        | K::TargetProfileKey
        | K::DeploymentGroupKey
        | K::CapabilityProfiles
        | K::CapabilityProfileId
        | K::DiagnosticDomain => &[NOT_YET],
        K::CompleteInput => &[NOT_YET; 10],
        K::ConfigurationSchemaSelection
        | K::RootField
        | K::ProfileField
        | K::TargetField
        | K::GroupField
        | K::ProfileSelector => &[NOT_YET; 4],
    }
}

/// No Finding: the check only establishes what later checks wait on.
const NO_FINDING: &[Code] = &[];

/// What a bound's check reports past it.
const BOUND: &[Code] = &[Code::ResourceLimitExceeded];

/// What canonicalizing one locale spelling reports: an invalid locale, a
/// spelling that is not canonical, which blocks nothing, or one past
/// `maxLocaleIdentifierBytes`.
const SPELLING: &[Code] = &[
    Code::LocaleInvalid,
    Code::LocaleNonCanonical,
    Code::ResourceLimitExceeded,
];

/// Each kind's Finding codes, one list for each of its checks in the order of
/// [`SubjectKind::checks`], in 015's table order: what the resolver's checks
/// report, and for the kinds declared only, what 015 gives their checks.
#[expect(clippy::too_many_lines, reason = "one list for each of 015's checks")]
const fn finding_codes(kind: SubjectKind) -> &'static [&'static [Code]] {
    use Code as C;
    use SubjectKind as K;
    match kind {
        K::ProjectProfileSlot => &[&[C::SpecificationInputIncomplete, C::SpecificationNotAdmitted]],
        K::EntryDependencies
        | K::MaterializedDependencies
        | K::ProfileSpecificationDependency
        | K::LocaleCanonicalizationDependency
        | K::DeclarationField
        | K::ProjectIdentityDependency
        | K::ProjectLocalesDependency
        | K::CoverageRule
        | K::CoverageTable
        | K::LocaleNegotiationDependency
        | K::MessageFallbackDependency
        | K::CoverageDependency
        | K::RequiredPoliciesDependency
        | K::OptionalPoliciesDependency
        | K::DeliveryPlacement
        | K::DeliveryDependency
        | K::TargetsDependency
        | K::DeploymentGroupsDependency => &[NO_FINDING],
        K::CompleteInput => &[
            &[C::BootstrapLimitExceeded],
            &[C::BootstrapLimitExceeded],
            &[C::JsonInvalid],
            &[C::JsonMemberDuplicate],
            &[C::InputNotJsonCompatible],
            BOUND,
            BOUND,
            BOUND,
            BOUND,
            BOUND,
        ],
        K::ConfigurationSchemaSelection => &[
            &[C::FieldTypeInvalid],
            &[C::RequiredFieldMissing],
            &[C::FieldTypeInvalid],
            &[C::SchemaVersionUnsupported],
        ],
        K::CapabilityStructuralAnalysis
        | K::CapabilityProfiles
        | K::CapabilityProfileId
        | K::CapabilityArtifacts
        | K::CapabilityPolicyArtifacts
        | K::CapabilityTargetProfileArtifacts
        | K::CapabilityReferences
        | K::CapabilityArtifact
        | K::CapabilityArtifactBytes
        | K::CapabilityPolicyReference
        | K::CapabilityTargetProfileReference
        | K::CapabilityReferenceBytes
        | K::RootNodes
        | K::RootDepth
        | K::RootCollectionEntries
        | K::RootStringBytes
        | K::RootLongestString
        | K::Profiles
        | K::ProfileId
        | K::Selector
        | K::StructuralAnalysis
        | K::Artifacts
        | K::PolicyArtifacts
        | K::TargetProfileArtifacts
        | K::References
        | K::Artifact
        | K::ArtifactBytes
        | K::PolicyReference
        | K::TargetProfileReference
        | K::ReferenceBytes
        | K::LocaleOccurrences
        | K::RequestedLocales
        | K::TargetProfiles
        | K::TargetId
        | K::NegotiationAliases
        | K::FallbackSources
        | K::FallbackSequence
        | K::CoverageRules
        | K::CoverageLocaleSelectors
        | K::CoverageSurfaceSelectors
        | K::CoverageCells
        | K::CoverageComparisons
        | K::Groups
        | K::Memberships
        | K::Group
        | K::GroupMembers
        | K::Relations => &[BOUND],
        K::RootField | K::ProfileField | K::TargetField | K::GroupField => &[
            &[C::RequiredFieldMissing],
            &[C::FieldTypeInvalid],
            &[C::FieldValueInvalid],
            &[C::UnknownField],
        ],
        K::ProfileKey | K::TargetProfileKey | K::DeploymentGroupKey => &[&[C::FieldValueInvalid]],
        K::ProfileSelector => &[
            BOUND,
            &[C::SelectorRequired],
            &[C::SelectorInvalid],
            &[C::SelectorUnknown],
        ],
        K::ResourceLimits => &[
            &[
                C::ArtifactReferenceNotAdmitted,
                C::ArtifactCollectionNotAdmitted,
                C::ResourcePolicyNotAdmitted,
            ],
            &[C::ResourcePolicyCapabilityIncompatible],
        ],
        K::LocaleCanonicalizationSlot => &[&[
            C::SpecificationInputIncomplete,
            C::LocaleSpecificationNotAdmitted,
        ]],
        K::LocaleNegotiationSlot => &[&[
            C::SpecificationInputIncomplete,
            C::NegotiationSpecificationNotAdmitted,
        ]],
        K::MessageLocaleFallbackSlot => &[&[
            C::SpecificationInputIncomplete,
            C::MessageFallbackSpecificationNotAdmitted,
        ]],
        K::CoverageSlot => &[&[
            C::SpecificationInputIncomplete,
            C::CoverageSpecificationNotAdmitted,
        ]],
        K::IntentSurfaceClassVocabularySlot => &[&[C::SpecificationInputIncomplete]],
        K::DeliveryGraphSlot | K::DeliveryPlacementSlot => &[&[
            C::SpecificationInputIncomplete,
            C::DeliverySpecificationNotAdmitted,
        ]],
        K::Vocabulary => &[
            &[C::IntentSurfaceClassVocabularyNotAdmitted],
            &[C::IntentSurfaceClassVocabularyNotAdmitted],
        ],
        K::CanonicalizationData => &[&[C::LocaleDataNotAdmitted]],
        K::RequestedLocale | K::TargetRequestedLocale => &[SPELLING, &[C::LocaleDuplicate]],
        K::DefaultSourceLocale
        | K::NegotiationAliasKey
        | K::NegotiationAliasValue
        | K::MessageFallbackKey
        | K::TargetDefaultRequestedLocale => &[SPELLING],
        K::DefaultRequestedLocale => &[SPELLING, &[C::DefaultRequestedLocaleNotInProject]],
        K::NegotiationKey => &[&[C::NegotiationAliasDuplicate]],
        K::NegotiationDestination => &[&[C::NegotiationAliasDestinationNotRequested]],
        K::FallbackSource => &[
            &[C::LocaleDuplicate],
            &[C::MessageFallbackSourceNotRequested],
        ],
        K::FallbackCandidate => &[SPELLING, &[C::MessageFallbackSequenceInvalid]],
        K::FallbackLiteral | K::FallbackIntentSource => &[&[C::MessageFallbackSequenceInvalid]],
        K::CoverageLocale => &[SPELLING, &[C::LocaleDuplicate], &[C::CoverageRuleInvalid]],
        K::CoverageSurfaceClass => &[&[C::CoverageRuleInvalid], &[C::CoverageRuleInvalid]],
        K::CoverageDomain => &[&[C::CoverageRuleDuplicate]],
        K::CoverageCell => &[&[C::CoverageRuleConflict]],
        K::CollectionMember => &[
            &[C::ArtifactCollectionNotAdmitted],
            &[C::ArtifactCollectionNotAdmitted],
        ],
        K::CollectionGroup => &[&[C::ArtifactCollectionNotAdmitted]],
        K::ResolvedPolicyReference | K::ResolvedTargetProfileReference => {
            &[&[C::ArtifactReferenceNotAdmitted]]
        }
        K::Target => &[
            &[C::TargetLocalesNotSubset],
            &[C::TargetDefaultLocaleNotSupported],
            &[C::TargetLocaleDirectionMissing],
        ],
        K::GroupMember => &[BOUND, &[C::GroupMemberUnknown], &[C::GroupMemberDuplicate]],
        K::Assignment => &[&[C::TargetUnassigned, C::TargetAssignedToMultipleGroups]],
        K::RelationEndpoint => &[
            BOUND,
            &[C::HydrationRelationInvalid],
            &[C::HydrationRelationInvalid],
            &[C::HydrationRelationInvalid],
        ],
        K::Relation => &[
            &[C::HydrationRelationInvalid],
            &[C::HydrationRelationInvalid],
            &[C::HydrationLocalePolicyIncompatible],
            &[C::HydrationLocalePolicyIncompatible],
            &[C::HydrationCapabilityIncompatible],
        ],
        K::Roles => &[&[C::HydrationRelationInvalid]],
        K::DiagnosticDomain => &[&[C::ReportingLimitExceeded]],
    }
}

/// The dependency records a kind's one check owns (D15:2510): entry
/// admission's first kind owns every entry slot's, structural admission's
/// first every common and materialized slot's, and each semantic group's
/// owner its group's.
fn owned_dependencies(kind: SubjectKind) -> Vec<OwnedDependency> {
    match kind {
        SubjectKind::EntryDependencies => EntrySlot::ALL.map(OwnedDependency::Entry).to_vec(),
        SubjectKind::MaterializedDependencies => MaterializedSlot::ALL
            .map(OwnedDependency::Materialized)
            .to_vec(),
        _ => SemanticGroup::ALL
            .into_iter()
            .filter(|&group| Subject::SemanticDependency(group).kind() == kind)
            .map(OwnedDependency::Semantic)
            .collect(),
    }
}

/// The ordinary Evidence record kinds a check's units may own, by 015's
/// rules: every unit's evaluation status, in the Entry domain for phases 0
/// and 1 and the Shared one after (D15:177, 1169); where the check reports
/// codes, a Finding in that domain, its Entry-owned occurrences, and the
/// evidence index they refer to (D15:2201, 2379), and in entry admission
/// also the causes of a Shared projection that cannot materialize
/// (D15:1163); both coverage decision records for the decision table
/// (Decision 015-161); and the dependency records the check owns. Reporting
/// finalization owns no ordinary record: its domains' terminal Finding and
/// state take their reserved slots (D15:2402).
fn record_kinds(kind: SubjectKind, codes: &[Code], owned: &[OwnedDependency]) -> Vec<RecordKind> {
    use RecordKind as R;
    let phase = kind.phase();
    if phase == Phase::ReportingFinalization {
        return Vec::new();
    }
    let entry = phase <= Phase::EntryAdmission;
    let mut kinds: BTreeSet<RecordKind> = owned
        .iter()
        .map(|dependency| dependency.record_kind())
        .collect();
    kinds.insert(if entry {
        R::EntryEvaluationStatus
    } else {
        R::SharedEvaluationStatus
    });
    if !codes.is_empty() {
        kinds.extend([
            if entry {
                R::EntryFinding
            } else {
                R::SharedFinding
            },
            R::FindingOccurrence,
            R::EvidenceIndex,
        ]);
        if phase == Phase::EntryAdmission {
            kinds.insert(R::SharedUnavailabilityCause);
        }
    }
    if kind == SubjectKind::CoverageTable {
        kinds.extend([R::CoverageDecisionEvidence, R::CoverageDecisionBasis]);
    }
    kinds.into_iter().collect()
}

/// Helpers for the tests that compare the resolver with the registry.
#[cfg(test)]
pub(crate) mod testing {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::resolver::evaluation::Check;
    use crate::resolver::subject::SubjectKind;

    use crate::resolver::evaluation::Code;

    use super::{check_ids, finding_codes, CheckRegistry};

    /// The Finding codes `check` of `kind` may report.
    pub(crate) fn possible_codes(kind: SubjectKind, check: Check) -> &'static [Code] {
        finding_codes(kind)[rank(kind, check)]
    }

    /// The place of `check` among the checks of `kind`.
    fn rank(kind: SubjectKind, check: Check) -> usize {
        kind.checks()
            .iter()
            .position(|&own| own == check)
            .expect("the kind runs the check")
    }

    /// The check ID of `check` of `kind`.
    pub(crate) fn check_id(kind: SubjectKind, check: Check) -> &'static str {
        check_ids(kind)[rank(kind, check)]
    }

    /// Every row each row of `registry` waits on, through its prerequisites
    /// and theirs.
    pub(crate) fn requirements(registry: &CheckRegistry) -> BTreeMap<&str, BTreeSet<&str>> {
        let prerequisites: BTreeMap<&str, &[String]> = registry
            .rows
            .iter()
            .map(|row| (row.check_id.as_str(), row.prerequisite_check_ids.as_slice()))
            .collect();
        prerequisites
            .keys()
            .map(|&id| {
                let mut found = BTreeSet::new();
                let mut pending = vec![id];
                while let Some(next) = pending.pop() {
                    for prerequisite in prerequisites[next] {
                        if found.insert(prerequisite.as_str()) {
                            pending.push(prerequisite.as_str());
                        }
                    }
                }
                (id, found)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::testing::requirements;
    use super::*;
    use crate::construction::admission::{ConstructionAdmission, DRAFT_7};
    use crate::construction::validator::schema::Schema;
    use crate::resolver::evaluation::Check;

    fn registry() -> CheckRegistry {
        CheckRegistry::revision_zero()
    }

    /// The row of `check` of `kind`.
    fn row(kind: SubjectKind, check: Check) -> CheckRow {
        let rank = kind
            .checks()
            .iter()
            .position(|&own| own == check)
            .unwrap()
            .to_string();
        registry()
            .rows
            .into_iter()
            .find(|row| row.subject_kind == kind && row.check_rank == rank)
            .unwrap()
    }

    #[test]
    fn the_registry_has_one_row_for_each_check_of_each_kind_in_unit_order() {
        let rows = registry().rows;
        let listed: Vec<(SubjectKind, &str)> = SubjectKind::ALL
            .into_iter()
            .flat_map(|kind| {
                kind.checks()
                    .iter()
                    .enumerate()
                    .map(move |(rank, _)| (kind, check_ids(kind)[rank]))
            })
            .collect();
        let written: Vec<(SubjectKind, &str)> = rows
            .iter()
            .map(|row| (row.subject_kind, row.check_id.as_str()))
            .collect();
        assert_eq!(written, listed);
        assert_eq!(rows.len(), 164);
        for kind in SubjectKind::ALL {
            assert_eq!(check_ids(kind).len(), kind.checks().len(), "{kind:?}");
        }
        // Phase, then kind rank, then check rank.
        let order: Vec<(Phase, usize, usize)> = rows
            .iter()
            .map(|row| {
                (
                    row.phase_id,
                    row.subject_kind_rank.parse().unwrap(),
                    row.check_rank.parse().unwrap(),
                )
            })
            .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn check_ids_are_unique_and_each_rule_id_is_its_check_id_under_015() {
        let rows = registry().rows;
        let ids: BTreeSet<&str> = rows.iter().map(|row| row.check_id.as_str()).collect();
        assert_eq!(ids.len(), rows.len());
        for row in &rows {
            assert_eq!(row.rule_id, format!("015.{}", row.check_id));
            assert_eq!(row.check_id.split('.').count(), 3, "{}", row.check_id);
        }
    }

    /// The check IDs 015 gives as examples name the checks they describe
    /// (D15:2167).
    #[test]
    fn the_example_check_ids_of_015_name_their_checks() {
        for (kind, check, id) in [
            (
                SubjectKind::DefaultRequestedLocale,
                Check::RequestedMember,
                "locale.default-requested.member",
            ),
            (
                SubjectKind::Target,
                Check::ProjectSubset,
                "target.requested-locales.project-subset",
            ),
            (
                SubjectKind::Assignment,
                Check::SingleGroup,
                "group.target.single-group",
            ),
            (
                SubjectKind::Relation,
                Check::SameRequestedLocales,
                "hydration.relation.same-requested-locales",
            ),
            (
                SubjectKind::Relation,
                Check::SameEffectiveDefault,
                "hydration.relation.same-effective-default",
            ),
        ] {
            let row = row(kind, check);
            assert_eq!(row.check_id, id);
            assert_eq!(row.phase_id, kind.phase());
        }
    }

    #[test]
    fn a_kinds_id_is_its_name_in_kebab_case_and_a_phases_its_015_id() {
        assert_eq!(kind_id(SubjectKind::DiagnosticDomain), "diagnostic-domain");
        assert_eq!(kind_id(SubjectKind::RelationEndpoint), "relation-endpoint");
        assert_eq!(
            kind_id(SubjectKind::CapabilityTargetProfileReference),
            "capability-target-profile-reference"
        );
        for phase in Phase::ALL {
            assert_eq!(serde_json::to_value(phase).unwrap(), phase.id());
        }
        let ids: BTreeSet<String> = SubjectKind::ALL.into_iter().map(kind_id).collect();
        assert_eq!(ids.len(), SubjectKind::ALL.len());
    }

    /// Every row of one kind agrees on the kind's key shape and rank
    /// (D15:2178), and only a kind keyed by an identity or content projects
    /// it.
    #[test]
    fn the_rows_of_a_kind_share_its_shape_and_only_identities_project_content() {
        let rows = registry().rows;
        for kind in SubjectKind::ALL {
            let own: Vec<&CheckRow> = rows.iter().filter(|row| row.subject_kind == kind).collect();
            let first = own[0];
            for row in &own {
                assert_eq!(row.phase_id, first.phase_id);
                assert_eq!(row.subject_key_strategy, first.subject_key_strategy);
                assert_eq!(
                    row.subject_content_projection,
                    first.subject_content_projection
                );
                assert_eq!(row.owning_scope, first.owning_scope);
                assert_eq!(row.parent_subject, first.parent_subject);
                assert_eq!(row.subject_kind_rank, first.subject_kind_rank);
            }
            let projects = first.subject_content_projection != ContentProjection::None;
            assert_eq!(
                projects,
                first.subject_key_strategy == KeyStrategy::CanonicalIdentityOrContent,
                "{kind:?}"
            );
            assert_ne!(
                first.subject_content_projection,
                ContentProjection::NonSecretContent,
                "{kind:?}"
            );
        }
    }

    /// A parent is a kind keyed by an identity or content, of the same or an
    /// earlier phase (D15:2169), and a singleton is never scoped.
    #[test]
    fn a_parent_is_an_identity_kind_of_the_same_or_an_earlier_phase() {
        let mut parents = 0;
        for kind in SubjectKind::ALL {
            let own = shape(kind);
            let Some(parent) = own.parent else {
                continue;
            };
            parents += 1;
            assert!(parent.phase() <= kind.phase(), "{kind:?}");
            assert_eq!(
                shape(parent).strategy,
                KeyStrategy::CanonicalIdentityOrContent,
                "{kind:?}"
            );
            assert_ne!(own.strategy, KeyStrategy::Singleton, "{kind:?}");
        }
        assert_eq!(parents, 20);
        // The structural key kinds name the `profiles` key kind (D15:2167).
        for kind in [
            SubjectKind::TargetProfileKey,
            SubjectKind::DeploymentGroupKey,
        ] {
            assert_eq!(shape(kind).parent, Some(SubjectKind::ProfileKey));
        }
    }

    /// A singleton's owning scope is its own kind; the others are scopes
    /// shared by the kinds keyed in one field, map, or collection.
    #[test]
    fn a_singletons_scope_is_its_own_kind() {
        for row in registry().rows {
            let singleton = row.subject_key_strategy == KeyStrategy::Singleton;
            assert_eq!(
                singleton,
                row.owning_scope == kind_id(row.subject_kind),
                "{}",
                row.check_id
            );
        }
        let declaration: Vec<SubjectKind> = SubjectKind::ALL
            .into_iter()
            .filter(|&kind| shape(kind).scope == Some("declaration"))
            .collect();
        assert!(declaration
            .iter()
            .all(|&kind| shape(kind).strategy == KeyStrategy::LogicalField
                && kind.phase() > Phase::ProfileSelection));
    }

    /// A parent names the `(phaseId, subjectKind)` of the rows whose subjects
    /// scope the row's own, in the same or an earlier phase (D15:2169).
    #[test]
    fn a_parented_row_names_its_parents_phase_and_kind() {
        let endpoint = row(SubjectKind::RelationEndpoint, Check::KnownEndpoint);
        assert_eq!(
            serde_json::to_value(&endpoint).unwrap(),
            json!({
                "checkId": "hydration.endpoint.known",
                "phaseId": "hydration-validation",
                "ruleId": "015.hydration.endpoint.known",
                "subjectKind": "relation-endpoint",
                "subjectKeyStrategy": "logical-field",
                "subjectContentProjection": "none",
                "owningScope": "relation",
                "parentSubject": {
                    "phaseId": "hydration-validation",
                    "subjectKind": "relation"
                },
                "subjectKindRank": "1",
                "checkRank": "1",
                "prerequisiteCheckIds": [
                    "locale.target-profiles.within-bound",
                    "hydration.endpoint.within-bound"
                ],
                "possibleFindingCodes": ["project-profile-hydration-relation-invalid"],
                "possibleEvidenceRecordKinds": [
                    "finding-occurrence",
                    "evidence-index",
                    "shared-finding",
                    "shared-evaluation-status"
                ],
                "ownedDependencies": []
            })
        );
        // An unscoped row has no parent member at all.
        let table =
            serde_json::to_value(row(SubjectKind::CoverageTable, Check::CompleteTable)).unwrap();
        assert_eq!(table.get("parentSubject"), None);
        assert_eq!(table["owningScope"], "coverage-table");
        // A relation is scoped by its group, whose rows are group
        // resolution's.
        let relation = row(SubjectKind::Relation, Check::DistinctEndpoints);
        assert_eq!(
            serde_json::to_value(relation.parent_subject).unwrap(),
            json!({"phaseId": "group-resolution", "subjectKind": "group"})
        );
        let rows = registry().rows;
        let kinds: BTreeSet<(Phase, SubjectKind)> = rows
            .iter()
            .map(|row| (row.phase_id, row.subject_kind))
            .collect();
        for row in &rows {
            if let Some(parent) = &row.parent_subject {
                assert!(
                    kinds.contains(&(parent.phase_id, parent.subject_kind)),
                    "{}",
                    row.check_id
                );
                assert!(parent.phase_id <= row.phase_id, "{}", row.check_id);
            }
        }
    }

    /// The parent-scoped subjects 015 names (D15:2184), a locale of one
    /// Target ID, an item of one coverage rule, a destination of one alias
    /// key, a member of one group, and a candidate of one fallback sequence,
    /// are scoped by their parents. The candidate is keyed by its position in
    /// a sequence whose order is semantic (D15:2176), and the others, elements
    /// of collections whose order is not, by identity or content (D15:2178).
    #[test]
    fn the_parent_scoped_subjects_015_names_are_elements_under_their_parents() {
        use KeyStrategy::{CanonicalIdentityOrContent as Identity, SemanticPosition};
        use SubjectKind as K;
        for (kind, parent, strategy) in [
            (K::TargetRequestedLocale, K::TargetId, Identity),
            (K::CoverageLocale, K::CoverageRule, Identity),
            (K::CoverageSurfaceClass, K::CoverageRule, Identity),
            (K::NegotiationAliasValue, K::NegotiationAliasKey, Identity),
            (K::NegotiationDestination, K::NegotiationKey, Identity),
            (K::GroupMember, K::Group, Identity),
            (K::FallbackCandidate, K::FallbackSource, SemanticPosition),
        ] {
            let own = shape(kind);
            assert_eq!(own.parent, Some(parent), "{kind:?}");
            assert_eq!(own.strategy, strategy, "{kind:?}");
        }
    }

    /// The prerequisites of `check` of `kind`.
    fn prerequisites(kind: SubjectKind, check: Check) -> Vec<String> {
        row(kind, check).prerequisite_check_ids
    }

    #[test]
    fn a_row_lists_the_checks_its_units_wait_on() {
        use SubjectKind as K;
        // A member is known once its bytes hold and the Target ID map's bound
        // does; it waits on the other bounds through those two.
        assert_eq!(
            prerequisites(K::GroupMember, Check::KnownMember),
            [
                "locale.target-profiles.within-bound",
                "group.member.within-bound"
            ]
        );
        assert_eq!(
            prerequisites(K::DefaultRequestedLocale, Check::RequestedMember),
            [
                "locale.requested.canonical",
                "locale.default-requested.canonical"
            ]
        );
        // Only a rule that normalized to a domain names it.
        assert_eq!(
            prerequisites(K::CoverageDomain, Check::UniqueDomain),
            ["coverage.rule.normalized-domain"]
        );
        // Phase 0, and the bounds before a policy is admitted, wait on nothing.
        for (kind, check) in [
            (K::ProjectProfileSlot, Check::SupportedAssertion),
            (K::CapabilityArtifacts, Check::WithinCapability),
        ] {
            assert!(prerequisites(kind, check).is_empty(), "{kind:?}");
        }
    }

    #[test]
    fn a_check_under_the_admitted_policy_waits_on_every_recheck() {
        let rows = registry().rows;
        let rechecks: Vec<&str> = rows
            .iter()
            .filter(|row| {
                row.phase_id == Phase::ResourcePolicyAdmission
                    && row.check_id.ends_with(".within-bound")
            })
            .map(|row| row.check_id.as_str())
            .collect();
        assert_eq!(rechecks, CONFIRMED_POLICY);
        for row in &rows {
            let listed = row
                .prerequisite_check_ids
                .iter()
                .filter(|id| rechecks.contains(&id.as_str()))
                .count();
            assert!(listed == 0 || listed == rechecks.len(), "{}", row.check_id);
        }
        assert_eq!(
            prerequisites(
                SubjectKind::LocaleNegotiationSlot,
                Check::SupportedAssertion
            ),
            CONFIRMED_POLICY
        );
    }

    #[test]
    fn prerequisites_are_earlier_rows_in_row_order_and_none_waits_on_another() {
        let registry = registry();
        let place: BTreeMap<&str, usize> = registry
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.check_id.as_str(), index))
            .collect();
        let requirements = requirements(&registry);
        for (index, row) in registry.rows.iter().enumerate() {
            let listed: Vec<usize> = row
                .prerequisite_check_ids
                .iter()
                .map(|id| place[id.as_str()])
                .collect();
            // A row of the same kind is listed only with a lower check rank.
            assert!(
                listed.iter().all(|&earlier| earlier < index),
                "{}",
                row.check_id
            );
            assert!(
                listed.windows(2).all(|pair| pair[0] < pair[1]),
                "{}",
                row.check_id
            );
            for first in &row.prerequisite_check_ids {
                for second in &row.prerequisite_check_ids {
                    assert!(
                        !requirements[first.as_str()].contains(second.as_str()),
                        "{} lists {first}, which waits on {second}",
                        row.check_id
                    );
                }
            }
        }
    }

    #[test]
    fn the_kinds_declared_only_list_and_are_listed_as_no_prerequisite_yet() {
        let rows = registry().rows;
        let declared: BTreeSet<&str> = rows
            .iter()
            .filter(|row| row.subject_kind.declared_only())
            .map(|row| row.check_id.as_str())
            .collect();
        assert_eq!(declared.len(), 43);
        for row in &rows {
            if row.subject_kind.declared_only() {
                assert!(row.prerequisite_check_ids.is_empty(), "{}", row.check_id);
            }
            for id in &row.prerequisite_check_ids {
                assert!(
                    !declared.contains(id.as_str()),
                    "{} lists {id}",
                    row.check_id
                );
            }
        }
    }

    /// The Finding codes `check` of `kind` may report.
    fn codes(kind: SubjectKind, check: Check) -> Vec<Code> {
        row(kind, check).possible_finding_codes
    }

    #[test]
    fn a_row_lists_the_codes_its_check_may_report() {
        use Code as C;
        use SubjectKind as K;
        // Canonicalizing a spelling may find it invalid, not canonical, which
        // blocks nothing, or past its byte bound, wherever it is spelled.
        for kind in SubjectKind::ALL {
            if let Some(rank) = kind
                .checks()
                .iter()
                .position(|&own| own == Check::CanonicalLocale)
            {
                assert_eq!(
                    finding_codes(kind)[rank],
                    [
                        C::LocaleInvalid,
                        C::LocaleNonCanonical,
                        C::ResourceLimitExceeded
                    ],
                    "{kind:?}"
                );
            }
        }
        assert_eq!(
            codes(K::Assignment, Check::SingleGroup),
            [C::TargetUnassigned, C::TargetAssignedToMultipleGroups]
        );
        assert_eq!(
            codes(K::ResourceLimits, Check::BootstrapCandidate),
            [
                C::ArtifactReferenceNotAdmitted,
                C::ArtifactCollectionNotAdmitted,
                C::ResourcePolicyNotAdmitted
            ]
        );
        // The kinds declared only report what 015 gives their checks: the raw
        // file's bounds are resolver-side bootstrap bounds (D15:2350), the
        // root's are the policy's (D15:1870), and so is an over-long selector
        // (D15:2290).
        for (kind, check, code) in [
            (
                K::CompleteInput,
                Check::WithinFileBytes,
                C::BootstrapLimitExceeded,
            ),
            (K::CompleteInput, Check::WellFormedJson, C::JsonInvalid),
            (
                K::CompleteInput,
                Check::WithinNodes,
                C::ResourceLimitExceeded,
            ),
            (
                K::ConfigurationSchemaSelection,
                Check::ObjectRoot,
                C::FieldTypeInvalid,
            ),
            (
                K::ConfigurationSchemaSelection,
                Check::SupportedSchemaVersion,
                C::SchemaVersionUnsupported,
            ),
            (K::GroupField, Check::KnownField, C::UnknownField),
            (
                K::ProfileSelector,
                Check::WithinCapability,
                C::ResourceLimitExceeded,
            ),
            (
                K::ProfileSelector,
                Check::DeclaredSelector,
                C::SelectorUnknown,
            ),
            (
                K::DiagnosticDomain,
                Check::CompleteReporting,
                C::ReportingLimitExceeded,
            ),
        ] {
            assert_eq!(codes(kind, check), [code], "{kind:?} {check:?}");
        }
        // A check that only establishes what later checks wait on reports
        // nothing.
        for (kind, check) in [
            (K::EntryDependencies, Check::EstablishedSlots),
            (K::CoverageDependency, Check::EstablishedGroup),
            (K::CoverageRule, Check::NormalizedDomain),
            (K::CoverageTable, Check::CompleteTable),
            (K::DeliveryPlacement, Check::EffectiveMode),
            (K::DeclarationField, Check::OpaqueIdentity),
        ] {
            assert!(codes(kind, check).is_empty(), "{kind:?} {check:?}");
        }
    }

    #[test]
    fn every_015_code_is_some_rows_and_each_row_lists_its_codes_once_in_table_order() {
        let rows = registry().rows;
        let place = |code: &Code| Code::ALL.iter().position(|own| own == code).unwrap();
        let mut reported = BTreeSet::new();
        for row in &rows {
            let places: Vec<usize> = row.possible_finding_codes.iter().map(place).collect();
            assert!(
                places.windows(2).all(|pair| pair[0] < pair[1]),
                "{}",
                row.check_id
            );
            reported.extend(places);
        }
        assert_eq!(reported, (0..Code::ALL.len()).collect());
    }

    /// The record kinds `check` of `kind` may own.
    fn records(kind: SubjectKind, check: Check) -> Vec<RecordKind> {
        row(kind, check).possible_evidence_record_kinds
    }

    #[test]
    fn each_dependency_record_has_one_owner_where_015_puts_it() {
        use OwnedDependency as D;
        let rows = registry().rows;
        let owners: Vec<(&CheckRow, &OwnedDependency)> = rows
            .iter()
            .flat_map(|row| row.owned_dependencies.iter().map(move |owned| (row, owned)))
            .collect();
        assert_eq!(owners.len(), 24);
        let owned: BTreeSet<&OwnedDependency> = owners.iter().map(|&(_, owned)| owned).collect();
        assert_eq!(owned.len(), 24);
        // The first kinds of entry and structural admission own the slots'.
        for (owner, phase, dependencies) in [
            (
                SubjectKind::EntryDependencies,
                Phase::EntryAdmission,
                EntrySlot::ALL.map(D::Entry).to_vec(),
            ),
            (
                SubjectKind::MaterializedDependencies,
                Phase::StructuralAdmission,
                MaterializedSlot::ALL.map(D::Materialized).to_vec(),
            ),
        ] {
            assert_eq!((owner.phase(), owner.rank()), (phase, 0), "{owner:?}");
            assert_eq!(shape(owner).strategy, KeyStrategy::Singleton);
            assert_eq!(
                row(owner, Check::EstablishedSlots).owned_dependencies,
                dependencies
            );
        }
        // Each semantic group's owner owns its group's.
        for group in SemanticGroup::ALL {
            let owner = Subject::SemanticDependency(group).kind();
            assert_eq!(
                row(owner, Check::EstablishedGroup).owned_dependencies,
                [D::Semantic(group)],
                "{group:?}"
            );
        }
    }

    #[test]
    fn a_row_may_own_the_records_015_gives_its_check() {
        use RecordKind as R;
        use SubjectKind as K;
        // A semantic check that reports codes: its Shared Finding and status,
        // and the occurrences and evidence index the Entry domain keeps.
        assert_eq!(
            records(K::GroupMember, Check::KnownMember),
            [
                R::FindingOccurrence,
                R::EvidenceIndex,
                R::SharedFinding,
                R::SharedEvaluationStatus
            ]
        );
        // Phase 0 is in the Entry domain, and entry admission's blocking
        // Findings also explain a Shared projection that cannot materialize.
        assert_eq!(
            records(K::ProjectProfileSlot, Check::SupportedAssertion),
            [
                R::EntryFinding,
                R::EntryEvaluationStatus,
                R::FindingOccurrence,
                R::EvidenceIndex
            ]
        );
        assert_eq!(
            records(K::CompleteInput, Check::WellFormedJson),
            [
                R::EntryFinding,
                R::EntryEvaluationStatus,
                R::FindingOccurrence,
                R::EvidenceIndex,
                R::SharedUnavailabilityCause
            ]
        );
        // The decision table owns both coverage decision records.
        assert_eq!(
            records(K::CoverageTable, Check::CompleteTable),
            [
                R::CoverageDecisionEvidence,
                R::SharedEvaluationStatus,
                R::CoverageDecisionBasis
            ]
        );
        // An owner owns its dependency records and its status.
        assert_eq!(
            records(K::EntryDependencies, Check::EstablishedSlots),
            [R::EntryEvaluationStatus, R::EntryDependency]
        );
        assert_eq!(
            records(K::CoverageDependency, Check::EstablishedGroup),
            [R::SharedEvaluationStatus, R::SemanticDependency]
        );
        // Reporting finalization's records are its domains' reserved ones.
        assert!(records(K::DiagnosticDomain, Check::CompleteReporting).is_empty());
    }

    #[test]
    fn every_record_kind_has_an_owner_check_and_each_row_its_domains_status() {
        use RecordKind as R;
        let rows = registry().rows;
        let owned: BTreeSet<RecordKind> = rows
            .iter()
            .flat_map(|row| row.possible_evidence_record_kinds.iter().copied())
            .collect();
        assert_eq!(owned.len(), 12);
        for row in &rows {
            let kinds = &row.possible_evidence_record_kinds;
            assert!(
                kinds.windows(2).all(|pair| pair[0] < pair[1]),
                "{}",
                row.check_id
            );
            if row.phase_id == Phase::ReportingFinalization {
                continue;
            }
            let entry = row.phase_id <= Phase::EntryAdmission;
            let (status, finding) = if entry {
                (R::EntryEvaluationStatus, R::EntryFinding)
            } else {
                (R::SharedEvaluationStatus, R::SharedFinding)
            };
            assert!(kinds.contains(&status), "{}", row.check_id);
            let reports = !row.possible_finding_codes.is_empty();
            for kind in [finding, R::FindingOccurrence, R::EvidenceIndex] {
                assert_eq!(kinds.contains(&kind), reports, "{} {kind:?}", row.check_id);
            }
        }
    }

    #[test]
    fn the_registry_reads_back_as_its_type() {
        let value = serde_json::to_value(registry()).unwrap();
        assert_eq!(
            serde_json::from_value::<CheckRegistry>(value).unwrap(),
            registry()
        );
    }

    /// The registry as JSON, with `change` made to every row `pick` picks.
    fn changed(pick: impl Fn(&Value) -> bool, change: impl Fn(&mut Value)) -> Value {
        let mut value = serde_json::to_value(registry()).unwrap();
        for row in value["rows"].as_array_mut().unwrap() {
            if pick(row) {
                change(row);
            }
        }
        value
    }

    /// Picks the rows of one kind.
    fn of_kind(id: &'static str) -> impl Fn(&Value) -> bool {
        move |row| row["subjectKind"] == id
    }

    /// Picks the row of one check.
    fn of_check(id: &'static str) -> impl Fn(&Value) -> bool {
        move |row| row["checkId"] == id
    }

    #[test]
    fn construction_reads_the_registry_as_a_closed_graph() {
        let value = serde_json::to_value(registry()).unwrap();
        assert_eq!(CheckRegistry::read(&value), Some(registry()));
        // A registry this type cannot read is not part of one.
        let known = of_check("hydration.endpoint.known");
        for (case, value) in [
            (
                "an unknown kind",
                changed(&known, |row| row["subjectKind"] = json!("endpoint")),
            ),
            (
                "an unknown phase",
                changed(of_kind("relation"), |row| {
                    row["phaseId"] = json!("hydration");
                }),
            ),
            (
                "another member",
                changed(&known, |row| row["note"] = json!("note")),
            ),
            (
                "a rank as a number",
                changed(of_kind("roles"), |row| row["subjectKindRank"] = json!(3)),
            ),
        ] {
            assert_eq!(CheckRegistry::read(&value), None, "{case}");
        }
    }

    #[test]
    fn a_check_id_names_one_row_under_its_own_rule_id() {
        let known = of_check("hydration.endpoint.known");
        for (case, value) in [
            (
                "a repeated check ID",
                changed(&known, |row| {
                    row["checkId"] = json!("hydration.endpoint.within-bound");
                    row["ruleId"] = json!("015.hydration.endpoint.within-bound");
                }),
            ),
            (
                "another check's rule ID",
                changed(&known, |row| {
                    row["ruleId"] = json!("015.hydration.endpoint.within-bound");
                }),
            ),
            (
                "a rule ID outside 015",
                changed(&known, |row| {
                    row["ruleId"] = json!("hydration.endpoint.known");
                }),
            ),
        ] {
            assert_eq!(CheckRegistry::read(&value), None, "{case}");
        }
    }

    /// One row of a kind differs from the kind's other rows in one field,
    /// each sound by itself.
    #[test]
    fn the_rows_of_a_kind_agree_on_its_shape_parent_and_rank() {
        let known = of_check("hydration.endpoint.known");
        for (case, value) in [
            (
                "strategy",
                changed(&known, |row| {
                    row["subjectKeyStrategy"] = json!("canonical-identity-or-content");
                    row["subjectContentProjection"] = json!("redacted-structure");
                }),
            ),
            (
                "projection",
                changed(of_check("hydration.relation.unique"), |row| {
                    row["subjectContentProjection"] = json!("non-secret-content");
                }),
            ),
            (
                "scope",
                changed(&known, |row| row["owningScope"] = json!("relations")),
            ),
            (
                "parent",
                changed(&known, |row| {
                    row["parentSubject"] =
                        json!({ "phaseId": "group-resolution", "subjectKind": "group" });
                }),
            ),
            (
                "rank",
                changed(&known, |row| row["subjectKindRank"] = json!("7")),
            ),
        ] {
            assert_eq!(CheckRegistry::read(&value), None, "{case}");
        }
    }

    #[test]
    fn kinds_rank_uniquely_within_a_phase_and_checks_within_a_kind() {
        for (case, value) in [
            (
                "an endpoint ranked as the relation",
                changed(of_kind("relation-endpoint"), |row| {
                    row["subjectKindRank"] = json!("2");
                }),
            ),
            (
                "a check ranked as another",
                changed(of_check("hydration.endpoint.known"), |row| {
                    row["checkRank"] = json!("0");
                }),
            ),
        ] {
            assert_eq!(CheckRegistry::read(&value), None, "{case}");
        }
        // The ranks need only differ, not count up from zero.
        let spread = changed(of_kind("deployment-groups-dependency"), |row| {
            row["subjectKindRank"] = json!("7");
        });
        assert!(CheckRegistry::read(&spread).is_some());
    }

    #[test]
    fn only_identity_and_content_keys_name_a_projection() {
        for (case, value) in [
            (
                "an identity key without one",
                changed(of_kind("group-member"), |row| {
                    row["subjectContentProjection"] = json!("none");
                }),
            ),
            (
                "a field with one",
                changed(of_kind("relation-endpoint"), |row| {
                    row["subjectContentProjection"] = json!("redacted-structure");
                }),
            ),
            (
                "a singleton with one",
                changed(of_kind("coverage-table"), |row| {
                    row["subjectContentProjection"] = json!("redacted-structure");
                }),
            ),
        ] {
            assert_eq!(CheckRegistry::read(&value), None, "{case}");
        }
        // Which closed projection an identity key names is the registry's
        // choice, which its file digest fixes.
        let disclosed = changed(of_kind("group-member"), |row| {
            row["subjectContentProjection"] = json!("non-secret-content");
        });
        assert!(CheckRegistry::read(&disclosed).is_some());
    }

    #[test]
    fn a_parent_is_a_kind_with_rows_in_the_same_or_an_earlier_phase() {
        for (case, value) in [
            (
                "a parent without rows in its phase",
                changed(of_kind("relation"), |row| {
                    row["parentSubject"] =
                        json!({ "phaseId": "hydration-validation", "subjectKind": "group" });
                }),
            ),
            (
                "a parent of a later phase",
                changed(of_kind("coverage-rule"), |row| {
                    row["parentSubject"] = json!({
                        "phaseId": "hydration-validation",
                        "subjectKind": "relation-endpoint"
                    });
                }),
            ),
            (
                "a scoped singleton",
                changed(of_kind("coverage-table"), |row| {
                    row["parentSubject"] = json!({
                        "phaseId": "locale-policy-resolution",
                        "subjectKind": "coverage-rule"
                    });
                }),
            ),
        ] {
            assert_eq!(CheckRegistry::read(&value), None, "{case}");
        }
    }

    #[test]
    fn no_kind_scopes_itself_through_its_parents() {
        for parent in ["relation", "relation-endpoint"] {
            let value = changed(of_kind("relation"), |row| {
                row["parentSubject"] =
                    json!({ "phaseId": "hydration-validation", "subjectKind": parent });
            });
            assert_eq!(CheckRegistry::read(&value), None, "{parent}");
        }
    }

    /// The registry with `check`'s prerequisites replaced by `ids`.
    fn waiting(check: &'static str, ids: &[&str]) -> Value {
        changed(of_check(check), |row| {
            row["prerequisiteCheckIds"] = json!(ids);
        })
    }

    #[test]
    fn a_prerequisite_names_a_row_once() {
        for (case, ids) in [
            (
                "a check ID no row has",
                ["hydration.endpoint.unknown"].as_slice(),
            ),
            (
                "a check ID twice",
                &[
                    "hydration.endpoint.within-bound",
                    "hydration.endpoint.within-bound",
                ],
            ),
        ] {
            let value = waiting("hydration.endpoint.known", ids);
            assert_eq!(CheckRegistry::read(&value), None, "{case}");
        }
    }

    #[test]
    fn no_prerequisite_follows_its_dependent() {
        for (case, prerequisite) in [
            ("of a later phase", "hydration.relations.within-bound"),
            (
                "of a kind its phase ranks later",
                "group.memberships.within-bound",
            ),
        ] {
            let value = waiting("group.groups.within-bound", &[prerequisite]);
            assert_eq!(CheckRegistry::read(&value), None, "{case}");
        }
        // A row of its own kind may be an earlier subject's check, whatever
        // its rank. (The committed rows already name kinds ranked 9 and 10
        // from one ranked 12, so ranks compare as numbers.)
        let value = waiting(
            "hydration.relation.distinct-endpoints",
            &[
                "hydration.endpoint.within-bound",
                "hydration.relation.unique",
            ],
        );
        assert!(CheckRegistry::read(&value).is_some());
    }

    #[test]
    fn no_row_waits_on_itself_through_its_prerequisites() {
        let unique = "hydration.relation.unique";
        let distinct = "hydration.relation.distinct-endpoints";
        let itself = waiting(unique, &[unique]);
        assert_eq!(CheckRegistry::read(&itself), None);
        let each_other = changed(
            |row| row["checkId"] == unique || row["checkId"] == distinct,
            |row| {
                let other = if row["checkId"] == unique {
                    distinct
                } else {
                    unique
                };
                row["prerequisiteCheckIds"] = json!(["hydration.endpoint.within-bound", other]);
            },
        );
        assert_eq!(CheckRegistry::read(&each_other), None);
    }

    #[test]
    fn each_dependency_record_is_owned_by_one_row_that_lists_its_kind() {
        let coverage = json!({ "recordKind": "semantic-dependency", "id": "coverage" });
        let owner = "dependency.coverage.established";
        for (case, value) in [
            (
                "a second owner",
                changed(of_check("dependency.targets.established"), |row| {
                    row["ownedDependencies"]
                        .as_array_mut()
                        .unwrap()
                        .insert(0, coverage.clone());
                }),
            ),
            (
                "no owner",
                changed(of_check(owner), |row| row["ownedDependencies"] = json!([])),
            ),
            (
                "an owner that may not own its kind",
                changed(of_check(owner), |row| {
                    row["possibleEvidenceRecordKinds"] = json!(["shared-evaluation-status"]);
                }),
            ),
        ] {
            assert_eq!(CheckRegistry::read(&value), None, "{case}");
        }
    }

    #[test]
    fn the_registry_names_what_construction_admits() {
        let admission = ConstructionAdmission::revision_zero();
        let members = &admission.registry_members;
        assert_eq!(members.check_registry.identity, IDENTITY);
        assert_eq!(members.check_registry.revisions, [REVISION]);
        assert_eq!(members.check_registry_schema.identity, SCHEMA_IDENTITY);
        assert_eq!(members.check_registry_schema.revisions, [REVISION]);
    }

    #[test]
    fn the_schema_closes_the_container_every_row_and_the_vocabularies() {
        let document = schema().unwrap();
        assert_eq!(document["$schema"], DRAFT_7);
        let read = Schema::compile(&document).unwrap();
        let oracle = jsonschema::draft7::new(&document).unwrap();
        let registry = serde_json::to_value(registry()).unwrap();
        assert!(read.validate(&registry).valid);
        assert!(oracle.is_valid(&registry));
        let changed = |change: &dyn Fn(&mut Value)| {
            let mut value = registry.clone();
            change(&mut value);
            value
        };
        for (case, value) in [
            ("no rows", json!({ "rows": [] })),
            (
                "another member",
                changed(&|value| value["note"] = json!("note")),
            ),
            (
                "a row's other member",
                changed(&|value| value["rows"][0]["note"] = json!("note")),
            ),
            (
                "a row without its check rank",
                changed(&|value| {
                    value["rows"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("checkRank");
                }),
            ),
            (
                "a check ID of two segments",
                changed(&|value| value["rows"][0]["checkId"] = json!("two.segments")),
            ),
            (
                "a check ID in another case",
                changed(&|value| value["rows"][0]["checkId"] = json!("Upper.case.id")),
            ),
            (
                "a rule ID of another document",
                changed(&|value| value["rows"][0]["ruleId"] = json!("016.a.b.c")),
            ),
            (
                "an unknown phase",
                changed(&|value| value["rows"][0]["phaseId"] = json!("validation")),
            ),
            (
                "an unknown kind",
                changed(&|value| value["rows"][0]["subjectKind"] = json!("no-such-kind")),
            ),
            (
                "an unknown strategy",
                changed(&|value| value["rows"][0]["subjectKeyStrategy"] = json!("position")),
            ),
            (
                "an unknown projection",
                changed(&|value| value["rows"][0]["subjectContentProjection"] = json!("public")),
            ),
            (
                "a scope of two words",
                changed(&|value| value["rows"][0]["owningScope"] = json!("two words")),
            ),
            (
                "a null parent",
                changed(&|value| value["rows"][0]["parentSubject"] = Value::Null),
            ),
            (
                "a parent without its kind",
                changed(&|value| {
                    value["rows"][0]["parentSubject"] = json!({ "phaseId": "group-resolution" });
                }),
            ),
            (
                "a rank with a leading zero",
                changed(&|value| value["rows"][0]["checkRank"] = json!("01")),
            ),
            (
                "a rank as a number",
                changed(&|value| value["rows"][0]["subjectKindRank"] = json!(0)),
            ),
            (
                "a row without its prerequisites",
                changed(&|value| {
                    value["rows"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("prerequisiteCheckIds");
                }),
            ),
            (
                "a prerequisite named twice",
                changed(&|value| {
                    value["rows"][0]["prerequisiteCheckIds"] = json!(["a.b.c", "a.b.c"]);
                }),
            ),
            (
                "a prerequisite that is no check ID",
                changed(&|value| value["rows"][0]["prerequisiteCheckIds"] = json!(["a.b"])),
            ),
            (
                "a row without its codes",
                changed(&|value| {
                    value["rows"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("possibleFindingCodes");
                }),
            ),
            (
                "a code twice",
                changed(&|value| {
                    value["rows"][0]["possibleFindingCodes"] = json!([
                        "project-profile-json-invalid",
                        "project-profile-json-invalid"
                    ]);
                }),
            ),
            (
                "a code 015 does not register",
                changed(&|value| {
                    value["rows"][0]["possibleFindingCodes"] = json!(["project-profile-unknown"]);
                }),
            ),
            (
                "a row without its record kinds",
                changed(&|value| {
                    value["rows"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("possibleEvidenceRecordKinds");
                }),
            ),
            (
                "a record kind 015 does not register",
                changed(&|value| {
                    value["rows"][0]["possibleEvidenceRecordKinds"] = json!(["terminal-state"]);
                }),
            ),
            (
                "a record kind twice",
                changed(&|value| {
                    value["rows"][0]["possibleEvidenceRecordKinds"] =
                        json!(["evidence-index", "evidence-index"]);
                }),
            ),
            (
                "a row without its dependencies",
                changed(&|value| {
                    value["rows"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("ownedDependencies");
                }),
            ),
            (
                "a dependency on another kind's slot",
                changed(&|value| {
                    value["rows"][0]["ownedDependencies"] =
                        json!([{ "recordKind": "semantic-dependency", "id": "entry-source" }]);
                }),
            ),
            (
                "a dependency without its ID",
                changed(&|value| {
                    value["rows"][0]["ownedDependencies"] =
                        json!([{ "recordKind": "entry-dependency" }]);
                }),
            ),
            (
                "a dependency's other member",
                changed(&|value| {
                    value["rows"][0]["ownedDependencies"] = json!([{
                        "recordKind": "entry-dependency",
                        "id": "origins",
                        "note": "note"
                    }]);
                }),
            ),
        ] {
            assert!(!read.validate(&value).valid, "{case}");
            assert!(!oracle.is_valid(&value), "{case}");
        }
        // Whether the rows agree with each other is not the schema's to say.
        for value in [
            changed(&|value| value["rows"][0]["checkRank"] = json!("7")),
            changed(&|value| value["rows"][0]["prerequisiteCheckIds"] = json!(["no.such.check"])),
        ] {
            assert!(read.validate(&value).valid);
            assert!(oracle.is_valid(&value));
        }
    }
}
