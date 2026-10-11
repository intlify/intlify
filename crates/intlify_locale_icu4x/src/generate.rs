// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Writes the four marker families from ICU4X's compiled data,
//! `icu_locale_data` 2.2.0 generated from CLDR 48.2.0, into one blob. Only
//! this module links compiled data, and only under the `generate` feature.

use icu_locale::provider::{
    Baked, LocaleAliasesV1, LocaleLikelySubtagsExtendedV1, LocaleLikelySubtagsLanguageV1,
    LocaleLikelySubtagsScriptRegionV1,
};
use icu_provider::dynutil::UpcastDataPayload;
use icu_provider::export::{DataExporter, ExportMarker, FlushMetadata};
use icu_provider::prelude::*;
use icu_provider_blob::export::BlobExporter;

use crate::markers::Marker;

/// Why the blob could not be written. It keeps no data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerateFailure {
    /// The compiled data has no payload for a marker family.
    Load(Marker),
    /// ICU4X's exporter refused a payload or could not finish the blob.
    Export,
}

/// The committed blob's exact bytes: the four marker families, each written
/// once. ICU4X's exporter orders everything it writes, so the same data
/// always gives the same bytes.
pub fn markers_blob() -> Result<Vec<u8>, GenerateFailure> {
    blob(&Marker::ALL.map(Family::Admitted))
}

/// A marker family the generator can write. Tests also write one the
/// adapter does not admit, to build a blob with an additional family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    Admitted(Marker),
    #[cfg(test)]
    Parents,
}

macro_rules! write_singleton {
    ($exporter:expr, $marker:ty, $failure:expr) => {{
        let payload: DataPayload<$marker> = Baked
            .load(DataRequest::default())
            .map_err(|_| $failure)?
            .payload;
        $exporter
            .flush_singleton(
                <$marker>::INFO,
                &ExportMarker::upcast(payload),
                FlushMetadata::default(),
            )
            .map_err(|_| GenerateFailure::Export)
    }};
}

pub(crate) fn blob(families: &[Family]) -> Result<Vec<u8>, GenerateFailure> {
    let mut sink = Vec::new();
    {
        let mut exporter = BlobExporter::new_with_sink(Box::new(&mut sink));
        for &family in families {
            write(&exporter, family)?;
        }
        exporter.close().map_err(|_| GenerateFailure::Export)?;
    }
    Ok(sink)
}

fn write(exporter: &BlobExporter<'_>, family: Family) -> Result<(), GenerateFailure> {
    use GenerateFailure::Load;
    match family {
        Family::Admitted(Marker::LocaleAliasesV1) => {
            write_singleton!(exporter, LocaleAliasesV1, Load(Marker::LocaleAliasesV1))
        }
        Family::Admitted(Marker::LocaleLikelySubtagsExtendedV1) => write_singleton!(
            exporter,
            LocaleLikelySubtagsExtendedV1,
            Load(Marker::LocaleLikelySubtagsExtendedV1)
        ),
        Family::Admitted(Marker::LocaleLikelySubtagsLanguageV1) => write_singleton!(
            exporter,
            LocaleLikelySubtagsLanguageV1,
            Load(Marker::LocaleLikelySubtagsLanguageV1)
        ),
        Family::Admitted(Marker::LocaleLikelySubtagsScriptRegionV1) => write_singleton!(
            exporter,
            LocaleLikelySubtagsScriptRegionV1,
            Load(Marker::LocaleLikelySubtagsScriptRegionV1)
        ),
        #[cfg(test)]
        Family::Parents => write_singleton!(
            exporter,
            icu_locale::provider::LocaleParentsV1,
            GenerateFailure::Export
        ),
    }
}

#[cfg(test)]
mod tests;
