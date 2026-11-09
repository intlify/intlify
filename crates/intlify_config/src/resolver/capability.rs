// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The bounds of 015's Project Profile Resolver Capability Specification:
//! `policyComparable`, which mirrors the Resource Limit Policy's
//! `projectProfileResolution` section, and `bootstrapOnly`, for the work no
//! Resource Limit Policy bounds, with the revision-"0" minimum of every bound
//! and the unit 015's tables count it in. Not the Capability Specification
//! artifact, which construction admits and which is written from this
//! inventory, and not the capability boundary (`crate::boundary::capability`),
//! which admits a submitted value against it.

use std::fmt;

use crate::input_limits::{Bound, RawInputLimits};

use super::limits::{BoundId, ResolutionLimits, BOUND_COUNT};

/// The namespace whose bounds mirror the Resource Limit Policy's.
pub(crate) const POLICY_COMPARABLE: &str = "policyComparable";

/// The namespace whose bounds no Resource Limit Policy has.
pub(crate) const BOOTSTRAP_ONLY: &str = "bootstrapOnly";

/// `bootstrapOnly`'s groups in 015's canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BootstrapGroup {
    RawInput,
    Snapshot,
    InputBoundary,
    ArtifactBootstrap,
    LocaleData,
    EvidenceMetadata,
    MandatoryReserve,
}

impl BootstrapGroup {
    pub(crate) const ALL: [Self; 7] = [
        Self::RawInput,
        Self::Snapshot,
        Self::InputBoundary,
        Self::ArtifactBootstrap,
        Self::LocaleData,
        Self::EvidenceMetadata,
        Self::MandatoryReserve,
    ];

    /// The group's member name inside `bootstrapOnly`.
    pub(crate) const fn path(self) -> &'static str {
        match self {
            Self::RawInput => "rawInput",
            Self::Snapshot => "snapshot",
            Self::InputBoundary => "inputBoundary",
            Self::ArtifactBootstrap => "artifactBootstrap",
            Self::LocaleData => "localeData",
            Self::EvidenceMetadata => "evidenceMetadata",
            Self::MandatoryReserve => "mandatoryReserve",
        }
    }
}

/// Where one bound sits in `bootstrapOnly`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BootstrapBoundId {
    pub(crate) group: BootstrapGroup,
    pub(crate) name: &'static str,
}

/// The number of bounds in `bootstrapOnly`.
pub(crate) const BOOTSTRAP_BOUND_COUNT: usize = 54;

/// The number of capability bounds in both namespaces.
pub(crate) const CAPABILITY_BOUND_COUNT: usize = BOUND_COUNT + BOOTSTRAP_BOUND_COUNT;

