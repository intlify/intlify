// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::adapter::{AdapterArtifactState, AdapterReescapePlan};
use crate::identity::{allocate_artifact_identity, ArtifactIdentity, IdentityInterner};
use crate::limits::check_limit;
use crate::registry::ResolvedHostFormat;
use crate::{
    CatalogKey, CatalogKeyDomain, EntryHandle, EntryKey, InternalResourceErrorReason,
    MessageOffsetMap, ResourceError, ResourceErrorDetails, ResourceErrorSite, ResourceLimit,
    ResourcePhase, StructuralPathKey, Utf8ByteSpan,
};

pub(crate) struct AdapterMessageEntry {
    structural_path: String,
    catalog_key_domain: CatalogKeyDomain,
    catalog_key: String,
    display_key: Option<String>,
    raw_value_span: Utf8ByteSpan,
    message_text: String,
    offset_map: MessageOffsetMap,
    read_only: bool,
}

impl AdapterMessageEntry {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        structural_path: String,
        catalog_key_domain: CatalogKeyDomain,
        catalog_key: String,
        display_key: Option<String>,
        raw_value_span: Utf8ByteSpan,
        message_text: String,
        offset_map: MessageOffsetMap,
        read_only: bool,
    ) -> Self {
        Self {
            structural_path,
            catalog_key_domain,
            catalog_key,
            display_key,
            raw_value_span,
            message_text,
            offset_map,
            read_only,
        }
    }
}

struct PendingMessageEntry {
    key: EntryKey,
    catalog_key_domain: CatalogKeyDomain,
    catalog_key: CatalogKey,
    display_key: Option<Arc<str>>,
    raw_value_span: Utf8ByteSpan,
    message_text: String,
    offset_map: MessageOffsetMap,
    read_only: bool,
}

pub(crate) struct ArtifactBuilder {
    source: Arc<str>,
    resolved: ResolvedHostFormat,
    phase: ResourcePhase,
    pending: Vec<PendingMessageEntry>,
    occurrences: HashMap<Arc<str>, u32>,
    interner: IdentityInterner,
    entry_count: u128,
    total_message_bytes: u128,
    offset_map_segments: u128,
    first_error: Option<ResourceError>,
}

impl ArtifactBuilder {
    fn new(source: Arc<str>, resolved: ResolvedHostFormat, phase: ResourcePhase) -> Self {
        Self {
            source,
            resolved,
            phase,
            pending: Vec::new(),
            occurrences: HashMap::new(),
            interner: IdentityInterner::default(),
            entry_count: 0,
            total_message_bytes: 0,
            offset_map_segments: 0,
            first_error: None,
        }
    }

    pub(crate) fn push_entry(&mut self, entry: AdapterMessageEntry) -> Result<(), ResourceError> {
        if let Some(error) = &self.first_error {
            return Err(error.clone());
        }

        match self.try_push_entry(entry) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.first_error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub(crate) fn preflight_entry(
        &self,
        structural_path: &str,
        catalog_key: &str,
        display_key: Option<&str>,
        raw_value_span: Utf8ByteSpan,
        message_bytes: u128,
    ) -> Result<u32, ResourceError> {
        if let Some(error) = &self.first_error {
            return Err(error.clone());
        }

        let occurrence = self.occurrences.get(structural_path).copied().unwrap_or(0);
        let check = |resource: ResourceLimit, actual: u128| {
            if actual <= resource.limit() {
                Ok(())
            } else {
                let key = EntryKey::new(
                    StructuralPathKey::from_shared(Arc::from(structural_path)),
                    occurrence,
                );
                Err(ResourceError::limit_exceeded(
                    resource,
                    actual,
                    self.phase,
                    Some(ResourceErrorSite::new(raw_value_span, Some(key))),
                ))
            }
        };

        check(ResourceLimit::Entries, self.entry_count + 1)?;
        check(ResourceLimit::MessageBytes, message_bytes)?;
        check(
            ResourceLimit::TotalMessageBytes,
            self.total_message_bytes + message_bytes,
        )?;

        let mut projected_identity_bytes = self.interner.distinct_bytes();
        let mut new_values = [None; 3];
        let mut new_value_count = 0;
        for value in [Some(structural_path), Some(catalog_key), display_key]
            .into_iter()
            .flatten()
        {
            if self.interner.contains(value) || new_values[..new_value_count].contains(&Some(value))
            {
                continue;
            }
            projected_identity_bytes += value.len() as u128;
            check(ResourceLimit::IdentityBytes, projected_identity_bytes)?;
            new_values[new_value_count] = Some(value);
            new_value_count += 1;
        }

        Ok(occurrence)
    }

    pub(crate) const fn offset_map_segment_count(&self) -> u128 {
        self.offset_map_segments
    }

    fn try_push_entry(&mut self, entry: AdapterMessageEntry) -> Result<(), ResourceError> {
        let occurrence = self
            .occurrences
            .get(entry.structural_path.as_str())
            .copied()
            .unwrap_or(0);
        let provisional_key = EntryKey::new(
            StructuralPathKey::from_shared(Arc::from(entry.structural_path.as_str())),
            occurrence,
        );
        let provisional_site =
            ResourceErrorSite::new(entry.raw_value_span, Some(provisional_key.clone()));

        if entry.offset_map.raw_value_span() != entry.raw_value_span
            || u128::from(entry.offset_map.message_len()) != entry.message_text.len() as u128
        {
            return Err(ResourceError::internal(
                InternalResourceErrorReason::AdapterInvariantFailed,
                self.phase,
                Some(provisional_site),
            ));
        }

        let next_entry_count = self.entry_count + 1;
        check_limit(
            ResourceLimit::Entries,
            next_entry_count,
            self.phase,
            Some(provisional_site.clone()),
        )?;

        let message_bytes = entry.message_text.len() as u128;
        check_limit(
            ResourceLimit::MessageBytes,
            message_bytes,
            self.phase,
            Some(provisional_site.clone()),
        )?;

        let next_total_message_bytes = self.total_message_bytes + message_bytes;
        check_limit(
            ResourceLimit::TotalMessageBytes,
            next_total_message_bytes,
            self.phase,
            Some(provisional_site.clone()),
        )?;

        let structural_path = StructuralPathKey::from_shared(self.interner.intern(
            &entry.structural_path,
            self.phase,
            Some(provisional_site.clone()),
        )?);
        let key = EntryKey::new(structural_path, occurrence);
        let site = ResourceErrorSite::new(entry.raw_value_span, Some(key.clone()));
        let catalog_key = CatalogKey::from_shared(self.interner.intern(
            &entry.catalog_key,
            self.phase,
            Some(site.clone()),
        )?);
        let display_key = entry
            .display_key
            .as_deref()
            .map(|display_key| {
                self.interner
                    .intern(display_key, self.phase, Some(site.clone()))
            })
            .transpose()?;

        let next_offset_map_segments =
            self.offset_map_segments + entry.offset_map.segment_count() as u128;
        check_limit(
            ResourceLimit::OffsetMapSegments,
            next_offset_map_segments,
            self.phase,
            Some(site),
        )?;

        self.occurrences
            .insert(Arc::clone(key.structural_path().shared()), occurrence + 1);
        self.pending.push(PendingMessageEntry {
            key,
            catalog_key_domain: entry.catalog_key_domain,
            catalog_key,
            display_key,
            raw_value_span: entry.raw_value_span,
            message_text: entry.message_text,
            offset_map: entry.offset_map,
            read_only: entry.read_only,
        });
        self.entry_count = next_entry_count;
        self.total_message_bytes = next_total_message_bytes;
        self.offset_map_segments = next_offset_map_segments;
        Ok(())
    }

    fn finalize(
        self,
        adapter_state: AdapterArtifactState,
    ) -> Result<ExtractedCatalog, ResourceError> {
        if let Some(error) = self.first_error {
            return Err(error);
        }
        validate_pending_spans(&self.source, &self.pending, self.phase)?;

        let artifact_identity = allocate_artifact_identity(self.phase)?;
        let entries = self
            .pending
            .into_iter()
            .enumerate()
            .map(|(index, entry)| MessageEntry {
                handle: EntryHandle::new(
                    artifact_identity,
                    u32::try_from(index).expect("the fixed entry limit keeps indices in u32"),
                ),
                key: entry.key,
                catalog_key_domain: entry.catalog_key_domain,
                catalog_key: entry.catalog_key,
                display_key: entry.display_key,
                raw_value_span: entry.raw_value_span,
                message_text: entry.message_text,
                offset_map: entry.offset_map,
                read_only: entry.read_only,
            })
            .collect();

        Ok(ExtractedCatalog {
            source: self.source,
            resolved: self.resolved,
            artifact_identity,
            entries,
            adapter_state,
        })
    }

    #[cfg(test)]
    fn set_counters_for_test(
        &mut self,
        entry_count: u128,
        total_message_bytes: u128,
        identity_bytes: u128,
        offset_map_segments: u128,
    ) {
        self.entry_count = entry_count;
        self.total_message_bytes = total_message_bytes;
        self.interner.set_distinct_bytes_for_test(identity_bytes);
        self.offset_map_segments = offset_map_segments;
    }
}

fn validate_pending_spans(
    source: &str,
    entries: &[PendingMessageEntry],
    phase: ResourcePhase,
) -> Result<(), ResourceError> {
    let mut previous: Option<&PendingMessageEntry> = None;

    for entry in entries {
        let span = entry.raw_value_span;
        let start = usize::try_from(span.start()).expect("u32 fits supported usize");
        let end = usize::try_from(span.end()).expect("u32 fits supported usize");
        let invalid_outer = span.start() > span.end()
            || end > source.len()
            || !source.is_char_boundary(start)
            || !source.is_char_boundary(end);
        let invalid_order = previous.is_some_and(|previous| {
            !spans_are_ordered_and_edit_disjoint(previous.raw_value_span, span)
        });

        if invalid_outer || invalid_order {
            return Err(ResourceError::internal(
                InternalResourceErrorReason::AdapterInvariantFailed,
                phase,
                Some(ResourceErrorSite::new(span, Some(entry.key.clone()))),
            ));
        }
        previous = Some(entry);
    }
    Ok(())
}

// The registry intentionally transfers both opaque dispatch context and source
// ownership into the immutable artifact, even though construction also borrows
// them while the private adapter runs.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn extract_resolved(
    resolved: ResolvedHostFormat,
    source: Arc<str>,
    phase: ResourcePhase,
) -> Result<ExtractedCatalog, ResourceError> {
    crate::preflight_host_bytes(source.len(), phase)?;
    let adapter = Arc::clone(resolved.adapter());
    let mut builder = ArtifactBuilder::new(Arc::clone(&source), resolved.clone(), phase);
    let adapter_state = adapter.extract(&resolved, &source, &mut builder, phase)?;
    builder.finalize(adapter_state)
}

