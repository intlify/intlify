// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::benchmark::locale_core::{require_observable, PreparedCore};
use crate::locale::core::{Failure, Issue, Resolution};

use super::{LogicalWork, UnavailableWork, WorkFailure, WorkKind, WorkStage, WorkValue};

fn count(value: usize) -> Result<u64, WorkFailure> {
    u64::try_from(value).map_err(|_| WorkFailure::UnrepresentableCounter)
}

fn add(left: u64, right: u64) -> Result<u64, WorkFailure> {
    left.checked_add(right)
        .ok_or(WorkFailure::UnrepresentableCounter)
}

fn strings<'a>(mut values: impl Iterator<Item = &'a str>) -> Result<(u64, u64), WorkFailure> {
    values.try_fold((0, 0), |(occurrences, bytes), value| {
        Ok((add(occurrences, 1)?, add(bytes, count(value.len())?)?))
    })
}

impl LogicalWork {
    pub(super) fn core(
        &mut self,
        prepared: &PreparedCore,
        result: &Resolution,
    ) -> Result<(), WorkFailure> {
        require_observable(result).map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        if prepared.input().is_none() {
            return Err(WorkFailure::InvalidOrdinaryResult);
        }
        let declaration = prepared
            .config
            .profiles()
            .get(&prepared.selected)
            .ok_or(WorkFailure::InvalidOrdinaryResult)?;
        let (occurrences, raw_bytes) = strings(
            declaration
                .default_source_locale
                .as_option()
                .into_iter()
                .chain(declaration.requested_locales.as_slice())
                .chain(std::iter::once(&declaration.default_requested_locale))
                .map(String::as_str),
        )?;
        if result.counts().active_occurrences != Some(occurrences) {
            return Err(WorkFailure::InvalidOrdinaryResult);
        }
        for (kind, value) in [
            (WorkKind::LocaleCoreOccurrences, occurrences),
            (WorkKind::LocaleCoreRawIdentifierBytes, raw_bytes),
        ] {
            self.set(kind, WorkStage::PreparedInput, WorkValue::exact(value));
        }
        self.set(
            WorkKind::LocaleCoreCanonicalRequestedLocales,
            WorkStage::OperationResult,
            result.counts().canonical_requested.map_or(
                WorkValue::Unavailable {
                    reason: UnavailableWork::LocaleCoreRequestedNotResolved,
                },
                WorkValue::exact,
            ),
        );
        // Logical values retained in Core only, not physical allocation or the
        // canonical strings separately retained in issues/corrections. Repeated
        // semantic roles count as separate values even when their Arc is shared.
        let (retained, bytes) = match result.value() {
            Ok(core) => strings(
                core.source_default()
                    .into_iter()
                    .chain(core.requested())
                    .chain(std::iter::once(core.requested_default()))
                    .map(crate::locale::CanonicalLocale::as_str),
            )?,
            Err(_) => (0, 0),
        };
        let (reasons, related) = match result.value() {
            Ok(_) => (0, 0),
            Err(Failure::OccurrenceLimit { .. }) => (1, 0),
            Err(Failure::Issues(issues)) => {
                let related = issues.iter().try_fold(0, |total, issue| match issue {
                    Issue::Duplicate { occurrences, .. } => add(total, count(occurrences.len())?),
                    _ => Ok(total),
                })?;
                (count(issues.len())?, related)
            }
            Err(Failure::AccountingOverflow) => return Err(WorkFailure::InvalidOrdinaryResult),
        };
        for (kind, value) in [
            (WorkKind::LocaleCoreRetainedValues, retained),
            (WorkKind::LocaleCoreRetainedValueBytes, bytes),
            (WorkKind::LocaleCoreBlockingReasons, reasons),
            (WorkKind::LocaleCoreRelatedDuplicateOccurrences, related),
            (
                WorkKind::LocaleCoreCorrections,
                count(result.corrections().len())?,
            ),
        ] {
            self.set(kind, WorkStage::OperationResult, WorkValue::exact(value));
        }
        Ok(())
    }
}
