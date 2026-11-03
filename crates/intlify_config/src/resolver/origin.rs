// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's evidence kinds and Safe Origin Kinds.
//!
//! An evidence kind says what an origin-bearing Evidence item describes; a
//! Safe Origin Kind says through which fixed input boundary the item entered
//! the invocation. Revision `"0"` closes both vocabularies: seven evidence
//! kinds, and ten Safe Origin Kinds in a fixed rank order, each permitting
//! the evidence kinds of its row of 015's table. Equality uses the ID, and
//! ordering the rank. The Safe Origin Kind Registry is written from these
//! values.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What an origin-bearing Evidence item describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EvidenceKind {
    EntrySource,
    ConfigurationValue,
    ProfileSelector,
    SpecificationInput,
    ArtifactInput,
    VerificationInput,
    ImplementationCapability,
}

impl EvidenceKind {
    /// Every evidence kind, in the order 015 lists them.
    pub(crate) const ALL: [Self; 7] = [
        Self::EntrySource,
        Self::ConfigurationValue,
        Self::ProfileSelector,
        Self::SpecificationInput,
        Self::ArtifactInput,
        Self::VerificationInput,
        Self::ImplementationCapability,
    ];

    /// 015's ID.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::EntrySource => "entry-source",
            Self::ConfigurationValue => "configuration-value",
            Self::ProfileSelector => "profile-selector",
            Self::SpecificationInput => "specification-input",
            Self::ArtifactInput => "artifact-input",
            Self::VerificationInput => "verification-input",
            Self::ImplementationCapability => "implementation-capability",
        }
    }
}

/// The fixed input boundary that supplied an origin-bearing Evidence item,
/// declared in rank order.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SafeOriginKind {
    ConfigurationFile,
    ConfigurationProgrammatic,
    ProfileSelectorCli,
    ProfileSelectorProgrammatic,
    ProfileResolutionSpecificationInput,
    IntentSurfaceClassVocabulary,
    LocaleCanonicalizationData,
    ProfileResolutionArtifactCollection,
    ResourceLimitPolicyVerification,
    ImplementationCapability,
}

impl SafeOriginKind {
    /// Every Safe Origin Kind, in rank order.
    pub(crate) const ALL: [Self; 10] = [
        Self::ConfigurationFile,
        Self::ConfigurationProgrammatic,
        Self::ProfileSelectorCli,
        Self::ProfileSelectorProgrammatic,
        Self::ProfileResolutionSpecificationInput,
        Self::IntentSurfaceClassVocabulary,
        Self::LocaleCanonicalizationData,
        Self::ProfileResolutionArtifactCollection,
        Self::ResourceLimitPolicyVerification,
        Self::ImplementationCapability,
    ];

    /// 015's ID.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::ConfigurationFile => "configuration-file",
            Self::ConfigurationProgrammatic => "configuration-programmatic",
            Self::ProfileSelectorCli => "profile-selector-cli",
            Self::ProfileSelectorProgrammatic => "profile-selector-programmatic",
            Self::ProfileResolutionSpecificationInput => "profile-resolution-specification-input",
            Self::IntentSurfaceClassVocabulary => "intent-surface-class-vocabulary",
            Self::LocaleCanonicalizationData => "locale-canonicalization-data",
            Self::ProfileResolutionArtifactCollection => "profile-resolution-artifact-collection",
            Self::ResourceLimitPolicyVerification => "resource-limit-policy-verification",
            Self::ImplementationCapability => "implementation-capability",
        }
    }

    /// 015's fixed rank.
    pub(crate) const fn rank(self) -> usize {
        self as usize
    }

    /// The evidence kinds the kind permits, in the order of 015's table.
    pub(crate) const fn permitted_evidence_kinds(self) -> &'static [EvidenceKind] {
        use EvidenceKind as E;
        match self {
            Self::ConfigurationFile | Self::ConfigurationProgrammatic => {
                &[E::EntrySource, E::ConfigurationValue]
            }
            Self::ProfileSelectorCli | Self::ProfileSelectorProgrammatic => &[E::ProfileSelector],
            Self::ProfileResolutionSpecificationInput => &[E::SpecificationInput],
            Self::IntentSurfaceClassVocabulary
            | Self::LocaleCanonicalizationData
            | Self::ProfileResolutionArtifactCollection => &[E::ArtifactInput],
            Self::ResourceLimitPolicyVerification => &[E::VerificationInput],
            Self::ImplementationCapability => &[E::ImplementationCapability],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_kinds_are_015s_seven() {
        assert_eq!(
            EvidenceKind::ALL.map(EvidenceKind::id),
            [
                "entry-source",
                "configuration-value",
                "profile-selector",
                "specification-input",
                "artifact-input",
                "verification-input",
                "implementation-capability",
            ]
        );
        for kind in EvidenceKind::ALL {
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.id(), "{kind:?}");
        }
    }

    #[test]
    fn safe_origin_kinds_are_015s_ten_in_rank_order() {
        let ids = |kinds: &[EvidenceKind]| kinds.iter().map(|kind| kind.id()).collect::<Vec<_>>();
        let table = SafeOriginKind::ALL
            .map(|kind| (kind.rank(), kind.id(), ids(kind.permitted_evidence_kinds())));
        assert_eq!(
            table,
            [
                (
                    0,
                    "configuration-file",
                    vec!["entry-source", "configuration-value"]
                ),
                (
                    1,
                    "configuration-programmatic",
                    vec!["entry-source", "configuration-value"]
                ),
                (2, "profile-selector-cli", vec!["profile-selector"]),
                (3, "profile-selector-programmatic", vec!["profile-selector"]),
                (
                    4,
                    "profile-resolution-specification-input",
                    vec!["specification-input"]
                ),
                (5, "intent-surface-class-vocabulary", vec!["artifact-input"]),
                (6, "locale-canonicalization-data", vec!["artifact-input"]),
                (
                    7,
                    "profile-resolution-artifact-collection",
                    vec!["artifact-input"]
                ),
                (
                    8,
                    "resource-limit-policy-verification",
                    vec!["verification-input"]
                ),
                (
                    9,
                    "implementation-capability",
                    vec!["implementation-capability"]
                ),
            ]
        );
        for kind in SafeOriginKind::ALL {
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.id(), "{kind:?}");
        }
        // The rank orders the kinds.
        let mut sorted = SafeOriginKind::ALL;
        sorted.sort();
        assert_eq!(sorted, SafeOriginKind::ALL);
    }

    #[test]
    fn every_evidence_kind_enters_through_some_origin() {
        for evidence in EvidenceKind::ALL {
            assert!(
                SafeOriginKind::ALL
                    .iter()
                    .any(|kind| kind.permitted_evidence_kinds().contains(&evidence)),
                "{evidence:?}"
            );
        }
    }
}
