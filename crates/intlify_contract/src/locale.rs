// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::error::{ValueConstructionError, ValueGrammar};
use crate::fingerprint::FingerprintPayload;
use crate::{ArtifactLimitEvidence, LinkLimitCounter, LinkLimitObservation, LinkLimits};

const LOCALE_BYTES: usize = 255;

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
        if value.len() > LOCALE_BYTES {
            return Err(ValueConstructionError::field_limit(
                LinkLimitCounter::LocaleBytes,
                LOCALE_BYTES as u64,
                LOCALE_BYTES as u64 + 1,
            ));
        }
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
        let counter = LinkLimitCounter::LocaleBytes;
        let limit = limits.effective_limit(counter);
        if self.0.len() as u64 > limit {
            return Err(ArtifactLimitEvidence::try_new(
                counter,
                limit,
                LinkLimitObservation::Exact(limit + 1),
            )
            .expect("checked first-over locale evidence is valid"));
        }
        Ok(())
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
