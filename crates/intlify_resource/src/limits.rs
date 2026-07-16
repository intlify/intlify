// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::{ResourceError, ResourceErrorSite, ResourcePhase};

/// Inclusive maximum complete host-document byte length.
pub const MAX_HOST_BYTES: u64 = 67_108_864;
/// Inclusive maximum open-container nesting depth per parser instance.
pub const MAX_NESTING_DEPTH: u32 = 256;
/// Inclusive maximum extracted entries per complete artifact.
pub const MAX_ENTRIES: u32 = 100_000;
/// Inclusive maximum UTF-8 byte length of one effective message.
pub const MAX_MESSAGE_BYTES: u64 = 1_048_576;
/// Inclusive maximum total UTF-8 message bytes per complete artifact.
pub const MAX_TOTAL_MESSAGE_BYTES: u64 = 67_108_864;
/// Inclusive maximum distinct interned identity payload bytes per artifact.
pub const MAX_IDENTITY_BYTES: u64 = 67_108_864;
/// Inclusive maximum canonical offset-map segments per artifact.
pub const MAX_OFFSET_MAP_SEGMENTS: u64 = 1_000_000;

/// Fixed resource representation governed by the catalog adapter contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceLimit {
    /// Complete top-level raw host source bytes.
    HostBytes,
    /// Simultaneously open host containers in one parser instance.
    NestingDepth,
    /// Extracted message entries in one artifact.
    Entries,
    /// UTF-8 bytes in one effective message.
    MessageBytes,
    /// Total UTF-8 bytes across all effective messages.
    TotalMessageBytes,
    /// Total exact bytes in distinct interned identity strings.
    IdentityBytes,
    /// Total final canonical message-to-raw map segments.
    OffsetMapSegments,
}

impl ResourceLimit {
    /// Return the stable CLI detail spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HostBytes => "host_bytes",
            Self::NestingDepth => "nesting_depth",
            Self::Entries => "entries",
            Self::MessageBytes => "message_bytes",
            Self::TotalMessageBytes => "total_message_bytes",
            Self::IdentityBytes => "identity_bytes",
            Self::OffsetMapSegments => "offset_map_segments",
        }
    }

    /// Return the fixed inclusive maximum as an exact error counter.
    #[must_use]
    pub const fn limit(self) -> u128 {
        match self {
            Self::HostBytes => MAX_HOST_BYTES as u128,
            Self::NestingDepth => MAX_NESTING_DEPTH as u128,
            Self::Entries => MAX_ENTRIES as u128,
            Self::MessageBytes => MAX_MESSAGE_BYTES as u128,
            Self::TotalMessageBytes => MAX_TOTAL_MESSAGE_BYTES as u128,
            Self::IdentityBytes => MAX_IDENTITY_BYTES as u128,
            Self::OffsetMapSegments => MAX_OFFSET_MAP_SEGMENTS as u128,
        }
    }
}

/// Check the raw byte length before UTF-8 decoding or host parsing.
///
/// The inclusive maximum succeeds; the first byte over returns an exact typed
/// counter and never exposes a partial artifact or outcome.
pub fn preflight_host_bytes(
    observed_bytes: usize,
    phase: ResourcePhase,
) -> Result<(), ResourceError> {
    check_limit(
        ResourceLimit::HostBytes,
        observed_bytes as u128,
        phase,
        None,
    )
}

pub(crate) fn check_limit(
    resource: ResourceLimit,
    actual: u128,
    phase: ResourcePhase,
    site: Option<ResourceErrorSite>,
) -> Result<(), ResourceError> {
    if actual <= resource.limit() {
        Ok(())
    } else {
        Err(ResourceError::limit_exceeded(resource, actual, phase, site))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        check_limit, preflight_host_bytes, ResourceLimit, MAX_ENTRIES, MAX_HOST_BYTES,
        MAX_IDENTITY_BYTES, MAX_MESSAGE_BYTES, MAX_NESTING_DEPTH, MAX_OFFSET_MAP_SEGMENTS,
        MAX_TOTAL_MESSAGE_BYTES,
    };
    use crate::{ResourceErrorCode, ResourceErrorDetails, ResourcePhase};

    #[test]
    fn product_constants_match_the_design() {
        assert_eq!(MAX_HOST_BYTES, 67_108_864);
        assert_eq!(MAX_NESTING_DEPTH, 256);
        assert_eq!(MAX_ENTRIES, 100_000);
        assert_eq!(MAX_MESSAGE_BYTES, 1_048_576);
        assert_eq!(MAX_TOTAL_MESSAGE_BYTES, 67_108_864);
        assert_eq!(MAX_IDENTITY_BYTES, 67_108_864);
        assert_eq!(MAX_OFFSET_MAP_SEGMENTS, 1_000_000);

        let limits = [
            (ResourceLimit::HostBytes, "host_bytes", 67_108_864),
            (ResourceLimit::NestingDepth, "nesting_depth", 256),
            (ResourceLimit::Entries, "entries", 100_000),
            (ResourceLimit::MessageBytes, "message_bytes", 1_048_576),
            (
                ResourceLimit::TotalMessageBytes,
                "total_message_bytes",
                67_108_864,
            ),
            (ResourceLimit::IdentityBytes, "identity_bytes", 67_108_864),
            (
                ResourceLimit::OffsetMapSegments,
                "offset_map_segments",
                1_000_000,
            ),
        ];

        for (resource, spelling, limit) in limits {
            assert_eq!(resource.as_str(), spelling);
            assert_eq!(resource.limit(), limit);
        }
    }

    #[test]
    fn host_preflight_accepts_the_inclusive_boundary_in_both_phases() {
        let boundary = usize::try_from(MAX_HOST_BYTES).unwrap();

        assert_eq!(
            preflight_host_bytes(boundary, ResourcePhase::Extract),
            Ok(())
        );
        assert_eq!(
            preflight_host_bytes(boundary, ResourcePhase::ValidateWriteBack),
            Ok(())
        );
    }

    #[test]
    fn host_preflight_reports_the_exact_first_byte_over() {
        let first_over = usize::try_from(MAX_HOST_BYTES + 1).unwrap();

        for phase in [ResourcePhase::Extract, ResourcePhase::ValidateWriteBack] {
            let error = preflight_host_bytes(first_over, phase).unwrap_err();

            assert_eq!(error.code(), ResourceErrorCode::LimitExceeded);
            assert_eq!(error.phase(), phase);
            assert!(error.site().is_none());
            assert_eq!(
                error.details(),
                &ResourceErrorDetails::LimitExceeded {
                    resource: ResourceLimit::HostBytes,
                    limit: 67_108_864,
                    actual: 67_108_865,
                }
            );
        }
    }

    #[test]
    fn typed_limit_counter_keeps_exact_u128_values() {
        let actual = u128::from(u64::MAX) + 37;
        let error = check_limit(
            ResourceLimit::TotalMessageBytes,
            actual,
            ResourcePhase::ValidateWriteBack,
            None,
        )
        .unwrap_err();

        assert_eq!(
            error.details(),
            &ResourceErrorDetails::LimitExceeded {
                resource: ResourceLimit::TotalMessageBytes,
                limit: 67_108_864,
                actual,
            }
        );
    }
}
