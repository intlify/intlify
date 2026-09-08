// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Data-free, crate-private canonicalization over one explicitly bound provider.
//! The enclosing owner supplies admitted immutable data and identities. Comparing
//! these non-serialized bindings is not artifact integrity or conformance admission.

use std::sync::Arc;

use crate::input_limits::Bound;

/// Opaque identity values deliberately do not reserve the 017 wire encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtifactReference<Id> {
    pub(crate) identity: Id,
    pub(crate) revision: Id,
    pub(crate) digest: Id,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VersionedIdentity<Id> {
    pub(crate) identity: Id,
    pub(crate) revision: Id,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderBinding<Id> {
    pub(crate) specification: ArtifactReference<Id>,
    pub(crate) dataset: ArtifactReference<Id>,
    pub(crate) provider: VersionedIdentity<Id>,
    pub(crate) provider_schema: VersionedIdentity<Id>,
    pub(crate) transport_digest: Id,
}

/// Only internal, read-only providers implement this interface. The provider
/// owns the validity/canonicalization semantics proved by its conformance suite;
/// the boundary does not equate successful syntax parsing with locale validity.
/// Unsupported coverage and invalid authoring are separate outcomes.
pub(crate) trait Provider {
    type Identity: Clone + Eq;

    fn binding(&self) -> &ProviderBinding<Self::Identity>;

    fn canonicalize(&self, input: &str) -> Result<Arc<str>, ProviderFailure>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderFailure {
    InvalidIdentifier,
    UnsupportedInput,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum BindingPart {
    SpecificationIdentity,
    SpecificationRevision,
    SpecificationDigest,
    DatasetIdentity,
    DatasetRevision,
    DatasetDigest,
    ProviderIdentity,
    ProviderRevision,
    ProviderSchemaIdentity,
    ProviderSchemaRevision,
    TransportDigest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BindingFailure {
    MissingProviderData,
    Mismatch(Vec<BindingPart>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Spelling {
    Raw,
    Canonical,
}

/// Private failures, not final profile Findings. No rejected authoring text or
/// mismatching metadata is retained in an error or interpolated into Debug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CanonicalizationFailure {
    ByteLimit {
        spelling: Spelling,
        limit: Bound,
        actual: u64,
    },
    UnrepresentableByteCount,
    Provider(ProviderFailure),
    ProviderBindingChanged,
    ProviderOutputInvariant,
}

/// Values own/share immutable storage; they never borrow the provider or scratch.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CanonicalLocale(Arc<str>);

impl CanonicalLocale {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Canonicalized {
    locale: CanonicalLocale,
    corrected: bool,
}

impl Canonicalized {
    pub(crate) const fn locale(&self) -> &CanonicalLocale {
        &self.locale
    }

    pub(crate) fn suggested_replacement(&self) -> Option<&str> {
        self.corrected.then(|| self.locale.as_str())
    }

    pub(crate) fn into_locale(self) -> CanonicalLocale {
        self.locale
    }
}

pub(crate) struct Canonicalizer<P: Provider> {
    provider: P,
    binding: ProviderBinding<P::Identity>,
    max_identifier_bytes: Bound,
}

impl<P: Provider> Canonicalizer<P> {
    pub(crate) fn bind(
        expected: &ProviderBinding<P::Identity>,
        provider: Option<P>,
        max_identifier_bytes: Bound,
    ) -> Result<Self, BindingFailure> {
        let provider = provider.ok_or(BindingFailure::MissingProviderData)?;
        let differences = binding_differences(expected, provider.binding());
        if !differences.is_empty() {
            return Err(BindingFailure::Mismatch(differences));
        }
        Ok(Self {
            provider,
            binding: expected.clone(),
            max_identifier_bytes,
        })
    }

    pub(crate) const fn binding(&self) -> &ProviderBinding<P::Identity> {
        &self.binding
    }

    pub(crate) const fn max_identifier_bytes(&self) -> Bound {
        self.max_identifier_bytes
    }

    pub(crate) fn canonicalize(
        &self,
        input: &str,
    ) -> Result<Canonicalized, CanonicalizationFailure> {
        // A misbehaving adapter must not silently change the pinned dataset or
        // specification between calls. There is no ambient data reacquisition.
        if self.provider.binding() != &self.binding {
            return Err(CanonicalizationFailure::ProviderBindingChanged);
        }
        self.check_bytes(input, Spelling::Raw)?;
        let canonical = self
            .provider
            .canonicalize(input)
            .map_err(CanonicalizationFailure::Provider)?;
        self.check_bytes(&canonical, Spelling::Canonical)?;
        // Basic adapter-output sanity, not a replacement for pinned validity
        // data or UTS #35 conformance. Do not repair a bad provider result.
        if !canonical
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        {
            return Err(CanonicalizationFailure::ProviderOutputInvariant);
        }
        let corrected = input != canonical.as_ref();
        Ok(Canonicalized {
            locale: CanonicalLocale(canonical),
            corrected,
        })
    }

    fn check_bytes(&self, spelling: &str, kind: Spelling) -> Result<(), CanonicalizationFailure> {
        let actual = u64::try_from(spelling.len())
            .map_err(|_| CanonicalizationFailure::UnrepresentableByteCount)?;
        if actual > self.max_identifier_bytes.get() {
            return Err(CanonicalizationFailure::ByteLimit {
                spelling: kind,
                limit: self.max_identifier_bytes,
                actual,
            });
        }
        Ok(())
    }
}

fn binding_differences<Id: Eq>(
    expected: &ProviderBinding<Id>,
    actual: &ProviderBinding<Id>,
) -> Vec<BindingPart> {
    use BindingPart as Part;
    [
        (
            Part::SpecificationIdentity,
            &expected.specification.identity,
            &actual.specification.identity,
        ),
        (
            Part::SpecificationRevision,
            &expected.specification.revision,
            &actual.specification.revision,
        ),
        (
            Part::SpecificationDigest,
            &expected.specification.digest,
            &actual.specification.digest,
        ),
        (
            Part::DatasetIdentity,
            &expected.dataset.identity,
            &actual.dataset.identity,
        ),
        (
            Part::DatasetRevision,
            &expected.dataset.revision,
            &actual.dataset.revision,
        ),
        (
            Part::DatasetDigest,
            &expected.dataset.digest,
            &actual.dataset.digest,
        ),
        (
            Part::ProviderIdentity,
            &expected.provider.identity,
            &actual.provider.identity,
        ),
        (
            Part::ProviderRevision,
            &expected.provider.revision,
            &actual.provider.revision,
        ),
        (
            Part::ProviderSchemaIdentity,
            &expected.provider_schema.identity,
            &actual.provider_schema.identity,
        ),
        (
            Part::ProviderSchemaRevision,
            &expected.provider_schema.revision,
            &actual.provider_schema.revision,
        ),
        (
            Part::TransportDigest,
            &expected.transport_digest,
            &actual.transport_digest,
        ),
    ]
    .into_iter()
    .filter_map(|(part, left, right)| (left != right).then_some(part))
    .collect()
}

#[cfg(any(test, feature = "benchmark"))]
pub(crate) mod fixtures;

#[cfg(test)]
mod tests;
