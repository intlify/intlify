// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite inputs of one file entry's complete invocation for 015's Evidence
//! materialization stages: 015's example as the resolver's tests submit it,
//! profile `app` of the complete fixture configuration with every policy and
//! Target Profile it names submitted once and each reference naming one
//! exactly, and one change of it for each kind of outcome a stage reports:
//! blocking Findings after materialization, many Findings, no value, and a
//! Shared domain its bound exhausts.
//! Every reference's semantic digest is computed from the envelope it names,
//! never written by hand.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::artifacts::content::sample;
use crate::artifacts::envelope::{envelope_of, ArtifactKind};
use crate::benchmark::work::WorkKind;
use crate::input_limits::Bound;
use crate::profile_fixtures::complete_config;
use crate::resolver::capability::EvidenceMetadataLimits;
use crate::resolver::evaluation::ExhaustedBound;
use crate::resolver::evidence_metadata::fixtures::{unbounded, with};
use crate::resolver::evidence_metadata::MetadataBound;
use crate::resolver::limits::{BoundId, Group, ResolutionLimits};

use super::policy_inputs::{capacity, reference_value, selected_policy, tuple_of};
use super::LimitKind;

/// Each bound the Shared projection stage counts itself, and the recipe
/// whose count of it is at least 2, so that its first-over edge is a bound:
/// the Finding and cause counts are the blocked recipe's, the others the
/// example's. A collision comparison under SHA-256 is never counted, so it
/// has no edge (plan 025 decisions #256 and #263).
pub(in crate::benchmark) const SHARED_EDGES: [(LimitKind, EvidenceRecipe); 10] = [
    (LimitKind::SharedFindingOccurrences, EvidenceRecipe::Blocked),
    (LimitKind::SharedFindings, EvidenceRecipe::Blocked),
    (
        LimitKind::SharedEvaluationStatusEntries,
        EvidenceRecipe::Storefront,
    ),
    (
        LimitKind::SharedSuppressionCausesPerStatus,
        EvidenceRecipe::Blocked,
    ),
    (
        LimitKind::SharedSuppressionCauseOccurrences,
        EvidenceRecipe::Blocked,
    ),
    (LimitKind::SharedRecords, EvidenceRecipe::Storefront),
    (
        LimitKind::SharedTotalCanonicalBytes,
        EvidenceRecipe::Storefront,
    ),
    (
        LimitKind::SharedScopeSubjectOccurrences,
        EvidenceRecipe::Storefront,
    ),
    (
        LimitKind::SharedScopeTokenInputs,
        EvidenceRecipe::Storefront,
    ),
    (
        LimitKind::SharedScopeMetadataWorkUnits,
        EvidenceRecipe::Storefront,
    ),
];

/// Each bound the Entry projection stage counts itself, and the recipe whose
/// count of it is at least 2: the Entry Finding, related Evidence, and cause
/// counts are the inadmissible recipe's, the others the example's, whose
/// largest path is one Evidence cannot do without. A collision comparison
/// under SHA-256 is never counted, so it has no edge (plan 025 decisions
/// #256 and #264).
pub(in crate::benchmark) const ENTRY_EDGES: [(LimitKind, EvidenceRecipe); 16] = [
    (
        LimitKind::EntryFindingOccurrences,
        EvidenceRecipe::Inadmissible,
    ),
    (LimitKind::EntryFindings, EvidenceRecipe::Inadmissible),
    (
        LimitKind::EntryRelatedEvidencePerOccurrenceGroup,
        EvidenceRecipe::Inadmissible,
    ),
    (
        LimitKind::EntryRelatedEvidenceOccurrences,
        EvidenceRecipe::Inadmissible,
    ),
    (
        LimitKind::EntrySingleEvidenceBytes,
        EvidenceRecipe::Inadmissible,
    ),
    (
        LimitKind::EntryTotalEvidenceBytes,
        EvidenceRecipe::Inadmissible,
    ),
    (
        LimitKind::EntryEvaluationStatusEntries,
        EvidenceRecipe::Storefront,
    ),
    (
        LimitKind::EntrySuppressionCausesPerStatus,
        EvidenceRecipe::Inadmissible,
    ),
    (
        LimitKind::EntrySuppressionCauseOccurrences,
        EvidenceRecipe::Inadmissible,
    ),
    (LimitKind::EntryRecords, EvidenceRecipe::Storefront),
    (
        LimitKind::EntryTotalCanonicalBytes,
        EvidenceRecipe::Storefront,
    ),
    (
        LimitKind::InvocationLogicalPathSegments,
        EvidenceRecipe::Storefront,
    ),
    (
        LimitKind::InvocationLogicalPathCanonicalBytes,
        EvidenceRecipe::Storefront,
    ),
    (
        LimitKind::InvocationSubjectOccurrences,
        EvidenceRecipe::Storefront,
    ),
    (LimitKind::InvocationTokenInputs, EvidenceRecipe::Storefront),
    (
        LimitKind::InvocationMetadataWorkUnits,
        EvidenceRecipe::Storefront,
    ),
];

/// A bound an Evidence projection stage counts: one of a diagnostic domain's,
/// which both the capability and the selected policy set, or one of the
/// `evidenceMetadata` counts, which only the capability sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) enum EvidenceBound {
    Diagnostic(Group, &'static str),
    Metadata(MetadataBound),
}

