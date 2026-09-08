// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Attach acquired build/environment inputs to profile-bound capture. The
//! context owns the measured clock; submitted rows cannot select another one.
//! This remains a partial owner context, not the complete 026 environment-field
//! inventory, a common Build Identity, Run Plan, or projection/report admission.

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

use super::build::{BuildIssue, BuildObservation, ObservedBuild};
use super::cases::registry::{AdmittedFixture, Registry};
use super::clock::{ClockFailure, MonotonicClock};
use super::environment::{EnvironmentInputs, EnvironmentIssue, ObservedEnvironment};
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
    environment: ObservedEnvironment,
    clock: MonotonicClock,
}

/// The complete context is retained once, not copied into every operation row.
/// Native acquisition cannot deserialize this payload into trusted context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ContextObservation {
    profile: MeasurementProfile,
    build: BuildObservation,
    environment: EnvironmentInputs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContextAcquisitionIssue {
    Build(BuildIssue),
    Clock(ClockFailure),
    Environment(EnvironmentIssue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContextIssue {
    Profile,
    Build(BuildIssue),
    BuildBinding,
    Environment(EnvironmentIssue),
    EnvironmentBinding,
    Operation(ProfileCollectionIssue),
}

impl CaptureContext {
    pub(super) fn acquire(registry: &Registry) -> Result<Self, ContextAcquisitionIssue> {
        let build = ObservedBuild::acquire().map_err(ContextAcquisitionIssue::Build)?;
        let clock = MonotonicClock::acquire().map_err(ContextAcquisitionIssue::Clock)?;
        let environment =
            ObservedEnvironment::acquire(&clock).map_err(ContextAcquisitionIssue::Environment)?;
        Ok(Self {
            profile: AdmittedProfile::smoke(registry),
            build,
            environment,
            clock,
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
        issues.extend(
            self.environment
                .validate(&submitted.environment)
                .into_iter()
                .map(ContextIssue::Environment),
        );
        issues
    }

    pub(super) fn collect(
        &self,
        ordinal: Quantity,
        fixture: &AdmittedFixture,
        binding: CaptureBinding,
    ) -> Result<ContextualOperation, ProfileCollectionFailure> {
        let operation = self
            .profile
            .collect(&self.clock, ordinal, fixture, binding)?;
        Ok(ContextualOperation {
            build_observation: self.build.checksum(),
            environment_observation: self.environment.checksum(),
            operation,
        })
    }
}

impl Serialize for CaptureContext {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut output = serializer.serialize_struct("ContextObservation", 3)?;
        output.serialize_field("profile", self.profile.document())?;
        output.serialize_field("build", self.build.document())?;
        output.serialize_field("environment", self.environment.document())?;
        output.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ContextualOperation {
    build_observation: Digest,
    environment_observation: Digest,
    operation: ProfiledOperation,
}

impl ContextualOperation {
    pub(super) fn validate(
        &self,
        context: &CaptureContext,
        ordinal: Quantity,
        fixture: &AdmittedFixture,
        binding: CaptureBinding,
    ) -> Vec<ContextIssue> {
        let mut issues = Vec::new();
        if self.build_observation != context.build.checksum() {
            issues.push(ContextIssue::BuildBinding);
        }
        if self.environment_observation != context.environment.checksum() {
            issues.push(ContextIssue::EnvironmentBinding);
        }
        issues.extend(
            self.operation
                .validate(
                    &context.profile,
                    ordinal,
                    fixture,
                    context.clock.description(),
                    binding,
                )
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
    fn every_native_case_retains_its_separately_acquired_context_and_clock_binding() {
        let registry = Registry::load().unwrap();
        let context = CaptureContext::acquire(&registry).unwrap();
        let encoded_context = serde_json::to_vec(&context).unwrap();
        let decoded_context: ContextObservation = serde_json::from_slice(&encoded_context).unwrap();
        assert!(context.validate(&decoded_context).is_empty());
        let mut retained = Vec::new();
        for (index, declaration) in context.profile().cases().iter().enumerate() {
            let fixture = registry.prepare(declaration).unwrap();
            let ordinal = Quantity::new(u64::try_from(index).unwrap());
            let binding = binding(index);
            let record = context.collect(ordinal, &fixture, binding).unwrap();
            let encoded = serde_json::to_vec(&record).unwrap();
            let decoded: ContextualOperation = serde_json::from_slice(&encoded).unwrap();
            assert!(decoded
                .validate(&context, ordinal, &fixture, binding)
                .is_empty());
            assert_eq!(decoded, record);
            for (field, issue) in [
                ("buildObservation", ContextIssue::BuildBinding),
                ("environmentObservation", ContextIssue::EnvironmentBinding),
            ] {
                let mut forged = serde_json::to_value(&record).unwrap();
                forged[field] = json!("0".repeat(64));
                let forged: ContextualOperation = serde_json::from_value(forged).unwrap();
                assert!(forged
                    .validate(&context, ordinal, &fixture, binding)
                    .contains(&issue));
            }
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
    fn a_submitted_context_does_not_define_the_current_build_profile_or_environment() {
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
            (
                "/environment/clock/resolutionNanoseconds",
                json!("0"),
                ContextIssue::Environment(EnvironmentIssue::ObservationMismatch),
            ),
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            let decoded: ContextObservation = serde_json::from_value(changed).unwrap();
            assert!(context.validate(&decoded).contains(&expected));
        }
        for key in ["profile", "build", "environment"] {
            let mut changed = original.clone();
            changed.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<ContextObservation>(changed).is_err());
        }
        let mut changed = original;
        changed["unknown"] = json!("private input must not be retained");
        assert!(serde_json::from_value::<ContextObservation>(changed).is_err());
    }

    #[test]
    fn changing_both_environment_and_sample_clock_cannot_create_self_certifying_evidence() {
        let registry = Registry::load().unwrap();
        let context = CaptureContext::acquire(&registry).unwrap();
        let fixture = registry.prepare(&context.profile().cases()[0]).unwrap();
        let ordinal = Quantity::new(0);
        let binding = binding(0);
        let record = context.collect(ordinal, &fixture, binding).unwrap();
        let mut submitted_context = serde_json::to_value(&context).unwrap();
        submitted_context["environment"]["clock"]["resolutionNanoseconds"] = json!("0");
        // Even recomputing a checksum from matching fabricated metadata cannot
        // replace the actual clock/context held by the native collection path.
        let mut environment_checksum = Frame::new("environment-inputs");
        environment_checksum.json(&submitted_context["environment"]);
        let mut submitted_record = serde_json::to_value(&record).unwrap();
        submitted_record["environmentObservation"] =
            serde_json::to_value(environment_checksum.finish()).unwrap();
        *submitted_record
            .pointer_mut("/operation/operation/descriptors/clockObservation/resolutionNanoseconds")
            .unwrap() = json!("0");
        let submitted_record: ContextualOperation =
            serde_json::from_value(submitted_record).unwrap();
        let issues = submitted_record.validate(&context, ordinal, &fixture, binding);
        assert!(issues.contains(&ContextIssue::EnvironmentBinding));
        assert!(issues
            .iter()
            .any(|issue| matches!(issue, ContextIssue::Operation(_))));
        let submitted_context: ContextObservation =
            serde_json::from_value(submitted_context).unwrap();
        assert!(context
            .validate(&submitted_context)
            .contains(&ContextIssue::Environment(
                EnvironmentIssue::ObservationMismatch
            )));
    }
}

#[cfg(all(test, not(any(target_os = "linux", target_os = "macos"))))]
#[test]
fn unsupported_clock_does_not_fallback_to_another_provider() {
    let registry = Registry::load().unwrap();
    assert!(matches!(
        CaptureContext::acquire(&registry),
        Err(ContextAcquisitionIssue::Clock(
            ClockFailure::UnsupportedPlatform
        ))
    ));
}
