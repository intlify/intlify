// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `external-profile-selector-binding` boundary: the binding's closed
//! report of what it established about the host selector, admitted as the
//! normalized External Profile Selector Input or refused with one of the
//! three reasons of its row (D15:307, 3142; 015-167).
//!
//! The report is exactly one of six variants. `absent`, a `string` within the
//! bound, `over-limit-string`, and `invalid-type` with one of the five type
//! tags become the normalized input. `unrepresentable` is a value the binding
//! classified safely but that has none of those forms, such as a function,
//! an ill-formed host string, or a dynamic container, and is refused as
//! `unrepresentable-selector`; `unsafe` is a value whose top-level category
//! cannot be established without executing user behavior, refused as
//! `unsafe-host-inspection`. A binding stops reading a string once it crosses
//! the bound, so a `string` report longer than the bound breaks that contract
//! and is `malformed-normalized-selector`: the boundary neither truncates it
//! nor turns it into `over-limit-string`.
//!
//! The bound is the Admitted Implementation Capability's
//! `policyComparable.configurationInput.maxProfileIdBytes`. The report is the
//! boundary's formal input, so tests and conformance supply every report
//! without a live host; inspecting a live host is the binding's work.

use crate::artifacts::count;
use crate::input_limits::Bound;
use crate::structural::selection::{InvalidSelectorType, SelectorInput};

use super::capability::ImplementationCapability;

/// The binding's closed report of what it established about the host
/// selector. No `Debug`: a string can still carry an arbitrary secret.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum SelectorReport {
    /// No selector was supplied.
    Absent,
    /// One complete Unicode-scalar string.
    String(String),
    /// A string the binding stopped reading once it crossed the bound.
    OverLimitString,
    /// A top-level JSON type other than a string, established safely.
    InvalidType(InvalidSelectorType),
    /// A value classified safely that has none of the accepted forms.
    Unrepresentable,
    /// A value whose top-level category cannot be established without
    /// executing user behavior.
    Unsafe,
}

/// Why the boundary refuses a report: one of the three reasons of its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectorRefusal {
    UnsafeHostInspection,
    Unrepresentable,
    MalformedNormalized,
}

impl SelectorRefusal {
    /// The reason as 015's boundary table spells it.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::UnsafeHostInspection => "unsafe-host-inspection",
            Self::Unrepresentable => "unrepresentable-selector",
            Self::MalformedNormalized => "malformed-normalized-selector",
        }
    }
}

/// Admit a report under the capability's bound.
pub(crate) fn admit(
    report: SelectorReport,
    capability: &ImplementationCapability,
) -> Result<SelectorInput, SelectorRefusal> {
    read_report(
        report,
        capability
            .policy_comparable()
            .configuration_input
            .structural
            .max_profile_id_bytes,
    )
}

