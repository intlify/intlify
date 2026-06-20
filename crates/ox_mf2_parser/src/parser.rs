//! Parser core, checkpoint, and recovery wiring.
//!
//! Full grammar implementation lands in Milestones 6 and 7.

use crate::scanner::ScannerState;

/// Speculative parser state used by recovery points.
///
/// Kept small: it does not own CST nodes, tokens, trivia, diagnostics, or
/// source text. Rollback truncates table lengths to the captured values.
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
