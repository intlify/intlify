// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Owner-local observations for the finite canonicalization slice. Symbolic
//! pins and fixture-content checksums are not production artifact identities.

use serde::{Deserialize, Serialize};

use crate::locale::fixtures::FixtureProvider;
use crate::locale::{
    CanonicalizationFailure, Canonicalized, Canonicalizer, ProviderFailure, Spelling,
};

use super::observation::{Digest, Frame, Observation};
use super::operation::OutputFailure;
use super::quantity::Quantity;

pub(super) type Core = Canonicalizer<FixtureProvider>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VersionedPin {
    identity: String,
    revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContentBinding {
    identity: String,
    revision: String,
    declared_content_pin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InputFacts {
    scope: String,
    specification: ContentBinding,
    dataset: ContentBinding,
    provider: VersionedPin,
    provider_schema: VersionedPin,
    declared_transport_pin: String,
    fixture_contents_observation: Digest,
    identifier_byte_limit: Quantity,
    identifier_byte_unit: String,
    provider_reuse: String,
    canonical_value_storage: String,
}

impl InputFacts {
    pub(super) fn observe(core: &Core) -> Self {
        let binding = core.binding();
        let content = |reference: &crate::locale::ArtifactReference<&str>| ContentBinding {
            identity: reference.identity.into(),
            revision: reference.revision.into(),
            declared_content_pin: reference.digest.into(),
        };
        let versioned = |reference: &crate::locale::VersionedIdentity<&str>| VersionedPin {
            identity: reference.identity.into(),
            revision: reference.revision.into(),
        };
        let rows = core.fixture_rows();
        let mut frame = Frame::new("finite-locale-provider-content");
        frame.uint(u64::try_from(rows.len()).expect("fixed finite provider table length"));
        for (input, canonical) in rows {
            frame.text(input);
            if let Some(canonical) = canonical {
                frame.uint(1);
                frame.text(canonical);
            } else {
                frame.uint(0);
            }
        }
        Self {
            scope: "finite-test-owned-provider-not-production-data".into(),
            specification: content(&binding.specification),
            dataset: content(&binding.dataset),
            provider: versioned(&binding.provider),
            provider_schema: versioned(&binding.provider_schema),
            declared_transport_pin: binding.transport_digest.into(),
            fixture_contents_observation: frame.finish(),
            identifier_byte_limit: Quantity::new(core.max_identifier_bytes().get()),
            identifier_byte_unit: "utf8-octet".into(),
            provider_reuse: "resident-immutable-provider-reused-between-invocations".into(),
            canonical_value_storage: "result-shares-immutable-provider-storage".into(),
        }
    }
}

pub(super) fn observe(
    output: &Result<Canonicalized, CanonicalizationFailure>,
) -> Result<Observation, OutputFailure> {
    let mut shared = Frame::new("single-locale-canonicalization-result");
    let entry = match output {
        Ok(result) => {
            shared.uint(0);
            shared.text(result.locale().as_str());
            let mut correction = Frame::new("single-locale-correction");
            if let Some(replacement) = result.suggested_replacement() {
                correction.uint(1);
                correction.text(replacement);
            } else {
                correction.uint(0);
            }
            Some(correction.finish())
        }
        Err(CanonicalizationFailure::ByteLimit {
            spelling,
            limit,
            actual,
        }) => {
            shared.uint(1);
            shared.uint(match spelling {
                Spelling::Raw => 0,
                Spelling::Canonical => 1,
            });
            shared.uint(limit.get());
            shared.uint(*actual);
            None
        }
        Err(CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier)) => {
            shared.uint(2);
            None
        }
        Err(CanonicalizationFailure::Provider(
            ProviderFailure::UnsupportedInput | ProviderFailure::Unavailable,
        )) => {
            return Err(OutputFailure::LocaleProviderUnavailable);
        }
        Err(_) => return Err(OutputFailure::LocaleProviderInvariant),
    };
    Ok(Observation {
        shared: shared.finish(),
        entry,
    })
}
