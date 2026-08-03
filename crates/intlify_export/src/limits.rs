// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Immutable programmatic limits for retained export-validation diagnostics.

/// Bounded diagnostic-retention policy for one preparation call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportValidationLimits {
    diagnostic_retention: u32,
}

impl ExportValidationLimits {
    pub const MAX_DIAGNOSTIC_RETENTION: u32 = 10_000;

    #[must_use]
    pub const fn protocol_defaults() -> Self {
        Self {
            diagnostic_retention: 1_000,
        }
    }

    pub const fn try_with_diagnostic_retention(
        self,
        value: u32,
    ) -> Result<Self, ExportValidationLimitConfigurationError> {
        if value > Self::MAX_DIAGNOSTIC_RETENTION {
            return Err(ExportValidationLimitConfigurationError { submitted: value });
        }
        Ok(Self {
            diagnostic_retention: value,
        })
    }

    #[must_use]
    pub const fn diagnostic_retention(&self) -> u32 {
        self.diagnostic_retention
    }
}

impl Default for ExportValidationLimits {
    fn default() -> Self {
        Self::protocol_defaults()
    }
}

/// Rejected programmatic diagnostic-retention value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportValidationLimitConfigurationError {
    submitted: u32,
}

impl ExportValidationLimitConfigurationError {
    #[must_use]
    pub const fn submitted(&self) -> u32 {
        self.submitted
    }
}

impl core::fmt::Display for ExportValidationLimitConfigurationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "diagnostic retention {} exceeds protocol ceiling {}",
            self.submitted,
            ExportValidationLimits::MAX_DIAGNOSTIC_RETENTION
        )
    }
}

impl std::error::Error for ExportValidationLimitConfigurationError {}

#[cfg(test)]
mod tests {
    use super::ExportValidationLimits;

    #[test]
    fn defaults_and_inclusive_boundaries_are_exact() {
        assert_eq!(
            ExportValidationLimits::default(),
            ExportValidationLimits::protocol_defaults()
        );
        assert_eq!(
            ExportValidationLimits::default().diagnostic_retention(),
            1_000
        );
        assert_eq!(
            ExportValidationLimits::default()
                .try_with_diagnostic_retention(0)
                .unwrap()
                .diagnostic_retention(),
            0
        );
        assert_eq!(
            ExportValidationLimits::default()
                .try_with_diagnostic_retention(ExportValidationLimits::MAX_DIAGNOSTIC_RETENTION)
                .unwrap()
                .diagnostic_retention(),
            ExportValidationLimits::MAX_DIAGNOSTIC_RETENTION
        );
    }

    #[test]
    fn values_above_the_ceiling_are_rejected_without_clamping() {
        let submitted = ExportValidationLimits::MAX_DIAGNOSTIC_RETENTION + 1;
        let error = ExportValidationLimits::default()
            .try_with_diagnostic_retention(submitted)
            .unwrap_err();
        assert_eq!(error.submitted(), submitted);
    }
}
