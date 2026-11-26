// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The owners of design 015's semantic dependency records (D15:2517,
//! 015-161). Each semantic group of the profile view has one singleton unit in
//! the phase of the last check that establishes the group. The unit ranks
//! after that check's kind and waits on the group's checks, so it is evaluated
//! exactly when the group is established. `profileSpecification`, which
//! `resolver-bootstrap-admission` establishes, is owned in
//! `structural-admission`, because a semantic dependency is a Shared record.
//! An owner finds nothing of its own, and nothing waits on it. Not the records
//! themselves: Resolution Evidence builds them, each under its owner. The
//! materialized and entry dependencies name the closed input slots 015 lists
//! (D15:2508), [`MaterializedSlot`] and [`EntrySlot`].
//!
//! A semantic dependency's `inputs` are fixed per group: the slots the group's
//! values are taken from and the slots 015's dependency paragraphs name as its
//! semantic inputs, never a slot that only decides whether the group is
//! established, which a materialized dependency records (D15:2517; plan 025
//! decisions #241 and #242).
//!
//! The materialized and entry dependencies' owners wait on nothing, so each is
//! evaluated whenever its phase runs. `structural-admission` runs only on a
//! materialized value, and the Resolver Invocation Input Set exists only after
//! every required component succeeds, so every common and materialized slot is
//! established then and its owner names all eight; an entry's `entry-source`,
//! `profile-selector`, and `origins` are established whenever
//! `entry-admission` runs, and so is a programmatic entry's `call-site-source`
//! once a sidecar was admitted beside it (D15:1175, 2517, 3164; plan 025
//! interpretation #246). A file entry's dependencies name its source's
//! identity as Evidence projects it, the Source-content revision its bytes
//! derive, and its locator when present; a programmatic entry's name the
//! Programmatic Entry's identity, the verified Snapshot digest, and its
//! locator, and the call-site source's identity, Call-Site Source revision,
//! and locator. Both name the selector under the final projection authority
//! and the Safe Origin Kind each common and entry input's slot fixes
//! (D15:702-713, 744, 2517). An owner's records take its slots' vocabulary
//! order (D15:2144, 2508; plan 025 interpretation #244).

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::boundary::locator::Locator;
use crate::boundary::source::file_source_revision;
use crate::input_limits::Bound;
use crate::model::ProfileId;
use crate::structural::selection::SelectorInput;

use super::call_site_span;
use super::entry::{ConfigurationEntry, EntryFile};
use super::evaluation::{
    Admitted, Blocking, Check, Evaluation, EvaluationInvariant, Subject, Unit,
};
use super::evidence::{EvidenceInvariant, Source};
use super::evidence_selector::{SelectorEvidence, SelectorProjection};
use super::evidence_source::{PortableSourceIdentity, SourceIdentity};
use super::evidence_token::{EvidenceTokens, Redacted};
use super::origin::SafeOriginKind;
use super::resource_policy::PolicySlot;
use super::snapshot_entry::EntrySnapshot;
use super::token::Specification;

/// One semantic group of the profile view, in the order of 015's semantic
/// inventory. It reads and writes as its semantic group ID.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SemanticGroup {
    ProfileSpecification,
    ProjectIdentity,
    LocaleCanonicalization,
    ProjectLocales,
    LocaleNegotiation,
    MessageFallback,
    Coverage,
    RequiredPolicies,
    OptionalPolicies,
    Targets,
    DeploymentGroups,
    Delivery,
}

impl SemanticGroup {
    /// Every group, in the inventory's order.
    pub(crate) const ALL: [Self; 12] = [
        Self::ProfileSpecification,
        Self::ProjectIdentity,
        Self::LocaleCanonicalization,
        Self::ProjectLocales,
        Self::LocaleNegotiation,
        Self::MessageFallback,
        Self::Coverage,
        Self::RequiredPolicies,
        Self::OptionalPolicies,
        Self::Targets,
        Self::DeploymentGroups,
        Self::Delivery,
    ];

    /// The group's semantic group ID: its member name in the profile view.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::ProfileSpecification => "profileSpecification",
            Self::ProjectIdentity => "projectIdentity",
            Self::LocaleCanonicalization => "localeCanonicalization",
            Self::ProjectLocales => "projectLocales",
            Self::LocaleNegotiation => "localeNegotiation",
            Self::MessageFallback => "messageFallback",
            Self::Coverage => "coverage",
            Self::RequiredPolicies => "requiredPolicies",
            Self::OptionalPolicies => "optionalPolicies",
            Self::Targets => "targets",
            Self::DeploymentGroups => "deploymentGroups",
            Self::Delivery => "delivery",
        }
    }

    /// The input slots the group is derived from, in the vocabulary's order:
    /// none for `profileSpecification`, whose values come from construction,
    /// and for every other group the slots its values are taken from and the
    /// slots 015 names as its semantic inputs (D15:2517, 2542-2574).
    pub(crate) const fn inputs(self) -> &'static [MaterializedSlot] {
        use MaterializedSlot::{
            ArtifactCollection as Collection, ConfigurationValue as Value,
            IntentSurfaceClassVocabulary as Vocabulary, LocaleCanonicalizationData as Data,
            SpecificationInput as Specification,
        };
        match self {
            Self::ProfileSpecification => &[],
            Self::ProjectIdentity => &[Value],
            Self::LocaleCanonicalization => &[Specification, Data],
            Self::ProjectLocales
            | Self::LocaleNegotiation
            | Self::MessageFallback
            | Self::Targets
            | Self::DeploymentGroups => &[Specification, Data, Collection, Value],
            Self::Coverage => &[Specification, Vocabulary, Data, Collection, Value],
            Self::RequiredPolicies | Self::OptionalPolicies => &[Collection, Value],
            Self::Delivery => &[Specification, Value],
        }
    }

    /// The view of the group's semantic dependency, `{ group, inputs }`
    /// (D15:2511).
    pub(crate) fn view(self) -> Value {
        json!({ "group": self.id(), "inputs": self.inputs() })
    }
}