/// Each `bootstrapOnly` bound's counting unit, as the cell of 015's table
/// writes it, in the canonical order.
const UNITS: [(BootstrapGroup, &str, &str); BOOTSTRAP_BOUND_COUNT] = {
    use BootstrapGroup::{
        ArtifactBootstrap, EvidenceMetadata, InputBoundary, LocaleData, MandatoryReserve, RawInput,
        Snapshot,
    };
    [
        (
            RawInput,
            "maxFileBytes",
            "One submitted raw file byte",
        ),
        (
            RawInput,
            "maxParserTokens",
            "One strict-JSON lexical value, member-name, or punctuation token",
        ),
        (
            Snapshot,
            "maxNodes",
            "One canonical Snapshot node, including a rejected marker",
        ),
        (
            Snapshot,
            "maxDepth",
            "One Snapshot tree level, with the root at one",
        ),
        (
            Snapshot,
            "maxCollectionEntries",
            "One Snapshot object-member or array-element occurrence",
        ),
        (
            Snapshot,
            "maxTotalStringBytes",
            "One UTF-8 byte in a Snapshot string or object key",
        ),
        (
            Snapshot,
            "maxSingleStringBytes",
            "UTF-8 byte length of one Snapshot string or object key",
        ),
        (
            Snapshot,
            "maxRejectedMarkers",
            "One canonical rejected-node marker",
        ),
        (
            Snapshot,
            "maxEnvelopeCanonicalBytes",
            "One canonical byte in the complete formal Snapshot envelope",
        ),
        (
            Snapshot,
            "maxSingleMarkerCanonicalBytes",
            "One canonical byte in one rejected-node marker",
        ),
        (
            Snapshot,
            "maxTraversalUnits",
            "One visited Snapshot node or edge",
        ),
        (
            InputBoundary,
            "maxDepth",
            "One JSON nesting level of one raw boundary input or collection member, with its root at one",
        ),
        (
            InputBoundary,
            "maxSpecificationInputBytes",
            "One byte of the raw Profile Resolution Specification Input",
        ),
        (
            InputBoundary,
            "maxVocabularyBytes",
            "One byte of the raw Intent Surface-Class Vocabulary Artifact",
        ),
        (
            InputBoundary,
            "maxVocabularyMembers",
            "One vocabulary member",
        ),
        (
            InputBoundary,
            "maxVocabularyMemberBytes",
            "UTF-8 byte length of one vocabulary member",
        ),
        (
            InputBoundary,
            "maxVocabularyTotalMemberBytes",
            "One UTF-8 byte across all vocabulary members",
        ),
        (
            InputBoundary,
            "maxLocaleDataManifestBytes",
            "One byte of the raw Locale Canonicalization Data Artifact manifest",
        ),
        (
            InputBoundary,
            "maxCollectionMemberBytes",
            "Raw byte length of one submitted collection member",
        ),
        (
            InputBoundary,
            "maxCollectionTotalBytes",
            "One raw byte across all submitted collection members",
        ),
        (
            InputBoundary,
            "maxVerificationInputBytes",
            "One byte of the raw Resource Limit Policy Verification Input",
        ),
        (
            InputBoundary,
            "maxVerificationPins",
            "One submitted pin",
        ),
        (
            ArtifactBootstrap,
            "maxEnvelopeDepth",
            "One object or array nesting level of the selected candidate member, with the member root at one",
        ),
        (
            ArtifactBootstrap,
            "maxDecodedAllocationBytes",
            "One UTF-8 byte of a string value or object key decoded from the selected candidate's `content`",
        ),
        (
            ArtifactBootstrap,
            "maxMetadataScanUnits",
            "One envelope field read from one member to establish its exact-reference tuple: `kind`, `specification.identity`, `specification.revision`, `body.identity`, or `body.revision`, in that order through the first that is absent or not a string",
        ),
        (
            ArtifactBootstrap,
            "maxValidationWorkUnits",
            "One evaluation, counted whether or not it runs, of the selected candidate's integrity, its pin lookup, its common body, or one `projectProfileResolution` rule: the section's presence, the closed shape of the section, of one of its five groups, or of one of its two `diagnostics` reporting-domain groups, or one bound's value",
        ),
        (
            LocaleData,
            "maxPayloads",
            "One payload entry of the admitted manifest",
        ),
        (
            LocaleData,
            "maxPayloadBytes",
            "Declared byte length of one payload",
        ),
        (
            LocaleData,
            "maxMarkerBytes",
            "One byte of the four marker payloads as compact JSON text",
        ),
        (
            LocaleData,
            "maxValidityGroups",
            "One conformance validity group",
        ),
        (
            LocaleData,
            "maxCodes",
            "One conformance code",
        ),
        (
            LocaleData,
            "maxExtensionKeys",
            "One conformance extension key",
        ),
        (
            LocaleData,
            "maxTypes",
            "One conformance type",
        ),
        (
            LocaleData,
            "maxCorrections",
            "One conformance correction",
        ),
        (
            LocaleData,
            "maxValidationWorkUnits",
            "One conformance group, code, key, type, or correction, each counted once",
        ),
        (
            EvidenceMetadata,
            "maxSourceIdentityBytes",
            "One byte in an exact source-identity input",
        ),
        (
            EvidenceMetadata,
            "maxSourceRevisionBytes",
            "One byte in an exact Source-content, Programmatic Entry, or Call-Site Source revision input",
        ),
        (
            EvidenceMetadata,
            "maxLocatorBytes",
            "One byte in a locator candidate",
        ),
        (
            EvidenceMetadata,
            "maxCallSiteSourceBytes",
            "One byte in the optional exact UTF-8 call-site source snapshot",
        ),
        (
            EvidenceMetadata,
            "maxCallSiteMappings",
            "One submitted Snapshot Structural Path and location-role mapping",
        ),
        (
            EvidenceMetadata,
            "maxLogicalPathSegments",
            "One internal or projected path segment",
        ),
        (
            EvidenceMetadata,
            "maxLogicalPathCanonicalBytes",
            "One canonical byte in one projected logical path",
        ),
        (
            EvidenceMetadata,
            "maxSubjectOccurrences",
            "One pre-deduplication logical subject occurrence",
        ),
        (
            EvidenceMetadata,
            "maxTokenInputs",
            "One safe projected token-input occurrence",
        ),
        (
            EvidenceMetadata,
            "maxCollisionComparisons",
            "One full safe-projection comparison inside a digest-collision bucket",
        ),
        (
            EvidenceMetadata,
            "maxMetadataWorkUnits",
            "One source, locator, path, projection, or token rule evaluation",
        ),
        (
            MandatoryReserve,
            "maxOutcomeEnvelopeSlots",
            "One complete checked-or-blocked outcome envelope",
        ),
        (
            MandatoryReserve,
            "maxConstructionIdentitySlots",
            "One outcome-level Resolver Construction Identity",
        ),
        (
            MandatoryReserve,
            "maxEntryTerminalFindingSlots",
            "One Entry-domain terminal Finding",
        ),
        (
            MandatoryReserve,
            "maxEntryTerminalStatusSlots",
            "One Entry-domain terminal evaluation-status record",
        ),
        (
            MandatoryReserve,
            "maxSharedTerminalFindingSlots",
            "One Shared-domain terminal Finding",
        ),
        (
            MandatoryReserve,
            "maxSharedTerminalStatusSlots",
            "One Shared-domain terminal evaluation-status record",
        ),
        (
            MandatoryReserve,
            "maxSharedUnavailableCauseSlots",
            "One Entry-owned Shared-unavailability cause reference",
        ),
        (
            MandatoryReserve,
            "maxCanonicalBytes",
            "One byte in the complete revision-`\"0\"` mandatory logical reserve representation",
        ),
    ]
};

impl BootstrapBoundId {
    /// The unit the bound counts in, as 015's table writes it.
    ///
    /// # Panics
    ///
    /// Panics for an ID no `bootstrapOnly` bound has.
    pub(crate) fn unit(self) -> &'static str {
        UNITS
            .iter()
            .find(|(group, name, _)| *group == self.group && *name == self.name)
            .map(|(_, _, unit)| *unit)
            .expect("every bootstrapOnly bound has a unit")
    }
}

/// Every `bootstrapOnly` bound. Each is required, and none has a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BootstrapLimits {
    pub(crate) raw_input: RawInputLimits,
    pub(crate) snapshot: SnapshotLimits,
    pub(crate) input_boundary: InputBoundaryLimits,
    pub(crate) artifact_bootstrap: ArtifactBootstrapLimits,
    pub(crate) locale_data: LocaleDataLimits,
    pub(crate) evidence_metadata: EvidenceMetadataLimits,
    pub(crate) mandatory_reserve: MandatoryReserveLimits,
}

/// Safe host inspection, construction, and admission of a Programmatic Entry
/// Snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct SnapshotLimits {
    pub(crate) max_nodes: Bound,
    pub(crate) max_depth: Bound,
    pub(crate) max_collection_entries: Bound,
    pub(crate) max_total_string_bytes: Bound,
    pub(crate) max_single_string_bytes: Bound,
    pub(crate) max_rejected_markers: Bound,
    pub(crate) max_envelope_canonical_bytes: Bound,
    pub(crate) max_single_marker_canonical_bytes: Bound,
    pub(crate) max_traversal_units: Bound,
}

