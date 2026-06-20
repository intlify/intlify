//! Scanner cursor and lexical helpers.
//!
//! Full scanner implementation lands in Milestone 5.

/// Snapshotable scanner state. Stored inside [`crate::parser::Checkpoint`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScannerState {
    pub offset: u32,
}

impl ScannerState {
    pub const fn new(offset: u32) -> Self {
        Self { offset }
    }
}
