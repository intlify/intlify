// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::error::{InternalResourceErrorReason, ResourceError, ResourceErrorSite, ResourcePhase};
use crate::limits::{check_limit, ResourceLimit};

/// A serialized concrete path within one host document.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StructuralPathKey(Arc<str>);

impl StructuralPathKey {
    #[allow(dead_code)]
    pub(crate) fn from_shared(value: Arc<str>) -> Self {
        Self(value)
    }

    /// Return the adapter-defined serialized structural path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[allow(dead_code)]
    pub(crate) fn shared(&self) -> &Arc<str> {
        &self.0
    }
}

/// The comparison domain attached to a logical catalog key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CatalogKeyDomain {
    /// The degenerate standalone `.mf2` shape; never catalog-grouped.
    StandaloneMf2,
    /// RFC 6901 JSON Pointer comparison.
    JsonPointer,
    /// Resource-owned typed YAML path comparison.
    YamlTypedPath,
    /// XLIFF 1.2 logical key comparison.
    Xliff12,
    /// XLIFF 2.x logical key comparison.
    Xliff2,
}

/// A logical message identity within a catalog-key comparison domain.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CatalogKey(Arc<str>);

impl CatalogKey {
    #[allow(dead_code)]
    pub(crate) fn from_shared(value: Arc<str>) -> Self {
        Self(value)
    }

    /// Return the adapter-defined serialized logical key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[allow(dead_code)]
    pub(crate) fn shared(&self) -> &Arc<str> {
        &self.0
    }
}

/// Stable concrete identity for one raw occurrence in an extracted artifact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryKey {
    structural_path: StructuralPathKey,
    occurrence: u32,
}

impl EntryKey {
    #[allow(dead_code)]
    pub(crate) const fn new(structural_path: StructuralPathKey, occurrence: u32) -> Self {
        Self {
            structural_path,
            occurrence,
        }
    }

    /// Return the concrete structural path.
    #[must_use]
    pub const fn structural_path(&self) -> &StructuralPathKey {
        &self.structural_path
    }

    /// Return the zero-based occurrence among equal structural paths.
    #[must_use]
    pub const fn occurrence(&self) -> u32 {
        self.occurrence
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ArtifactIdentity(NonZeroU64);

impl ArtifactIdentity {
    #[allow(dead_code)]
    const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Opaque artifact-local reference to one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntryHandle {
    artifact: ArtifactIdentity,
    entry_index: u32,
}

#[allow(dead_code)]
impl EntryHandle {
    pub(crate) const fn new(artifact: ArtifactIdentity, entry_index: u32) -> Self {
        Self {
            artifact,
            entry_index,
        }
    }

    pub(crate) const fn artifact_identity(self) -> ArtifactIdentity {
        self.artifact
    }

    pub(crate) const fn entry_index(self) -> u32 {
        self.entry_index
    }
}

/// Monotonic allocator used by artifact finalization.
///
/// PR 2 wires this foundation into the concrete artifact builder. Keeping the
/// allocator private prevents process identities from becoming report or cache
/// identities.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct ArtifactIdentityAllocator {
    last_allocated: AtomicU64,
}

#[allow(dead_code)]
impl ArtifactIdentityAllocator {
    pub(crate) const fn new() -> Self {
        Self {
            last_allocated: AtomicU64::new(0),
        }
    }

    #[cfg(test)]
    const fn with_last_allocated(last_allocated: u64) -> Self {
        Self {
            last_allocated: AtomicU64::new(last_allocated),
        }
    }

    pub(crate) fn allocate(&self, phase: ResourcePhase) -> Result<ArtifactIdentity, ResourceError> {
        let previous = self
            .last_allocated
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |last| {
                last.checked_add(1)
            })
            .map_err(|_| {
                ResourceError::internal(
                    InternalResourceErrorReason::ArtifactIdentityExhausted,
                    phase,
                    None,
                )
            })?;

        let allocated = previous + 1;
        let identity = NonZeroU64::new(allocated).ok_or_else(|| {
            ResourceError::internal(
                InternalResourceErrorReason::ArtifactIdentityExhausted,
                phase,
                None,
            )
        })?;
        Ok(ArtifactIdentity(identity))
    }
}

#[allow(dead_code)]
static ARTIFACT_IDENTITIES: ArtifactIdentityAllocator = ArtifactIdentityAllocator::new();

#[allow(dead_code)]
pub(crate) fn allocate_artifact_identity(
    phase: ResourcePhase,
) -> Result<ArtifactIdentity, ResourceError> {
    ARTIFACT_IDENTITIES.allocate(phase)
}

/// Artifact-local exact-byte identity interner.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub(crate) struct IdentityInterner {
    values: HashMap<Arc<str>, Arc<str>>,
    distinct_bytes: u128,
}