/// The raw transport bytes of the other pre-invocation inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct InputBoundaryLimits {
    pub(crate) max_depth: Bound,
    pub(crate) max_specification_input_bytes: Bound,
    pub(crate) max_vocabulary_bytes: Bound,
    pub(crate) max_vocabulary_members: Bound,
    pub(crate) max_vocabulary_member_bytes: Bound,
    pub(crate) max_vocabulary_total_member_bytes: Bound,
    pub(crate) max_locale_data_manifest_bytes: Bound,
    pub(crate) max_collection_member_bytes: Bound,
    pub(crate) max_collection_total_bytes: Bound,
    pub(crate) max_verification_input_bytes: Bound,
    pub(crate) max_verification_pins: Bound,
}

/// Decoding and verifying the selected Resource Limit Policy candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct ArtifactBootstrapLimits {
    pub(crate) max_envelope_depth: Bound,
    pub(crate) max_decoded_allocation_bytes: Bound,
    pub(crate) max_metadata_scan_units: Bound,
    pub(crate) max_validation_work_units: Bound,
}

/// Reading the Locale Canonicalization Data Artifact at specification
/// admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct LocaleDataLimits {
    pub(crate) max_payloads: Bound,
    pub(crate) max_payload_bytes: Bound,
    pub(crate) max_marker_bytes: Bound,
    pub(crate) max_validity_groups: Bound,
    pub(crate) max_codes: Bound,
    pub(crate) max_extension_keys: Bound,
    pub(crate) max_types: Bound,
    pub(crate) max_corrections: Bound,
    pub(crate) max_validation_work_units: Bound,
}

/// Evidence metadata before a Resource Limit Policy is admitted: sources,
/// locators, paths, subjects, tokens, and digest collisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct EvidenceMetadataLimits {
    pub(crate) max_source_identity_bytes: Bound,
    pub(crate) max_source_revision_bytes: Bound,
    pub(crate) max_locator_bytes: Bound,
    pub(crate) max_call_site_source_bytes: Bound,
    pub(crate) max_call_site_mappings: Bound,
    pub(crate) max_logical_path_segments: Bound,
    pub(crate) max_logical_path_canonical_bytes: Bound,
    pub(crate) max_subject_occurrences: Bound,
    pub(crate) max_token_inputs: Bound,
    pub(crate) max_collision_comparisons: Bound,
    pub(crate) max_metadata_work_units: Bound,
}

/// The fixed records every outcome must be able to hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct MandatoryReserveLimits {
    pub(crate) max_outcome_envelope_slots: Bound,
    pub(crate) max_construction_identity_slots: Bound,
    pub(crate) max_entry_terminal_finding_slots: Bound,
    pub(crate) max_entry_terminal_status_slots: Bound,
    pub(crate) max_shared_terminal_finding_slots: Bound,
    pub(crate) max_shared_terminal_status_slots: Bound,
    pub(crate) max_shared_unavailable_cause_slots: Bound,
    pub(crate) max_canonical_bytes: Bound,
}

/// `value` as a bound, for the constant minimum vector.
const fn minimum(value: u64) -> Bound {
    match Bound::new(value) {
        Some(bound) => bound,
        None => panic!("a minimum is a bound"),
    }
}

impl BootstrapLimits {
    /// Revision "0"'s mandatory minimum vector. A capability may set any bound
    /// higher, never lower. Every minimum is one except the two Snapshot byte
    /// minima and the mandatory reserve's canonical bytes.
    pub(crate) const MINIMUM: Self = {
        let one = minimum(1);
        Self {
            raw_input: RawInputLimits {
                max_file_bytes: one,
                max_parser_tokens: one,
            },
            snapshot: SnapshotLimits {
                max_nodes: one,
                max_depth: one,
                max_collection_entries: one,
                max_total_string_bytes: one,
                max_single_string_bytes: one,
                max_rejected_markers: one,
                max_envelope_canonical_bytes: minimum(4096),
                max_single_marker_canonical_bytes: minimum(1024),
                max_traversal_units: one,
            },
            input_boundary: InputBoundaryLimits {
                max_depth: one,
                max_specification_input_bytes: one,
                max_vocabulary_bytes: one,
                max_vocabulary_members: one,
                max_vocabulary_member_bytes: one,
                max_vocabulary_total_member_bytes: one,
                max_locale_data_manifest_bytes: one,
                max_collection_member_bytes: one,
                max_collection_total_bytes: one,
                max_verification_input_bytes: one,
                max_verification_pins: one,
            },
            artifact_bootstrap: ArtifactBootstrapLimits {
                max_envelope_depth: one,
                max_decoded_allocation_bytes: one,
                max_metadata_scan_units: one,
                max_validation_work_units: one,
            },
            locale_data: LocaleDataLimits {
                max_payloads: one,
                max_payload_bytes: one,
                max_marker_bytes: one,
                max_validity_groups: one,
                max_codes: one,
                max_extension_keys: one,
                max_types: one,
                max_corrections: one,
                max_validation_work_units: one,
            },
            evidence_metadata: EvidenceMetadataLimits {
                max_source_identity_bytes: one,
                max_source_revision_bytes: one,
                max_locator_bytes: one,
                max_call_site_source_bytes: one,
                max_call_site_mappings: one,
                max_logical_path_segments: one,
                max_logical_path_canonical_bytes: one,
                max_subject_occurrences: one,
                max_token_inputs: one,
                max_collision_comparisons: one,
                max_metadata_work_units: one,
            },
            mandatory_reserve: MandatoryReserveLimits {
                max_outcome_envelope_slots: one,
                max_construction_identity_slots: one,
                max_entry_terminal_finding_slots: one,
                max_entry_terminal_status_slots: one,
                max_shared_terminal_finding_slots: one,
                max_shared_terminal_status_slots: one,
                max_shared_unavailable_cause_slots: one,
                max_canonical_bytes: minimum(65536),
            },
        }
    };

