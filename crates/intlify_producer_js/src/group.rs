// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Canonical physical-source group input.
//!
//! This module turns one host assertion of physical identity into one immutable
//! producer participant. It canonicalizes logical paths, chooses the first path
//! as primary, removes repeated discovery of the same logical path, and retains
//! one exact selected byte snapshot. It does not inspect filesystem identity,
//! read files, select editor overrides, or scan source text.

use std::error::Error;
use std::fmt;

use intlify_contract::SourceDocumentIdentity;

/// One immutable selected physical source and all of its logical project paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsPhysicalSourceGroup {
    primary: SourceDocumentIdentity,
    aliases: Box<[SourceDocumentIdentity]>,
    source_bytes: Box<[u8]>,
}

impl JsPhysicalSourceGroup {
    /// Canonicalize one non-empty host-grouped set of logical paths.
    ///
    /// Exact repeated paths from overlapping discovery routes are removed.
    /// Distinct symlink or hard-link paths remain aliases because the host has
    /// already asserted that they select the same physical snapshot.
    pub fn try_new(
        mut logical_sources: Vec<SourceDocumentIdentity>,
        source_bytes: impl Into<Box<[u8]>>,
    ) -> Result<Self, JsPhysicalSourceGroupError> {
        logical_sources.sort();
        logical_sources.dedup();
        let mut logical_sources = logical_sources.into_iter();
        let Some(primary) = logical_sources.next() else {
            return Err(JsPhysicalSourceGroupError::EmptyLogicalSources);
        };
        Ok(Self {
            primary,
            aliases: logical_sources.collect::<Vec<_>>().into_boxed_slice(),
            source_bytes: source_bytes.into(),
        })
    }

    /// Return the canonical first logical path used by artifact identity and origins.
    #[must_use]
    pub const fn primary(&self) -> &SourceDocumentIdentity {
        &self.primary
    }

    /// Return remaining logical paths in canonical exact path order.
    #[must_use]
    pub const fn aliases(&self) -> &[SourceDocumentIdentity] {
        &self.aliases
    }

    /// Return the complete immutable selected source snapshot.
    #[must_use]
    pub const fn source_bytes(&self) -> &[u8] {
        &self.source_bytes
    }

    pub(crate) fn all_sources(&self) -> impl Iterator<Item = &SourceDocumentIdentity> {
        std::iter::once(&self.primary).chain(self.aliases.iter())
    }
}

/// Invalid physical-source group construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsPhysicalSourceGroupError {
    /// A physical group must name at least one logical project source.
    EmptyLogicalSources,
}

impl fmt::Display for JsPhysicalSourceGroupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JavaScript physical source group requires a logical source")
    }
}

impl Error for JsPhysicalSourceGroupError {}

#[cfg(test)]
mod tests {
    use intlify_contract::{
        ArtifactNamespace, PortablePathSegment, PortableRelativePath, SourceDocumentIdentity,
    };

    use super::{JsPhysicalSourceGroup, JsPhysicalSourceGroupError};

    fn source(path: &[&str]) -> SourceDocumentIdentity {
        SourceDocumentIdentity::new(
            ArtifactNamespace::Project,
            PortableRelativePath::try_new(
                path.iter()
                    .map(|segment| PortablePathSegment::try_new(*segment).unwrap())
                    .collect(),
            )
            .unwrap(),
        )
    }

    #[test]
    fn canonicalizes_aliases_and_removes_repeated_logical_discovery() {
        let first = source(&["src", "a.ts"]);
        let second = source(&["src", "linked.ts"]);
        let group = JsPhysicalSourceGroup::try_new(
            vec![second.clone(), first.clone(), second],
            b"t('a')".as_slice(),
        )
        .unwrap();

        assert_eq!(group.primary(), &first);
        assert_eq!(group.aliases(), &[source(&["src", "linked.ts"])]);
        assert_eq!(group.source_bytes(), b"t('a')");
    }

    #[test]
    fn rejects_an_empty_host_group() {
        assert_eq!(
            JsPhysicalSourceGroup::try_new(Vec::new(), Box::<[u8]>::default()),
            Err(JsPhysicalSourceGroupError::EmptyLogicalSources)
        );
    }
}