/// One immutable extracted message entry.
#[derive(Debug, Clone)]
pub struct MessageEntry {
    handle: EntryHandle,
    key: EntryKey,
    catalog_key_domain: CatalogKeyDomain,
    catalog_key: CatalogKey,
    display_key: Option<Arc<str>>,
    raw_value_span: Utf8ByteSpan,
    message_text: String,
    offset_map: MessageOffsetMap,
    read_only: bool,
}

impl MessageEntry {
    #[must_use]
    pub const fn handle(&self) -> EntryHandle {
        self.handle
    }

    #[must_use]
    pub const fn key(&self) -> &EntryKey {
        &self.key
    }

    #[must_use]
    pub const fn catalog_key_domain(&self) -> CatalogKeyDomain {
        self.catalog_key_domain
    }

    #[must_use]
    pub const fn catalog_key(&self) -> &CatalogKey {
        &self.catalog_key
    }

    #[must_use]
    pub fn display_key(&self) -> Option<&str> {
        self.display_key.as_deref()
    }

    #[must_use]
    pub const fn raw_value_span(&self) -> Utf8ByteSpan {
        self.raw_value_span
    }

    #[must_use]
    pub fn message_text(&self) -> &str {
        &self.message_text
    }

    #[must_use]
    pub const fn offset_map(&self) -> &MessageOffsetMap {
        &self.offset_map
    }

    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }
}

/// Complete immutable extraction artifact for one host document.
pub struct ExtractedCatalog {
    source: Arc<str>,
    resolved: ResolvedHostFormat,
    artifact_identity: ArtifactIdentity,
    entries: Vec<MessageEntry>,
    adapter_state: AdapterArtifactState,
}

impl ExtractedCatalog {
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn entries(&self) -> &[MessageEntry] {
        &self.entries
    }