    /// Every bound in 015's canonical order: the groups, then each group's
    /// rows in the table's order. The capability boundary tests the minimum
    /// vector in this order.
    pub(crate) fn bounds(&self) -> [(BootstrapBoundId, Bound); BOOTSTRAP_BOUND_COUNT] {
        use BootstrapGroup::{
            ArtifactBootstrap, EvidenceMetadata, InputBoundary, LocaleData, MandatoryReserve,
            RawInput, Snapshot,
        };
        let id = |group, name| BootstrapBoundId { group, name };
        let raw = &self.raw_input;
        let snapshot = &self.snapshot;
        let input = &self.input_boundary;
        let artifact = &self.artifact_bootstrap;
        let locale = &self.locale_data;
        let evidence = &self.evidence_metadata;
        let reserve = &self.mandatory_reserve;
        [
            (id(RawInput, "maxFileBytes"), raw.max_file_bytes),
            (id(RawInput, "maxParserTokens"), raw.max_parser_tokens),
            (id(Snapshot, "maxNodes"), snapshot.max_nodes),
            (id(Snapshot, "maxDepth"), snapshot.max_depth),
            (
                id(Snapshot, "maxCollectionEntries"),
                snapshot.max_collection_entries,
            ),
            (
                id(Snapshot, "maxTotalStringBytes"),
                snapshot.max_total_string_bytes,
            ),
            (
                id(Snapshot, "maxSingleStringBytes"),
                snapshot.max_single_string_bytes,
            ),
            (
                id(Snapshot, "maxRejectedMarkers"),
                snapshot.max_rejected_markers,
            ),
            (
                id(Snapshot, "maxEnvelopeCanonicalBytes"),
                snapshot.max_envelope_canonical_bytes,
            ),
            (
                id(Snapshot, "maxSingleMarkerCanonicalBytes"),
                snapshot.max_single_marker_canonical_bytes,
            ),
            (
                id(Snapshot, "maxTraversalUnits"),
                snapshot.max_traversal_units,
            ),
            (id(InputBoundary, "maxDepth"), input.max_depth),
            (
                id(InputBoundary, "maxSpecificationInputBytes"),
                input.max_specification_input_bytes,
            ),
            (
                id(InputBoundary, "maxVocabularyBytes"),
                input.max_vocabulary_bytes,
            ),
            (
                id(InputBoundary, "maxVocabularyMembers"),
                input.max_vocabulary_members,
            ),
            (
                id(InputBoundary, "maxVocabularyMemberBytes"),
                input.max_vocabulary_member_bytes,
            ),
            (
                id(InputBoundary, "maxVocabularyTotalMemberBytes"),
                input.max_vocabulary_total_member_bytes,
            ),
            (
                id(InputBoundary, "maxLocaleDataManifestBytes"),
                input.max_locale_data_manifest_bytes,
            ),
            (
                id(InputBoundary, "maxCollectionMemberBytes"),
                input.max_collection_member_bytes,
            ),
            (
                id(InputBoundary, "maxCollectionTotalBytes"),
                input.max_collection_total_bytes,
            ),
            (
                id(InputBoundary, "maxVerificationInputBytes"),
                input.max_verification_input_bytes,
            ),
            (
                id(InputBoundary, "maxVerificationPins"),
                input.max_verification_pins,
            ),
            (
                id(ArtifactBootstrap, "maxEnvelopeDepth"),
                artifact.max_envelope_depth,
            ),
            (
                id(ArtifactBootstrap, "maxDecodedAllocationBytes"),
                artifact.max_decoded_allocation_bytes,
            ),
            (
                id(ArtifactBootstrap, "maxMetadataScanUnits"),
                artifact.max_metadata_scan_units,
            ),
            (
                id(ArtifactBootstrap, "maxValidationWorkUnits"),
                artifact.max_validation_work_units,
            ),
            (id(LocaleData, "maxPayloads"), locale.max_payloads),
            (id(LocaleData, "maxPayloadBytes"), locale.max_payload_bytes),
            (id(LocaleData, "maxMarkerBytes"), locale.max_marker_bytes),
            (
                id(LocaleData, "maxValidityGroups"),
                locale.max_validity_groups,
            ),
            (id(LocaleData, "maxCodes"), locale.max_codes),
            (
                id(LocaleData, "maxExtensionKeys"),
                locale.max_extension_keys,
            ),
            (id(LocaleData, "maxTypes"), locale.max_types),
            (id(LocaleData, "maxCorrections"), locale.max_corrections),
            (
                id(LocaleData, "maxValidationWorkUnits"),
                locale.max_validation_work_units,
            ),
            (
                id(EvidenceMetadata, "maxSourceIdentityBytes"),
                evidence.max_source_identity_bytes,
            ),
            (
                id(EvidenceMetadata, "maxSourceRevisionBytes"),
                evidence.max_source_revision_bytes,
            ),
            (
                id(EvidenceMetadata, "maxLocatorBytes"),
                evidence.max_locator_bytes,
            ),
            (
                id(EvidenceMetadata, "maxCallSiteSourceBytes"),
                evidence.max_call_site_source_bytes,
            ),
            (
                id(EvidenceMetadata, "maxCallSiteMappings"),
                evidence.max_call_site_mappings,
            ),
            (
                id(EvidenceMetadata, "maxLogicalPathSegments"),
                evidence.max_logical_path_segments,
            ),
            (
                id(EvidenceMetadata, "maxLogicalPathCanonicalBytes"),
                evidence.max_logical_path_canonical_bytes,
            ),
            (
                id(EvidenceMetadata, "maxSubjectOccurrences"),
                evidence.max_subject_occurrences,
            ),
            (
                id(EvidenceMetadata, "maxTokenInputs"),
                evidence.max_token_inputs,
            ),
            (
                id(EvidenceMetadata, "maxCollisionComparisons"),
                evidence.max_collision_comparisons,
            ),
            (
                id(EvidenceMetadata, "maxMetadataWorkUnits"),
                evidence.max_metadata_work_units,
            ),
            (
                id(MandatoryReserve, "maxOutcomeEnvelopeSlots"),
                reserve.max_outcome_envelope_slots,
            ),
            (
                id(MandatoryReserve, "maxConstructionIdentitySlots"),
                reserve.max_construction_identity_slots,
            ),
            (
                id(MandatoryReserve, "maxEntryTerminalFindingSlots"),
                reserve.max_entry_terminal_finding_slots,
            ),
            (
                id(MandatoryReserve, "maxEntryTerminalStatusSlots"),
                reserve.max_entry_terminal_status_slots,
            ),
            (
                id(MandatoryReserve, "maxSharedTerminalFindingSlots"),
                reserve.max_shared_terminal_finding_slots,
            ),
            (
                id(MandatoryReserve, "maxSharedTerminalStatusSlots"),
                reserve.max_shared_terminal_status_slots,
            ),
            (
                id(MandatoryReserve, "maxSharedUnavailableCauseSlots"),
                reserve.max_shared_unavailable_cause_slots,
            ),
            (
                id(MandatoryReserve, "maxCanonicalBytes"),
                reserve.max_canonical_bytes,
            ),
        ]
    }