/// The boundary's mapping of a report under `bound`, which [`admit`] takes
/// from the capability.
pub(crate) fn read_report(
    report: SelectorReport,
    bound: Bound,
) -> Result<SelectorInput, SelectorRefusal> {
    match report {
        SelectorReport::Absent => Ok(SelectorInput::absent(bound)),
        SelectorReport::String(value) if count(value.len()) > bound.get() => {
            Err(SelectorRefusal::MalformedNormalized)
        }
        SelectorReport::String(value) => Ok(SelectorInput::string(&value, bound)),
        SelectorReport::OverLimitString => Ok(SelectorInput::over_limit(bound)),
        SelectorReport::InvalidType(kind) => Ok(SelectorInput::invalid_type(kind, bound)),
        SelectorReport::Unrepresentable => Err(SelectorRefusal::Unrepresentable),
        SelectorReport::Unsafe => Err(SelectorRefusal::UnsafeHostInspection),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::capability::{IDENTITY, REVISION};
    use crate::locale::VersionedIdentity;
    use crate::resolver::limits::fixture_limits;
    use crate::structural::selection::StringLengthFact;

    const TYPES: [InvalidSelectorType; 5] = [
        InvalidSelectorType::Null,
        InvalidSelectorType::Boolean,
        InvalidSelectorType::Number,
        InvalidSelectorType::Array,
        InvalidSelectorType::Object,
    ];

    fn bound(value: u64) -> Bound {
        Bound::new(value).unwrap()
    }

    /// The normalized input a report becomes, or the refusal.
    fn admitted(report: SelectorReport, under: Bound) -> SelectorInput {
        match read_report(report, under) {
            Ok(input) => input,
            Err(refusal) => panic!("refused as {}", refusal.reason()),
        }
    }

    fn refused(report: SelectorReport, under: Bound) -> Option<SelectorRefusal> {
        read_report(report, under).err()
    }

    #[test]
    fn the_accepted_reports_become_the_normalized_input() {
        let under = bound(8);
        assert!(admitted(SelectorReport::Absent, under)
            .benchmark_matches_fixture(&SelectorInput::absent(under)));
        assert!(admitted(SelectorReport::String("app".to_owned()), under)
            .benchmark_matches_fixture(&SelectorInput::string("app", under)));
        assert!(admitted(SelectorReport::OverLimitString, under)
            .benchmark_matches_fixture(&SelectorInput::string("123456789", under)));
        for kind in TYPES {
            assert!(admitted(SelectorReport::InvalidType(kind), under)
                .benchmark_matches_fixture(&SelectorInput::invalid_type(kind, under)));
        }
        // The empty string is a complete string; its syntax is the resolver's.
        assert!(admitted(SelectorReport::String(String::new()), under)
            .benchmark_matches_fixture(&SelectorInput::string("", under)));
    }

    /// The variant, not the payload, tells a string from a type tag.
    #[test]
    fn a_string_spelling_a_type_is_a_string() {
        let under = bound(8);
        let input = admitted(SelectorReport::String("array".to_owned()), under);
        assert!(
            !input.benchmark_matches_fixture(&SelectorInput::invalid_type(
                InvalidSelectorType::Array,
                under
            ))
        );
        assert_eq!(
            input.length_fact(under),
            Ok(Some(StringLengthFact::Exact(5)))
        );
        let tag = admitted(
            SelectorReport::InvalidType(InvalidSelectorType::Array),
            under,
        );
        assert_eq!(tag.length_fact(under), Ok(None));
    }

    /// A complete string up to the bound is admitted; one byte more breaks the
    /// binding's contract to stop reading, so the boundary refuses it rather
    /// than truncating it or making it `over-limit-string`.
    #[test]
    fn a_complete_string_past_the_bound_is_malformed() {
        let under = bound(4);
        assert!(admitted(SelectorReport::String("abcd".to_owned()), under)
            .benchmark_matches_fixture(&SelectorInput::string("abcd", under)));
        assert_eq!(
            refused(SelectorReport::String("abcde".to_owned()), under),
            Some(SelectorRefusal::MalformedNormalized)
        );
        // The bound counts UTF-8 bytes, not characters.
        assert_eq!(
            refused(
                SelectorReport::String("\u{e9}\u{e9}\u{e9}".to_owned()),
                under
            ),
            Some(SelectorRefusal::MalformedNormalized)
        );
        assert!(
            admitted(SelectorReport::String("\u{e9}\u{e9}".to_owned()), under)
                .benchmark_matches_fixture(&SelectorInput::string("\u{e9}\u{e9}", under))
        );
    }

    #[test]
    fn an_unrepresentable_or_unsafe_value_is_refused() {
        let under = bound(8);
        assert_eq!(
            refused(SelectorReport::Unrepresentable, under),
            Some(SelectorRefusal::Unrepresentable)
        );
        assert_eq!(
            refused(SelectorReport::Unsafe, under),
            Some(SelectorRefusal::UnsafeHostInspection)
        );
    }

    /// The capability whose `maxProfileIdBytes` is `bytes`, and every other
    /// bound as the fixture capability has it.
    fn capability(bytes: u64) -> ImplementationCapability {
        let mut value = ImplementationCapability::fixture_owned(fixture_limits(1)).normalized();
        value["policyComparable"]["configurationInput"]["maxProfileIdBytes"] =
            json!(bytes.to_string());
        let constructed = VersionedIdentity {
            identity: IDENTITY,
            revision: REVISION,
        };
        crate::boundary::capability::admit(Some(&value), &constructed).unwrap()
    }

    /// The bound is the capability's `maxProfileIdBytes`, and the normalized
    /// input carries it.
    #[test]
    fn the_capability_bounds_the_report() {
        let five = capability(5);
        let input = admit(SelectorReport::String("abcde".to_owned()), &five)
            .unwrap_or_else(|refusal| panic!("refused as {}", refusal.reason()));
        assert!(input.benchmark_matches_fixture(&SelectorInput::string("abcde", bound(5))));
        assert_eq!(
            admit(SelectorReport::String("abcdef".to_owned()), &five).err(),
            Some(SelectorRefusal::MalformedNormalized)
        );
        let absent = admit(SelectorReport::Absent, &five)
            .unwrap_or_else(|refusal| panic!("refused as {}", refusal.reason()));
        assert!(absent.benchmark_matches_fixture(&SelectorInput::absent(bound(5))));
        assert!(!absent.benchmark_matches_fixture(&SelectorInput::absent(bound(6))));
    }

    /// Each refusal spells one of the three reasons of 015's boundary row.
    #[test]
    fn each_refusal_has_its_reason() {
        for (refusal, reason) in [
            (
                SelectorRefusal::UnsafeHostInspection,
                "unsafe-host-inspection",
            ),
            (SelectorRefusal::Unrepresentable, "unrepresentable-selector"),
            (
                SelectorRefusal::MalformedNormalized,
                "malformed-normalized-selector",
            ),
        ] {
            assert_eq!(refusal.reason(), reason);
        }
    }
}