    #[must_use]
    pub const fn begin_candidate_message_admission(&self) -> CandidateMessageAdmission<'_> {
        CandidateMessageAdmission {
            catalog: self,
            next_entry: 0,
            total_message_bytes: 0,
            terminal: false,
        }
    }

    pub fn build_and_validate_write_back(
        &self,
        formatted_entries: &[FormattedEntry<'_>],
    ) -> Result<WriteBackOutcome, ResourceError> {
        let mut has_changes = false;
        for formatted in formatted_entries {
            if formatted.entry.artifact_identity() != self.artifact_identity
                || usize::try_from(formatted.entry.entry_index())
                    .ok()
                    .is_none_or(|index| index >= self.entries.len())
            {
                return Err(Self::invalid_handle_error(None));
            }

            let index = usize::try_from(formatted.entry.entry_index())
                .expect("validated u32 entry index fits usize");
            has_changes |= formatted.formatted_message.as_bytes()
                != self.entries[index].message_text.as_bytes();
        }

        if formatted_entries.is_empty() {
            return Ok(WriteBackOutcome::Unchanged);
        }

        if !has_changes {
            self.validate_unchanged_formatted_entries(formatted_entries)?;
            return Ok(WriteBackOutcome::Unchanged);
        }

        let mut counts = vec![0_usize; self.entries.len()];
        let mut formatted_by_index = vec![None; self.entries.len()];

        for formatted in formatted_entries {
            let index = usize::try_from(formatted.entry.entry_index())
                .expect("validated u32 entry index fits usize");
            counts[index] += 1;
            formatted_by_index[index] = Some(formatted.formatted_message);
        }

        for (index, entry) in self.entries.iter().enumerate() {
            if counts[index] > 1 || (counts[index] == 1 && entry.read_only) {
                return Err(Self::invalid_handle_error(Some(entry)));
            }
        }

        let mut total_message_bytes = 0_u128;
        let mut plans = (0..self.entries.len())
            .map(|_| None)
            .collect::<Vec<Option<AdapterReescapePlan>>>();

        for (index, entry) in self.entries.iter().enumerate() {
            let effective = formatted_by_index[index].unwrap_or(&entry.message_text);
            let message_bytes = effective.len() as u128;
            let site = Some(Self::entry_site(entry));
            check_limit(
                ResourceLimit::MessageBytes,
                message_bytes,
                ResourcePhase::ValidateWriteBack,
                site.clone(),
            )?;
            total_message_bytes += message_bytes;
            check_limit(
                ResourceLimit::TotalMessageBytes,
                total_message_bytes,
                ResourcePhase::ValidateWriteBack,
                site,
            )?;

            if counts[index] == 1 && effective.as_bytes() != entry.message_text.as_bytes() {
                plans[index] = Some(self.resolved.adapter().plan_reescape(
                    self.adapter_state.as_ref(),
                    entry,
                    effective,
                    ResourcePhase::ValidateWriteBack,
                )?);
            }
        }

        if plans.iter().all(Option::is_none) {
            return Ok(WriteBackOutcome::Unchanged);
        }

        let projected_host_bytes = self.projected_host_bytes(&plans)?;
        check_limit(
            ResourceLimit::HostBytes,
            projected_host_bytes,
            ResourcePhase::ValidateWriteBack,
            None,
        )?;

        let mut replacements = Vec::with_capacity(plans.iter().flatten().count());
        for (index, plan) in plans.iter().enumerate() {
            let Some(plan) = plan else {
                continue;
            };
            let entry = &self.entries[index];
            let formatted_message = formatted_by_index[index]
                .expect("changed plans only exist for supplied formatted entries");
            let raw_text = self.resolved.adapter().materialize(
                self.adapter_state.as_ref(),
                entry,
                formatted_message,
                plan,
                ResourcePhase::ValidateWriteBack,
            )?;
            if raw_text.len() as u128 != u128::from(plan.measured_len()) {
                return Err(ResourceError::internal(
                    InternalResourceErrorReason::AdapterInvariantFailed,
                    ResourcePhase::ValidateWriteBack,
                    Some(Self::entry_site(entry)),
                ));
            }
            replacements.push(RawReplacement {
                entry: entry.handle,
                span: entry.raw_value_span,
                raw_text,
            });
        }

        let candidate_source = self.apply_replacements(&replacements, projected_host_bytes)?;
        let candidate = extract_resolved(
            self.resolved.clone(),
            candidate_source,
            ResourcePhase::ValidateWriteBack,
        )
        .map_err(|error| self.convert_candidate_error(error))?;
        self.validate_candidate_equivalence(&candidate, &formatted_by_index)?;

        Ok(WriteBackOutcome::Changed(ValidatedWriteBack {
            replacements,
            candidate,
        }))
    }

    fn validate_unchanged_formatted_entries(
        &self,
        formatted_entries: &[FormattedEntry<'_>],
    ) -> Result<(), ResourceError> {
        const WORD_BITS: usize = u64::BITS as usize;
        const WORDS: usize = (crate::MAX_ENTRIES as usize).div_ceil(WORD_BITS);

        let mut seen = [0_u64; WORDS];
        let mut first_duplicate = None;
        let mut first_read_only = None;

        for formatted in formatted_entries {
            let index = usize::try_from(formatted.entry.entry_index())
                .expect("unchanged prepass receives a validated entry index");
            let word = index / WORD_BITS;
            let mask = 1_u64 << (index % WORD_BITS);
            if seen[word] & mask != 0 {
                first_duplicate =
                    Some(first_duplicate.map_or(index, |first: usize| first.min(index)));
            } else {
                seen[word] |= mask;
            }
            if self.entries[index].read_only {
                first_read_only =
                    Some(first_read_only.map_or(index, |first: usize| first.min(index)));
            }
        }

        let invalid_index = match (first_duplicate, first_read_only) {
            (Some(duplicate), Some(read_only)) => Some(duplicate.min(read_only)),
            (Some(duplicate), None) => Some(duplicate),
            (None, Some(read_only)) => Some(read_only),
            (None, None) => None,
        };
        if let Some(index) = invalid_index {
            return Err(Self::invalid_handle_error(Some(&self.entries[index])));
        }

        Ok(())
    }

    fn projected_host_bytes(
        &self,
        plans: &[Option<AdapterReescapePlan>],
    ) -> Result<u128, ResourceError> {
        project_final_host_bytes(
            self.source.len() as u128,
            self.entries.iter().zip(plans).filter_map(|(entry, plan)| {
                plan.as_ref().map(|plan| {
                    (
                        u128::from(
                            entry
                                .raw_value_span
                                .checked_len()
                                .expect("published entry spans are ordered"),
                        ),
                        u128::from(plan.measured_len()),
                    )
                })
            }),
        )
        .ok_or_else(|| {
            ResourceError::internal(
                InternalResourceErrorReason::AdapterInvariantFailed,
                ResourcePhase::ValidateWriteBack,
                None,
            )
        })
    }

    fn apply_replacements(
        &self,
        replacements: &[RawReplacement],
        projected_host_bytes: u128,
    ) -> Result<Arc<str>, ResourceError> {
        let mut seen = vec![false; self.entries.len()];
        let mut previous_span: Option<Utf8ByteSpan> = None;
        for replacement in replacements {
            let index = usize::try_from(replacement.entry.entry_index())
                .expect("u32 entry index fits supported usize");
            let valid_handle = replacement.entry.artifact_identity() == self.artifact_identity
                && index < self.entries.len();
            let valid_entry = valid_handle
                && !seen[index]
                && replacement.span == self.entries[index].raw_value_span;
            let valid_order = previous_span.is_none_or(|previous| {
                spans_are_ordered_and_edit_disjoint(previous, replacement.span)
            });
            if !valid_entry || !valid_order {
                return Err(ResourceError::internal(
                    InternalResourceErrorReason::AdapterInvariantFailed,
                    ResourcePhase::ValidateWriteBack,
                    None,
                ));
            }
            seen[index] = true;
            previous_span = Some(replacement.span);
        }

        let capacity = usize::try_from(projected_host_bytes).map_err(|_| {
            ResourceError::internal(
                InternalResourceErrorReason::AdapterInvariantFailed,
                ResourcePhase::ValidateWriteBack,
                None,
            )
        })?;
        let mut candidate = String::with_capacity(capacity);
        candidate.push_str(&self.source);
        for replacement in replacements.iter().rev() {
            let start = usize::try_from(replacement.span.start())
                .expect("validated u32 span fits supported usize");
            let end = usize::try_from(replacement.span.end())
                .expect("validated u32 span fits supported usize");
            candidate.replace_range(start..end, &replacement.raw_text);
        }
        if candidate.len() != capacity {
            return Err(ResourceError::internal(
                InternalResourceErrorReason::AdapterInvariantFailed,
                ResourcePhase::ValidateWriteBack,
                None,
            ));
        }
        Ok(Arc::from(candidate))
    }

    fn validate_candidate_equivalence(
        &self,
        candidate: &ExtractedCatalog,
        formatted_by_index: &[Option<&str>],
    ) -> Result<(), ResourceError> {
        if candidate.entries.len() != self.entries.len() {
            return Err(Self::write_back_error(None));
        }

        for (index, (original, candidate)) in
            self.entries.iter().zip(&candidate.entries).enumerate()
        {
            let expected_message = formatted_by_index[index].unwrap_or(&original.message_text);
            let mismatch = candidate.key != original.key
                || candidate.catalog_key_domain != original.catalog_key_domain
                || candidate.catalog_key != original.catalog_key
                || candidate.display_key != original.display_key
                || candidate.read_only != original.read_only
                || candidate.message_text.as_bytes() != expected_message.as_bytes();
            if mismatch {
                return Err(Self::write_back_error(Some(original)));
            }
        }
        Ok(())
    }

    fn convert_candidate_error(&self, error: ResourceError) -> ResourceError {
        let mapped_site = error.site().and_then(|site| {
            site.entry_key().and_then(|candidate_key| {
                self.entries
                    .iter()
                    .find(|entry| entry.key() == candidate_key)
                    .map(Self::entry_site)
            })
        });
        let preserve = match error.details() {
            ResourceErrorDetails::LimitExceeded { .. } => true,
            ResourceErrorDetails::Internal { reason } => matches!(
                reason,
                InternalResourceErrorReason::ArtifactIdentityExhausted
                    | InternalResourceErrorReason::OffsetMapInvariantFailed
                    | InternalResourceErrorReason::AdapterInvariantFailed
            ),
            ResourceErrorDetails::FormatUnsupported { .. }
            | ResourceErrorDetails::ParseFailed { .. }
            | ResourceErrorDetails::EntryUnsupported { .. }
            | ResourceErrorDetails::DocumentUnsupported { .. } => false,
        };

        if preserve {
            error.with_phase_and_site(ResourcePhase::ValidateWriteBack, mapped_site)
        } else {
            ResourceError::internal(
                InternalResourceErrorReason::WriteBackFailed,
                ResourcePhase::ValidateWriteBack,
                mapped_site,
            )
        }
    }

    fn invalid_handle_error(entry: Option<&MessageEntry>) -> ResourceError {
        ResourceError::internal(
            InternalResourceErrorReason::InvalidEntryHandle,
            ResourcePhase::ValidateWriteBack,
            entry.map(Self::entry_site),
        )
    }

    fn write_back_error(entry: Option<&MessageEntry>) -> ResourceError {
        ResourceError::internal(
            InternalResourceErrorReason::WriteBackFailed,
            ResourcePhase::ValidateWriteBack,
            entry.map(Self::entry_site),
        )
    }

    fn entry_site(entry: &MessageEntry) -> ResourceErrorSite {
        ResourceErrorSite::new(entry.raw_value_span, Some(entry.key.clone()))
    }
}

const fn spans_are_ordered_and_edit_disjoint(
    previous: Utf8ByteSpan,
    current: Utf8ByteSpan,
) -> bool {
    if current.start() <= previous.start() {
        return false;
    }
    if previous.is_empty() {
        return true;
    }
    if current.is_empty() {
        return previous.end() < current.start();
    }
    previous.end() <= current.start()
}

fn project_final_host_bytes(
    original_host_bytes: u128,
    changed_lengths: impl IntoIterator<Item = (u128, u128)>,
) -> Option<u128> {
    let mut removed = 0_u128;
    let mut added = 0_u128;
    for (removed_bytes, added_bytes) in changed_lengths {
        removed = removed.checked_add(removed_bytes)?;
        added = added.checked_add(added_bytes)?;
    }
    original_host_bytes.checked_sub(removed)?.checked_add(added)
}

impl fmt::Debug for ExtractedCatalog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExtractedCatalog")
            .field("format", &self.resolved.format())
            .field("source_bytes", &self.source.len())
            .field("entries", &self.entries)
            .finish_non_exhaustive()
    }
}