    /// The limits whose every bound `lookup` supplies, asked for in 015's
    /// canonical order, or `None` as soon as one is missing. Each bound's name
    /// sits beside the field it fills, as in [`Self::bounds`].
    pub(crate) fn from_bounds(
        mut lookup: impl FnMut(BootstrapBoundId) -> Option<Bound>,
    ) -> Option<Self> {
        use BootstrapGroup::{
            ArtifactBootstrap, EvidenceMetadata, InputBoundary, LocaleData, MandatoryReserve,
            RawInput, Snapshot,
        };
        let mut bound = |group, name| lookup(BootstrapBoundId { group, name });
        Some(Self {
            raw_input: RawInputLimits {
                max_file_bytes: bound(RawInput, "maxFileBytes")?,
                max_parser_tokens: bound(RawInput, "maxParserTokens")?,
            },
            snapshot: SnapshotLimits {
                max_nodes: bound(Snapshot, "maxNodes")?,
                max_depth: bound(Snapshot, "maxDepth")?,
                max_collection_entries: bound(Snapshot, "maxCollectionEntries")?,
                max_total_string_bytes: bound(Snapshot, "maxTotalStringBytes")?,
                max_single_string_bytes: bound(Snapshot, "maxSingleStringBytes")?,
                max_rejected_markers: bound(Snapshot, "maxRejectedMarkers")?,
                max_envelope_canonical_bytes: bound(Snapshot, "maxEnvelopeCanonicalBytes")?,
                max_single_marker_canonical_bytes: bound(
                    Snapshot,
                    "maxSingleMarkerCanonicalBytes",
                )?,
                max_traversal_units: bound(Snapshot, "maxTraversalUnits")?,
            },
            input_boundary: InputBoundaryLimits {
                max_depth: bound(InputBoundary, "maxDepth")?,
                max_specification_input_bytes: bound(InputBoundary, "maxSpecificationInputBytes")?,
                max_vocabulary_bytes: bound(InputBoundary, "maxVocabularyBytes")?,
                max_vocabulary_members: bound(InputBoundary, "maxVocabularyMembers")?,
                max_vocabulary_member_bytes: bound(InputBoundary, "maxVocabularyMemberBytes")?,
                max_vocabulary_total_member_bytes: bound(
                    InputBoundary,
                    "maxVocabularyTotalMemberBytes",
                )?,
                max_locale_data_manifest_bytes: bound(InputBoundary, "maxLocaleDataManifestBytes")?,
                max_collection_member_bytes: bound(InputBoundary, "maxCollectionMemberBytes")?,
                max_collection_total_bytes: bound(InputBoundary, "maxCollectionTotalBytes")?,
                max_verification_input_bytes: bound(InputBoundary, "maxVerificationInputBytes")?,
                max_verification_pins: bound(InputBoundary, "maxVerificationPins")?,
            },
            artifact_bootstrap: ArtifactBootstrapLimits {
                max_envelope_depth: bound(ArtifactBootstrap, "maxEnvelopeDepth")?,
                max_decoded_allocation_bytes: bound(
                    ArtifactBootstrap,
                    "maxDecodedAllocationBytes",
                )?,
                max_metadata_scan_units: bound(ArtifactBootstrap, "maxMetadataScanUnits")?,
                max_validation_work_units: bound(ArtifactBootstrap, "maxValidationWorkUnits")?,
            },
            locale_data: LocaleDataLimits {
                max_payloads: bound(LocaleData, "maxPayloads")?,
                max_payload_bytes: bound(LocaleData, "maxPayloadBytes")?,
                max_marker_bytes: bound(LocaleData, "maxMarkerBytes")?,
                max_validity_groups: bound(LocaleData, "maxValidityGroups")?,
                max_codes: bound(LocaleData, "maxCodes")?,
                max_extension_keys: bound(LocaleData, "maxExtensionKeys")?,
                max_types: bound(LocaleData, "maxTypes")?,
                max_corrections: bound(LocaleData, "maxCorrections")?,
                max_validation_work_units: bound(LocaleData, "maxValidationWorkUnits")?,
            },
            evidence_metadata: EvidenceMetadataLimits {
                max_source_identity_bytes: bound(EvidenceMetadata, "maxSourceIdentityBytes")?,
                max_source_revision_bytes: bound(EvidenceMetadata, "maxSourceRevisionBytes")?,
                max_locator_bytes: bound(EvidenceMetadata, "maxLocatorBytes")?,
                max_call_site_source_bytes: bound(EvidenceMetadata, "maxCallSiteSourceBytes")?,
                max_call_site_mappings: bound(EvidenceMetadata, "maxCallSiteMappings")?,
                max_logical_path_segments: bound(EvidenceMetadata, "maxLogicalPathSegments")?,
                max_logical_path_canonical_bytes: bound(
                    EvidenceMetadata,
                    "maxLogicalPathCanonicalBytes",
                )?,
                max_subject_occurrences: bound(EvidenceMetadata, "maxSubjectOccurrences")?,
                max_token_inputs: bound(EvidenceMetadata, "maxTokenInputs")?,
                max_collision_comparisons: bound(EvidenceMetadata, "maxCollisionComparisons")?,
                max_metadata_work_units: bound(EvidenceMetadata, "maxMetadataWorkUnits")?,
            },
            mandatory_reserve: MandatoryReserveLimits {
                max_outcome_envelope_slots: bound(MandatoryReserve, "maxOutcomeEnvelopeSlots")?,
                max_construction_identity_slots: bound(
                    MandatoryReserve,
                    "maxConstructionIdentitySlots",
                )?,
                max_entry_terminal_finding_slots: bound(
                    MandatoryReserve,
                    "maxEntryTerminalFindingSlots",
                )?,
                max_entry_terminal_status_slots: bound(
                    MandatoryReserve,
                    "maxEntryTerminalStatusSlots",
                )?,
                max_shared_terminal_finding_slots: bound(
                    MandatoryReserve,
                    "maxSharedTerminalFindingSlots",
                )?,
                max_shared_terminal_status_slots: bound(
                    MandatoryReserve,
                    "maxSharedTerminalStatusSlots",
                )?,
                max_shared_unavailable_cause_slots: bound(
                    MandatoryReserve,
                    "maxSharedUnavailableCauseSlots",
                )?,
                max_canonical_bytes: bound(MandatoryReserve, "maxCanonicalBytes")?,
            },
        })
    }
}

