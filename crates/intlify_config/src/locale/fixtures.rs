// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite test-owned provider, absent from ordinary library builds. These
//! symbolic pins exercise exact binding equality; they are NOT artifact digests,
//! an admitted 017 data representation, or a production canonicalization corpus.
//! Every answer is declared below. Unlisted input is unsupported, not invalid.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::{ArtifactReference, Provider, ProviderBinding, ProviderFailure, VersionedIdentity};

pub(crate) fn fixture_binding() -> ProviderBinding<&'static str> {
    ProviderBinding {
        specification: ArtifactReference {
            identity: "test-only-locale-specification",
            revision: "0",
            digest: "test-only-specification-content-pin",
        },
        dataset: ArtifactReference {
            identity: "test-only-finite-locale-data",
            revision: "0",
            digest: "test-only-dataset-content-pin",
        },
        provider: VersionedIdentity {
            identity: "test-only-finite-map-provider",
            revision: "0",
        },
        provider_schema: VersionedIdentity {
            identity: "test-only-rust-fixture-table",
            revision: "0",
        },
        transport_digest: "test-only-in-memory-representation-pin",
    }
}

const CANONICAL: &[&str] = &[
    "ar-EG-u-nu-latn",
    "de",
    "de-DE",
    "en",
    "en-US",
    "en-u-ca-gregory-nu-latn",
    "fr",
    "fr-FR",
    "he-IL",
    "ja",
    "und",
    "und-u-ca-islamic-civil",
    "zh-Hant-TW",
];

const ALIASES: &[(&str, &str)] = &[
    ("EN", "en"),
    ("EN-us", "en-US"),
    ("en-u-nu-latn-ca-gregory", "en-u-ca-gregory-nu-latn"),
    ("iw-IL", "he-IL"),
    ("und-u-ca-islamicc", "und-u-ca-islamic-civil"),
];

const INVALID: &[&str] = &[
    "Latn",
    "en-a-foo",
    "en-u-ca-madeup",
    "en-u-zz-abc",
    "en-x-brand",
    "en_US",
    "en_US.UTF-8",
    "en_US@calendar=gregorian",
    "root",
    "zz",
];

#[derive(Clone)]
enum Answer {
    Canonical(Arc<str>),
    Invalid,
}

/// Private fields and a fixed constructor prevent arbitrary caller-supplied
/// rows from posing as the fixture. Canonical strings are interned once and
/// shared by aliases and retained outputs, with no mutable invocation cache.
pub(crate) struct FixtureProvider {
    binding: ProviderBinding<&'static str>,
    rows: BTreeMap<&'static str, Answer>,
}

impl FixtureProvider {
    pub(crate) fn new() -> Self {
        let mut rows = BTreeMap::new();
        for &canonical in CANONICAL {
            assert!(rows
                .insert(canonical, Answer::Canonical(Arc::from(canonical)))
                .is_none());
        }
        for &(input, canonical) in ALIASES {
            let answer = rows
                .get(canonical)
                .expect("fixed fixture canonical target")
                .clone();
            assert!(rows.insert(input, answer).is_none());
        }
        for &invalid in INVALID {
            assert!(rows.insert(invalid, Answer::Invalid).is_none());
        }
        Self {
            binding: fixture_binding(),
            rows,
        }
    }
}

impl Provider for FixtureProvider {
    type Identity = &'static str;

    fn binding(&self) -> &ProviderBinding<Self::Identity> {
        &self.binding
    }

    fn canonicalize(&self, input: &str) -> Result<Arc<str>, ProviderFailure> {
        match self.rows.get(input) {
            Some(Answer::Canonical(value)) => Ok(Arc::clone(value)),
            Some(Answer::Invalid) => Err(ProviderFailure::InvalidIdentifier),
            None => Err(ProviderFailure::UnsupportedInput),
        }
    }
}
