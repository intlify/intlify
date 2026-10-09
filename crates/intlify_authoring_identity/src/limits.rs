// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Explicit finite limits for reading registry history and planning updates.
//!
//! Design 017 puts every size and collection count under a caller-supplied
//! finite limit. The shared decoder already bounds the bytes and the nesting;
//! these bound what the decoded value asks the reader to look at. Like
//! `AuthoringLimits`, the type has no `Default`, so a caller that does not know
//! a bound decides one rather than inheriting a value from this crate.

/// Which named bound a registry artifact exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IdentityLimitKind {
    /// Entries in one registry snapshot.
    Entries,
    /// Decisions in one update.
    Decisions,
    /// Lineage links in one update.
    LineageLinks,
    /// Intent IDs named across one update's lineage links.
    LineageMembers,
    /// Source edits across one update's decisions.
    SourceEdits,
    /// Replacements across one update's source edits.
    Replacements,
    /// Bytes of replacement text across one update.
    ReplacementBytes,
    /// Updates replayed to verify one chain from its anchor.
    HistorySteps,
    /// Allocation candidates one reconciliation is offered.
    Candidates,
    /// Diagnostics one reconciliation reports.
    Diagnostics,
}

/// Inclusive upper bounds applied to reading one registry artifact, and to
/// planning one update.
///
/// Every bound admits zero: a registry with no entries, an update with no
/// decisions and a chain verified at its anchor are all meaningful, so no
/// bound is unsatisfiable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityLimits {
    /// Entries in one registry snapshot.
    pub entries: u64,
    /// Decisions in one update.
    pub decisions: u64,
    /// Lineage links in one update.
    pub lineage_links: u64,
    /// Intent IDs named across one update's lineage links.
    pub lineage_members: u64,
    /// Source edits across one update's decisions.
    pub source_edits: u64,
    /// Replacements across one update's source edits.
    pub replacements: u64,
    /// Bytes of replacement text across one update.
    pub replacement_bytes: u64,
    /// Updates replayed to verify one chain from its anchor.
    pub history_steps: u64,
    /// Allocation candidates one reconciliation is offered.
    pub candidates: u64,
    /// Diagnostics one reconciliation reports.
    pub diagnostics: u64,
}
