// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Minimum project source/requested/default locale resolution. This private
//! slice is not a Profile, final Resolver Outcome, Policy admission, or 017 wire
//! representation. No arbitrary raw declaration can construct its input view.

use crate::input_limits::Bound;
use crate::model::{IntlifyConfig, ProfileId};

use super::{CanonicalLocale, CanonicalizationFailure, Canonicalizer, Provider};

/// Explicit internal execution bounds, never product policy defaults. The
/// active occurrence domain is only source + project requested + project default;
/// it does not claim the full 015 maxLocaleOccurrences accounting over other roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Limits {
    pub(crate) max_active_occurrences: Bound,
    pub(crate) max_requested_locales: Bound,
}

/// Borrow only the selected fields of a completely admitted configuration.
/// This avoids copying raw locale strings or unrelated policy/target structures.
/// No Debug or Serialize: raw authoring strings may contain rejected secrets.
pub(crate) struct Input<'config> {
    source: Option<&'config str>,
    requested: &'config [String],
    default: &'config str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Core {
    source: Option<CanonicalLocale>,
    requested: Vec<CanonicalLocale>,
    default: CanonicalLocale,
}

impl Core {
    pub(crate) const fn source_default(&self) -> Option<&CanonicalLocale> {
        self.source.as_ref()
    }

    pub(crate) fn requested(&self) -> &[CanonicalLocale] {
        &self.requested
    }

    pub(crate) const fn requested_default(&self) -> &CanonicalLocale {
        &self.default
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Location {
    SourceDefault,
    Requested(usize),
    RequestedDefault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Correction {
    pub(crate) location: Location,
    pub(crate) replacement: CanonicalLocale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Issue {
    Canonicalization {
        location: Location,
        reason: CanonicalizationFailure,
    },
    Duplicate {
        locale: CanonicalLocale,
        occurrences: Vec<usize>,
    },
    RequestedLimit {
        limit: Bound,
        actual: u64,
    },
    DefaultNotRequested {
        locale: CanonicalLocale,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    AccountingOverflow,
    OccurrenceLimit { limit: Bound, actual: u64 },
    Issues(Vec<Issue>),
}

/// Actual logical counts. None means the complete domain was not obtained,
/// not a zero count. These are not full Profile Resource Policy observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) active_occurrences: Option<u64>,
    pub(crate) canonical_requested: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Resolution {
    outcome: Result<Core, Failure>,
    corrections: Vec<Correction>,
    counts: Counts,
}

impl Resolution {
    pub(crate) fn value(&self) -> Result<&Core, &Failure> {
        self.outcome.as_ref()
    }

    pub(crate) fn corrections(&self) -> &[Correction] {
        &self.corrections
    }

    pub(crate) const fn counts(&self) -> Counts {
        self.counts
    }
}

impl<'config> Input<'config> {
    /// Selection happens outside this semantic operation. Both the complete
    /// root and a declared ID are required; no first-profile or host fallback.
    pub(crate) fn from_selected<Policy, Target>(
        config: &'config IntlifyConfig<Policy, Target>,
        id: &ProfileId,
    ) -> Option<Self> {
        let declaration = config.profiles().get(id)?;
        Some(Self {
            source: declaration
                .default_source_locale
                .as_option()
                .map(String::as_str),
            requested: declaration.requested_locales.as_slice(),
            default: &declaration.default_requested_locale,
        })
    }

    pub(crate) fn resolve<P: Provider>(
        &self,
        canonicalizer: &Canonicalizer<P>,
        limits: Limits,
    ) -> Resolution {
        let occurrences = u64::try_from(self.requested.len())
            .ok()
            .and_then(|requested| active_occurrences(requested, self.source.is_some()));
        let mut counts = Counts {
            active_occurrences: occurrences,
            canonical_requested: None,
        };
        let early_failure = match occurrences {
            None => Some(Failure::AccountingOverflow),
            Some(actual) if actual > limits.max_active_occurrences.get() => {
                Some(Failure::OccurrenceLimit {
                    limit: limits.max_active_occurrences,
                    actual,
                })
            }
            Some(_) => None,
        };
        if let Some(failure) = early_failure {
            // No canonicalization, locale-index allocation, or partial value.
            return Resolution {
                outcome: Err(failure),
                corrections: Vec::new(),
                counts,
            };
        }

        let mut issues = Vec::new();
        let mut corrections = Vec::new();
        let mut canonicalize = |location, raw| match canonicalizer.canonicalize(raw) {
            Ok(value) => {
                let corrected = value.suggested_replacement().is_some();
                let locale = value.into_locale();
                if corrected {
                    corrections.push(Correction {
                        location,
                        replacement: locale.clone(),
                    });
                }
                Some(locale)
            }
            Err(reason) => {
                issues.push(Issue::Canonicalization { location, reason });
                None
            }
        };
        let source = self
            .source
            .and_then(|raw| canonicalize(Location::SourceDefault, raw));
        // Reserve from the actual bounded input, never an attacker-chosen maximum.
        let mut requested = Vec::with_capacity(self.requested.len());
        let mut complete_requested = true;
        for (index, raw) in self.requested.iter().enumerate() {
            if let Some(locale) = canonicalize(Location::Requested(index), raw) {
                requested.push((locale, index));
            } else {
                complete_requested = false;
            }
        }
        let default = canonicalize(Location::RequestedDefault, self.default);

        // Canonical bytes decide set order; original indices only order related
        // occurrences of one duplicate. No host collation or hash iteration.
        requested.sort_unstable_by(|(left, left_index), (right, right_index)| {
            left.cmp(right).then_with(|| left_index.cmp(right_index))
        });
        let mut unique = 0_u64;
        let mut start = 0;
        while start < requested.len() {
            let mut end = start + 1;
            while end < requested.len() && requested[start].0 == requested[end].0 {
                end += 1;
            }
            unique += 1; // At most the already checked positive occurrence bound.
            if end - start > 1 {
                issues.push(Issue::Duplicate {
                    locale: requested[start].0.clone(),
                    occurrences: requested[start..end]
                        .iter()
                        .map(|(_, index)| *index)
                        .collect(),
                });
            }
            start = end;
        }

        if complete_requested {
            counts.canonical_requested = Some(unique);
            if unique > limits.max_requested_locales.get() {
                issues.push(Issue::RequestedLimit {
                    limit: limits.max_requested_locales,
                    actual: unique,
                });
            }
            if let Some(default) = &default {
                if requested
                    .binary_search_by(|(locale, _)| locale.cmp(default))
                    .is_err()
                {
                    issues.push(Issue::DefaultNotRequested {
                        locale: default.clone(),
                    });
                }
            }
        }
        // An unavailable canonical set suppresses only set-dependent checks.
        // Independent locale errors, duplicates, and corrections remain visible.
        let outcome = if issues.is_empty() {
            Ok(Core {
                source,
                requested: requested.into_iter().map(|(locale, _)| locale).collect(),
                default: default.expect("no issues implies an admitted canonical default"),
            })
        } else {
            Err(Failure::Issues(issues))
        };
        Resolution {
            outcome,
            corrections,
            counts,
        }
    }
}

fn active_occurrences(requested: u64, has_source: bool) -> Option<u64> {
    requested.checked_add(1)?.checked_add(u64::from(has_source))
}

#[cfg(test)]
mod tests;
