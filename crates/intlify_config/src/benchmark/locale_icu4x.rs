// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Canonicalization through the ICU4X provider, over the reference adapter's
//! committed revision "0" artifact. The benchmark build compiles those bytes in
//! as its fixture and admits them in 017's six steps, as a host admits bytes it
//! acquired; the measured call is the same boundary the finite cases measure.
//! A case reuses one admitted engine for every invocation, or admits a fresh
//! one before each, outside the interval.

use serde::{Deserialize, Serialize};

use intlify_locale_icu4x::{ConformanceLimits, ReadLimits};

use crate::input_limits::Bound;
use crate::locale::icu4x::Icu4xProvider;
use crate::locale::{Canonicalizer, ProviderBinding, Supported, VersionedIdentity};

use super::cases::Engine;
use super::locale::VersionedPin;
use super::quantity::Quantity;

pub(super) type Core = Canonicalizer<Icu4xProvider>;

// The adapter's artifact for revision "0", generated from CLDR 48.2.0.
const MANIFEST: &[u8] = include_bytes!("../../../intlify_locale_icu4x/data/manifest.json");
const CONFORMANCE: &[u8] = include_bytes!("../../../intlify_locale_icu4x/data/conformance.json");
const MARKERS: &[u8] = include_bytes!("../../../intlify_locale_icu4x/data/markers.postcard");

/// Explicit owner fixture capacity that the committed artifact is within, as a
/// host would give it. Not a resolver default or a Resource Limit Policy.
const LIMITS: ReadLimits = ReadLimits {
    manifest_bytes: 16_384,
    manifest_depth: 8,
    payloads: 2,
    payload_bytes: 1 << 20,
    marker_bytes: 1 << 20,
    conformance: ConformanceLimits {
        validity_groups: 64,
        codes: 1 << 15,
        extension_keys: 64,
        types: 2_048,
        corrections: 128,
        work: 1 << 16,
    },
};

/// The committed payloads, at the addresses the committed manifest names.
fn acquire(address: &str) -> Option<Vec<u8>> {
    match address {
        "conformance.json" => Some(CONFORMANCE.to_vec()),
        "markers.postcard" => Some(MARKERS.to_vec()),
        _ => None,
    }
}

/// The committed artifact was not admitted, or did not bind what revision "0"
/// expects. The fixture is broken; no case measures this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Unavailable;

/// Admits the committed artifact and binds it where revision "0" is asserted.
fn admit(limit: Bound) -> Result<Core, Unavailable> {
    let provider = Icu4xProvider::admit(MANIFEST, acquire, &LIMITS).map_err(|_| Unavailable)?;
    let supported = Supported::revision_zero();
    let expected = supported
        .expected(&VersionedIdentity {
            identity: "intlify-locale-canonicalization".into(),
            revision: "0".into(),
        })
        .map_err(|_| Unavailable)?;
    Canonicalizer::bind(expected, Some(provider), limit).map_err(|_| Unavailable)
}

/// One case's engine: either the one every invocation reuses, or what each
/// invocation's fresh engine must bind.
pub(super) enum PreparedEngine {
    Resident(Box<Core>),
    Fresh {
        binding: Box<ProviderBinding<String>>,
        limit: Bound,
    },
}

impl PreparedEngine {
    /// Admits the artifact once. A reused engine stays resident; for a fresh
    /// one only what it bound is kept.
    pub(super) fn prepare(engine: Engine, limit: Bound) -> Result<Self, Unavailable> {
        let core = admit(limit)?;
        Ok(match engine {
            Engine::Reused => Self::Resident(Box::new(core)),
            Engine::Fresh => Self::Fresh {
                binding: Box::new(core.binding().clone()),
                limit,
            },
        })
    }

    pub(super) const fn engine(&self) -> Engine {
        match self {
            Self::Resident(_) => Engine::Reused,
            Self::Fresh { .. } => Engine::Fresh,
        }
    }

    pub(super) fn binding(&self) -> &ProviderBinding<String> {
        match self {
            Self::Resident(core) => core.binding(),
            Self::Fresh { binding, .. } => binding.as_ref(),
        }
    }

    pub(super) fn limit(&self) -> Bound {
        match self {
            Self::Resident(core) => core.max_identifier_bytes(),
            Self::Fresh { limit, .. } => *limit,
        }
    }

