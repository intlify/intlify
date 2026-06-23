// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Cross-domain API error code guards, TypeScript mirror sync, and evidence
//! output for verification logs.

use std::collections::BTreeMap;

use ox_mf2_parser::{
    ox_mf2_error_code_name, ox_mf2_error_domain, BindingValidationErrorCode, DecodeError,
    DecodeErrorCode, DiagnosticCode, InitializationErrorCode, OxMf2ErrorDomain, SnapshotWriteError,
    SnapshotWriteErrorCode, SourceStoreError, SourceTextErrorCode, SourceTextUnavailable,
    OX_MF2_API_ERROR_MIN,
};

const TS_MIRROR: &str = include_str!("../../../packages/ox-mf2-shared/src/error-codes.ts");

fn rust_api_error_code_table() -> BTreeMap<String, u32> {
    let mut table = BTreeMap::new();
    let decode = [
        DecodeErrorCode::BufferTooShort,
        DecodeErrorCode::InvalidMagic,
        DecodeErrorCode::UnsupportedMajorVersion,
        DecodeErrorCode::UnsupportedMinorVersion,
        DecodeErrorCode::InvalidHeaderLength,
        DecodeErrorCode::InvalidFeatureFlags,
        DecodeErrorCode::InvalidReservedField,
        DecodeErrorCode::SectionTableOutOfBounds,
        DecodeErrorCode::DuplicateSection,
        DecodeErrorCode::MissingRequiredSection,
        DecodeErrorCode::UnknownSection,
        DecodeErrorCode::UnknownRequiredSection,
        DecodeErrorCode::InvalidSectionFlags,
        DecodeErrorCode::InvalidSectionAlignment,
        DecodeErrorCode::InvalidSectionBounds,
        DecodeErrorCode::InvalidRecordSize,
        DecodeErrorCode::InvalidSectionCount,
        DecodeErrorCode::OverlappingSection,
        DecodeErrorCode::InvalidPadding,
        DecodeErrorCode::TrailingPadding,
        DecodeErrorCode::InvalidStringOffset,
        DecodeErrorCode::InvalidUtf8,
        DecodeErrorCode::InvalidStringRef,
        DecodeErrorCode::InvalidSourceRef,
        DecodeErrorCode::InvalidRootRef,
        DecodeErrorCode::InvalidNodeRef,
        DecodeErrorCode::InvalidTokenRef,
        DecodeErrorCode::InvalidTriviaRef,
        DecodeErrorCode::UnknownSyntaxKind,
        DecodeErrorCode::InvalidDiagnosticSeverity,
        DecodeErrorCode::UnknownDiagnosticCode,
        DecodeErrorCode::InvalidDiagnosticRange,
        DecodeErrorCode::InvalidSourceTextRange,
        DecodeErrorCode::InvalidExtendedData,
        DecodeErrorCode::InvalidEdgeKind,
        DecodeErrorCode::InvalidSpan,
    ];
    for code in decode {
        table.insert(code.name().to_string(), code.as_u32());
    }

    let write = [
        SnapshotWriteErrorCode::SourceTooLarge,
        SnapshotWriteErrorCode::TooManyRoots,
        SnapshotWriteErrorCode::TooManySources,
        SnapshotWriteErrorCode::TooManyStrings,
        SnapshotWriteErrorCode::TooManyNodes,
        SnapshotWriteErrorCode::TooManyEdges,
        SnapshotWriteErrorCode::TooManyTokens,
        SnapshotWriteErrorCode::TooManyTrivia,
        SnapshotWriteErrorCode::TooManyDiagnostics,
        SnapshotWriteErrorCode::TooManyDiagnosticLabels,
        SnapshotWriteErrorCode::SectionTooLarge,
        SnapshotWriteErrorCode::MissingRoot,
        SnapshotWriteErrorCode::InvalidSourceId,
        SnapshotWriteErrorCode::InconsistentSourceId,
    ];
    for code in write {
        table.insert(code.name().to_string(), code.as_u32());
    }

    let source_text = [
        SourceTextErrorCode::SourceTextNotIncluded,
        SourceTextErrorCode::SourceTextSpanOutOfBounds,
        SourceTextErrorCode::SourceTextTooLarge,
        SourceTextErrorCode::SourceTextCountMismatch,
        SourceTextErrorCode::SourceTextUnpairedSurrogate,
    ];
    for code in source_text {
        table.insert(code.name().to_string(), code.as_u32());
    }

    let init = [
        InitializationErrorCode::WasmNotInitialized,
        InitializationErrorCode::NativeBindingUnavailable,
    ];
    for code in init {
        table.insert(code.name().to_string(), code.as_u32());
    }

    table.insert(
        BindingValidationErrorCode::InvalidOptions
            .name()
            .to_string(),
        BindingValidationErrorCode::InvalidOptions.as_u32(),
    );

    table
}

