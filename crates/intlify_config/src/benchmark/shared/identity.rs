// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The instance-identity domains this owner registers.
//!
//! The common domains, the Measurement Case presentation, and the governing
//! specification belong to `intlify_measurement`. This module only registers
//! the two domains this owner owns and narrows the shared names to the
//! visibility used inside the measurement implementation.

pub(in crate::benchmark) use intlify_measurement::identity::{
    specification, AnyIdentity, CaseIdentity, CommonDomain as InstanceDomain, IdentityFailure,
    IntegrityDigest, OwnerRecordIdentity, RecordIdentity, Token, VersionedIdentity,
};

/// Native schema v1 explicitly gives one fresh immutable ID to the one-shot
/// owner result. The native content checksum remains independently retained.
pub(in crate::benchmark) const OWNER_RESULT_DOMAIN: &str = "intlify-config-owner-result-v1";

/// Per-run local harness instance, not a machine identity or qualification.
pub(in crate::benchmark) const LOCAL_RUNNER_DOMAIN: &str =
    "intlify-config-local-runner-instance-v0";
