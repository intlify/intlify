// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Exact locale identity used at artifact and linker boundaries.
//!
//! This module owns non-empty locale spellings and their byte-limit validation.
//! A `Locale` is deliberately opaque, case-sensitive, and preserved byte for
//! byte so producers and consumers compare the same submitted identity.
//!
//! It does not validate or canonicalize BCP 47 language tags, negotiate locales,
//! or resolve fallback chains. Configuration and linker policy own those
//! semantics.

use crate::error::{ValueConstructionError, ValueGrammar};
use crate::fingerprint::FingerprintPayload;
use crate::{ArtifactLimitEvidence, LinkLimitCounter, LinkLimits};

/// Exact, opaque locale identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Locale(Box<str>);

impl Locale {
    /// Validate and retain one non-empty locale spelling.
    pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, ValueConstructionError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Empty));
        }
        LinkLimitCounter::LocaleBytes.check_construction_limit(value.len() as u64)?;
        Ok(Self(value))
    }

    /// Return the exact locale spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_limit(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        LinkLimitCounter::LocaleBytes.check_artifact_limit(self.0.len() as u64, limits)
    }
}

impl FingerprintPayload for Locale {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.0.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::Locale;
    use crate::{
        LinkLimitCounter, LinkLimitObservation, LinkLimits, ValueConstructionError, ValueGrammar,
    };

    #[test]
    fn locale_is_exact_case_sensitive_and_not_bcp47_normalized() {
        let underscore = Locale::try_new("pt_BR").unwrap();
        let hyphen = Locale::try_new("pt-BR").unwrap();
        let lower = Locale::try_new("pt-br").unwrap();
        assert_ne!(underscore, hyphen);
        assert_ne!(hyphen, lower);
        assert_eq!(Locale::try_new("  ").unwrap().as_str(), "  ");
        assert!(matches!(
            Locale::try_new(""),
            Err(ValueConstructionError::Grammar(ValueGrammar::Empty))
        ));
    }

    #[test]
    fn locale_byte_boundary_is_inclusive() {
        assert!(Locale::try_new("a".repeat(255)).is_ok());
        assert!(matches!(
            Locale::try_new("a".repeat(256)),
            Err(ValueConstructionError::FieldLimit {
                counter: LinkLimitCounter::LocaleBytes,
                limit: 255,
                attempted: 256,
            })
        ));
    }

    #[test]
    fn lower_locale_limit_accepts_exact_and_rejects_first_over() {
        let locale = Locale::try_new("é").unwrap();
        let zero = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::LocaleBytes, 0)
            .unwrap();
        assert_eq!(
            locale.revalidate_limit(&zero).unwrap_err().observation(),
            LinkLimitObservation::Exact(1)
        );

        let exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::LocaleBytes, 2)
            .unwrap();
        assert!(locale.revalidate_limit(&exact).is_ok());

        let first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::LocaleBytes, 1)
            .unwrap();
        assert_eq!(
            locale
                .revalidate_limit(&first_over)
                .unwrap_err()
                .observation(),
            LinkLimitObservation::Exact(2)
        );
    }
}
