// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Phase 1 [`crate::span::SourceId`] → snapshot-local
//! [`crate::span::SourceId`] remap.
//!
//! [`crate::source::SourceStore`] `SourceId` numbers may be sparse
//! from the snapshot writer's point of view (one snapshot can carry
//! a subset of a larger batch). v0.1 keeps the snapshot compact by
//! assigning dense snapshot-local `SourceId` values in first-seen
//! order.
//!
//! The lookup uses a small linear scan instead of `HashMap`:
//! snapshot source counts are dominated by batch size, which is
//! small in practice (typically <= a few dozen), and a `Vec`-backed
//! linear scan beats `HashMap` for these sizes by sidestepping the
//! hasher cost on every intern.

use crate::snapshot::error::SnapshotWriteError;
use crate::snapshot::format::checked_u32;
use crate::span::SourceId as PhaseOneSourceId;

/// First-seen-order Phase 1 `SourceId` → snapshot-local `SourceId`
/// map.
#[derive(Debug, Default)]
pub(crate) struct SourceMap {
    order: Vec<PhaseOneSourceId>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert `phase_one` and return its assigned snapshot-local
    /// `SourceId`. Returns the same id on subsequent calls for the
    /// same input.
    pub fn intern(&mut self, phase_one: PhaseOneSourceId) -> Result<u32, SnapshotWriteError> {
        if phase_one.is_none() {
            return Err(SnapshotWriteError::InvalidSourceId);
        }
        // Linear scan: snapshot source counts stay small in practice.
        // If a future workload pushes the count high enough to matter,
        // swap this for `FxHashMap` rather than the std `HashMap`
        // hasher cost.
        for (i, existing) in self.order.iter().enumerate() {
            if existing.raw() == phase_one.raw() {
                return Ok(i as u32);
            }
        }
        let local = checked_u32(self.order.len()).ok_or(SnapshotWriteError::TooManySources)?;
        self.order.push(phase_one);
        Ok(local)
    }

    /// Iterator over interned Phase 1 `SourceId` values in
    /// snapshot-local order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, PhaseOneSourceId)> + '_ {
        self.order.iter().enumerate().map(|(i, p1)| (i as u32, *p1))
    }

    /// Number of interned `SourceId` values.
    #[inline]
    pub fn len(&self) -> usize {
        self.order.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_assigns_dense_indices_in_first_seen_order() {
        let mut map = SourceMap::new();
        let a = PhaseOneSourceId::new(5);
        let b = PhaseOneSourceId::new(3);
        let a2 = PhaseOneSourceId::new(5);
        assert_eq!(map.intern(a).unwrap(), 0);
        assert_eq!(map.intern(b).unwrap(), 1);
        assert_eq!(map.intern(a2).unwrap(), 0);
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn rejects_none_sentinel() {
        let mut map = SourceMap::new();
        assert_eq!(
            map.intern(PhaseOneSourceId::NONE).unwrap_err(),
            SnapshotWriteError::InvalidSourceId
        );
    }

    #[test]
    fn iter_returns_pairs_in_snapshot_local_order() {
        let mut map = SourceMap::new();
        map.intern(PhaseOneSourceId::new(9)).unwrap();
        map.intern(PhaseOneSourceId::new(0)).unwrap();
        map.intern(PhaseOneSourceId::new(2)).unwrap();
        let pairs: Vec<_> = map.iter().collect();
        assert_eq!(pairs.len(), 3);
        assert_eq!(pairs[0], (0, PhaseOneSourceId::new(9)));
        assert_eq!(pairs[1], (1, PhaseOneSourceId::new(0)));
        assert_eq!(pairs[2], (2, PhaseOneSourceId::new(2)));
    }
}
