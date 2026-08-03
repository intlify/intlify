// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Object-safe platform representation boundary.
//!
//! Implementations consume only a borrowed validated batch and return the same
//! checked in-memory artifact/error contract. Exporter selection, typed option
//! construction, destination mapping, filesystem registration, and scheduling
//! remain integration responsibilities.

use crate::{ExportArtifactSet, ExportError, ValidatedExportBatch};

/// Platform representation generator for one independent export transaction.
pub trait PlatformExporter: Send {
    /// Convert one fully validated batch into one complete checked artifact set.
    fn export(&self, batch: &ValidatedExportBatch<'_>) -> Result<ExportArtifactSet, ExportError>;
}