/// One capability bound, by its namespace, group, and name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityBound {
    PolicyComparable(BoundId),
    BootstrapOnly(BootstrapBoundId),
}

impl CapabilityBound {
    /// Every capability bound with its revision-"0" minimum, in the Capability
    /// Specification's canonical order: `policyComparable` in
    /// `projectProfileResolution`'s order, each with the minimum one, then
    /// `bootstrapOnly`.
    pub(crate) fn inventory() -> [(Self, Bound); CAPABILITY_BOUND_COUNT] {
        let one = minimum(1);
        let mirrored = ResolutionLimits::from_bounds(|_| Some(one))
            .expect("every bound is supplied")
            .bounds();
        let bootstrap = BootstrapLimits::MINIMUM.bounds();
        std::array::from_fn(|index| match index.checked_sub(BOUND_COUNT) {
            None => (Self::PolicyComparable(mirrored[index].0), one),
            Some(index) => (Self::BootstrapOnly(bootstrap[index].0), bootstrap[index].1),
        })
    }

    /// The namespace the bound belongs to.
    pub(crate) const fn namespace(self) -> &'static str {
        match self {
            Self::PolicyComparable(_) => POLICY_COMPARABLE,
            Self::BootstrapOnly(_) => BOOTSTRAP_ONLY,
        }
    }

    /// The bound's path inside its namespace, such as `snapshot.maxNodes`: its
    /// group's path and its name, joined by a dot. A `policyComparable` bound's
    /// path is the path of the policy bound it mirrors.
    pub(crate) fn path(self) -> String {
        let (group, name) = match self {
            Self::PolicyComparable(id) => (id.group.path(), id.name),
            Self::BootstrapOnly(id) => (id.group.path(), id.name),
        };
        format!("{group}.{name}")
    }

    /// The unit the bound counts in, as 015's tables write it.
    pub(crate) fn unit(self) -> &'static str {
        match self {
            Self::PolicyComparable(id) => id.unit(),
            Self::BootstrapOnly(id) => id.unit(),
        }
    }
}

impl fmt::Display for CapabilityBound {
    /// The bound's capability bound path, such as
    /// `bootstrapOnly.snapshot.maxNodes`: its namespace and its path inside
    /// it, joined by a dot.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.namespace(), self.path())
    }
}

#[cfg(test)]
mod tests {
    use super::super::limits::{fixture_limits, Group};
    use super::*;

    /// Distinct bounds counting up from `first` in field declaration order,
    /// written without the bound names.
    fn fixture(first: u64) -> BootstrapLimits {
        let mut next = first;
        let mut bound = || {
            let value = Bound::new(next).unwrap();
            next += 1;
            value
        };
        BootstrapLimits {
            raw_input: RawInputLimits {
                max_file_bytes: bound(),
                max_parser_tokens: bound(),
            },
            snapshot: SnapshotLimits {
                max_nodes: bound(),
                max_depth: bound(),
                max_collection_entries: bound(),
                max_total_string_bytes: bound(),
                max_single_string_bytes: bound(),
                max_rejected_markers: bound(),
                max_envelope_canonical_bytes: bound(),
                max_single_marker_canonical_bytes: bound(),
                max_traversal_units: bound(),
            },
            input_boundary: InputBoundaryLimits {
                max_depth: bound(),
                max_specification_input_bytes: bound(),
                max_vocabulary_bytes: bound(),
                max_vocabulary_members: bound(),
                max_vocabulary_member_bytes: bound(),
                max_vocabulary_total_member_bytes: bound(),
                max_locale_data_manifest_bytes: bound(),
                max_collection_member_bytes: bound(),
                max_collection_total_bytes: bound(),
                max_verification_input_bytes: bound(),
                max_verification_pins: bound(),
            },
            artifact_bootstrap: ArtifactBootstrapLimits {
                max_envelope_depth: bound(),
                max_decoded_allocation_bytes: bound(),
                max_metadata_scan_units: bound(),
                max_validation_work_units: bound(),
            },
            locale_data: LocaleDataLimits {
                max_payloads: bound(),
                max_payload_bytes: bound(),
                max_marker_bytes: bound(),
                max_validity_groups: bound(),
                max_codes: bound(),
                max_extension_keys: bound(),
                max_types: bound(),
                max_corrections: bound(),
                max_validation_work_units: bound(),
            },
            evidence_metadata: EvidenceMetadataLimits {
                max_source_identity_bytes: bound(),
                max_source_revision_bytes: bound(),
                max_locator_bytes: bound(),
                max_call_site_source_bytes: bound(),
                max_call_site_mappings: bound(),
                max_logical_path_segments: bound(),
                max_logical_path_canonical_bytes: bound(),
                max_subject_occurrences: bound(),
                max_token_inputs: bound(),
                max_collision_comparisons: bound(),
                max_metadata_work_units: bound(),
            },
            mandatory_reserve: MandatoryReserveLimits {
                max_outcome_envelope_slots: bound(),
                max_construction_identity_slots: bound(),
                max_entry_terminal_finding_slots: bound(),
                max_entry_terminal_status_slots: bound(),
                max_shared_terminal_finding_slots: bound(),
                max_shared_terminal_status_slots: bound(),
                max_shared_unavailable_cause_slots: bound(),
                max_canonical_bytes: bound(),
            },
        }
    }