    /// Runs one invocation's `invoke` with the resident engine, or with a
    /// fresh one admitted now, which must bind what preparation bound and is
    /// released once `invoke` returns.
    pub(super) fn with_engine<T>(&self, invoke: impl FnOnce(&Core) -> T) -> Result<T, Unavailable> {
        match self {
            Self::Resident(core) => Ok(invoke(core)),
            Self::Fresh { binding, limit } => {
                let core = admit(*limit)?;
                if core.binding() != binding.as_ref() {
                    return Err(Unavailable);
                }
                Ok(invoke(&core))
            }
        }
    }
}

/// The dataset as the reader admitted it: the digest is the one it recomputed,
/// not a declared pin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdmittedDataset {
    identity: String,
    semantic_digest: String,
}

/// The read limits the artifact was admitted within.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdmissionLimits {
    manifest_bytes: Quantity,
    manifest_depth: Quantity,
    payloads: Quantity,
    payload_bytes: Quantity,
    marker_bytes: Quantity,
    validity_groups: Quantity,
    codes: Quantity,
    extension_keys: Quantity,
    types: Quantity,
    corrections: Quantity,
    conformance_work: Quantity,
}

impl AdmissionLimits {
    fn of(limits: &ReadLimits) -> Self {
        let conformance = &limits.conformance;
        Self {
            manifest_bytes: Quantity::new(limits.manifest_bytes),
            manifest_depth: Quantity::new(limits.manifest_depth),
            payloads: Quantity::new(limits.payloads),
            payload_bytes: Quantity::new(limits.payload_bytes),
            marker_bytes: Quantity::new(limits.marker_bytes),
            validity_groups: Quantity::new(conformance.validity_groups),
            codes: Quantity::new(conformance.codes),
            extension_keys: Quantity::new(conformance.extension_keys),
            types: Quantity::new(conformance.types),
            corrections: Quantity::new(conformance.corrections),
            conformance_work: Quantity::new(conformance.work),
        }
    }
}

/// What one case canonicalizes against: the admitted specification, dataset,
/// provider, and artifact, the limits of their admission and of the boundary,
/// and the engine state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AdmittedInputFacts {
    scope: String,
    specification: VersionedPin,
    dataset: AdmittedDataset,
    provider: VersionedPin,
    provider_schema: VersionedPin,
    artifact_integrity: String,
    admission_limits: AdmissionLimits,
    identifier_byte_limit: Quantity,
    identifier_byte_unit: String,
    provider_reuse: String,
    canonical_value_storage: String,
}

