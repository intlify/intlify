// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::benchmark::locale_resolution::{require_observable, PreparedLocales, Resolved};
use crate::model::FallbackCandidate;
use crate::resolver::evaluation::{Actual, Check, Status};
use crate::resolver::locales::unit;

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
    pub(super) fn locales(
        &mut self,
        prepared: &PreparedLocales,
        resolved: &Resolved,
    ) -> Result<(), WorkFailure> {
        let locales =
            require_observable(resolved).map_err(|_| WorkFailure::InvalidOrdinaryResult)?;
        if prepared.fields().is_none() {
            return Err(WorkFailure::InvalidOrdinaryResult);
        }
        let declaration = prepared
            .config
            .profiles()
            .get(&prepared.selected)
            .ok_or(WorkFailure::InvalidOrdinaryResult)?;
        // Every spelling 015's maxLocaleOccurrences counts, read from the
        // declaration itself rather than from the resolver under test. The
        // coverage selectors are counted, but Phase 7 never canonicalizes them.
        let mut spellings = declaration
            .requested_locales
            .as_slice()
            .iter()
            .map(String::as_str)
            .chain(
                declaration
                    .default_source_locale
                    .as_option()
                    .map(String::as_str),
            )
            .chain(std::iter::once(
                declaration.default_requested_locale.as_str(),
            ))
            .collect::<Vec<_>>();
        if let Some(aliases) = declaration
            .locale_negotiation
            .as_option()
            .and_then(|negotiation| negotiation.aliases.as_option())
        {
            for (key, value) in aliases {
                spellings.extend([key.as_str(), value.as_str()]);
            }
        }
        for (key, candidates) in declaration
            .message_fallback
            .as_option()
            .into_iter()
            .flatten()
        {
            spellings.push(key);
            spellings.extend(candidates.as_slice().iter().filter_map(
                |candidate| match candidate {
                    FallbackCandidate::Locale(locale) => Some(locale.as_str()),
                    FallbackCandidate::IntentSource(_) => None,
                },
            ));
        }
        let mut selectors = 0;
        let rules = declaration
            .coverage
            .as_option()
            .and_then(|coverage| coverage.rules.as_option());
        for items in rules
            .into_iter()
            .flatten()
            .filter_map(|rule| rule.requested_locales())
        {
            selectors += items.len();
            spellings.extend(items.iter().map(String::as_str));
        }
        for target in declaration.target_profiles.values() {
            spellings.extend(
                target
                    .requested_locales
                    .as_slice()
                    .iter()
                    .map(String::as_str),
            );
            spellings.extend(
                target
                    .default_requested_locale
                    .as_option()
                    .map(String::as_str),
            );
        }
        let (occurrences, raw_bytes) = strings(spellings.into_iter())?;
        if locales.occurrence_total != Some(Actual::Exact(occurrences)) {
            return Err(WorkFailure::InvalidOrdinaryResult);
        }
        // A spelling reached the provider when its subject's unit was evaluated:
        // none of them over the occurrence bound, every one but the selectors
        // within it.
        let evaluation = &resolved.evaluation;
        let canonicalized = count(
            locales
                .occurrences
                .iter()
                .filter(|occurrence| {
                    evaluation.status(&unit(occurrence.subject.clone(), Check::CanonicalLocale))
                        == Some(&Status::Evaluated)
                })
                .count(),
        )?;
        if canonicalized != 0 && Some(canonicalized) != occurrences.checked_sub(count(selectors)?) {
            return Err(WorkFailure::InvalidOrdinaryResult);
        }
        for (kind, value) in [
            (WorkKind::LocaleResolutionOccurrences, occurrences),
            (WorkKind::LocaleResolutionRawIdentifierBytes, raw_bytes),
        ] {
            self.set(kind, WorkStage::PreparedInput, WorkValue::exact(value));
        }
        self.set(
            WorkKind::LocaleResolutionCanonicalRequestedLocales,
            WorkStage::OperationResult,
            match &locales.requested_set {
                Ok(set) => WorkValue::exact(count(set.as_slice().len())?),
                Err(_) => WorkValue::Unavailable {
                    reason: UnavailableWork::LocaleResolutionRequestedNotResolved,
                },
            },
        );
        let not_evaluated = evaluation
            .statuses()
            .filter(|(_, status)| matches!(status, Status::NotEvaluated(_)))
            .count();
        for (kind, value) in [
            (
                WorkKind::LocaleResolutionCanonicalizedOccurrences,
                canonicalized,
            ),
            (
                WorkKind::LocaleResolutionEvaluationUnits,
                count(evaluation.statuses().count())?,
            ),
            (
                WorkKind::LocaleResolutionNotEvaluatedUnits,
                count(not_evaluated)?,
            ),
            (
                WorkKind::LocaleResolutionBlockingIssues,
                count(evaluation.issues().count())?,
            ),
            (
                WorkKind::LocaleResolutionCorrections,
                count(evaluation.notices().count())?,
            ),
        ] {
            self.set(kind, WorkStage::OperationResult, WorkValue::exact(value));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::benchmark::cases::prepare::prepare;
    use crate::benchmark::cases::{declarations, LocaleResolutionRecipe, Recipe};
    use crate::benchmark::operation::Output;
    use crate::resolver::evaluation::Actual;

    use super::super::{LogicalWork, WorkFailure};

    /// A total or a canonicalization the declaration cannot account for is no
    /// workload of it.
    #[test]
    fn work_disagreeing_with_the_declaration_is_rejected() {
        let declaration = declarations()
            .into_iter()
            .find(|case| {
                case.fixture == Recipe::LocaleResolution(LocaleResolutionRecipe::EveryRole)
                    && case.limit.is_none()
            })
            .unwrap();
        for change in 0..3 {
            let mut candidate = prepare(&declaration).unwrap();
            let Output::LocaleResolution(resolved) = &mut candidate.output else {
                unreachable!()
            };
            let locales = resolved.result.as_mut().unwrap();
            match change {
                0 => locales.occurrence_total = Some(Actual::Exact(22)),
                1 => locales.occurrence_total = Some(Actual::AtLeast(21)),
                // 17 spellings reached the provider, not 0 or 21 - 3.
                _ => {
                    locales.occurrences.pop();
                }
            }
            assert_eq!(
                LogicalWork::observe(&candidate.prepared, &candidate.output),
                Err(WorkFailure::InvalidOrdinaryResult),
                "change {change}"
            );
        }
    }
}