#[allow(dead_code)]
impl IdentityInterner {
    pub(crate) fn contains(&self, value: &str) -> bool {
        self.values.contains_key(value)
    }

    pub(crate) fn intern(
        &mut self,
        value: &str,
        phase: ResourcePhase,
        site: Option<ResourceErrorSite>,
    ) -> Result<Arc<str>, ResourceError> {
        if let Some((existing, _)) = self.values.get_key_value(value) {
            return Ok(Arc::clone(existing));
        }

        let actual = self.distinct_bytes + value.len() as u128;
        check_limit(ResourceLimit::IdentityBytes, actual, phase, site)?;

        let value: Arc<str> = Arc::from(value);
        self.values.insert(Arc::clone(&value), Arc::clone(&value));
        self.distinct_bytes = actual;
        Ok(value)
    }

    pub(crate) const fn distinct_bytes(&self) -> u128 {
        self.distinct_bytes
    }

    #[cfg(test)]
    pub(crate) fn set_distinct_bytes_for_test(&mut self, distinct_bytes: u128) {
        self.distinct_bytes = distinct_bytes;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::thread;

    use super::{
        ArtifactIdentityAllocator, CatalogKey, EntryHandle, IdentityInterner, StructuralPathKey,
    };
    use crate::{InternalResourceErrorReason, ResourceErrorDetails, ResourcePhase};

    #[test]
    fn interns_exact_bytes_across_semantic_identity_types() {
        let mut interner = IdentityInterner::default();
        let shared = interner
            .intern("/greeting", ResourcePhase::Extract, None)
            .unwrap();
        let same = interner
            .intern("/greeting", ResourcePhase::Extract, None)
            .unwrap();
        let structural = StructuralPathKey::from_shared(shared);
        let catalog = CatalogKey::from_shared(same);

        assert!(Arc::ptr_eq(structural.shared(), catalog.shared()));
        assert_eq!(interner.distinct_bytes(), 9);
    }

    #[test]
    fn does_not_normalize_unicode() {
        let mut interner = IdentityInterner::default();
        let composed = interner
            .intern("\u{e9}", ResourcePhase::Extract, None)
            .unwrap();
        let decomposed = interner
            .intern("e\u{301}", ResourcePhase::Extract, None)
            .unwrap();

        assert!(!Arc::ptr_eq(&composed, &decomposed));
        assert_ne!(composed, decomposed);
        assert_eq!(interner.distinct_bytes(), 5);
    }

    #[test]
    fn allocator_never_returns_zero_and_does_not_reuse() {
        let allocator = ArtifactIdentityAllocator::new();
        let first = allocator.allocate(ResourcePhase::Extract).unwrap();
        let second = allocator.allocate(ResourcePhase::Extract).unwrap();

        assert_eq!(first.get(), 1);
        assert_eq!(second.get(), 2);
        assert_ne!(first, second);
    }

    #[test]
    fn allocator_is_unique_under_concurrency() {
        let allocator = Arc::new(ArtifactIdentityAllocator::new());
        let threads = (0..8)
            .map(|_| {
                let allocator = Arc::clone(&allocator);
                thread::spawn(move || {
                    (0..1_000)
                        .map(|_| allocator.allocate(ResourcePhase::Extract).unwrap().get())
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();

        let identities = threads
            .into_iter()
            .flat_map(|thread| thread.join().unwrap())
            .collect::<HashSet<_>>();

        assert_eq!(identities.len(), 8_000);
        assert!(!identities.contains(&0));
    }

    #[test]
    fn allocator_reports_injected_exhaustion_without_wrapping() {
        let allocator = ArtifactIdentityAllocator::with_last_allocated(u64::MAX);
        let error = allocator
            .allocate(ResourcePhase::ValidateWriteBack)
            .unwrap_err();

        assert_eq!(error.phase(), ResourcePhase::ValidateWriteBack);
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::Internal {
                reason: InternalResourceErrorReason::ArtifactIdentityExhausted
            }
        ));
    }

    #[test]
    fn same_index_handles_from_different_artifacts_are_not_equal() {
        let allocator = ArtifactIdentityAllocator::new();
        let first = EntryHandle::new(allocator.allocate(ResourcePhase::Extract).unwrap(), 4);
        let second = EntryHandle::new(allocator.allocate(ResourcePhase::Extract).unwrap(), 4);

        assert_ne!(first, second);
        assert_ne!(first.artifact_identity(), second.artifact_identity());
        assert_eq!(first.entry_index(), second.entry_index());
    }
}