impl EvidenceBound {
    /// The bound `kind` names and the work kind that counts it, in its unit.
    pub(in crate::benchmark) fn of(kind: LimitKind) -> Option<(Self, WorkKind)> {
        let shared = |name| Self::Diagnostic(Group::DiagnosticsShared, name);
        let entry = |name| Self::Diagnostic(Group::DiagnosticsEntry, name);
        Some(match kind {
            LimitKind::SharedFindingOccurrences => (
                shared("maxFindingOccurrences"),
                WorkKind::SharedEvidenceFindingCandidates,
            ),
            LimitKind::SharedFindings => (shared("maxFindings"), WorkKind::SharedEvidenceFindings),
            LimitKind::SharedEvaluationStatusEntries => (
                shared("maxEvaluationStatusEntries"),
                WorkKind::SharedEvidenceEvaluationStatuses,
            ),
            LimitKind::SharedSuppressionCausesPerStatus => (
                shared("maxSuppressionCausesPerStatus"),
                WorkKind::SharedEvidenceMaximumCausesPerStatus,
            ),
            LimitKind::SharedSuppressionCauseOccurrences => (
                shared("maxSuppressionCauseOccurrences"),
                WorkKind::SharedEvidenceCauseOccurrences,
            ),
            LimitKind::SharedRecords => (shared("maxRecords"), WorkKind::SharedEvidenceRecords),
            LimitKind::SharedTotalCanonicalBytes => (
                shared("maxTotalCanonicalBytes"),
                WorkKind::SharedEvidenceCanonicalBytes,
            ),
            LimitKind::SharedScopeSubjectOccurrences => (
                Self::Metadata(MetadataBound::SubjectOccurrences),
                WorkKind::SharedEvidenceSubjectOccurrences,
            ),
            LimitKind::SharedScopeTokenInputs => (
                Self::Metadata(MetadataBound::TokenInputs),
                WorkKind::SharedEvidenceTokenInputs,
            ),
            LimitKind::SharedScopeMetadataWorkUnits => (
                Self::Metadata(MetadataBound::MetadataWorkUnits),
                WorkKind::SharedEvidenceMetadataWorkUnits,
            ),
            LimitKind::EntryFindingOccurrences => (
                entry("maxFindingOccurrences"),
                WorkKind::EntryEvidenceFindingCandidates,
            ),
            LimitKind::EntryFindings => (entry("maxFindings"), WorkKind::EntryEvidenceFindings),
            LimitKind::EntryRelatedEvidencePerOccurrenceGroup => (
                entry("maxRelatedEvidencePerOccurrenceGroup"),
                WorkKind::EntryEvidenceMaximumRelatedEvidencePerOccurrenceGroup,
            ),
            LimitKind::EntryRelatedEvidenceOccurrences => (
                entry("maxRelatedEvidenceOccurrences"),
                WorkKind::EntryEvidenceRelatedEvidenceOccurrences,
            ),
            LimitKind::EntrySingleEvidenceBytes => (
                entry("maxSingleEvidenceBytes"),
                WorkKind::EntryEvidenceMaximumSingleEvidenceBytes,
            ),
            LimitKind::EntryTotalEvidenceBytes => (
                entry("maxTotalEvidenceBytes"),
                WorkKind::EntryEvidenceTotalEvidenceBytes,
            ),
            LimitKind::EntryEvaluationStatusEntries => (
                entry("maxEvaluationStatusEntries"),
                WorkKind::EntryEvidenceEvaluationStatuses,
            ),
            LimitKind::EntrySuppressionCausesPerStatus => (
                entry("maxSuppressionCausesPerStatus"),
                WorkKind::EntryEvidenceMaximumCausesPerStatus,
            ),
            LimitKind::EntrySuppressionCauseOccurrences => (
                entry("maxSuppressionCauseOccurrences"),
                WorkKind::EntryEvidenceCauseOccurrences,
            ),
            LimitKind::EntryRecords => (entry("maxRecords"), WorkKind::EntryEvidenceRecords),
            LimitKind::EntryTotalCanonicalBytes => (
                entry("maxTotalCanonicalBytes"),
                WorkKind::EntryEvidenceCanonicalBytes,
            ),
            LimitKind::InvocationLogicalPathSegments => (
                Self::Metadata(MetadataBound::LogicalPathSegments),
                WorkKind::EntryEvidenceMaximumLogicalPathSegments,
            ),
            LimitKind::InvocationLogicalPathCanonicalBytes => (
                Self::Metadata(MetadataBound::LogicalPathCanonicalBytes),
                WorkKind::EntryEvidenceMaximumLogicalPathCanonicalBytes,
            ),
            LimitKind::InvocationSubjectOccurrences => (
                Self::Metadata(MetadataBound::SubjectOccurrences),
                WorkKind::EntryEvidenceSubjectOccurrences,
            ),
            LimitKind::InvocationTokenInputs => (
                Self::Metadata(MetadataBound::TokenInputs),
                WorkKind::EntryEvidenceTokenInputs,
            ),
            LimitKind::InvocationMetadataWorkUnits => (
                Self::Metadata(MetadataBound::MetadataWorkUnits),
                WorkKind::EntryEvidenceMetadataWorkUnits,
            ),
            _ => return None,
        })
    }

    /// The bound an exhausted domain's terminal Finding names.
    pub(in crate::benchmark) fn exhausted(self) -> ExhaustedBound {
        match self {
            Self::Diagnostic(group, name) => ExhaustedBound::Domain(BoundId { group, name }),
            Self::Metadata(bound) => ExhaustedBound::Capability(bound.id()),
        }
    }