/// The group whose semantic dependency `unit` owns, if it owns one.
pub(crate) fn owned(unit: &Unit) -> Option<SemanticGroup> {
    match unit.subject {
        Subject::SemanticDependency(group) if *unit == self::unit(group) => Some(group),
        _ => None,
    }
}

/// A common or materialized input slot, which a materialized dependency
/// names. Each reads and writes as its slot ID.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MaterializedSlot {
    SpecificationInput,
    IntentSurfaceClassVocabulary,
    LocaleCanonicalizationData,
    ArtifactCollection,
    VerificationInput,
    ImplementationCapability,
    ConfigurationValue,
    ProfileSelector,
}

impl MaterializedSlot {
    /// Every slot, the six common ones first, in 015's order.
    pub(crate) const ALL: [Self; 8] = [
        Self::SpecificationInput,
        Self::IntentSurfaceClassVocabulary,
        Self::LocaleCanonicalizationData,
        Self::ArtifactCollection,
        Self::VerificationInput,
        Self::ImplementationCapability,
        Self::ConfigurationValue,
        Self::ProfileSelector,
    ];

    /// The slot's ID (D15:2508).
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::SpecificationInput => "specification-input",
            Self::IntentSurfaceClassVocabulary => "intent-surface-class-vocabulary",
            Self::LocaleCanonicalizationData => "locale-canonicalization-data",
            Self::ArtifactCollection => "artifact-collection",
            Self::VerificationInput => "verification-input",
            Self::ImplementationCapability => "implementation-capability",
            Self::ConfigurationValue => "configuration-value",
            Self::ProfileSelector => "profile-selector",
        }
    }

    /// The view of the slot's materialized dependency, `{ slot }` (D15:2512).
    pub(crate) fn view(self) -> Value {
        json!({ "slot": self.id() })
    }

    /// The Safe Origin Kind of a common slot, which every entry fixes alike,
    /// or none for a materialized slot, whose origin depends on the entry.
    const fn common_origin(self) -> Option<SafeOriginKind> {
        match self {
            Self::SpecificationInput => Some(SafeOriginKind::ProfileResolutionSpecificationInput),
            Self::IntentSurfaceClassVocabulary => {
                Some(SafeOriginKind::IntentSurfaceClassVocabulary)
            }
            Self::LocaleCanonicalizationData => Some(SafeOriginKind::LocaleCanonicalizationData),
            Self::ArtifactCollection => Some(SafeOriginKind::ProfileResolutionArtifactCollection),
            Self::VerificationInput => Some(SafeOriginKind::ResourceLimitPolicyVerification),
            Self::ImplementationCapability => Some(SafeOriginKind::ImplementationCapability),
            Self::ConfigurationValue | Self::ProfileSelector => None,
        }
    }
}

/// An entry input slot, which an entry dependency names. Each reads and
/// writes as its slot ID.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EntrySlot {
    EntrySource,
    CallSiteSource,
    ProfileSelector,
    Origins,
}

impl EntrySlot {
    /// Every slot, in 015's order.
    pub(crate) const ALL: [Self; 4] = [
        Self::EntrySource,
        Self::CallSiteSource,
        Self::ProfileSelector,
        Self::Origins,
    ];

    /// The slot's ID (D15:2508).
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::EntrySource => "entry-source",
            Self::CallSiteSource => "call-site-source",
            Self::ProfileSelector => "profile-selector",
            Self::Origins => "origins",
        }
    }
}

/// What an entry's dependency records name besides their slots: the entry
/// source's identity as Evidence projects it, its revision, and its locator
/// when present, the same of the call-site source beside a programmatic
/// entry's admitted sidecar, the selector as the final authority projects it,
/// and the Safe Origin Kinds the entry fixes (D15:205, 2513, 2517).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryInputs {
    source: SourceInputs,
    call_site: Option<SourceInputs>,
    selector: SelectorProjection,
    origins: EntryOrigins,
}

