//! Parser entry point, checkpoint, and recovery wiring.
//!
//! The real recursive descent grammar lands in Milestone 6 / 7. For now,
//! `run_parse` is a no-op that the API layer can call so that the end-to-end
//! lifecycle (workspace reuse, owned vs borrowed results, batch parse) is
//! exercised before the grammar exists.

use crate::api::ParseOptions;
use crate::scanner::ScannerState;
use crate::source::SourceStore;
use crate::span::SourceId;
use crate::workspace::ParseWorkspace;

/// Speculative parser state used by recovery points. Captured lazily on
/// entry to ambiguous regions and applied to truncate table lengths on
/// rollback. Defined now so the table builder can already accept it.
#[allow(dead_code)] // populated in Milestone 7.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Checkpoint {
    pub offset: u32,
    pub node_len: u32,
    pub edge_len: u32,
    pub token_len: u32,
    pub trivia_len: u32,
    pub diagnostic_len: u32,
    pub scanner_state: ScannerState,
}

/// Parser entry point invoked by every owned and borrowed-session API.
///
/// Milestone 3 ships a no-op stub so that workspace lifecycle, batch parse,
/// and API plumbing can be exercised before the grammar exists. The
/// recursive descent grammar is filled in by Milestone 6.
pub(crate) fn run_parse(
    _sources: &SourceStore,
    _source_id: SourceId,
    _workspace: &mut ParseWorkspace,
    _options: &ParseOptions,
) {
    // intentionally empty — see Milestone 6.
}