impl AdmittedInputFacts {
    pub(super) fn observe(engine: &PreparedEngine) -> Self {
        let binding = engine.binding();
        Self {
            scope: "revision-0-artifact-compiled-into-the-benchmark-build".into(),
            specification: VersionedPin::of(&binding.pins.specification),
            dataset: AdmittedDataset {
                identity: binding.pins.dataset.identity.clone(),
                semantic_digest: binding.pins.dataset.semantic_digest.clone(),
            },
            provider: VersionedPin::of(&binding.admission.provider),
            provider_schema: VersionedPin::of(&binding.admission.provider_schema),
            artifact_integrity: binding.admission.artifact.clone(),
            admission_limits: AdmissionLimits::of(&LIMITS),
            identifier_byte_limit: Quantity::new(engine.limit().get()),
            identifier_byte_unit: "utf8-octet".into(),
            provider_reuse: match engine.engine() {
                Engine::Reused => "resident-admitted-provider-reused-between-invocations",
                Engine::Fresh => "artifact-admitted-and-bound-before-each-invocation",
            }
            .into(),
            // ICU4X writes each canonical form anew; no result shares the
            // provider's storage.
            canonical_value_storage: "result-owns-value-written-by-its-invocation".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use intlify_locale_icu4x::{ENGINE, MARKERS_FORMAT};
    use serde_json::{json, Value};

    use super::*;

    fn bound(bytes: u64) -> Bound {
        Bound::new(bytes).unwrap()
    }

    fn manifest() -> Value {
        serde_json::from_slice(MANIFEST).unwrap()
    }

    #[test]
    fn the_committed_artifact_binds_the_dataset_revision_0_admits() {
        let manifest = manifest();
        let supported = Supported::revision_zero();
        let asserted = VersionedIdentity {
            identity: "intlify-locale-canonicalization".into(),
            revision: "0".into(),
        };
        for engine in [Engine::Reused, Engine::Fresh] {
            let prepared = PreparedEngine::prepare(engine, bound(128)).unwrap();
            assert_eq!(prepared.engine(), engine);
            assert_eq!(prepared.limit(), bound(128));
            let binding = prepared.binding();
            assert_eq!(supported.expected(&asserted), Ok(&binding.pins));
            assert_eq!(
                binding.admission.provider,
                VersionedIdentity {
                    identity: ENGINE.0.into(),
                    revision: ENGINE.1.into(),
                }
            );
            assert_eq!(
                binding.admission.provider_schema,
                VersionedIdentity {
                    identity: MARKERS_FORMAT.0.into(),
                    revision: MARKERS_FORMAT.1.into(),
                }
            );
            assert_eq!(
                binding.admission.artifact,
                manifest["integrityDigest"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn a_reused_engine_stays_resident_and_a_fresh_one_keeps_only_its_binding() {
        let reused = PreparedEngine::prepare(Engine::Reused, bound(128)).unwrap();
        let PreparedEngine::Resident(resident) = &reused else {
            panic!("a reused engine is resident")
        };
        for _ in 0..2 {
            assert!(reused
                .with_engine(|core| std::ptr::eq(core, resident.as_ref()))
                .unwrap());
        }
        let fresh = PreparedEngine::prepare(Engine::Fresh, bound(64)).unwrap();
        let PreparedEngine::Fresh { binding, limit } = &fresh else {
            panic!("a fresh engine keeps no engine")
        };
        assert_eq!(binding.as_ref(), reused.binding());
        assert_eq!(*limit, bound(64));
        // Each invocation's engine is admitted as preparation's was.
        assert_eq!(
            fresh
                .with_engine(|core| (core.binding().clone(), core.max_identifier_bytes()))
                .unwrap(),
            (reused.binding().clone(), bound(64))
        );
        let answer = fresh
            .with_engine(|core| core.canonicalize("iw-IL"))
            .unwrap()
            .unwrap();
        assert_eq!(answer.locale().as_str(), "he-IL");
    }

    #[test]
    fn a_fresh_engine_that_binds_otherwise_is_never_invoked() {
        let PreparedEngine::Fresh { mut binding, limit } =
            PreparedEngine::prepare(Engine::Fresh, bound(128)).unwrap()
        else {
            unreachable!()
        };
        binding.admission.artifact = format!("sha256:{}", "0".repeat(64));
        let changed = PreparedEngine::Fresh { binding, limit };
        let mut invoked = false;
        assert_eq!(changed.with_engine(|_| invoked = true), Err(Unavailable));
        assert!(!invoked);
    }

    #[test]
    fn the_admitted_facts_pin_the_binding_both_limits_and_the_engine_state() {
        let manifest = manifest();
        let facts = |engine, limit| {
            let prepared = PreparedEngine::prepare(engine, bound(limit)).unwrap();
            serde_json::to_value(AdmittedInputFacts::observe(&prepared)).unwrap()
        };
        let reused = facts(Engine::Reused, 128);
        assert_eq!(
            reused,
            json!({
                "scope": "revision-0-artifact-compiled-into-the-benchmark-build",
                "specification": {
                    "identity": "intlify-locale-canonicalization",
                    "revision": "0"
                },
                "dataset": {
                    "identity": "intlify-locale-canonicalization-cldr-48.2.0",
                    "semanticDigest": manifest["dataset"]["semanticDigest"]
                },
                "provider": {"identity": "icu4x", "revision": "2.2.0"},
                "providerSchema": {"identity": "icu4x-blob", "revision": "2.2"},
                "artifactIntegrity": manifest["integrityDigest"],
                "admissionLimits": {
                    "manifestBytes": "16384",
                    "manifestDepth": "8",
                    "payloads": "2",
                    "payloadBytes": "1048576",
                    "markerBytes": "1048576",
                    "validityGroups": "64",
                    "codes": "32768",
                    "extensionKeys": "64",
                    "types": "2048",
                    "corrections": "128",
                    "conformanceWork": "65536"
                },
                "identifierByteLimit": "128",
                "identifierByteUnit": "utf8-octet",
                "providerReuse": "resident-admitted-provider-reused-between-invocations",
                "canonicalValueStorage": "result-owns-value-written-by-its-invocation"
            })
        );
        // A fresh engine differs only in its reuse, and a bound only in itself.
        let mut expected = reused;
        expected["identifierByteLimit"] = json!("64");
        assert_eq!(facts(Engine::Reused, 64), expected);
        expected["providerReuse"] = json!("artifact-admitted-and-bound-before-each-invocation");
        assert_eq!(facts(Engine::Fresh, 64), expected);
        expected["identifierByteLimit"] = json!("128");
        assert_eq!(facts(Engine::Fresh, 128), expected);
    }
}