impl EntryInputs {
    /// The inputs of the file entry `file`, whose selector `selector` the
    /// final projection authority `authority` projects beside the
    /// independently admitted declared Profile IDs `declared` (D15:807-820).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves the source identity or the
    /// selector without a projection.
    pub(crate) fn file<'d>(
        file: &EntryFile<'_>,
        selector: &SelectorInput,
        authority: Bound,
        declared: impl IntoIterator<Item = &'d ProfileId>,
    ) -> Result<Self, EvidenceInvariant> {
        Ok(Self {
            source: SourceInputs::project(
                file.origin(),
                file.source,
                &file_source_revision(&file.bytes),
            )?,
            call_site: None,
            selector: SelectorProjection::project(
                file.selector_origin(),
                selector,
                authority,
                declared,
            )
            .map_err(EvidenceInvariant::Selector)?,
            origins: EntryOrigins::of(file),
        })
    }

    /// The inputs of the programmatic entry `entry`, its selector projected
    /// as a file's: the Programmatic Entry's source, whose revision is the
    /// verified Snapshot digest, and, when a sidecar was admitted beside it,
    /// the call-site source at its Call-Site Source revision, both supplied
    /// under the `configuration-programmatic` slot (D15:205, 744, 2517).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves a source identity or the selector
    /// without a projection.
    pub(crate) fn programmatic<'d>(
        entry: &EntrySnapshot<'_>,
        selector: &SelectorInput,
        authority: Bound,
        declared: impl IntoIterator<Item = &'d ProfileId>,
    ) -> Result<Self, EvidenceInvariant> {
        use SafeOriginKind::{ConfigurationProgrammatic, ProfileSelectorProgrammatic};

        let call_site = entry
            .call_site
            .map(|call_site| {
                let revision = call_site.pair().revision();
                SourceInputs::project(
                    ConfigurationProgrammatic,
                    call_site_span::source(call_site),
                    revision,
                )
            })
            .transpose()?;
        Ok(Self {
            source: SourceInputs::project(
                ConfigurationProgrammatic,
                entry.source,
                entry.snapshot.digest(),
            )?,
            selector: SelectorProjection::project(
                ProfileSelectorProgrammatic,
                selector,
                authority,
                declared,
            )
            .map_err(EvidenceInvariant::Selector)?,
            origins: EntryOrigins {
                entry_source: ConfigurationProgrammatic,
                call_site_source: call_site.as_ref().map(|_| ConfigurationProgrammatic),
                profile_selector: ProfileSelectorProgrammatic,
            },
            call_site,
        })
    }

    /// The components that tokens replace: an unmarked source identity of
    /// either source and a redacted selector (plan 025 interpretation #245).
    pub(crate) fn redacted(&self) -> impl Iterator<Item = &Redacted> {
        std::iter::once(&self.source)
            .chain(&self.call_site)
            .filter_map(|source| source.identity.redacted())
            .chain(self.selector.redacted())
    }

    /// The metadata work units of the records' projections: one for each
    /// source identity and each locator present, and one for the selector
    /// (D15:1840, 1852; plan 025 interpretation #245).
    pub(crate) fn work(&self) -> u64 {
        std::iter::once(&self.source)
            .chain(&self.call_site)
            .map(|source| 1 + u64::from(source.locator.is_some()))
            .sum::<u64>()
            + 1
    }

    /// The entry dependency records, three or, beside a sidecar, four, in
    /// their slots' order, `None` when `tokens` was not assigned one of their
    /// components (D15:2508; plan 025 interpretation #244).
    pub(crate) fn retained(
        &self,
        specification: Specification<'_>,
        tokens: &EvidenceTokens,
    ) -> Option<Vec<EntryDependency>> {
        let mut records = vec![EntryDependency::Source(
            self.source.retained(specification, tokens)?,
        )];
        if let Some(call_site) = &self.call_site {
            records.push(EntryDependency::CallSite(
                call_site.retained(specification, tokens)?,
            ));
        }
        records.push(EntryDependency::Selector(
            self.selector.retained(specification, tokens)?,
        ));
        records.push(EntryDependency::Origins(self.origins));
        Some(records)
    }
}

/// What an `entry-source` or `call-site-source` record names: its source's
/// identity as Evidence projects it, its revision, and its locator when
/// present (D15:2517).
#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceInputs {
    identity: SourceIdentity,
    revision: Arc<str>,
    locator: Option<Locator>,
}

impl SourceInputs {
    /// `source`, which an input of `origin` supplied, at `revision`.
    fn project(
        origin: SafeOriginKind,
        source: Source<'_>,
        revision: &str,
    ) -> Result<Self, EvidenceInvariant> {
        Ok(Self {
            identity: SourceIdentity::project(origin, source.identity, source.marked)
                .map_err(EvidenceInvariant::Token)?,
            revision: Arc::from(revision),
            locator: source.locator.cloned(),
        })
    }

    /// The record, `None` when `tokens` was not assigned its identity's
    /// component.
    fn retained(
        &self,
        specification: Specification<'_>,
        tokens: &EvidenceTokens,
    ) -> Option<SourceDependency> {
        Some(SourceDependency {
            source: self.identity.retained(specification, tokens)?,
            revision: Arc::clone(&self.revision),
            locator: self.locator.clone(),
        })
    }
}

/// The Safe Origin Kinds an entry fixes for its `entry-source`,
/// `call-site-source`, and `profile-selector` slots, the call-site source's
/// only beside an admitted sidecar (D15:205, 2517).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EntryOrigins {
    entry_source: SafeOriginKind,
    call_site_source: Option<SafeOriginKind>,
    profile_selector: SafeOriginKind,
}

impl EntryOrigins {
    /// The Safe Origin Kinds `entry` fixes without a sidecar.
    pub(crate) fn of(entry: &dyn ConfigurationEntry) -> Self {
        Self {
            entry_source: entry.origin(),
            call_site_source: None,
            profile_selector: entry.selector_origin(),
        }
    }

    /// The Safe Origin Kind fixed for `slot`, none for a slot without an
    /// input.
    const fn fixed(self, slot: EntrySlot) -> Option<SafeOriginKind> {
        match slot {
            EntrySlot::EntrySource => Some(self.entry_source),
            EntrySlot::CallSiteSource => self.call_site_source,
            EntrySlot::ProfileSelector => Some(self.profile_selector),
            EntrySlot::Origins => None,
        }
    }
}

/// A retained `entry-source` or `call-site-source` record's source view,
/// revision, and locator when present (D15:2517).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceDependency {
    source: PortableSourceIdentity,
    revision: Arc<str>,
    locator: Option<Locator>,
}