    /// `recipe`'s bounds, the capability's and the policy's, and the
    /// capability's `evidenceMetadata` bounds, with this bound at `limit`.
    pub(in crate::benchmark) fn limits(
        self,
        recipe: EvidenceRecipe,
        limit: Bound,
    ) -> (ResolutionLimits, EvidenceMetadataLimits) {
        let limits = recipe.limits();
        match self {
            Self::Diagnostic(group, name) => {
                let bounds = limits.bounds();
                let limits = ResolutionLimits::from_bounds(|id| {
                    if id.group == group && id.name == name {
                        Some(limit)
                    } else {
                        bounds
                            .iter()
                            .find(|(bound, _)| *bound == id)
                            .map(|&(_, value)| value)
                    }
                })
                .expect("every bound of the limits is set");
                (limits, unbounded())
            }
            Self::Metadata(bound) => (limits, with(bound, limit.get())),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum EvidenceRecipe {
    /// 015's example: no Finding.
    Storefront,
    /// The example without its project default and with a target locale that
    /// is no string: blocking Findings after materialization (D15:1907; plan
    /// 025 interpretation #252).
    Blocked,
    /// The example with [`DENSE`] more targets, each requesting a locale
    /// outside the project's that its Target Profile has no direction for:
    /// many Findings, each of a target of its own.
    Dense,
    /// The example's file without its last byte, which is no JSON: no value,
    /// so no unit reports into the Shared domain (D15:2369; plan 025
    /// decision #260).
    Malformed,
    /// The example under a Shared `maxRecords` its Shared records outgrow:
    /// the Shared domain is exhausted, so the bundle is incomplete and the
    /// outcome blocked (D15:1185, 2408; plan 025 decision #262).
    Truncated,
    /// The example's file with its project default three times and a locale
    /// string a lone surrogate: two Entry Findings, one of the duplicate
    /// members, whose two occurrences each relate the other two, and one of
    /// the string that is no JSON-compatible value, and the statuses that
    /// wait on both name both as causes; no value (D15:3440; plan 025
    /// decision #264).
    Inadmissible,
}

/// A configuration and the members submitted with it, in submission order.
pub(in crate::benchmark) struct EvidenceFixture {
    pub(in crate::benchmark) config: Value,
    pub(in crate::benchmark) members: Vec<Value>,
}

/// The number of targets `Dense` adds.
pub(in crate::benchmark) const DENSE: usize = 64;

/// A value no diagnostic count of these inputs reaches.
const ROOM: u64 = 1 << 40;

/// The Shared records `Truncated` admits, fewer than the example's.
const TRUNCATED: u64 = 100;

/// Each policy slot besides `resourceLimits`, the kind its role takes, and
/// the identity of the artifact it names.
const POLICIES: [(&str, ArtifactKind, &str); 6] = [
    ("trust", ArtifactKind::TrustPolicy, "storefront-trust"),
    (
        "sourceAdmission",
        ArtifactKind::SourceAdmissionPolicy,
        "storefront-source",
    ),
    (
        "approval",
        ArtifactKind::ApprovalPolicy,
        "storefront-approval",
    ),
    (
        "selection",
        ArtifactKind::SelectionPolicy,
        "storefront-selection",
    ),
    (
        "providerRouting",
        ArtifactKind::ProviderRoutingPolicy,
        "storefront-routing",
    ),
    (
        "glossarySet",
        ArtifactKind::GlossarySet,
        "storefront-glossary",
    ),
];

/// Each target, the identity of the Target Profile it names, and the
/// hydration role that profile declares, the relation's (024).
const TARGETS: [(&str, &str, &str); 2] = [
    ("browser", "storefront-browser", "browser-hydration-client"),
    ("server", "storefront-server", "ssr-renderer"),
];

impl EvidenceRecipe {
    pub(in crate::benchmark) const ALL: [Self; 6] = [
        Self::Storefront,
        Self::Blocked,
        Self::Dense,
        Self::Malformed,
        Self::Truncated,
        Self::Inadmissible,
    ];

    /// The recipes of the Entry projection stage: every one but the one that
    /// exhausts only the Shared domain, which the Shared bounds' edges cover.
    pub(in crate::benchmark) const ENTRY: [Self; 5] = [
        Self::Storefront,
        Self::Blocked,
        Self::Dense,
        Self::Malformed,
        Self::Inadmissible,
    ];

    /// The recipes with a value, the only ones the Shared projection stage
    /// applies to (D15:2369, 2771).
    pub(in crate::benchmark) const VALUED: [Self; 3] =
        [Self::Storefront, Self::Blocked, Self::Dense];

    /// Whether the recipe's outcome is checked: only the example's is.
    pub(in crate::benchmark) fn checked(self) -> bool {
        self == Self::Storefront
    }

    /// The bounds both the capability and the selected policy set for this
    /// recipe: [`limits`], but `Truncated`'s Shared `maxRecords`.
    pub(in crate::benchmark) fn limits(self) -> ResolutionLimits {
        let limits = limits();
        if self != Self::Truncated {
            return limits;
        }
        let bounds = limits.bounds();
        ResolutionLimits::from_bounds(|id| {
            if id.group == Group::DiagnosticsShared && id.name == "maxRecords" {
                Bound::new(TRUNCATED)
            } else {
                bounds
                    .iter()
                    .find(|(bound, _)| *bound == id)
                    .map(|&(_, value)| value)
            }
        })
        .expect("every bound of the limits is set")
    }

    /// The configuration file of this recipe under `limits`.
    pub(in crate::benchmark) fn source(self, limits: &ResolutionLimits) -> Vec<u8> {
        let mut source =
            serde_json::to_vec(&self.fixture(limits).config).expect("a JSON value encodes");
        match self {
            Self::Malformed => {
                source.pop();
            }
            Self::Inadmissible => {
                let default = r#""defaultRequestedLocale":"en""#;
                source = String::from_utf8(source)
                    .expect("an encoded JSON value is UTF-8")
                    .replacen(default, &[default; 3].join(","), 1)
                    .replacen(r#""ja""#, r#""\ud800""#, 1)
                    .into_bytes();
            }
            Self::Storefront | Self::Blocked | Self::Dense | Self::Truncated => {}
        }
        source
    }

    /// The configuration and members of this recipe, the selected policy
    /// setting `limits` and submitted first.
    pub(in crate::benchmark) fn fixture(self, limits: &ResolutionLimits) -> EvidenceFixture {
        let mut config = complete_config();
        let app = &mut config["profiles"]["app"];
        app["requestedLocales"] = json!(["ja", "en"]);
        app["defaultRequestedLocale"] = json!("en");
        app["localeNegotiation"] = json!({"aliases": {"de": "en"}});
        let policy = selected_policy(limits);
        app["policies"]["resourceLimits"] = reference_value(&tuple_of(&policy));
        let mut members = vec![policy];
        for (slot, kind, identity) in POLICIES {
            let member = envelope_of(kind, identity, "1", sample(kind));
            app["policies"][slot] = reference_value(&tuple_of(&member));
            members.push(member);
        }
        for (target, identity, role) in TARGETS {
            let mut content = sample(ArtifactKind::TargetProfile);
            content["hydration"]["roles"] = json!([role]);
            let member = envelope_of(ArtifactKind::TargetProfile, identity, "1", content);
            app["targetProfiles"][target]["requestedLocales"] = json!(["ja", "en"]);
            app["targetProfiles"][target]["profile"] = reference_value(&tuple_of(&member));
            members.push(member);
        }
        match self {
            Self::Storefront | Self::Malformed | Self::Truncated | Self::Inadmissible => {}
            Self::Blocked => {
                app.as_object_mut()
                    .expect("the example's declaration is an object")
                    .remove("defaultRequestedLocale");
                app["targetProfiles"]["server"]["requestedLocales"] = json!(["en", 42]);
            }
            Self::Dense => {
                let browser = app["targetProfiles"]["browser"].clone();
                let mut members = app["deploymentGroups"]["web"]["members"]
                    .as_array()
                    .cloned()
                    .expect("the example's group lists its members");
                for target in 0..DENSE {
                    let id = format!("target-{target:02}");
                    let mut declared = browser.clone();
                    declared["requestedLocales"] = json!(["ja", "en", "fr"]);
                    app["targetProfiles"][&id] = declared;
                    members.push(json!(id));
                }
                app["deploymentGroups"]["web"]["members"] = Value::Array(members);
            }
        }
        EvidenceFixture { config, members }
    }
}

/// The bounds both the capability and the selected policy set: the cases'
/// capacity, but every diagnostic bound at a value no count here reaches,
/// since the example's records outgrow the fixture values. The policy's
/// equal the capability's, so it is compatible with it.
pub(in crate::benchmark) fn limits() -> ResolutionLimits {
    let current = capacity().bounds();
    let room = Bound::new(ROOM).expect("a finite positive bound");
    ResolutionLimits::from_bounds(|id| {
        if matches!(id.group, Group::DiagnosticsEntry | Group::DiagnosticsShared) {
            Some(room)
        } else {
            current
                .iter()
                .find(|(bound, _)| *bound == id)
                .map(|&(_, value)| value)
        }
    })
    .expect("every bound of the capacity is set")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each recipe submits the selected policy first, the six other policies,
    /// and the two Target Profiles, each once, and the declaration names each
    /// of them exactly, and nothing else.
    #[test]
    fn each_recipe_names_every_member_it_submits_exactly() {
        let limits = limits();
        for recipe in EvidenceRecipe::ALL {
            let fixture = recipe.fixture(&limits);
            assert_eq!(fixture.members.len(), 9, "{recipe:?}");
            assert_eq!(fixture.members[0], selected_policy(&limits));
            let app = &fixture.config["profiles"]["app"];
            let mut named: Vec<&Value> = app["policies"].as_object().unwrap().values().collect();
            named.extend(
                app["targetProfiles"]
                    .as_object()
                    .unwrap()
                    .values()
                    .map(|target| &target["profile"]),
            );
            let submitted: Vec<Value> = fixture
                .members
                .iter()
                .map(|member| reference_value(&tuple_of(member)))
                .collect();
            assert!(named.iter().all(|reference| submitted.contains(reference)));
            assert!(submitted.iter().all(|member| named.contains(&member)));
        }
    }

    /// Only the recipe's own change differs from the example.
    #[test]
    fn each_recipe_changes_only_what_it_names() {
        let limits = limits();
        let example = EvidenceRecipe::Storefront.fixture(&limits).config;
        assert_eq!(EvidenceRecipe::Malformed.fixture(&limits).config, example);
        assert_eq!(EvidenceRecipe::Truncated.fixture(&limits).config, example);
        let blocked = EvidenceRecipe::Blocked.fixture(&limits).config;
        let dense = EvidenceRecipe::Dense.fixture(&limits).config;
        let mut restored = blocked.clone();
        restored["profiles"]["app"]["defaultRequestedLocale"] = json!("en");
        restored["profiles"]["app"]["targetProfiles"]["server"]["requestedLocales"] =
            json!(["ja", "en"]);
        assert_eq!(restored, example);
        let mut restored = dense.clone();
        let app = &mut restored["profiles"]["app"];
        let targets = app["targetProfiles"].as_object_mut().unwrap();
        let added: Vec<String> = targets
            .keys()
            .filter(|id| id.starts_with("target-"))
            .cloned()
            .collect();
        assert_eq!(added.len(), DENSE);
        for id in &added {
            assert_eq!(targets[id]["requestedLocales"], json!(["ja", "en", "fr"]));
            targets.remove(id);
        }
        app["deploymentGroups"]["web"]["members"] =
            example["profiles"]["app"]["deploymentGroups"]["web"]["members"].clone();
        assert_eq!(restored, example);
    }

    /// The input context of either stage refuses a candidate bound to
    /// another input: another recipe kind, another selector even with its
    /// own selector input, another selector input, another recipe's
    /// configuration, no vocabulary assertion, another recipe whose file
    /// differs only in its bounds, and a vocabulary that is not admitted.
    #[test]
    fn the_input_context_refuses_a_candidate_bound_to_another_input() {
        use std::sync::Arc;

        use crate::artifacts::vocabulary::{
            self, IntentSurfaceClassVocabularyArtifact, VocabularyLimits,
        };
        use crate::benchmark::cases::context::{observe, ContextFailure};
        use crate::benchmark::cases::prepare::{prepare, selector_input};
        use crate::benchmark::cases::{declarations, Recipe, Selector};
        use crate::benchmark::operation::{Operation, Prepared};

        let room = VocabularyLimits {
            artifact_bytes: u64::MAX,
            depth: u64::MAX,
            members: u64::MAX,
            member_bytes: u64::MAX,
            total_member_bytes: u64::MAX,
        };
        for (operation, change) in [
            Operation::SharedEvidence,
            Operation::EntryEvidence,
            Operation::OutcomeFinalization,
        ]
        .into_iter()
        .flat_map(|operation| (0..7).map(move |change| (operation, change)))
        {
            let declaration = declarations()
                .into_iter()
                .find(|case| {
                    case.operation == operation
                        && case.fixture == Recipe::Evidence(EvidenceRecipe::Storefront)
                })
                .unwrap();
            let mut candidate = prepare(&declaration).unwrap();
            assert!(observe(&candidate).is_ok());
            let prepared = match &mut candidate.prepared {
                Prepared::SharedEvidence(prepared) | Prepared::EntryEvidence(prepared) => {
                    &mut **prepared
                }
                Prepared::OutcomeFinalization(prepared) => &mut prepared.evidence,
                _ => unreachable!(),
            };
            let bound = prepared.structural.max_profile_id_bytes;
            match change {
                0 => candidate.declaration.fixture = Recipe::Minimal,
                1 => {
                    candidate.declaration.selector = Selector::Absent;
                    prepared.selector = selector_input(Selector::Absent, bound);
                }
                2 => prepared.selector = selector_input(Selector::Unknown, bound),
                3 => {
                    let blocked = EvidenceRecipe::Blocked.fixture(&limits()).config;
                    prepared.source = Arc::from(serde_json::to_vec(&blocked).unwrap());
                }
                4 => prepared.specifications.intent_surface_class_vocabulary = None,
                // The same file but for its Shared `maxRecords`.
                5 => {
                    candidate.declaration.fixture = Recipe::Evidence(EvidenceRecipe::Truncated);
                }
                // Members out of order: read, but not admitted.
                _ => {
                    let artifact = IntentSurfaceClassVocabularyArtifact::sealed(
                        "intlify-benchmark-vocabulary",
                        "0",
                        &["nav", "checkout"],
                    )
                    .unwrap();
                    let bytes = serde_json::to_vec(&artifact).unwrap();
                    prepared.vocabulary = vocabulary::read(Some(&bytes), &room).unwrap();
                }
            }
            assert_eq!(
                observe(&candidate),
                Err(ContextFailure::EvidenceInputMismatch),
                "{operation:?} change {change}"
            );
        }
    }

    /// The input context of each stage binds what the resolver supports in
    /// every slot it compares an assertion with: another identity, revision,
    /// canonicalization dataset, or active digest in any slot changes it.
    #[test]
    fn the_input_context_binds_every_supported_specification() {
        use crate::benchmark::cases::context::observe;
        use crate::benchmark::cases::prepare::{prepare, Candidate};
        use crate::benchmark::cases::{declarations, Recipe};
        use crate::benchmark::evidence::{supported, PreparedEvidence, ACTIVE};
        use crate::benchmark::operation::{Operation, Prepared};
        use crate::locale::fixtures::fixture_pins;
        use crate::locale::{DatasetPin, SemanticPins, Supported, VersionedIdentity};
        use crate::resolver::specification::{
            ActiveProfileSpecification, SupportedSpecifications, FIXTURE_PROFILE_DIGEST,
        };

        fn evidence(candidate: &mut Candidate) -> &mut PreparedEvidence {
            match &mut candidate.prepared {
                Prepared::SharedEvidence(prepared) | Prepared::EntryEvidence(prepared) => prepared,
                Prepared::OutcomeFinalization(prepared) => &mut prepared.evidence,
                _ => unreachable!(),
            }
        }

        // Every supported value in slot order: the active specification and
        // its digest, the canonicalization specification and its dataset,
        // and the five others.
        let pins = fixture_pins();
        let baseline = [
            ACTIVE.identity,
            ACTIVE.revision,
            FIXTURE_PROFILE_DIGEST,
            pins.specification.identity,
            pins.specification.revision,
            pins.dataset.identity,
            pins.dataset.semantic_digest,
            "intlify-locale-negotiation",
            "0",
            "intlify-message-locale-fallback",
            "0",
            "intlify-localization-coverage",
            "0",
            "intlify-delivery-graph",
            "0",
            "intlify-delivery-placement",
            "0",
        ];
        let built = |values: &[&'static str; 17]| {
            let versioned = |at: usize| VersionedIdentity {
                identity: values[at],
                revision: values[at + 1],
            };
            let canonicalization = SemanticPins {
                specification: versioned(3),
                dataset: DatasetPin {
                    identity: values[5],
                    semantic_digest: values[6],
                },
            };
            SupportedSpecifications::new(
                ActiveProfileSpecification {
                    specification: versioned(0),
                    semantic_digest: values[2],
                },
                Supported::new(vec![canonicalization]).unwrap(),
                versioned(7),
                versioned(9),
                versioned(11),
                versioned(13),
                versioned(15),
            )
        };
        assert_eq!(built(&baseline), supported().unwrap());
        for operation in [
            Operation::SharedEvidence,
            Operation::EntryEvidence,
            Operation::OutcomeFinalization,
        ] {
            let declaration = declarations()
                .into_iter()
                .find(|case| {
                    case.operation == operation
                        && case.fixture == Recipe::Evidence(EvidenceRecipe::Storefront)
                })
                .unwrap();
            let mut candidate = prepare(&declaration).unwrap();
            let mut seen = vec![observe(&candidate).unwrap()];
            for index in 0..baseline.len() {
                let mut values = baseline;
                values[index] = "another";
                evidence(&mut candidate).supported = built(&values);
                let observed = observe(&candidate).unwrap();
                assert!(!seen.contains(&observed), "{operation:?} value {index}");
                seen.push(observed);
            }
        }
    }

    /// Each file is its configuration's JSON, but the malformed one, which is
    /// the example's without its last byte and so no JSON, and the
    /// inadmissible one, which no JSON-compatible value holds.
    #[test]
    fn only_the_malformed_and_inadmissible_files_are_no_json() {
        let limits = limits();
        for recipe in EvidenceRecipe::ALL {
            let source = recipe.source(&limits);
            let parsed = serde_json::from_slice::<Value>(&source);
            if recipe == EvidenceRecipe::Malformed {
                assert!(parsed.is_err());
                let mut example = EvidenceRecipe::Storefront.source(&limits);
                assert_eq!(example.pop(), Some(b'}'));
                assert_eq!(source, example);
            } else if recipe == EvidenceRecipe::Inadmissible {
                assert!(parsed.is_err());
            } else {
                assert_eq!(
                    parsed.unwrap(),
                    recipe.fixture(&limits).config,
                    "{recipe:?}"
                );
            }
        }
        assert!(!EvidenceRecipe::VALUED.contains(&EvidenceRecipe::Malformed));
    }

    /// Only `Truncated`'s Shared `maxRecords` differs from every other
    /// recipe's bounds, and its Shared records outgrow it.
    #[test]
    fn only_the_truncated_recipe_tightens_a_bound() {
        let base = limits().bounds();
        for recipe in EvidenceRecipe::ALL {
            let differ: Vec<_> = recipe
                .limits()
                .bounds()
                .into_iter()
                .filter(|bound| !base.contains(bound))
                .collect();
            if recipe == EvidenceRecipe::Truncated {
                assert_eq!(differ.len(), 1);
                assert_eq!(differ[0].0.group, Group::DiagnosticsShared);
                assert_eq!(differ[0].0.name, "maxRecords");
                assert_eq!(differ[0].1.get(), TRUNCATED);
            } else {
                assert!(differ.is_empty(), "{recipe:?}");
            }
        }
    }

    /// What counts each edge's bound, and the bound by its group's path and
    /// its name, written apart from [`EvidenceBound::of`], which the edges'
    /// preparation reads, so that two bounds counting alike cannot trade
    /// places unseen (plan 025 decision #295).
    const COUNTED: [(LimitKind, WorkKind, &str); 26] = [
        (
            LimitKind::SharedFindingOccurrences,
            WorkKind::SharedEvidenceFindingCandidates,
            "diagnostics.shared.maxFindingOccurrences",
        ),
        (
            LimitKind::SharedFindings,
            WorkKind::SharedEvidenceFindings,
            "diagnostics.shared.maxFindings",
        ),
        (
            LimitKind::SharedEvaluationStatusEntries,
            WorkKind::SharedEvidenceEvaluationStatuses,
            "diagnostics.shared.maxEvaluationStatusEntries",
        ),
        (
            LimitKind::SharedSuppressionCausesPerStatus,
            WorkKind::SharedEvidenceMaximumCausesPerStatus,
            "diagnostics.shared.maxSuppressionCausesPerStatus",
        ),
        (
            LimitKind::SharedSuppressionCauseOccurrences,
            WorkKind::SharedEvidenceCauseOccurrences,
            "diagnostics.shared.maxSuppressionCauseOccurrences",
        ),
        (
            LimitKind::SharedRecords,
            WorkKind::SharedEvidenceRecords,
            "diagnostics.shared.maxRecords",
        ),
        (
            LimitKind::SharedTotalCanonicalBytes,
            WorkKind::SharedEvidenceCanonicalBytes,
            "diagnostics.shared.maxTotalCanonicalBytes",
        ),
        (
            LimitKind::SharedScopeSubjectOccurrences,
            WorkKind::SharedEvidenceSubjectOccurrences,
            "evidenceMetadata.maxSubjectOccurrences",
        ),
        (
            LimitKind::SharedScopeTokenInputs,
            WorkKind::SharedEvidenceTokenInputs,
            "evidenceMetadata.maxTokenInputs",
        ),
        (
            LimitKind::SharedScopeMetadataWorkUnits,
            WorkKind::SharedEvidenceMetadataWorkUnits,
            "evidenceMetadata.maxMetadataWorkUnits",
        ),
        (
            LimitKind::EntryFindingOccurrences,
            WorkKind::EntryEvidenceFindingCandidates,
            "diagnostics.entry.maxFindingOccurrences",
        ),
        (
            LimitKind::EntryFindings,
            WorkKind::EntryEvidenceFindings,
            "diagnostics.entry.maxFindings",
        ),
        (
            LimitKind::EntryRelatedEvidencePerOccurrenceGroup,
            WorkKind::EntryEvidenceMaximumRelatedEvidencePerOccurrenceGroup,
            "diagnostics.entry.maxRelatedEvidencePerOccurrenceGroup",
        ),
        (
            LimitKind::EntryRelatedEvidenceOccurrences,
            WorkKind::EntryEvidenceRelatedEvidenceOccurrences,
            "diagnostics.entry.maxRelatedEvidenceOccurrences",
        ),
        (
            LimitKind::EntrySingleEvidenceBytes,
            WorkKind::EntryEvidenceMaximumSingleEvidenceBytes,
            "diagnostics.entry.maxSingleEvidenceBytes",
        ),
        (
            LimitKind::EntryTotalEvidenceBytes,
            WorkKind::EntryEvidenceTotalEvidenceBytes,
            "diagnostics.entry.maxTotalEvidenceBytes",
        ),
        (
            LimitKind::EntryEvaluationStatusEntries,
            WorkKind::EntryEvidenceEvaluationStatuses,
            "diagnostics.entry.maxEvaluationStatusEntries",
        ),
        (
            LimitKind::EntrySuppressionCausesPerStatus,
            WorkKind::EntryEvidenceMaximumCausesPerStatus,
            "diagnostics.entry.maxSuppressionCausesPerStatus",
        ),
        (
            LimitKind::EntrySuppressionCauseOccurrences,
            WorkKind::EntryEvidenceCauseOccurrences,
            "diagnostics.entry.maxSuppressionCauseOccurrences",
        ),
        (
            LimitKind::EntryRecords,
            WorkKind::EntryEvidenceRecords,
            "diagnostics.entry.maxRecords",
        ),
        (
            LimitKind::EntryTotalCanonicalBytes,
            WorkKind::EntryEvidenceCanonicalBytes,
            "diagnostics.entry.maxTotalCanonicalBytes",
        ),
        (
            LimitKind::InvocationLogicalPathSegments,
            WorkKind::EntryEvidenceMaximumLogicalPathSegments,
            "evidenceMetadata.maxLogicalPathSegments",
        ),
        (
            LimitKind::InvocationLogicalPathCanonicalBytes,
            WorkKind::EntryEvidenceMaximumLogicalPathCanonicalBytes,
            "evidenceMetadata.maxLogicalPathCanonicalBytes",
        ),
        (
            LimitKind::InvocationSubjectOccurrences,
            WorkKind::EntryEvidenceSubjectOccurrences,
            "evidenceMetadata.maxSubjectOccurrences",
        ),
        (
            LimitKind::InvocationTokenInputs,
            WorkKind::EntryEvidenceTokenInputs,
            "evidenceMetadata.maxTokenInputs",
        ),
        (
            LimitKind::InvocationMetadataWorkUnits,
            WorkKind::EntryEvidenceMetadataWorkUnits,
            "evidenceMetadata.maxMetadataWorkUnits",
        ),
    ];

    /// The group's path and the name of the bound that exhausted a domain.
    fn path(bound: ExhaustedBound) -> String {
        match bound {
            ExhaustedBound::Domain(id) => format!("{}.{}", id.group.path(), id.name),
            ExhaustedBound::Capability(id) => format!("{}.{}", id.group.path(), id.name),
        }
    }

    /// Each edge of `edges` on `operation` sits on its recipe at the count
    /// the recipe's unbounded case reports in its logical work, and one
    /// under it: the capability sets the bound to that limit, and only the
    /// first-over edge's domain is exhausted, naming the bound, as `exhausted`
    /// reads it from the output. What counts the bound and the bound itself
    /// are the ones [`COUNTED`] writes.
    fn each_edge_sits_at_its_count_and_one_under(
        operation: crate::benchmark::operation::Operation,
        edges: &[(LimitKind, EvidenceRecipe)],
        exhausted: impl Fn(&crate::benchmark::operation::Output) -> Option<ExhaustedBound>,
    ) {
        use crate::benchmark::cases::prepare::prepare;
        use crate::benchmark::cases::{declarations, Declaration, LimitEdge, Recipe};
        use crate::benchmark::operation::Prepared;

        let case = |recipe, limit: Option<(LimitKind, LimitEdge)>| -> Declaration {
            declarations()
                .into_iter()
                .find(|case| {
                    case.operation == operation
                        && case.fixture == Recipe::Evidence(recipe)
                        && case.limit == limit
                })
                .unwrap()
        };
        for &(kind, recipe) in edges {
            let (bound, work) = EvidenceBound::of(kind).unwrap();
            let &(_, counts, named) = COUNTED
                .iter()
                .find(|(written, ..)| *written == kind)
                .unwrap_or_else(|| panic!("{kind:?} is not written"));
            assert_eq!(work, counts, "{kind:?}");
            assert_eq!(path(bound.exhausted()), named, "{kind:?}");
            let counted = prepare(&case(recipe, None))
                .unwrap()
                .logical_work
                .exact(work)
                .unwrap();
            for (edge, limit) in [
                (LimitEdge::Exact, counted),
                (LimitEdge::FirstOver, counted - 1),
            ] {
                let candidate = prepare(&case(recipe, Some((kind, edge))))
                    .unwrap_or_else(|error| panic!("{kind:?} {edge:?}: {error:?}"));
                let (Prepared::SharedEvidence(prepared) | Prepared::EntryEvidence(prepared)) =
                    &candidate.prepared
                else {
                    unreachable!()
                };
                let capability = &prepared.capability;
                let set = match bound {
                    EvidenceBound::Diagnostic(group, name) => capability
                        .policy_comparable()
                        .bounds()
                        .into_iter()
                        .find(|(id, _)| id.group == group && id.name == name)
                        .map(|(_, value)| value),
                    EvidenceBound::Metadata(metadata) => capability
                        .bootstrap_only()
                        .bounds()
                        .into_iter()
                        .find(|(id, _)| *id == metadata.id())
                        .map(|(_, value)| value),
                };
                assert_eq!(set.unwrap().get(), limit, "{kind:?} {edge:?}");
                let expected = (edge == LimitEdge::FirstOver).then(|| bound.exhausted());
                assert_eq!(exhausted(&candidate.output), expected, "{kind:?} {edge:?}");
            }
        }
    }

    /// Each bound the Shared projection stage counts has an exact and a
    /// first-over edge on its recipe, one apart: the exact edge's limit is the
    /// count the recipe's unbounded case reports in its logical work, and its
    /// Shared domain commits whole, and the first-over edge's bound exhausts
    /// the Shared domain, which its terminal Finding names (plan 025 decision
    /// #263).
    #[test]
    fn each_shared_edge_sits_at_its_count_and_one_under() {
        use crate::benchmark::operation::{Operation, Output};

        each_edge_sits_at_its_count_and_one_under(
            Operation::SharedEvidence,
            &SHARED_EDGES,
            |output| {
                let Output::SharedEvidence(resolved) = output else {
                    unreachable!()
                };
                let retained = resolved.result.as_ref().unwrap();
                retained.exhausted.as_ref().map(|(_, bound)| *bound)
            },
        );
    }

    /// Each bound the Entry projection stage counts has an exact and a
    /// first-over edge on its recipe, one apart: the exact edge's limit is the
    /// count the recipe's unbounded Entry case reports in its logical work,
    /// and its Entry domain commits whole, and the first-over edge's bound
    /// exhausts the Entry domain, which its terminal Finding names (plan 025
    /// decision #264).
    #[test]
    fn each_entry_edge_sits_at_its_count_and_one_under() {
        use crate::benchmark::operation::{Operation, Output};

        each_edge_sits_at_its_count_and_one_under(
            Operation::EntryEvidence,
            &ENTRY_EDGES,
            |output| {
                let Output::EntryEvidence(resolved) = output else {
                    unreachable!()
                };
                let retained = resolved.result.as_ref().unwrap();
                retained.exhausted.as_ref().map(|(_, bound)| *bound)
            },
        );
    }

    /// The inadmissible file is the example's with its project default three
    /// times and the first `ja` a lone surrogate, which no JSON-compatible
    /// value holds.
    #[test]
    fn the_inadmissible_file_repeats_a_member_and_holds_a_lone_surrogate() {
        let limits = limits();
        let source = String::from_utf8(EvidenceRecipe::Inadmissible.source(&limits)).unwrap();
        let example = String::from_utf8(EvidenceRecipe::Storefront.source(&limits)).unwrap();
        let default = r#""defaultRequestedLocale":"en""#;
        assert_eq!(
            source.matches(default).count(),
            example.matches(default).count() + 2
        );
        assert_eq!(source.matches(r#""\ud800""#).count(), 1);
        assert_eq!(
            source.replacen(r#""\ud800""#, r#""ja""#, 1).replacen(
                &[default; 3].join(","),
                default,
                1
            ),
            example
        );
        assert!(serde_json::from_str::<Value>(&source).is_err());
    }

    /// Every diagnostic bound is at its room, and every other bound is the
    /// capacity's.
    #[test]
    fn only_the_diagnostic_bounds_differ_from_the_capacity() {
        let capacity = capacity().bounds();
        for (id, value) in limits().bounds() {
            if matches!(id.group, Group::DiagnosticsEntry | Group::DiagnosticsShared) {
                assert_eq!(value.get(), ROOM, "{id:?}");
            } else {
                assert!(capacity.contains(&(id, value)), "{id:?}");
            }
        }
    }
}