/// Raw-order, one-pass candidate message budget admission.
pub struct CandidateMessageAdmission<'a> {
    catalog: &'a ExtractedCatalog,
    next_entry: usize,
    total_message_bytes: u128,
    terminal: bool,
}

impl CandidateMessageAdmission<'_> {
    pub fn admit_original(&mut self, entry: EntryHandle) -> Result<(), ResourceError> {
        let observed_bytes = self
            .catalog
            .entries
            .get(self.next_entry)
            .filter(|expected| expected.handle == entry)
            .map(|expected| expected.message_text.len() as u128);
        self.admit(entry, observed_bytes, false)
    }

    pub fn admit_formatted_bytes(
        &mut self,
        entry: EntryHandle,
        observed_bytes: u64,
    ) -> Result<(), ResourceError> {
        self.admit(entry, Some(u128::from(observed_bytes)), true)
    }

    pub fn finish(self) -> Result<(), ResourceError> {
        if self.terminal || self.next_entry != self.catalog.entries.len() {
            let entry = self.catalog.entries.get(self.next_entry);
            Err(ExtractedCatalog::invalid_handle_error(entry))
        } else {
            Ok(())
        }
    }

    fn admit(
        &mut self,
        handle: EntryHandle,
        observed_bytes: Option<u128>,
        formatted: bool,
    ) -> Result<(), ResourceError> {
        if self.terminal {
            return Err(ExtractedCatalog::invalid_handle_error(None));
        }

        let expected = self.catalog.entries.get(self.next_entry);
        let Some(expected) = expected.filter(|entry| entry.handle == handle) else {
            self.terminal = true;
            return Err(ExtractedCatalog::invalid_handle_error(expected));
        };
        if formatted && expected.read_only {
            self.terminal = true;
            return Err(ExtractedCatalog::invalid_handle_error(Some(expected)));
        }
        let Some(observed_bytes) = observed_bytes else {
            self.terminal = true;
            return Err(ExtractedCatalog::invalid_handle_error(Some(expected)));
        };

        let site = Some(ExtractedCatalog::entry_site(expected));
        if let Err(error) = check_limit(
            ResourceLimit::MessageBytes,
            observed_bytes,
            ResourcePhase::ValidateWriteBack,
            site.clone(),
        ) {
            self.terminal = true;
            return Err(error);
        }
        let next_total = self.total_message_bytes + observed_bytes;
        if let Err(error) = check_limit(
            ResourceLimit::TotalMessageBytes,
            next_total,
            ResourcePhase::ValidateWriteBack,
            site,
        ) {
            self.terminal = true;
            return Err(error);
        }

        self.total_message_bytes = next_total;
        self.next_entry += 1;
        Ok(())
    }
}

/// Successfully admitted formatter output for one writable entry.
#[derive(Debug, Clone, Copy)]
pub struct FormattedEntry<'a> {
    pub entry: EntryHandle,
    pub formatted_message: &'a str,
}

/// Integrated validated write-back result.
pub enum WriteBackOutcome {
    Unchanged,
    Changed(ValidatedWriteBack),
}

impl fmt::Debug for WriteBackOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unchanged => formatter.write_str("Unchanged"),
            Self::Changed(write_back) => {
                formatter.debug_tuple("Changed").field(write_back).finish()
            }
        }
    }
}

/// Complete replacement set and fully re-extracted candidate artifact.
pub struct ValidatedWriteBack {
    replacements: Vec<RawReplacement>,
    candidate: ExtractedCatalog,
}

impl ValidatedWriteBack {
    #[must_use]
    pub fn replacements(&self) -> &[RawReplacement] {
        &self.replacements
    }

    #[must_use]
    pub const fn candidate(&self) -> &ExtractedCatalog {
        &self.candidate
    }

    #[must_use]
    pub fn into_candidate(self) -> ExtractedCatalog {
        self.candidate
    }
}

impl fmt::Debug for ValidatedWriteBack {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ValidatedWriteBack")
            .field("replacements", &self.replacements)
            .field("candidate", &self.candidate)
            .finish()
    }
}

/// One adapter-produced raw replacement from a validated write-back.
pub struct RawReplacement {
    entry: EntryHandle,
    span: Utf8ByteSpan,
    raw_text: String,
}

impl RawReplacement {
    #[must_use]
    pub const fn span(&self) -> Utf8ByteSpan {
        self.span
    }

    #[must_use]
    pub fn raw_text(&self) -> &str {
        &self.raw_text
    }
}