/// One retained entry dependency record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EntryDependency {
    Source(SourceDependency),
    CallSite(SourceDependency),
    Selector(SelectorEvidence),
    Origins(EntryOrigins),
}

impl EntryDependency {
    /// The record's entry slot.
    pub(crate) const fn slot(&self) -> EntrySlot {
        match self {
            Self::Source(_) => EntrySlot::EntrySource,
            Self::CallSite(_) => EntrySlot::CallSiteSource,
            Self::Selector(_) => EntrySlot::ProfileSelector,
            Self::Origins(_) => EntrySlot::Origins,
        }
    }

    /// `{ slot, source, revision, locator? }`, `{ slot, selector }`, or
    /// `{ slot, origins }`, where `origins` maps each common input's slot ID
    /// to the Safe Origin Kind its slot fixes and each entry input's to the
    /// one the entry fixes (D15:205, 2513-2514, 2517, 3162).
    pub(crate) fn view(&self) -> Value {
        let mut view = Map::new();
        view.insert("slot".to_owned(), json!(self.slot().id()));
        match self {
            Self::Source(record) | Self::CallSite(record) => {
                view.insert("source".to_owned(), record.source.view());
                view.insert("revision".to_owned(), json!(record.revision.as_ref()));
                if let Some(locator) = &record.locator {
                    view.insert("locator".to_owned(), locator.view());
                }
            }
            Self::Selector(selector) => {
                view.insert("selector".to_owned(), selector.view());
            }
            Self::Origins(entry) => {
                let common = MaterializedSlot::ALL
                    .into_iter()
                    .filter_map(|slot| Some((slot.id(), slot.common_origin()?)));
                let fixed = EntrySlot::ALL
                    .into_iter()
                    .filter_map(|slot| Some((slot.id(), entry.fixed(slot)?)));
                let origins: Map<String, Value> = common
                    .chain(fixed)
                    .map(|(slot, origin)| (slot.to_owned(), json!(origin.id())))
                    .collect();
                view.insert("origins".to_owned(), Value::Object(origins));
            }
        }
        Value::Object(view)
    }
}

/// The `policies` slots whose references the `requiredPolicies` group holds.
pub(crate) const REQUIRED_POLICIES: [PolicySlot; 5] = [
    PolicySlot::ResourceLimits,
    PolicySlot::Trust,
    PolicySlot::SourceAdmission,
    PolicySlot::Approval,
    PolicySlot::Selection,
];

/// The `policies` slots the `optionalPolicies` group holds, each an explicit
/// `null` or a reference.
pub(crate) const OPTIONAL_POLICIES: [PolicySlot; 2] =
    [PolicySlot::ProviderRouting, PolicySlot::GlossarySet];

/// The unit that owns `group`'s semantic dependency, in its kind's phase.
pub(crate) const fn unit(group: SemanticGroup) -> Unit {
    let subject = Subject::SemanticDependency(group);
    Unit {
        phase: subject.kind().phase(),
        subject,
        check: Check::EstablishedGroup,
    }
}