fn parse_ts_ox_mf2_error_code_object(ts: &str) -> BTreeMap<String, u32> {
    let mut table = BTreeMap::new();
    let mut in_object = false;
    for line in ts.lines() {
        let line = line.trim();
        if line.starts_with("export const OxMf2ErrorCode") {
            in_object = true;
            continue;
        }
        if !in_object {
            continue;
        }
        if line.starts_with("} as const") {
            break;
        }
        if line.starts_with("//") || line.is_empty() {
            continue;
        }
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let digits: String = rest
            .trim()
            .trim_end_matches(',')
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        if let Ok(value) = digits.parse::<u32>() {
            table.insert(key.trim().to_string(), value);
        }
    }
    table
}

#[test]
fn numeric_table_evidence_for_compat_logs() {
    let rust = rust_api_error_code_table();
    eprintln!("EVIDENCE numeric_table begin");
    for (name, value) in &rust {
        eprintln!("EVIDENCE locked {name}={value}");
        assert!(*value >= OX_MF2_API_ERROR_MIN, "{name} uses reserved range");
        assert_eq!(ox_mf2_error_code_name(*value), name.as_str());
    }
    eprintln!(
        "EVIDENCE samples InvalidMagic=1001 InvalidSpan=1035 SnapshotWriteMissingRoot=2011 SourceTextNotIncluded=3000"
    );
    assert_eq!(rust.get("DecodeInvalidMagic"), Some(&1001));
    assert_eq!(rust.get("DecodeInvalidSpan"), Some(&1035));
    assert_eq!(rust.get("SnapshotWriteMissingRoot"), Some(&2011));
    assert_eq!(rust.get("SourceTextNotIncluded"), Some(&3000));
    eprintln!("EVIDENCE numeric_table end count={}", rust.len());
}

#[test]
fn surface_check_evidence_for_bindings() {
    let decode_err = DecodeError::new(DecodeErrorCode::InvalidMagic);
    let decode_code = decode_err.as_ox_mf2_error_code();
    let decode_name = ox_mf2_error_code_name(decode_code);
    eprintln!("EVIDENCE DecodeError::new(InvalidMagic) code={decode_code} name={decode_name}");
    assert_eq!(decode_code, 1001);
    assert_eq!(decode_name, "DecodeInvalidMagic");

    let write_err = SnapshotWriteError::MissingRoot;
    let write_code = write_err.as_ox_mf2_error_code();
    let write_name = ox_mf2_error_code_name(write_code);
    eprintln!("EVIDENCE SnapshotWriteError::MissingRoot code={write_code} name={write_name}");
    assert_eq!(write_code, 2011);
    assert_eq!(write_name, "SnapshotWriteMissingRoot");

    let not_included = SourceTextUnavailable::NotIncluded;
    let source_code = not_included.as_ox_mf2_error_code();
    let source_name = ox_mf2_error_code_name(source_code);
    eprintln!("EVIDENCE SourceTextUnavailable::NotIncluded code={source_code} name={source_name}");
    assert_eq!(source_code, 3000);
    assert_eq!(source_name, "SourceTextNotIncluded");

    let store_code = SourceStoreError::SourceTooLarge.as_ox_mf2_error_code();
    let store_name = ox_mf2_error_code_name(store_code);
    eprintln!("EVIDENCE SourceStoreError::SourceTooLarge code={store_code} name={store_name}");
    assert_eq!(store_code, 3002);
    assert_eq!(store_name, "SourceTextTooLarge");

    let unknown = ox_mf2_error_code_name(999);
    eprintln!("EVIDENCE ox_mf2_error_code_name(999)={unknown}");
    assert_eq!(unknown, "unknown");
}