    fn path(id: BootstrapBoundId) -> String {
        format!("{}.{}", id.group.path(), id.name)
    }

    #[test]
    fn the_groups_are_015s_in_canonical_order() {
        assert_eq!(
            BootstrapGroup::ALL.map(BootstrapGroup::path),
            [
                "rawInput",
                "snapshot",
                "inputBoundary",
                "artifactBootstrap",
                "localeData",
                "evidenceMetadata",
                "mandatoryReserve",
            ]
        );
        // Each group's bounds are contiguous and the groups come in that order.
        let mut groups = BootstrapLimits::MINIMUM
            .bounds()
            .map(|(id, _)| id.group)
            .to_vec();
        groups.dedup();
        assert_eq!(groups, BootstrapGroup::ALL);
    }

    #[test]
    fn bootstrap_only_has_015s_bounds_in_canonical_order() {
        assert_eq!(
            BootstrapLimits::MINIMUM.bounds().map(|(id, _)| path(id)),
            [
                "rawInput.maxFileBytes",
                "rawInput.maxParserTokens",
                "snapshot.maxNodes",
                "snapshot.maxDepth",
                "snapshot.maxCollectionEntries",
                "snapshot.maxTotalStringBytes",
                "snapshot.maxSingleStringBytes",
                "snapshot.maxRejectedMarkers",
                "snapshot.maxEnvelopeCanonicalBytes",
                "snapshot.maxSingleMarkerCanonicalBytes",
                "snapshot.maxTraversalUnits",
                "inputBoundary.maxDepth",
                "inputBoundary.maxSpecificationInputBytes",
                "inputBoundary.maxVocabularyBytes",
                "inputBoundary.maxVocabularyMembers",
                "inputBoundary.maxVocabularyMemberBytes",
                "inputBoundary.maxVocabularyTotalMemberBytes",
                "inputBoundary.maxLocaleDataManifestBytes",
                "inputBoundary.maxCollectionMemberBytes",
                "inputBoundary.maxCollectionTotalBytes",
                "inputBoundary.maxVerificationInputBytes",
                "inputBoundary.maxVerificationPins",
                "artifactBootstrap.maxEnvelopeDepth",
                "artifactBootstrap.maxDecodedAllocationBytes",
                "artifactBootstrap.maxMetadataScanUnits",
                "artifactBootstrap.maxValidationWorkUnits",
                "localeData.maxPayloads",
                "localeData.maxPayloadBytes",
                "localeData.maxMarkerBytes",
                "localeData.maxValidityGroups",
                "localeData.maxCodes",
                "localeData.maxExtensionKeys",
                "localeData.maxTypes",
                "localeData.maxCorrections",
                "localeData.maxValidationWorkUnits",
                "evidenceMetadata.maxSourceIdentityBytes",
                "evidenceMetadata.maxSourceRevisionBytes",
                "evidenceMetadata.maxLocatorBytes",
                "evidenceMetadata.maxCallSiteSourceBytes",
                "evidenceMetadata.maxCallSiteMappings",
                "evidenceMetadata.maxLogicalPathSegments",
                "evidenceMetadata.maxLogicalPathCanonicalBytes",
                "evidenceMetadata.maxSubjectOccurrences",
                "evidenceMetadata.maxTokenInputs",
                "evidenceMetadata.maxCollisionComparisons",
                "evidenceMetadata.maxMetadataWorkUnits",
                "mandatoryReserve.maxOutcomeEnvelopeSlots",
                "mandatoryReserve.maxConstructionIdentitySlots",
                "mandatoryReserve.maxEntryTerminalFindingSlots",
                "mandatoryReserve.maxEntryTerminalStatusSlots",
                "mandatoryReserve.maxSharedTerminalFindingSlots",
                "mandatoryReserve.maxSharedTerminalStatusSlots",
                "mandatoryReserve.maxSharedUnavailableCauseSlots",
                "mandatoryReserve.maxCanonicalBytes",
            ]
        );
    }

    #[test]
    fn every_bound_reads_its_own_field() {
        // The fixture counts up in field order, so a bound read from another
        // field, or a field read twice, breaks the sequence.
        let values = fixture(9).bounds().map(|(_, bound)| bound.get());
        assert_eq!(values, std::array::from_fn(|index| index as u64 + 9));
    }

    #[test]
    fn from_bounds_fills_each_field_from_its_own_name() {
        // Every value differs, so a name paired with another field breaks the
        // round trip.
        let bounds = fixture(30).bounds();
        let mut asked = Vec::new();
        let rebuilt = BootstrapLimits::from_bounds(|id| {
            asked.push(id);
            bounds
                .iter()
                .find(|(other, _)| *other == id)
                .map(|(_, bound)| *bound)
        })
        .unwrap();
        assert_eq!(rebuilt, fixture(30));
        // It asks once per bound, in canonical order, and stops at the first
        // bound it cannot get.
        assert_eq!(asked, bounds.map(|(id, _)| id));
        let mut count = 0;
        let partial = BootstrapLimits::from_bounds(|_| {
            count += 1;
            (count < 3).then(|| Bound::new(1).unwrap())
        });
        assert!(partial.is_none());
        assert_eq!(count, 3);
    }

    #[test]
    fn the_minimum_is_one_except_for_three_byte_bounds() {
        let minima = BootstrapLimits::MINIMUM.bounds();
        let larger: Vec<_> = minima
            .iter()
            .filter(|(_, bound)| bound.get() != 1)
            .map(|(id, bound)| (path(*id), bound.get()))
            .collect();
        assert_eq!(
            larger,
            [
                ("snapshot.maxEnvelopeCanonicalBytes".to_owned(), 4096),
                ("snapshot.maxSingleMarkerCanonicalBytes".to_owned(), 1024),
                ("mandatoryReserve.maxCanonicalBytes".to_owned(), 65536),
            ]
        );
    }