/// Records the owner of `group`'s semantic dependency, which waits on
/// `established`: the result of every check that establishes the group.
pub(crate) fn own(
    evaluation: &mut Evaluation,
    group: SemanticGroup,
    established: Admitted<()>,
) -> Result<(), EvaluationInvariant> {
    evaluation
        .check(unit(group), established, Ok::<(), Blocking>)
        .map(|_owned| ())
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::sync::{Arc, LazyLock};

    use serde_json::json;

    use crate::boundary::locator::{read_file_locator, Locator, LocatorCandidate, LocatorVariant};
    use crate::input_limits::Bound;
    use crate::model::ProfileId;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::entry::EntryFile;
    use crate::resolver::limits::fixture_limits;
    use crate::structural::selection::SelectorInput;

    use super::EntryInputs;

    /// The final projection authority of the test entries' selectors.
    pub(crate) fn authority() -> Bound {
        Bound::new(64).unwrap()
    }

    /// `app`, the one declared Profile ID of the test entries.
    pub(crate) fn declared() -> Vec<ProfileId> {
        vec![serde_json::from_value(json!("app")).unwrap()]
    }

    /// The selector `text`, or an absent one, under [`authority`].
    pub(crate) fn selector(text: Option<&str>) -> SelectorInput {
        text.map_or_else(
            || SelectorInput::absent(authority()),
            |text| SelectorInput::string(text, authority()),
        )
    }

    /// The entry inputs of `file` with the selector `text`, or an absent
    /// one.
    pub(crate) fn inputs(file: &EntryFile<'_>, text: Option<&str>) -> EntryInputs {
        EntryInputs::file(file, &selector(text), authority(), &declared()).unwrap()
    }

    /// The non-secret relative-path locator of a test file.
    pub(crate) fn locator() -> Locator {
        read_file_locator(
            Some(LocatorCandidate {
                variant: LocatorVariant::RelativePath,
                payload: "storefront/intlify.config.json",
                non_secret: true,
            }),
            Bound::new(u64::MAX - 1).unwrap(),
        )
        .unwrap()
    }

    /// The entry inputs of a test file of `{}`, whose owner marks its source
    /// identity non-secret, without a locator, and with an absent selector:
    /// nothing in them is redacted.
    pub(crate) fn file_entry() -> &'static EntryInputs {
        static ENTRY: LazyLock<EntryInputs> = LazyLock::new(|| {
            let fixture = FileFixture::new(&fixture_limits(100));
            inputs(&fixture.file(Arc::from(&b"{}"[..])), None)
        });
        &ENTRY
    }

    /// The entry inputs of a test file of `{}`, whose owner did not mark its
    /// source identity, with a locator, and with a selector that names no
    /// declared profile: the source identity and the selector are redacted.
    pub(crate) fn redacted_entry() -> EntryInputs {
        let fixture = FileFixture::new(&fixture_limits(100));
        let locator = locator();
        let mut file = fixture.file(Arc::from(&b"{}"[..]));
        file.source.marked = false;
        file.source.locator = Some(&locator);
        inputs(&file, Some("other"))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::fixtures::{authority, declared, inputs, locator, redacted_entry, selector};
    use super::*;
    use crate::boundary::locator::{read_programmatic_locator, LocatorCandidate, LocatorVariant};
    use crate::boundary::snapshot::AdmittedSnapshot;
    use crate::boundary::source::call_site_source_revision;
    use crate::resolver::call_site_span::fixtures::{marked_as, sidecar, CALL_SITE};
    use crate::resolver::entry::fixtures::{FileFixture, Relabeled, FILE};
    use crate::resolver::evaluation::{Code, Phase, Status};
    use crate::resolver::evidence_source::hex;
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::snapshot_entry::fixtures::{SnapshotFixture, ENTRY};
    use crate::resolver::subject::SubjectKind;
    use crate::resolver::targets::{self, TargetSubject};
    use crate::resolver::token::Sha256;
    use crate::snapshot::from_json;

    #[test]
    fn groups_and_slots_read_and_write_as_015_spells_them() {
        for group in SemanticGroup::ALL {
            let value = serde_json::to_value(group).unwrap();
            assert_eq!(value, group.id());
            assert_eq!(
                serde_json::from_value::<SemanticGroup>(value).unwrap(),
                group
            );
        }
        assert_eq!(
            serde_json::to_value(MaterializedSlot::ALL).unwrap(),
            json!([
                "specification-input",
                "intent-surface-class-vocabulary",
                "locale-canonicalization-data",
                "artifact-collection",
                "verification-input",
                "implementation-capability",
                "configuration-value",
                "profile-selector"
            ])
        );
        assert_eq!(
            serde_json::to_value(EntrySlot::ALL).unwrap(),
            json!([
                "entry-source",
                "call-site-source",
                "profile-selector",
                "origins"
            ])
        );
    }

    #[test]
    fn each_group_is_owned_in_the_phase_of_its_last_establishing_check() {
        let phases = SemanticGroup::ALL.map(|group| (group, unit(group).phase));
        assert_eq!(
            phases,
            [
                // Phase 0 establishes it, but a semantic dependency is Shared.
                (
                    SemanticGroup::ProfileSpecification,
                    Phase::StructuralAdmission
                ),
                (
                    SemanticGroup::ProjectIdentity,
                    Phase::ProjectIdentityResolution
                ),
                (
                    SemanticGroup::LocaleCanonicalization,
                    Phase::SpecificationAdmission
                ),
                (SemanticGroup::ProjectLocales, Phase::LocaleResolution),
                (
                    SemanticGroup::LocaleNegotiation,
                    Phase::LocalePolicyResolution
                ),
                (
                    SemanticGroup::MessageFallback,
                    Phase::LocalePolicyResolution
                ),
                (SemanticGroup::Coverage, Phase::LocalePolicyResolution),
                (
                    SemanticGroup::RequiredPolicies,
                    Phase::ArtifactReferenceResolution
                ),
                (
                    SemanticGroup::OptionalPolicies,
                    Phase::ArtifactReferenceResolution
                ),
                (SemanticGroup::Targets, Phase::TargetResolution),
                (SemanticGroup::DeploymentGroups, Phase::HydrationValidation),
                (SemanticGroup::Delivery, Phase::DeliveryResolution),
            ]
        );
    }

    /// An owner ranks after every kind of its phase, so it follows the kinds
    /// of the checks that establish its group; one phase's owners keep the
    /// inventory's order.
    #[test]
    fn the_owners_come_last_in_their_phase_in_inventory_order() {
        for phase in Phase::ALL {
            let kinds: Vec<SubjectKind> = SubjectKind::ALL
                .into_iter()
                .filter(|kind| kind.phase() == phase)
                .collect();
            let owners: Vec<SubjectKind> = SemanticGroup::ALL
                .into_iter()
                .map(|group| unit(group).subject.kind())
                .filter(|kind| kind.phase() == phase)
                .collect();
            assert!(kinds.ends_with(&owners), "{phase:?}");
        }
    }

    #[test]
    fn the_ids_are_the_profile_views_members_in_inventory_order() {
        assert!(SemanticGroup::ALL.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(
            SemanticGroup::ALL.map(SemanticGroup::id),
            [
                "profileSpecification",
                "projectIdentity",
                "localeCanonicalization",
                "projectLocales",
                "localeNegotiation",
                "messageFallback",
                "coverage",
                "requiredPolicies",
                "optionalPolicies",
                "targets",
                "deploymentGroups",
                "delivery",
            ]
        );
    }

    #[test]
    fn the_two_policy_groups_split_the_policy_slots() {
        let mut slots: Vec<PolicySlot> = REQUIRED_POLICIES
            .into_iter()
            .chain(OPTIONAL_POLICIES)
            .collect();
        slots.sort();
        assert_eq!(
            slots,
            [
                PolicySlot::Approval,
                PolicySlot::GlossarySet,
                PolicySlot::ProviderRouting,
                PolicySlot::ResourceLimits,
                PolicySlot::Selection,
                PolicySlot::SourceAdmission,
                PolicySlot::Trust,
            ]
        );
    }

    #[test]
    fn an_owner_is_evaluated_exactly_when_its_group_is_established() {
        // The test makes the target's result up, without what its check
        // observes.
        let mut evaluation = Evaluation::fabricated();
        // A target whose locales are not the project's leaves the targets
        // group unestablished.
        let id = serde_json::from_value(json!("browser")).unwrap();
        let subset = targets::unit(TargetSubject::Target(id), Check::ProjectSubset);
        let failed = evaluation
            .check(subset, Ok(()), |()| {
                Err::<(), _>(Blocking {
                    code: Code::TargetLocalesNotSubset,
                    reason: None,
                    details: None,
                })
            })
            .unwrap();
        let causes = failed.clone().unwrap_err();
        own(&mut evaluation, SemanticGroup::Targets, failed).unwrap();
        own(&mut evaluation, SemanticGroup::Delivery, Ok(())).unwrap();
        assert_eq!(
            evaluation.status(&unit(SemanticGroup::Targets)),
            Some(&Status::NotEvaluated(causes))
        );
        assert_eq!(
            evaluation.status(&unit(SemanticGroup::Delivery)),
            Some(&Status::Evaluated)
        );
        // An owner reports nothing of its own.
        assert_eq!(evaluation.issues().count(), 1);
        assert_eq!(evaluation.notices().count(), 0);
        // And owns its group once.
        assert_eq!(
            own(&mut evaluation, SemanticGroup::Delivery, Ok(())),
            Err(EvaluationInvariant::UnitRecordedTwice(unit(
                SemanticGroup::Delivery
            )))
        );
    }

    /// Each group's semantic dependency names the slots 015 derives it from,
    /// in the vocabulary's order, and never a slot that only gates it (plan
    /// 025 decisions #241 and #242).
    #[test]
    fn each_group_names_its_fixed_inputs() {
        let views = SemanticGroup::ALL.map(SemanticGroup::view);
        let common = [
            "specification-input",
            "locale-canonicalization-data",
            "artifact-collection",
            "configuration-value",
        ];
        assert_eq!(
            views,
            [
                json!({ "group": "profileSpecification", "inputs": [] }),
                json!({ "group": "projectIdentity", "inputs": ["configuration-value"] }),
                json!({
                    "group": "localeCanonicalization",
                    "inputs": ["specification-input", "locale-canonicalization-data"],
                }),
                json!({ "group": "projectLocales", "inputs": common }),
                json!({ "group": "localeNegotiation", "inputs": common }),
                json!({ "group": "messageFallback", "inputs": common }),
                json!({
                    "group": "coverage",
                    "inputs": [
                        "specification-input",
                        "intent-surface-class-vocabulary",
                        "locale-canonicalization-data",
                        "artifact-collection",
                        "configuration-value",
                    ],
                }),
                json!({
                    "group": "requiredPolicies",
                    "inputs": ["artifact-collection", "configuration-value"],
                }),
                json!({
                    "group": "optionalPolicies",
                    "inputs": ["artifact-collection", "configuration-value"],
                }),
                json!({ "group": "targets", "inputs": common }),
                json!({ "group": "deploymentGroups", "inputs": common }),
                json!({
                    "group": "delivery",
                    "inputs": ["specification-input", "configuration-value"],
                }),
            ]
        );
        for group in SemanticGroup::ALL {
            let inputs = group.inputs();
            assert!(inputs.windows(2).all(|pair| pair[0] < pair[1]), "{group:?}");
            for gate in [
                MaterializedSlot::VerificationInput,
                MaterializedSlot::ImplementationCapability,
                MaterializedSlot::ProfileSelector,
            ] {
                assert!(!inputs.contains(&gate), "{group:?}");
            }
        }
    }

    /// Only a group's owner owns its semantic dependency.
    #[test]
    fn only_a_groups_owner_owns_its_semantic_dependency() {
        for group in SemanticGroup::ALL {
            assert_eq!(owned(&unit(group)), Some(group));
            let other = Unit {
                check: Check::EstablishedSlots,
                ..unit(group)
            };
            assert_eq!(owned(&other), None);
        }
        let materialized = Unit {
            phase: Phase::StructuralAdmission,
            subject: Subject::MaterializedDependencies,
            check: Check::EstablishedSlots,
        };
        assert_eq!(owned(&materialized), None);
    }

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// Each slot's ID is the one it reads and writes as, and a materialized
    /// dependency's view is its slot (D15:2508, 2512).
    #[test]
    fn each_slot_is_its_id() {
        for slot in MaterializedSlot::ALL {
            assert_eq!(serde_json::to_value(slot).unwrap(), slot.id());
            assert_eq!(slot.view(), json!({ "slot": slot.id() }));
        }
        for slot in EntrySlot::ALL {
            assert_eq!(serde_json::to_value(slot).unwrap(), slot.id());
        }
    }

    /// A file entry's dependencies name its marked source's identity, the
    /// Source-content revision of its bytes, the selector that names a
    /// declared Profile ID, and the origin each common and entry input's slot
    /// fixes (D15:702-713, 744, 808, 2517, 3162).
    #[test]
    fn the_dependencies_of_a_file_entry_name_its_source_selector_and_origins() {
        let fixture = FileFixture::new(&fixture_limits(100));
        let bytes: Arc<[u8]> = Arc::from(&br#"{"profiles":{}}"#[..]);
        let entry = inputs(&fixture.file(Arc::clone(&bytes)), Some("app"));
        assert_eq!(entry.work(), 2);
        assert_eq!(entry.redacted().count(), 0);
        let tokens = EvidenceTokens::assign(PROFILE, entry.redacted(), &Sha256);
        let records = entry.retained(PROFILE, &tokens).unwrap();
        let slots: Vec<EntrySlot> = records.iter().map(EntryDependency::slot).collect();
        assert_eq!(
            slots,
            [
                EntrySlot::EntrySource,
                EntrySlot::ProfileSelector,
                EntrySlot::Origins
            ]
        );
        let views: Vec<Value> = records.iter().map(EntryDependency::view).collect();
        assert_eq!(
            views,
            [
                json!({
                    "slot": "entry-source",
                    "source": { "safe-opaque": hex(FILE.as_bytes()) },
                    "revision": file_source_revision(&bytes),
                }),
                json!({ "slot": "profile-selector", "selector": { "safe-profile-id": "app" } }),
                json!({
                    "slot": "origins",
                    "origins": {
                        "artifact-collection": "profile-resolution-artifact-collection",
                        "entry-source": "configuration-file",
                        "implementation-capability": "implementation-capability",
                        "intent-surface-class-vocabulary": "intent-surface-class-vocabulary",
                        "locale-canonicalization-data": "locale-canonicalization-data",
                        "profile-selector": "profile-selector-cli",
                        "specification-input": "profile-resolution-specification-input",
                        "verification-input": "resource-limit-policy-verification",
                    },
                }),
            ]
        );
    }

    /// The `origins` record names the Safe Origin Kinds its entry fixes for
    /// the entry's slots, and every common slot's own (D15:205, 2517).
    #[test]
    fn the_origins_record_names_the_origins_its_entry_fixes() {
        let fixture = FileFixture::new(&fixture_limits(100));
        let file = fixture.file(Arc::from(&b"{}"[..]));
        let relabeled = Relabeled(fixture.file(Arc::from(&b"{}"[..])));
        let origins = |entry: &dyn ConfigurationEntry| {
            EntryDependency::Origins(EntryOrigins::of(entry)).view()["origins"].clone()
        };
        let mut expected = origins(&file);
        assert_eq!(expected["entry-source"], "configuration-file");
        assert_eq!(expected["profile-selector"], "profile-selector-cli");
        expected["entry-source"] = json!("configuration-programmatic");
        expected["profile-selector"] = json!("profile-selector-programmatic");
        assert_eq!(origins(&relabeled), expected);
    }

    /// An unmarked source identity and an undeclared selector are redacted,
    /// so their records wait on the invocation's evidence tokens, and a
    /// present locator is kept and counted (D15:735-738, 807-809, 1852,
    /// 2516).
    #[test]
    fn a_redacted_entry_dependency_waits_on_its_tokens() {
        let entry = redacted_entry();
        assert_eq!(entry.work(), 3);
        assert_eq!(entry.redacted().count(), 2);
        let none = EvidenceTokens::assign(PROFILE, std::iter::empty(), &Sha256);
        assert_eq!(entry.retained(PROFILE, &none), None);
        let tokens = EvidenceTokens::assign(PROFILE, entry.redacted(), &Sha256);
        let records = entry.retained(PROFILE, &tokens).unwrap();
        let [source, projected, _] = records.as_slice() else {
            panic!("a file entry has three records");
        };
        // The file's source identity and selector, each under the origin its
        // slot fixes.
        let fixture = FileFixture::new(&fixture_limits(100));
        let file = fixture.file(Arc::from(&b"{}"[..]));
        let identity = SourceIdentity::project(
            SafeOriginKind::ConfigurationFile,
            file.source.identity,
            false,
        )
        .unwrap();
        let expected = identity.retained(PROFILE, &tokens).unwrap();
        let source = source.view();
        assert_eq!(source["source"], expected.view());
        assert!(source["source"]["redacted"]["token"].is_string());
        assert_eq!(source["locator"], locator().view());
        let expected = SelectorProjection::project(
            SafeOriginKind::ProfileSelectorCli,
            &selector(Some("other")),
            authority(),
            &declared(),
        )
        .unwrap()
        .retained(PROFILE, &tokens)
        .unwrap();
        assert_eq!(projected.view()["selector"], expected.view());
        assert!(expected.view()["redacted-value"]["token"]["token"].is_string());
    }

    /// The owner's mark classifies a source identity and nothing else: with
    /// or without it, a declared selector stays `safe-profile-id` and any
    /// other stays what it was, so no caller overrides the selector's
    /// classification (D15:808, 3488).
    #[test]
    fn a_callers_mark_leaves_the_selector_evidence_unchanged() {
        let fixture = FileFixture::new(&fixture_limits(100));
        let selector_view = |marked: bool, text: Option<&str>| {
            let mut file = fixture.file(Arc::from(&b"{}"[..]));
            file.source.marked = marked;
            record_views(&inputs(&file, text))[1]["selector"].clone()
        };
        assert_eq!(
            selector_view(false, Some("app")),
            json!({ "safe-profile-id": "app" })
        );
        for text in [Some("app"), Some("other"), None] {
            assert_eq!(
                selector_view(true, text),
                selector_view(false, text),
                "{text:?}"
            );
        }
    }

    /// The views of `entry`'s records, its redacted components' tokens
    /// assigned on their own.
    fn record_views(entry: &EntryInputs) -> Vec<Value> {
        let tokens = EvidenceTokens::assign(PROFILE, entry.redacted(), &Sha256);
        let records = entry.retained(PROFILE, &tokens).unwrap();
        records.iter().map(EntryDependency::view).collect()
    }

    /// D15:744, 748, 2508, 2517: a programmatic entry's dependencies name the
    /// Programmatic Entry's identity, the verified Snapshot digest as its
    /// revision, and its own locator; beside a sidecar, in the slots' order,
    /// the call-site source's identity, Call-Site Source revision, and own
    /// locator; the selector under `profile-selector-programmatic`; and
    /// `configuration-programmatic` for each source's slot.
    #[test]
    fn the_dependencies_of_a_programmatic_entry_name_both_of_its_sources() {
        let root = from_json(&json!({ "profiles": {} }));
        let snapshot = AdmittedSnapshot::of(root.clone());
        let fixture = SnapshotFixture::new(&fixture_limits(100));
        let label = LocatorCandidate {
            variant: LocatorVariant::SafeLabel,
            payload: "storefront entry",
            non_secret: true,
        };
        let locator = read_programmatic_locator(Some(label), authority()).unwrap();
        let mut entry = fixture.entry(&snapshot);
        entry.source.locator = Some(&locator);
        let source = "defineConfig({ profiles: {} })";
        let call_site = sidecar(source, Some("app.ts"), Vec::new());
        let named = |entry: &EntrySnapshot<'_>| {
            EntryInputs::programmatic(entry, &selector(Some("app")), authority(), &declared())
                .unwrap()
        };
        let without = named(&entry);
        entry.call_site = Some(&call_site);
        let beside = named(&entry);
        assert_eq!((without.work(), beside.work()), (3, 5));
        assert_eq!(beside.redacted().count(), 0);
        let entry_source = json!({
            "slot": "entry-source",
            "source": { "safe-opaque": hex(ENTRY.as_bytes()) },
            "revision": root.digest(),
            "locator": { "safe-label": "storefront entry" },
        });
        let call_site_source = json!({
            "slot": "call-site-source",
            "source": { "safe-opaque": hex(CALL_SITE.as_bytes()) },
            "revision": call_site_source_revision(source.as_bytes()),
            "locator": { "safe-label": "app.ts" },
        });
        let selected =
            json!({ "slot": "profile-selector", "selector": { "safe-profile-id": "app" } });
        let origins = |call_site: bool| {
            let mut origins = json!({
                "artifact-collection": "profile-resolution-artifact-collection",
                "entry-source": "configuration-programmatic",
                "implementation-capability": "implementation-capability",
                "intent-surface-class-vocabulary": "intent-surface-class-vocabulary",
                "locale-canonicalization-data": "locale-canonicalization-data",
                "profile-selector": "profile-selector-programmatic",
                "specification-input": "profile-resolution-specification-input",
                "verification-input": "resource-limit-policy-verification",
            });
            if call_site {
                origins["call-site-source"] = json!("configuration-programmatic");
            }
            json!({ "slot": "origins", "origins": origins })
        };
        assert_eq!(
            record_views(&without),
            [entry_source.clone(), selected.clone(), origins(false)]
        );
        assert_eq!(
            record_views(&beside),
            [entry_source, call_site_source, selected, origins(true)]
        );
    }

    /// An unmarked call-site identity is redacted as the Programmatic Entry's
    /// is, each its own component under `configuration-programmatic`, and an
    /// undeclared selector under `profile-selector-programmatic`, so the
    /// records wait on their tokens, and the token inputs count all three
    /// (D15:735-738, 805, 809, 1852, 2517; plan 025 interpretation #245).
    #[test]
    fn an_unmarked_call_site_identity_waits_on_its_own_token() {
        let snapshot = AdmittedSnapshot::of(from_json(&json!({})));
        let fixture = SnapshotFixture::new(&fixture_limits(100));
        let call_site = marked_as(false, "defineConfig({})", None, Vec::new());
        let mut entry = fixture.entry(&snapshot);
        entry.source.marked = false;
        entry.call_site = Some(&call_site);
        let inputs =
            EntryInputs::programmatic(&entry, &selector(Some("other")), authority(), &declared())
                .unwrap();
        assert_eq!(inputs.redacted().count(), 3);
        assert_eq!(inputs.work(), 3);
        let none = EvidenceTokens::assign(PROFILE, std::iter::empty(), &Sha256);
        assert_eq!(inputs.retained(PROFILE, &none), None);
        let tokens = EvidenceTokens::assign(PROFILE, inputs.redacted(), &Sha256);
        let records = inputs.retained(PROFILE, &tokens).unwrap();
        let [entry_source, call_site_source, projected, _] = records.as_slice() else {
            panic!("a programmatic entry beside a sidecar has four records");
        };
        for (record, identity) in [
            (entry_source, entry.source.identity),
            (call_site_source, call_site.pair().identity()),
        ] {
            let expected =
                SourceIdentity::project(SafeOriginKind::ConfigurationProgrammatic, identity, false)
                    .unwrap()
                    .retained(PROFILE, &tokens)
                    .unwrap();
            assert_eq!(record.view()["source"], expected.view());
        }
        assert_ne!(
            entry_source.view()["source"],
            call_site_source.view()["source"]
        );
        let expected = SelectorProjection::project(
            SafeOriginKind::ProfileSelectorProgrammatic,
            &selector(Some("other")),
            authority(),
            &declared(),
        )
        .unwrap()
        .retained(PROFILE, &tokens)
        .unwrap();
        assert_eq!(projected.view()["selector"], expected.view());
    }
}