#[test]
fn typescript_mirror_file_matches_rust_numeric_table() {
    let rust = rust_api_error_code_table();
    let ts = parse_ts_ox_mf2_error_code_object(TS_MIRROR);
    assert!(!ts.is_empty(), "failed to parse TS OxMf2ErrorCode object");

    for (name, value) in &ts {
        assert_eq!(
            rust.get(name),
            Some(value),
            "Rust table missing or mismatched TS entry {name}={value}"
        );
        assert_eq!(
            ox_mf2_error_code_name(*value),
            name.as_str(),
            "ox_mf2_error_code_name mismatch for TS entry {name}={value}"
        );
    }

    for (name, value) in &rust {
        assert_eq!(
            ts.get(name),
            Some(value),
            "TS mirror missing Rust entry {name}={value}"
        );
    }
}

#[test]
fn decode_errors_use_1000_plus_range() {
    let err = DecodeError::new(DecodeErrorCode::InvalidMagic);
    assert_eq!(err.as_ox_mf2_error_code(), 1001);
    assert!(err.as_ox_mf2_error_code() >= OX_MF2_API_ERROR_MIN);
    assert_eq!(
        ox_mf2_error_domain(err.as_ox_mf2_error_code()),
        OxMf2ErrorDomain::Decode
    );
    assert_eq!(
        ox_mf2_error_code_name(err.as_ox_mf2_error_code()),
        "DecodeInvalidMagic"
    );
}

#[test]
fn snapshot_write_errors_use_2000_plus_range() {
    let err = SnapshotWriteError::InconsistentSourceId;
    assert_eq!(err.as_ox_mf2_error_code(), 2013);
    assert_eq!(
        ox_mf2_error_domain(err.as_ox_mf2_error_code()),
        OxMf2ErrorDomain::SnapshotWrite
    );
    assert_eq!(
        ox_mf2_error_code_name(err.as_ox_mf2_error_code()),
        "SnapshotWriteInconsistentSourceId"
    );
}

#[test]
fn source_text_errors_use_3000_plus_range() {
    assert_eq!(
        SourceTextUnavailable::NotIncluded.as_ox_mf2_error_code(),
        3000
    );
    assert_eq!(
        SourceTextUnavailable::SpanOutOfBounds.as_ox_mf2_error_code(),
        3001
    );
    assert_eq!(
        SourceStoreError::SourceTooLarge.as_ox_mf2_error_code(),
        3002
    );
    assert_eq!(
        ox_mf2_error_domain(SourceTextErrorCode::SourceTextTooLarge.as_ox_mf2_error_code()),
        OxMf2ErrorDomain::SourceText
    );
}

#[test]
fn binding_skeleton_codes_have_stable_names() {
    assert_eq!(
        InitializationErrorCode::WasmNotInitialized.as_ox_mf2_error_code(),
        10_000
    );
    assert_eq!(
        ox_mf2_error_code_name(InitializationErrorCode::WasmNotInitialized.as_ox_mf2_error_code()),
        "InitializationWasmNotInitialized"
    );
    assert_eq!(
        BindingValidationErrorCode::InvalidOptions.as_ox_mf2_error_code(),
        11_000
    );
    assert_eq!(
        ox_mf2_error_code_name(BindingValidationErrorCode::InvalidOptions.as_ox_mf2_error_code()),
        "BindingValidationInvalidOptions"
    );
}

#[test]
fn unknown_code_returns_stable_fallback() {
    assert_eq!(ox_mf2_error_code_name(0), "unknown");
    assert_eq!(ox_mf2_error_code_name(999), "unknown");
    assert_eq!(ox_mf2_error_code_name(12_000), "unknown");
    assert_eq!(ox_mf2_error_domain(999), OxMf2ErrorDomain::Unknown);
}

#[test]
fn diagnostic_code_is_not_api_error_namespace() {
    assert_eq!(DiagnosticCode::UnclosedExpression.as_u16(), 2);
    assert_eq!(ox_mf2_error_code_name(2), "unknown");
    assert_ne!(
        ox_mf2_error_code_name(DecodeErrorCode::InvalidMagic.as_ox_mf2_error_code()),
        ox_mf2_error_code_name(DiagnosticCode::UnclosedExpression.as_u16() as u32)
    );
}