    #[test]
    fn policy_comparable_mirrors_the_policy_section_with_the_minimum_one() {
        let inventory = CapabilityBound::inventory();
        let mirrored = fixture_limits(1).bounds();
        for (index, (id, _)) in mirrored.into_iter().enumerate() {
            assert_eq!(
                inventory[index],
                (
                    CapabilityBound::PolicyComparable(id),
                    Bound::new(1).unwrap()
                )
            );
        }
    }

    #[test]
    fn bootstrap_only_follows_with_its_minimum_vector() {
        let inventory = CapabilityBound::inventory();
        for (index, (id, minimum)) in BootstrapLimits::MINIMUM.bounds().into_iter().enumerate() {
            assert_eq!(
                inventory[BOUND_COUNT + index],
                (CapabilityBound::BootstrapOnly(id), minimum)
            );
        }
    }

    #[test]
    fn a_capability_bound_path_joins_the_namespace_the_group_and_the_name() {
        let paths = CapabilityBound::inventory().map(|(bound, _)| bound.to_string());
        assert_eq!(paths.len(), 106);
        assert_eq!(paths[0], "policyComparable.configurationInput.maxNodes");
        assert_eq!(
            paths[BOUND_COUNT - 1],
            "policyComparable.diagnostics.shared.maxTotalCanonicalBytes"
        );
        assert_eq!(paths[BOUND_COUNT], "bootstrapOnly.rawInput.maxFileBytes");
        assert_eq!(
            paths[105],
            "bootstrapOnly.mandatoryReserve.maxCanonicalBytes"
        );
        assert_eq!(
            CapabilityBound::PolicyComparable(BoundId {
                group: Group::DiagnosticsEntry,
                name: "maxFindings",
            })
            .to_string(),
            "policyComparable.diagnostics.entry.maxFindings"
        );
        assert_eq!(
            CapabilityBound::BootstrapOnly(BootstrapBoundId {
                group: BootstrapGroup::Snapshot,
                name: "maxNodes",
            })
            .to_string(),
            "bootstrapOnly.snapshot.maxNodes"
        );
        // A bound name used in several groups, such as `maxNodes`, still gives
        // every bound its own path.
        let mut sorted = paths.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), paths.len());
    }

    #[test]
    fn a_bound_s_path_inside_its_namespace_leaves_the_namespace_out() {
        for (bound, _) in CapabilityBound::inventory() {
            assert_eq!(
                bound.to_string(),
                format!("{}.{}", bound.namespace(), bound.path())
            );
        }
        let entry = CapabilityBound::PolicyComparable(BoundId {
            group: Group::DiagnosticsEntry,
            name: "maxFindings",
        });
        assert_eq!(entry.namespace(), "policyComparable");
        assert_eq!(entry.path(), "diagnostics.entry.maxFindings");
        let snapshot = CapabilityBound::BootstrapOnly(BootstrapBoundId {
            group: BootstrapGroup::Snapshot,
            name: "maxNodes",
        });
        assert_eq!(snapshot.namespace(), "bootstrapOnly");
        assert_eq!(snapshot.path(), "snapshot.maxNodes");
        // A mirrored bound's path is its policy bound's.
        let mirrored = fixture_limits(1)
            .bounds()
            .map(|(id, _)| format!("{}.{}", id.group.path(), id.name));
        let listed = CapabilityBound::inventory()[..BOUND_COUNT]
            .iter()
            .map(|(bound, _)| bound.path())
            .collect::<Vec<_>>();
        assert_eq!(listed, mirrored);
    }

    #[test]
    fn each_bootstrap_bound_counts_in_the_unit_of_its_own_row_of_015s_table() {
        // The table has one row for each bound, in the bounds' order, and each
        // bound reads its own row.
        let ids = BootstrapLimits::MINIMUM.bounds().map(|(id, _)| id);
        assert_eq!(
            UNITS.map(|(group, name, _)| (group, name)),
            ids.map(|id| (id.group, id.name))
        );
        assert_eq!(
            ids.map(BootstrapBoundId::unit),
            UNITS.map(|(_, _, unit)| unit)
        );
        // A bound of the same name in another group may count in another unit.
        let unit = |group, name| BootstrapBoundId { group, name }.unit();
        assert_eq!(
            unit(BootstrapGroup::Snapshot, "maxDepth"),
            "One Snapshot tree level, with the root at one"
        );
        assert_eq!(
            unit(BootstrapGroup::InputBoundary, "maxDepth"),
            "One JSON nesting level of one raw boundary input or collection member, with its \
             root at one"
        );
        // A unit is the table's cell, code spans included, and every
        // `bootstrapOnly` bound has a unit of its own.
        assert_eq!(
            unit(BootstrapGroup::MandatoryReserve, "maxCanonicalBytes"),
            "One byte in the complete revision-`\"0\"` mandatory logical reserve representation"
        );
        let distinct = UNITS
            .iter()
            .map(|(_, _, unit)| *unit)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(distinct.len(), BOOTSTRAP_BOUND_COUNT);
    }

    #[test]
    fn a_capability_bound_counts_in_the_unit_of_its_table() {
        // A mirrored bound counts in its policy bound's unit, a `bootstrapOnly`
        // bound in its own, and no unit of one namespace is a unit of the
        // other.
        let inventory = CapabilityBound::inventory();
        assert_eq!(
            inventory[1].0.unit(),
            "Maximum value level with the root at depth one"
        );
        assert_eq!(
            inventory[BOUND_COUNT + 3].0.unit(),
            "One Snapshot tree level, with the root at one"
        );
        let policy = inventory[..BOUND_COUNT]
            .iter()
            .map(|(bound, _)| bound.unit())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(inventory[BOUND_COUNT..]
            .iter()
            .all(|(bound, _)| !policy.contains(bound.unit())));
    }

    #[test]
    #[should_panic(expected = "every bootstrapOnly bound has a unit")]
    fn an_id_of_no_bootstrap_bound_has_no_unit() {
        let _ = BootstrapBoundId {
            group: BootstrapGroup::RawInput,
            name: "maxNodes",
        }
        .unit();
    }
}
