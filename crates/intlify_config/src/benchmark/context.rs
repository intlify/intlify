// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Attach the executing library's build observation to profile-bound capture.
//! This is a partial owner context, not a common Build Identity or Run Plan.
//! Environment/Runner acquisition, full common records, and projection/report
//! admission are still required before the adopting harness can claim support.

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

use super::build::{BuildIssue, BuildObservation, ObservedBuild};
use super::cases::registry::{AdmittedFixture, Registry};
use super::clock::{ClockDescription, MonotonicClock};
use super::observation::Digest;
use super::profile::{
    AdmittedProfile, MeasurementProfile, ProfileCollectionFailure, ProfileCollectionIssue,
    ProfiledOperation,
};
use super::quantity::Quantity;
use super::sample::CaptureBinding;

pub(super) struct CaptureContext {
    profile: AdmittedProfile,
    build: ObservedBuild,
}

/// The complete context is retained once, not copied into every operation row.
/// Native acquisition cannot deserialize this payload into trusted context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ContextObservation {
    profile: MeasurementProfile,
    build: BuildObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContextIssue {
    Profile,
    Build(BuildIssue),
    BuildBinding,
    Operation(ProfileCollectionIssue),
}

impl CaptureContext {
    pub(super) fn acquire(registry: &Registry) -> Result<Self, BuildIssue> {
        Ok(Self {
            profile: AdmittedProfile::smoke(registry),
            build: ObservedBuild::acquire()?,
        })
    }

    pub(super) fn profile(&self) -> &MeasurementProfile {
        self.profile.document()
    }

    pub(super) fn validate(&self, submitted: &ContextObservation) -> Vec<ContextIssue> {
        let mut issues = Vec::new();
        if submitted.profile != *self.profile.document() {
            issues.push(ContextIssue::Profile);
        }
        issues.extend(
            self.build
                .validate(&submitted.build)
                .into_iter()
                .map(ContextIssue::Build),
        );
        issues
    }

    pub(super) fn collect(
        &self,
        clock: &MonotonicClock,
        ordinal: Quantity,
        fixture: &AdmittedFixture,
        binding: CaptureBinding,
    ) -> Result<ContextualOperation, ProfileCollectionFailure> {
        let operation = self.profile.collect(clock, ordinal, fixture, binding)?;
        Ok(ContextualOperation {
            build_observation: self.build.checksum(),
            operation,
        })
    }
}

impl Serialize for CaptureContext {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut output = serializer.serialize_struct("ContextObservation", 2)?;
        output.serialize_field("profile", self.profile.document())?;
        output.serialize_field("build", self.build.document())?;
        output.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ContextualOperation {
    build_observation: Digest,
    operation: ProfiledOperation,
}

impl ContextualOperation {
    pub(super) fn validate(
        &self,
        context: &CaptureContext,
        ordinal: Quantity,
        fixture: &AdmittedFixture,
        clock: ClockDescription,
        binding: CaptureBinding,
    ) -> Vec<ContextIssue> {
        let mut issues = Vec::new();
        if self.build_observation != context.build.checksum() {
            issues.push(ContextIssue::BuildBinding);
        }
        issues.extend(
            self.operation
                .validate(&context.profile, ordinal, fixture, clock, binding)
                .into_iter()
                .map(ContextIssue::Operation),
        );
        issues
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use crate::benchmark::observation::Frame;
    use serde_json::json;

    fn binding(ordinal: usize) -> CaptureBinding {
        let mut run = Frame::new("build-context-test-run");
        run.text("one-test-run");
        let mut case = Frame::new("build-context-test-case");
        case.uint(u64::try_from(ordinal).unwrap());
        CaptureBinding {
            run: run.finish(),
            case: case.finish(),
        }
    }

    #[test]
    fn every_native_case_retains_its_separately_acquired_build_and_profile_binding() {
        let registry = Registry::load().unwrap();
        let context = CaptureContext::acquire(&registry).unwrap();
        let clock = MonotonicClock::acquire().unwrap();
        let encoded_context = serde_json::to_vec(&context).unwrap();
        let decoded_context: ContextObservation = serde_json::from_slice(&encoded_context).unwrap();
        assert!(context.validate(&decoded_context).is_empty());
        let mut retained = Vec::new();
        for (index, declaration) in context.profile().cases().iter().enumerate() {
            let fixture = registry.prepare(declaration).unwrap();
            let ordinal = Quantity::new(u64::try_from(index).unwrap());
            let binding = binding(index);
            let record = context.collect(&clock, ordinal, &fixture, binding).unwrap();
            let encoded = serde_json::to_vec(&record).unwrap();
            let decoded: ContextualOperation = serde_json::from_slice(&encoded).unwrap();
            assert!(decoded
                .validate(&context, ordinal, &fixture, clock.description(), binding)
                .is_empty());
            assert_eq!(decoded, record);
            let mut forged = serde_json::to_value(&record).unwrap();
            forged["buildObservation"] = json!("0".repeat(64));
            let forged: ContextualOperation = serde_json::from_value(forged).unwrap();
            assert!(forged
                .validate(&context, ordinal, &fixture, clock.description(), binding)
                .contains(&ContextIssue::BuildBinding));
            retained.push(record);
        }
        drop(context);
        drop(registry);
        assert_eq!(retained.len(), 127);
        assert!(!serde_json::to_vec(&retained).unwrap().is_empty());
        assert_eq!(
            serde_json::to_vec(&decoded_context).unwrap(),
            encoded_context
        );
    }

    #[test]
    fn a_submitted_context_does_not_define_the_current_build_or_profile() {
        let registry = Registry::load().unwrap();
        let context = CaptureContext::acquire(&registry).unwrap();
        let original = serde_json::to_value(&context).unwrap();
        for (pointer, replacement, expected) in [
            ("/profile/revision", json!("other"), ContextIssue::Profile),
            (
                "/build/package/revision",
                json!("other"),
                ContextIssue::Build(BuildIssue::ObservationMismatch),
            ),
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            let decoded: ContextObservation = serde_json::from_value(changed).unwrap();
            assert!(context.validate(&decoded).contains(&expected));
        }
        for key in ["profile", "build"] {
            let mut changed = original.clone();
            changed.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<ContextObservation>(changed).is_err());
        }
        let mut changed = original;
        changed["unknown"] = json!("private input must not be retained");
        assert!(serde_json::from_value::<ContextObservation>(changed).is_err());
    }
}