impl fmt::Debug for RawReplacement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RawReplacement")
            .field("span", &self.span)
            .field("raw_bytes", &self.raw_text.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::collections::HashSet;
    use std::fmt::Write as _;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    use allocation_counter::measure;

    use super::{
        project_final_host_bytes, AdapterMessageEntry, ArtifactBuilder, ExtractedCatalog,
        FormattedEntry, WriteBackOutcome,
    };
    use crate::adapter::{AdapterArtifactState, AdapterReescapePlan, HostAdapter};
    use crate::offset_map::MessageOffsetMapBuilder;
    use crate::registry::{HostFormatRegistry, ResolvedHostFormat};
    use crate::{
        CatalogKeyDomain, DeclaredFormat, DocumentUnsupportedFeature, EntryHandle,
        EntryUnsupportedReason, FormatClassificationSource, InternalResourceErrorReason,
        ResourceError, ResourceErrorCode, ResourceErrorDetails, ResourceErrorSite, ResourceLimit,
        ResourcePhase, StructuralPathKey, Utf8ByteSpan, MAX_ENTRIES, MAX_HOST_BYTES,
        MAX_IDENTITY_BYTES, MAX_MESSAGE_BYTES, MAX_OFFSET_MAP_SEGMENTS, MAX_TOTAL_MESSAGE_BYTES,
    };

    #[derive(Default)]
    struct AdapterCalls {
        extracts: AtomicUsize,
        plans: AtomicUsize,
        materializations: AtomicUsize,
    }

    struct LineAdapter {
        calls: Arc<AdapterCalls>,
    }

    impl LineAdapter {
        fn new(calls: Arc<AdapterCalls>) -> Self {
            Self { calls }
        }

        fn candidate_error(source: &str, phase: ResourcePhase) -> Option<ResourceError> {
            const MARKERS: &[&str] = &[
                "@error:format",
                "@error:parse",
                "@error:entry",
                "@error:document",
                "@error:limit",
                "@error:identity",
                "@error:offset",
                "@error:adapter",
            ];
            let marker = MARKERS
                .iter()
                .copied()
                .find(|marker| source.contains(marker))?;
            let site = marker_site(source, marker);
            Some(match marker {
                "@error:format" => ResourceError::format_unsupported(
                    FormatClassificationSource::Embedded,
                    DeclaredFormat::Value(Arc::from("unsupported")),
                    None,
                    Arc::from(".json"),
                    Some("json"),
                    Arc::from(&["json"][..]),
                    phase,
                    Some(site),
                ),
                "@error:parse" => ResourceError::parse_failed("json", None, phase, site),
                "@error:entry" => ResourceError::entry_unsupported(
                    "json",
                    None,
                    EntryUnsupportedReason::MessageTextUnrepresentable,
                    phase,
                    site,
                ),
                "@error:document" => ResourceError::document_unsupported(
                    "json",
                    None,
                    DocumentUnsupportedFeature::MultipleDocuments,
                    phase,
                    site,
                ),
                "@error:limit" => ResourceError::limit_exceeded(
                    ResourceLimit::MessageBytes,
                    u128::from(MAX_MESSAGE_BYTES) + 1,
                    phase,
                    Some(site),
                ),
                "@error:identity" => ResourceError::internal(
                    InternalResourceErrorReason::ArtifactIdentityExhausted,
                    phase,
                    Some(site),
                ),
                "@error:offset" => ResourceError::internal(
                    InternalResourceErrorReason::OffsetMapInvariantFailed,
                    phase,
                    Some(site),
                ),
                "@error:adapter" => ResourceError::internal(
                    InternalResourceErrorReason::AdapterInvariantFailed,
                    phase,
                    Some(site),
                ),
                _ => unreachable!(),
            })
        }
    }

    impl HostAdapter for LineAdapter {
        fn extract(
            &self,
            _resolved: &ResolvedHostFormat,
            source: &Arc<str>,
            builder: &mut ArtifactBuilder,
            phase: ResourcePhase,
        ) -> Result<AdapterArtifactState, ResourceError> {
            self.calls.extracts.fetch_add(1, Ordering::Relaxed);
            if let Some(error) = Self::candidate_error(source, phase) {
                return Err(error);
            }

            let mut base = 0_usize;
            for raw_line in source.split_inclusive('\n') {
                let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
                if line.is_empty() {
                    return Err(ResourceError::parse_failed(
                        "json",
                        None,
                        phase,
                        ResourceErrorSite::new(byte_span(base, base + raw_line.len()), None),
                    ));
                }

                let (read_only_prefix, body) = line
                    .strip_prefix('!')
                    .map_or((false, line), |body| (true, body));
                let Some(separator) = body.find('=') else {
                    return Err(ResourceError::parse_failed(
                        "json",
                        None,
                        phase,
                        ResourceErrorSite::new(byte_span(base, base + line.len()), None),
                    ));
                };
                let key = &body[..separator];
                let value = &body[separator + 1..];
                let value_start = base + usize::from(read_only_prefix) + separator + 1;
                let raw_span = byte_span(value_start, value_start + value.len());

                if value == "@drop" {
                    base += raw_line.len();
                    continue;
                }

                let structural_path = if value == "@key-mismatch" {
                    format!("{key}-candidate")
                } else {
                    key.to_owned()
                };
                let catalog_key = if value == "@catalog-mismatch" {
                    format!("{key}-catalog")
                } else {
                    key.to_owned()
                };
                let display_key = if value == "@display-mismatch" {
                    Some(format!("{key}-display"))
                } else {
                    Some(key.to_owned())
                };
                let domain = if value == "@domain-mismatch" {
                    CatalogKeyDomain::StandaloneMf2
                } else {
                    CatalogKeyDomain::JsonPointer
                };
                let message_text = if value == "@message-mismatch" {
                    "different".to_owned()
                } else {
                    value.to_owned()
                };
                let read_only = read_only_prefix || value == "@readonly-mismatch";
                let offset_map = test_offset_map(source, raw_span, value, &message_text);
                builder.push_entry(AdapterMessageEntry::new(
                    structural_path,
                    domain,
                    catalog_key,
                    display_key,
                    raw_span,
                    message_text,
                    offset_map,
                    read_only,
                ))?;
                base += raw_line.len();
            }

            Ok(Arc::new(()))
        }

        fn plan_reescape(
            &self,
            artifact_state: &(dyn Any + Send + Sync),
            _entry: &super::MessageEntry,
            formatted_message: &str,
            _phase: ResourcePhase,
        ) -> Result<AdapterReescapePlan, ResourceError> {
            assert!(artifact_state.is::<()>());
            self.calls.plans.fetch_add(1, Ordering::Relaxed);
            let measured_len = u64::try_from(formatted_message.len()).unwrap()
                + u64::from(formatted_message == "@length-mismatch");
            Ok(AdapterReescapePlan::new(measured_len, Box::new(())))
        }

        fn materialize(
            &self,
            artifact_state: &(dyn Any + Send + Sync),
            entry: &super::MessageEntry,
            formatted_message: &str,
            plan: &AdapterReescapePlan,
            phase: ResourcePhase,
        ) -> Result<String, ResourceError> {
            assert!(artifact_state.is::<()>());
            assert!(plan.state().is::<()>());
            self.calls.materializations.fetch_add(1, Ordering::Relaxed);
            if formatted_message == "@materialize-error" {
                return Err(ResourceError::internal(
                    InternalResourceErrorReason::AdapterInvariantFailed,
                    phase,
                    Some(ResourceErrorSite::new(
                        entry.raw_value_span(),
                        Some(entry.key().clone()),
                    )),
                ));
            }
            Ok(formatted_message.to_owned())
        }
    }

    fn byte_span(start: usize, end: usize) -> Utf8ByteSpan {
        Utf8ByteSpan::new(u32::try_from(start).unwrap(), u32::try_from(end).unwrap())
    }

    fn marker_site(source: &str, marker: &str) -> ResourceErrorSite {
        let marker_start = source.find(marker).unwrap();
        let line_start = source[..marker_start]
            .rfind('\n')
            .map_or(0, |position| position + 1);
        let key_prefix = source[line_start..marker_start]
            .strip_prefix('!')
            .unwrap_or(&source[line_start..marker_start]);
        let key = key_prefix.strip_suffix('=').unwrap();
        ResourceErrorSite::new(
            byte_span(marker_start, marker_start + marker.len()),
            Some(crate::EntryKey::new(
                StructuralPathKey::from_shared(Arc::from(key)),
                0,
            )),
        )
    }

    fn test_offset_map(
        source: &str,
        raw_span: Utf8ByteSpan,
        raw_value: &str,
        message_text: &str,
    ) -> crate::MessageOffsetMap {
        let mut builder = MessageOffsetMapBuilder::new(raw_span);
        if message_text.is_empty() {
            if !raw_span.is_empty() {
                builder.push_raw_only(0, raw_span);
            }
            builder.set_empty_message_anchor(raw_span.start()).unwrap();
        } else if raw_value == message_text {
            builder.push_identity(byte_span(0, message_text.len()), raw_span);
        } else {
            builder.push_unescape(byte_span(0, message_text.len()), raw_span);
        }
        builder.finish(source, message_text).unwrap()
    }

    fn test_registry() -> (HostFormatRegistry, Arc<AdapterCalls>) {
        let calls = Arc::new(AdapterCalls::default());
        let registry =
            HostFormatRegistry::with_json_adapter(Arc::new(LineAdapter::new(Arc::clone(&calls))));
        (registry, calls)
    }

    fn extract(registry: &HostFormatRegistry, source: &str) -> ExtractedCatalog {
        let resolved = registry.resolve_direct_extension(".JSON").unwrap();
        registry.extract(resolved, Arc::from(source)).unwrap()
    }

    fn new_builder(source: &str) -> ArtifactBuilder {
        let (registry, _) = test_registry();
        let resolved = registry.resolve_direct_extension(".json").unwrap();
        ArtifactBuilder::new(Arc::from(source), resolved, ResourcePhase::Extract)
    }

    fn line_source(entry_count: usize) -> String {
        let mut source = String::new();
        for index in 0..entry_count {
            writeln!(source, "k{index}=x").unwrap();
        }
        source
    }

    fn test_entry(
        source: &str,
        structural_path: &str,
        raw_span: Utf8ByteSpan,
    ) -> AdapterMessageEntry {
        let start = usize::try_from(raw_span.start()).unwrap();
        let end = usize::try_from(raw_span.end()).unwrap();
        let message = &source[start..end];
        AdapterMessageEntry::new(
            structural_path.to_owned(),
            CatalogKeyDomain::JsonPointer,
            structural_path.to_owned(),
            Some(structural_path.to_owned()),
            raw_span,
            message.to_owned(),
            test_offset_map(source, raw_span, message, message),
            false,
        )
    }

    fn assert_internal(error: &ResourceError, expected: InternalResourceErrorReason) {
        assert_eq!(error.code(), ResourceErrorCode::Internal);
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::Internal { reason } if *reason == expected
        ));
    }

    fn assert_limit(error: &ResourceError, resource: ResourceLimit, actual: u128) {
        assert_eq!(error.code(), ResourceErrorCode::LimitExceeded);
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::LimitExceeded {
                resource: observed_resource,
                actual: observed_actual,
                ..
            } if *observed_resource == resource && *observed_actual == actual
        ));
    }

    #[test]
    fn assigns_occurrences_in_artifact_wide_raw_order() {
        let (registry, _) = test_registry();
        let catalog = extract(&registry, "a=one\na=two\nb=three\n");

        assert_eq!(catalog.source(), "a=one\na=two\nb=three\n");
        assert_eq!(catalog.entries().len(), 3);
        let observed = catalog
            .entries()
            .iter()
            .map(|entry| {
                (
                    entry.key().structural_path().as_str(),
                    entry.key().occurrence(),
                    entry.catalog_key_domain(),
                    entry.catalog_key().as_str(),
                    entry.display_key(),
                    entry.message_text(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            observed,
            vec![
                ("a", 0, CatalogKeyDomain::JsonPointer, "a", Some("a"), "one"),
                ("a", 1, CatalogKeyDomain::JsonPointer, "a", Some("a"), "two"),
                (
                    "b",
                    0,
                    CatalogKeyDomain::JsonPointer,
                    "b",
                    Some("b"),
                    "three"
                ),
            ]
        );
    }

    #[test]
    fn rejects_conflicting_entry_spans_and_accepts_touching_nonempty_spans() {
        let source = "abcdef";
        let mut adjacent = new_builder(source);
        adjacent
            .push_entry(test_entry(source, "a", Utf8ByteSpan::new(0, 2)))
            .unwrap();
        adjacent
            .push_entry(test_entry(source, "b", Utf8ByteSpan::new(2, 4)))
            .unwrap();
        assert_eq!(adjacent.finalize(Arc::new(())).unwrap().entries().len(), 2);

        for spans in [
            [Utf8ByteSpan::new(0, 3), Utf8ByteSpan::new(2, 4)],
            [Utf8ByteSpan::new(1, 3), Utf8ByteSpan::new(3, 3)],
            [Utf8ByteSpan::new(1, 1), Utf8ByteSpan::new(1, 2)],
            [Utf8ByteSpan::new(1, 1), Utf8ByteSpan::new(1, 1)],
        ] {
            let mut builder = new_builder(source);
            builder
                .push_entry(test_entry(source, "a", spans[0]))
                .unwrap();
            builder
                .push_entry(test_entry(source, "b", spans[1]))
                .unwrap();
            let error = builder.finalize(Arc::new(())).unwrap_err();
            assert_internal(&error, InternalResourceErrorReason::AdapterInvariantFailed);
            assert_eq!(error.phase(), ResourcePhase::Extract);
        }
    }

    #[test]
    fn enforces_every_cumulative_artifact_budget() {
        let source = "x";
        let cases = [
            (
                (u128::from(MAX_ENTRIES), 0, 0, 0),
                ResourceLimit::Entries,
                u128::from(MAX_ENTRIES) + 1,
            ),
            (
                (0, u128::from(MAX_TOTAL_MESSAGE_BYTES), 0, 0),
                ResourceLimit::TotalMessageBytes,
                u128::from(MAX_TOTAL_MESSAGE_BYTES) + 1,
            ),
            (
                (0, 0, u128::from(MAX_IDENTITY_BYTES), 0),
                ResourceLimit::IdentityBytes,
                u128::from(MAX_IDENTITY_BYTES) + 1,
            ),
            (
                (0, 0, 0, u128::from(MAX_OFFSET_MAP_SEGMENTS)),
                ResourceLimit::OffsetMapSegments,
                u128::from(MAX_OFFSET_MAP_SEGMENTS) + 1,
            ),
        ];

        for ((entries, total, identity, segments), resource, actual) in cases {
            let mut builder = new_builder(source);
            builder.set_counters_for_test(entries, total, identity, segments);
            let error = builder
                .push_entry(test_entry(source, "x", Utf8ByteSpan::new(0, 1)))
                .unwrap_err();
            assert_limit(&error, resource, actual);
            assert_eq!(error.phase(), ResourcePhase::Extract);
        }
    }

    #[test]
    fn entry_preflight_preserves_limit_order_without_mutating_admission_state() {
        let source = "x";
        let raw_span = Utf8ByteSpan::new(0, 1);
        let cases = [
            (
                (
                    u128::from(MAX_ENTRIES),
                    u128::from(MAX_TOTAL_MESSAGE_BYTES),
                    u128::from(MAX_IDENTITY_BYTES),
                    u128::from(MAX_MESSAGE_BYTES) + 1,
                ),
                ResourceLimit::Entries,
                u128::from(MAX_ENTRIES) + 1,
            ),
            (
                (
                    0,
                    u128::from(MAX_TOTAL_MESSAGE_BYTES),
                    u128::from(MAX_IDENTITY_BYTES),
                    u128::from(MAX_MESSAGE_BYTES) + 1,
                ),
                ResourceLimit::MessageBytes,
                u128::from(MAX_MESSAGE_BYTES) + 1,
            ),
            (
                (
                    0,
                    u128::from(MAX_TOTAL_MESSAGE_BYTES),
                    u128::from(MAX_IDENTITY_BYTES),
                    1,
                ),
                ResourceLimit::TotalMessageBytes,
                u128::from(MAX_TOTAL_MESSAGE_BYTES) + 1,
            ),
            (
                (0, 0, u128::from(MAX_IDENTITY_BYTES), 1),
                ResourceLimit::IdentityBytes,
                u128::from(MAX_IDENTITY_BYTES) + 1,
            ),
        ];

        for ((entries, total, identity, message), resource, actual) in cases {
            let mut builder = new_builder(source);
            builder.set_counters_for_test(entries, total, identity, 0);
            let error = builder
                .preflight_entry("x", "x", None, raw_span, message)
                .unwrap_err();
            assert_limit(&error, resource, actual);
            assert_eq!(error.site().unwrap().span(), raw_span);
            assert_eq!(
                error
                    .site()
                    .unwrap()
                    .entry_key()
                    .unwrap()
                    .structural_path()
                    .as_str(),
                "x"
            );
        }

        let mut builder = new_builder(source);
        assert_eq!(builder.preflight_entry("x", "x", None, raw_span, 1), Ok(0));
        assert_eq!(builder.preflight_entry("x", "x", None, raw_span, 1), Ok(0));
        builder
            .push_entry(test_entry(source, "x", raw_span))
            .unwrap();
        assert_eq!(builder.preflight_entry("x", "x", None, raw_span, 1), Ok(1));
    }

    #[test]
    fn one_registry_supports_isolated_concurrent_extractions() {
        let (registry, calls) = test_registry();
        let registry = Arc::new(registry);
        let threads = (0..8)
            .map(|_| {
                let registry = Arc::clone(&registry);
                thread::spawn(move || {
                    let catalog = extract(&registry, "a=one\na=two\n");
                    assert_eq!(catalog.entries()[1].key().occurrence(), 1);
                    catalog.entries()[0].handle()
                })
            })
            .collect::<Vec<_>>();
        let handles = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<HashSet<_>>();

        assert_eq!(handles.len(), 8);
        assert_eq!(calls.extracts.load(Ordering::Relaxed), 8);
    }

    #[test]
    fn candidate_admission_accepts_raw_order_original_and_formatted_entries() {
        let (registry, _) = test_registry();
        let catalog = extract(&registry, "a=one\n!locked=stay\nb=two\n");
        let entries = catalog.entries();
        let mut admission = catalog.begin_candidate_message_admission();

        admission
            .admit_formatted_bytes(entries[0].handle(), 3)
            .unwrap();
        admission.admit_original(entries[1].handle()).unwrap();
        admission.admit_original(entries[2].handle()).unwrap();
        assert_eq!(admission.finish(), Ok(()));
    }

    #[test]
    fn candidate_admission_rejects_invalid_order_scope_and_read_only_use() {
        let (registry, _) = test_registry();
        let catalog = extract(&registry, "a=one\n!locked=stay\nb=two\n");
        let foreign = extract(&registry, "a=one\n").entries()[0].handle();
        let entries = catalog.entries();
        let nonexistent = EntryHandle::new(catalog.artifact_identity, u32::MAX);

        let invalid_first_handles = [
            foreign,
            nonexistent,
            entries[1].handle(),
            entries[2].handle(),
        ];
        for handle in invalid_first_handles {
            let mut admission = catalog.begin_candidate_message_admission();
            let error = admission.admit_original(handle).unwrap_err();
            assert_internal(&error, InternalResourceErrorReason::InvalidEntryHandle);
            assert_eq!(error.phase(), ResourcePhase::ValidateWriteBack);
        }

        let mut repeated = catalog.begin_candidate_message_admission();
        repeated.admit_original(entries[0].handle()).unwrap();
        assert_internal(
            &repeated.admit_original(entries[0].handle()).unwrap_err(),
            InternalResourceErrorReason::InvalidEntryHandle,
        );

        let mut read_only = catalog.begin_candidate_message_admission();
        read_only.admit_original(entries[0].handle()).unwrap();
        assert_internal(
            &read_only
                .admit_formatted_bytes(entries[1].handle(), 4)
                .unwrap_err(),
            InternalResourceErrorReason::InvalidEntryHandle,
        );

        let unfinished = catalog.begin_candidate_message_admission();
        assert_internal(
            &unfinished.finish().unwrap_err(),
            InternalResourceErrorReason::InvalidEntryHandle,
        );
    }

    #[test]
    fn candidate_admission_checks_message_before_running_total_and_becomes_terminal() {
        let (registry, _) = test_registry();
        let catalog = extract(&registry, "a=x\nb=y\n");
        let first = catalog.entries()[0].handle();

        let mut message = catalog.begin_candidate_message_admission();
        message.total_message_bytes = u128::from(MAX_TOTAL_MESSAGE_BYTES);
        let error = message
            .admit_formatted_bytes(first, MAX_MESSAGE_BYTES + 1)
            .unwrap_err();
        assert_limit(
            &error,
            ResourceLimit::MessageBytes,
            u128::from(MAX_MESSAGE_BYTES) + 1,
        );
        assert_internal(
            &message.admit_original(first).unwrap_err(),
            InternalResourceErrorReason::InvalidEntryHandle,
        );

        let mut total = catalog.begin_candidate_message_admission();
        total.total_message_bytes = u128::from(MAX_TOTAL_MESSAGE_BYTES);
        let error = total.admit_formatted_bytes(first, 1).unwrap_err();
        assert_limit(
            &error,
            ResourceLimit::TotalMessageBytes,
            u128::from(MAX_TOTAL_MESSAGE_BYTES) + 1,
        );
    }

    #[test]
    fn write_back_validates_the_complete_slice_before_raw_order_violations() {
        let (registry, _) = test_registry();
        let catalog = extract(&registry, "!a=one\nb=two\n");
        let foreign_catalog = extract(&registry, "x=value\n");
        let foreign = foreign_catalog.entries()[0].handle();
        let read_only = catalog.entries()[0].handle();
        let nonexistent = EntryHandle::new(catalog.artifact_identity, u32::MAX);
        let value = "changed";

        for entries in [
            vec![
                FormattedEntry {
                    entry: read_only,
                    formatted_message: value,
                },
                FormattedEntry {
                    entry: foreign,
                    formatted_message: value,
                },
            ],
            vec![
                FormattedEntry {
                    entry: foreign,
                    formatted_message: value,
                },
                FormattedEntry {
                    entry: read_only,
                    formatted_message: value,
                },
            ],
            vec![FormattedEntry {
                entry: nonexistent,
                formatted_message: value,
            }],
        ] {
            let error = catalog.build_and_validate_write_back(&entries).unwrap_err();
            assert_internal(&error, InternalResourceErrorReason::InvalidEntryHandle);
            assert!(error.site().is_none());
        }

        let duplicate_read_only = [
            FormattedEntry {
                entry: read_only,
                formatted_message: value,
            },
            FormattedEntry {
                entry: read_only,
                formatted_message: value,
            },
        ];
        let error = catalog
            .build_and_validate_write_back(&duplicate_read_only)
            .unwrap_err();
        assert_internal(&error, InternalResourceErrorReason::InvalidEntryHandle);
        assert_eq!(
            error.site().unwrap().entry_key(),
            Some(catalog.entries()[0].key())
        );
    }

    #[test]
    fn duplicate_selection_is_independent_of_caller_slice_order() {
        let (registry, _) = test_registry();
        let catalog = extract(&registry, "a=one\nb=two\n");
        let a = catalog.entries()[0].handle();
        let b = catalog.entries()[1].handle();
        let entries = [
            FormattedEntry {
                entry: b,
                formatted_message: "B1",
            },
            FormattedEntry {
                entry: b,
                formatted_message: "B2",
            },
            FormattedEntry {
                entry: a,
                formatted_message: "A1",
            },
            FormattedEntry {
                entry: a,
                formatted_message: "A2",
            },
        ];

        let error = catalog.build_and_validate_write_back(&entries).unwrap_err();
        assert_eq!(
            error.site().unwrap().entry_key(),
            Some(catalog.entries()[0].key())
        );
    }

    #[test]
    fn changed_write_back_returns_raw_order_replacements_and_a_fresh_equivalent_artifact() {
        let (registry, calls) = test_registry();
        let catalog = extract(&registry, "a=one\nb=two\n");
        let original_handles = catalog
            .entries()
            .iter()
            .map(super::MessageEntry::handle)
            .collect::<Vec<_>>();
        let formatted = [
            FormattedEntry {
                entry: original_handles[1],
                formatted_message: "TWO",
            },
            FormattedEntry {
                entry: original_handles[0],
                formatted_message: "ONE",
            },
        ];

        let outcome = catalog.build_and_validate_write_back(&formatted).unwrap();
        let WriteBackOutcome::Changed(write_back) = outcome else {
            panic!("changed input must produce a validated write-back");
        };
        let replacements = write_back.replacements();
        assert_eq!(replacements.len(), 2);
        assert!(replacements[0].span().start() < replacements[1].span().start());
        assert_eq!(replacements[0].raw_text(), "ONE");
        assert_eq!(replacements[1].raw_text(), "TWO");

        let candidate = write_back.candidate();
        assert_eq!(candidate.source(), "a=ONE\nb=TWO\n");
        assert_eq!(
            candidate
                .entries()
                .iter()
                .map(super::MessageEntry::message_text)
                .collect::<Vec<_>>(),
            ["ONE", "TWO"]
        );
        for (index, entry) in candidate.entries().iter().enumerate() {
            assert_ne!(entry.handle(), original_handles[index]);
            assert_eq!(entry.key(), catalog.entries()[index].key());
            assert_eq!(entry.catalog_key_domain(), CatalogKeyDomain::JsonPointer);
            assert_eq!(entry.catalog_key(), catalog.entries()[index].catalog_key());
            assert_eq!(entry.display_key(), catalog.entries()[index].display_key());
            assert_eq!(
                entry.is_read_only(),
                catalog.entries()[index].is_read_only()
            );
        }
        assert_eq!(calls.extracts.load(Ordering::Relaxed), 2);
        assert_eq!(calls.plans.load(Ordering::Relaxed), 2);
        assert_eq!(calls.materializations.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn byte_identical_or_empty_input_takes_the_unchanged_fast_path() {
        let (registry, calls) = test_registry();
        let catalog = extract(&registry, "a=one\n");

        assert!(matches!(
            catalog.build_and_validate_write_back(&[]).unwrap(),
            WriteBackOutcome::Unchanged
        ));
        let identical = [FormattedEntry {
            entry: catalog.entries()[0].handle(),
            formatted_message: "one",
        }];
        assert!(matches!(
            catalog.build_and_validate_write_back(&identical).unwrap(),
            WriteBackOutcome::Unchanged
        ));
        assert_eq!(calls.extracts.load(Ordering::Relaxed), 1);
        assert_eq!(calls.plans.load(Ordering::Relaxed), 0);
        assert_eq!(calls.materializations.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn empty_and_byte_identical_write_back_do_not_allocate() {
        let (registry, _) = test_registry();
        let catalog = extract(&registry, "a=one\nb=two\n");
        let identical = [
            FormattedEntry {
                entry: catalog.entries()[1].handle(),
                formatted_message: "two",
            },
            FormattedEntry {
                entry: catalog.entries()[0].handle(),
                formatted_message: "one",
            },
        ];

        let empty_allocations = measure(|| {
            assert!(matches!(
                catalog.build_and_validate_write_back(&[]).unwrap(),
                WriteBackOutcome::Unchanged
            ));
        });
        let identical_allocations = measure(|| {
            assert!(matches!(
                catalog.build_and_validate_write_back(&identical).unwrap(),
                WriteBackOutcome::Unchanged
            ));
        });

        assert_eq!(empty_allocations.count_total, 0);
        assert_eq!(identical_allocations.count_total, 0);
    }

    #[test]
    fn byte_identical_fast_path_preserves_duplicate_and_read_only_validation() {
        let (registry, _) = test_registry();
        let catalog = extract(&registry, "!locked=stay\nb=two\n");
        let locked = catalog.entries()[0].handle();
        let writable = catalog.entries()[1].handle();

        let duplicate = [
            FormattedEntry {
                entry: writable,
                formatted_message: "two",
            },
            FormattedEntry {
                entry: writable,
                formatted_message: "two",
            },
        ];
        let error = catalog
            .build_and_validate_write_back(&duplicate)
            .unwrap_err();
        assert_internal(&error, InternalResourceErrorReason::InvalidEntryHandle);
        assert_eq!(
            error.site().unwrap().entry_key(),
            Some(catalog.entries()[1].key())
        );

        let duplicate_and_read_only = [
            FormattedEntry {
                entry: writable,
                formatted_message: "two",
            },
            FormattedEntry {
                entry: writable,
                formatted_message: "two",
            },
            FormattedEntry {
                entry: locked,
                formatted_message: "stay",
            },
        ];
        let error = catalog
            .build_and_validate_write_back(&duplicate_and_read_only)
            .unwrap_err();
        assert_internal(&error, InternalResourceErrorReason::InvalidEntryHandle);
        assert_eq!(
            error.site().unwrap().entry_key(),
            Some(catalog.entries()[0].key())
        );
    }

    #[test]
    fn integrated_measurement_checks_per_message_before_running_total() {
        let (registry, calls) = test_registry();
        let source = line_source(65);
        let catalog = extract(&registry, &source);
        let at_limit = "x".repeat(usize::try_from(MAX_MESSAGE_BYTES).unwrap());
        let first_over = "x".repeat(usize::try_from(MAX_MESSAGE_BYTES + 1).unwrap());
        let formatted = catalog
            .entries()
            .iter()
            .enumerate()
            .map(|(index, entry)| FormattedEntry {
                entry: entry.handle(),
                formatted_message: if index == 64 { &first_over } else { &at_limit },
            })
            .collect::<Vec<_>>();

        let error = catalog
            .build_and_validate_write_back(&formatted)
            .unwrap_err();
        assert_limit(
            &error,
            ResourceLimit::MessageBytes,
            u128::from(MAX_MESSAGE_BYTES) + 1,
        );
        assert_eq!(
            error.site().unwrap().entry_key(),
            Some(catalog.entries()[64].key())
        );
        assert_eq!(calls.plans.load(Ordering::Relaxed), 64);
        assert_eq!(calls.materializations.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn integrated_measurement_reports_the_first_running_total_overrun() {
        let (registry, calls) = test_registry();
        let source = line_source(65);
        let catalog = extract(&registry, &source);
        let at_limit = "x".repeat(usize::try_from(MAX_MESSAGE_BYTES).unwrap());
        let formatted = catalog
            .entries()
            .iter()
            .map(|entry| FormattedEntry {
                entry: entry.handle(),
                formatted_message: &at_limit,
            })
            .collect::<Vec<_>>();

        let error = catalog
            .build_and_validate_write_back(&formatted)
            .unwrap_err();
        assert_limit(
            &error,
            ResourceLimit::TotalMessageBytes,
            u128::from(MAX_TOTAL_MESSAGE_BYTES) + u128::from(MAX_MESSAGE_BYTES),
        );
        assert_eq!(
            error.site().unwrap().entry_key(),
            Some(catalog.entries()[64].key())
        );
        assert_eq!(calls.plans.load(Ordering::Relaxed), 64);
        assert_eq!(calls.materializations.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn complete_candidate_projection_checks_host_bytes_before_materialization() {
        let (registry, calls) = test_registry();
        let source = line_source(64);
        let catalog = extract(&registry, &source);
        let at_limit = "x".repeat(usize::try_from(MAX_MESSAGE_BYTES).unwrap());
        let formatted = catalog
            .entries()
            .iter()
            .map(|entry| FormattedEntry {
                entry: entry.handle(),
                formatted_message: &at_limit,
            })
            .collect::<Vec<_>>();
        let expected = source.len() as u128 - 64 + u128::from(MAX_TOTAL_MESSAGE_BYTES);

        let error = catalog
            .build_and_validate_write_back(&formatted)
            .unwrap_err();
        assert_limit(&error, ResourceLimit::HostBytes, expected);
        assert!(error.site().is_none());
        assert_eq!(calls.plans.load(Ordering::Relaxed), 64);
        assert_eq!(calls.materializations.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn materialized_length_mismatch_is_an_adapter_invariant_failure() {
        let (registry, _) = test_registry();
        let catalog = extract(&registry, "a=one\n");
        let formatted = [FormattedEntry {
            entry: catalog.entries()[0].handle(),
            formatted_message: "@length-mismatch",
        }];

        let error = catalog
            .build_and_validate_write_back(&formatted)
            .unwrap_err();
        assert_internal(&error, InternalResourceErrorReason::AdapterInvariantFailed);
        assert_eq!(error.phase(), ResourcePhase::ValidateWriteBack);
        assert_eq!(
            error.site().unwrap().entry_key(),
            Some(catalog.entries()[0].key())
        );
    }

    #[test]
    fn final_projection_allows_early_expansion_offset_by_later_shrink() {
        let limit = u128::from(MAX_HOST_BYTES);
        assert_eq!(project_final_host_bytes(limit, [(1, 2)]), Some(limit + 1));
        assert_eq!(
            project_final_host_bytes(limit, [(1, 2), (10, 9)]),
            Some(limit)
        );
        assert_eq!(project_final_host_bytes(1, [(2, 0)]), None);
    }

    #[test]
    fn candidate_extraction_failures_follow_the_conversion_matrix() {
        let (registry, _) = test_registry();
        let catalog = extract(&registry, "a=one\n");
        let handle = catalog.entries()[0].handle();
        let cases = [
            (
                "@error:format",
                Some(InternalResourceErrorReason::WriteBackFailed),
            ),
            (
                "@error:parse",
                Some(InternalResourceErrorReason::WriteBackFailed),
            ),
            (
                "@error:entry",
                Some(InternalResourceErrorReason::WriteBackFailed),
            ),
            (
                "@error:document",
                Some(InternalResourceErrorReason::WriteBackFailed),
            ),
            ("@error:limit", None),
            (
                "@error:identity",
                Some(InternalResourceErrorReason::ArtifactIdentityExhausted),
            ),
            (
                "@error:offset",
                Some(InternalResourceErrorReason::OffsetMapInvariantFailed),
            ),
            (
                "@error:adapter",
                Some(InternalResourceErrorReason::AdapterInvariantFailed),
            ),
        ];

        for (message, expected_reason) in cases {
            let formatted = [FormattedEntry {
                entry: handle,
                formatted_message: message,
            }];
            let error = catalog
                .build_and_validate_write_back(&formatted)
                .unwrap_err();
            assert_eq!(error.phase(), ResourcePhase::ValidateWriteBack);
            assert_eq!(
                error.site().and_then(ResourceErrorSite::entry_key),
                Some(catalog.entries()[0].key()),
                "candidate site must map back for {message}"
            );
            if let Some(reason) = expected_reason {
                assert_internal(&error, reason);
            } else {
                assert_limit(
                    &error,
                    ResourceLimit::MessageBytes,
                    u128::from(MAX_MESSAGE_BYTES) + 1,
                );
            }
        }
    }

    #[test]
    fn candidate_count_identity_metadata_and_value_mismatches_fail_write_back() {
        let (registry, _) = test_registry();
        let catalog = extract(&registry, "a=one\n");
        let handle = catalog.entries()[0].handle();
        for (message, has_site) in [
            ("@drop", false),
            ("@key-mismatch", true),
            ("@domain-mismatch", true),
            ("@catalog-mismatch", true),
            ("@display-mismatch", true),
            ("@readonly-mismatch", true),
            ("@message-mismatch", true),
        ] {
            let formatted = [FormattedEntry {
                entry: handle,
                formatted_message: message,
            }];
            let error = catalog
                .build_and_validate_write_back(&formatted)
                .unwrap_err();
            assert_internal(&error, InternalResourceErrorReason::WriteBackFailed);
            assert_eq!(
                error.site().is_some(),
                has_site,
                "unexpected site for {message}"
            );
        }
    }

    #[test]
    fn a_late_materialization_failure_exposes_no_partial_outcome() {
        let (registry, calls) = test_registry();
        let catalog = extract(&registry, "a=one\nb=two\n");
        let formatted = [
            FormattedEntry {
                entry: catalog.entries()[0].handle(),
                formatted_message: "ONE",
            },
            FormattedEntry {
                entry: catalog.entries()[1].handle(),
                formatted_message: "@materialize-error",
            },
        ];

        let error = catalog
            .build_and_validate_write_back(&formatted)
            .unwrap_err();
        assert_internal(&error, InternalResourceErrorReason::AdapterInvariantFailed);
        assert_eq!(calls.materializations.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn one_artifact_supports_concurrent_independent_write_backs() {
        let (registry, _) = test_registry();
        let catalog = Arc::new(extract(&registry, "a=one\n"));
        let threads = (0..8)
            .map(|index| {
                let catalog = Arc::clone(&catalog);
                thread::spawn(move || {
                    let message = format!("value-{index}");
                    let formatted = [FormattedEntry {
                        entry: catalog.entries()[0].handle(),
                        formatted_message: &message,
                    }];
                    let WriteBackOutcome::Changed(write_back) =
                        catalog.build_and_validate_write_back(&formatted).unwrap()
                    else {
                        panic!("different text must change the catalog");
                    };
                    assert_eq!(write_back.candidate().entries()[0].message_text(), message);
                    write_back.candidate().entries()[0].handle()
                })
            })
            .collect::<Vec<_>>();
        let handles = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<HashSet<EntryHandle>>();

        assert_eq!(handles.len(), 8);
    }
}
