// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Private no-op observation seam around ordinary export operations.
//!
//! Default product execution uses the zero-state implementation. The dedicated
//! non-default benchmark feature supplies a recorder without changing the
//! checked preparation, renderer, or artifact-set constructors.

/// One exact export-preparation or ESM-generation interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ExportBenchmarkStage {
    SelectedMessageParse,
    MessageSemanticValidation,
    PortableDiagnosticMapping,
    ArgumentSignatureDerivation,
    ValidatedExportBatchConstruction,
    LocaleAssetRendering,
    LoaderMapRendering,
    TypedKeyAccessorRendering,
    ExportArtifactSetConstruction,
}

pub(crate) trait ExportBenchmarkObserver {
    fn enabled(&self) -> bool {
        false
    }

    fn begin(&mut self, _stage: ExportBenchmarkStage) {}

    fn finish(&mut self, _stage: ExportBenchmarkStage, _checksum: u32) {}

    #[cfg(feature = "benchmark")]
    fn associate(
        &mut self,
        _association: crate::benchmark::BenchmarkArtifactAssociation,
        _entry_root: bool,
    ) {
    }
}

pub(crate) struct NoopExportBenchmarkObserver;

impl ExportBenchmarkObserver for NoopExportBenchmarkObserver {}

pub(crate) fn finish_observation(
    observer: &mut impl ExportBenchmarkObserver,
    stage: ExportBenchmarkStage,
    checksum: impl FnOnce() -> u32,
) {
    let checksum = if observer.enabled() { checksum() } else { 0 };
    observer.finish(stage, checksum);
}

pub(crate) fn observation_checksum<'a>(
    domain: &[u8],
    fields: impl IntoIterator<Item = &'a [u8]>,
) -> u32 {
    let fields = fields.into_iter().collect::<Vec<_>>();
    let mut checksum = 2_166_136_261_u32;
    update(&mut checksum, domain);
    update(&mut checksum, &[0]);
    write_field(
        &mut checksum,
        0x01,
        &u64::try_from(fields.len())
            .expect("supported observation field counts fit u64")
            .to_be_bytes(),
    );
    for field in fields {
        write_field(&mut checksum, 0x02, field);
    }
    checksum
}

fn write_field(checksum: &mut u32, tag: u8, payload: &[u8]) {
    update(checksum, &[tag]);
    update(
        checksum,
        &u64::try_from(payload.len())
            .expect("supported observation fields fit u64")
            .to_be_bytes(),
    );
    update(checksum, payload);
}

fn update(checksum: &mut u32, bytes: &[u8]) {
    for byte in bytes {
        *checksum ^= u32::from(*byte);
        *checksum = checksum.wrapping_mul(16_777_619);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_observer_does_not_evaluate_the_checksum() {
        finish_observation(
            &mut NoopExportBenchmarkObserver,
            ExportBenchmarkStage::LocaleAssetRendering,
            || panic!("disabled observation must remain lazy"),
        );
    }
}
