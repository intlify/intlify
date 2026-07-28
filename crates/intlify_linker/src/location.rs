// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Linker-owned portable identity for one admitted definition record.
//!
//! This module composes contract-owned source and entry identities without
//! widening either component's construction boundary. The linker uses the
//! composition in findings and resolved-message snapshots.

use std::cmp::Ordering;

use intlify_contract::{EntryReference, SourceDocumentIdentity};

/// Portable identity of one exact definition record.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DefinitionLocation {
    source: SourceDocumentIdentity,
    entry: EntryReference,
}

impl PartialOrd for DefinitionLocation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DefinitionLocation {
    fn cmp(&self, other: &Self) -> Ordering {
        self.source
            .cmp(&other.source)
            .then_with(|| self.entry.cmp(&other.entry))
    }
}

impl DefinitionLocation {
    pub(crate) const fn new(source: SourceDocumentIdentity, entry: EntryReference) -> Self {
        Self { source, entry }
    }

    /// Return the primary definition-source identity.
    #[must_use]
    pub const fn source(&self) -> &SourceDocumentIdentity {
        &self.source
    }

    /// Return the exact entry occurrence within that source.
    #[must_use]
    pub const fn entry(&self) -> &EntryReference {
        &self.entry
    }
}
