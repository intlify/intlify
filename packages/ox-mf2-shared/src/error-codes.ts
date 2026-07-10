/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

/**
 * Unified ox-mf2 API error codes for language bindings.
 *
 * Numeric values MUST stay in sync with `crates/ox_mf2_parser` guard tests
 * (`tests/snapshot_compat.rs`, `tests/error_codes.rs`).
 *
 * @see design/appendix-ox-mf2-error-code.md
 */
export const OxMf2ErrorCode = {
  // Decode (1000..1999)
  DecodeBufferTooShort: 1000,
  DecodeInvalidMagic: 1001,
  DecodeUnsupportedMajorVersion: 1002,
  DecodeUnsupportedMinorVersion: 1003,
  DecodeInvalidHeaderLength: 1004,
  DecodeInvalidFeatureFlags: 1005,
  DecodeInvalidReservedField: 1006,
  DecodeSectionTableOutOfBounds: 1007,
  DecodeDuplicateSection: 1008,
  DecodeMissingRequiredSection: 1009,
  DecodeUnknownSection: 1010,
  DecodeUnknownRequiredSection: 1011,
  DecodeInvalidSectionFlags: 1012,
  DecodeInvalidSectionAlignment: 1013,
  DecodeInvalidSectionBounds: 1014,
  DecodeInvalidRecordSize: 1015,
  DecodeInvalidSectionCount: 1016,
  DecodeOverlappingSection: 1017,
  DecodeInvalidPadding: 1018,
  DecodeTrailingPadding: 1019,
  DecodeInvalidStringOffset: 1020,
  DecodeInvalidUtf8: 1021,
  DecodeInvalidStringRef: 1022,
  DecodeInvalidSourceRef: 1023,
  DecodeInvalidRootRef: 1024,
  DecodeInvalidNodeRef: 1025,
  DecodeInvalidTokenRef: 1026,
  DecodeInvalidTriviaRef: 1027,
  DecodeUnknownSyntaxKind: 1028,
  DecodeInvalidDiagnosticSeverity: 1029,
  DecodeUnknownDiagnosticCode: 1030,
  DecodeInvalidDiagnosticRange: 1031,
  DecodeInvalidSourceTextRange: 1032,
  DecodeInvalidExtendedData: 1033,
  DecodeInvalidEdgeKind: 1034,
  DecodeInvalidSpan: 1035,

  // Snapshot write (2000..2999)
  SnapshotWriteSourceTooLarge: 2000,
  SnapshotWriteTooManyRoots: 2001,
  SnapshotWriteTooManySources: 2002,
  SnapshotWriteTooManyStrings: 2003,
  SnapshotWriteTooManyNodes: 2004,
  SnapshotWriteTooManyEdges: 2005,
  SnapshotWriteTooManyTokens: 2006,
  SnapshotWriteTooManyTrivia: 2007,
  SnapshotWriteTooManyDiagnostics: 2008,
  SnapshotWriteTooManyDiagnosticLabels: 2009,
  SnapshotWriteSectionTooLarge: 2010,
  SnapshotWriteMissingRoot: 2011,
  SnapshotWriteInvalidSourceId: 2012,
  SnapshotWriteInconsistentSourceId: 2013,
  SnapshotWriteTriviaNotCollected: 2014,

  // Source text (3000..3999)
  SourceTextNotIncluded: 3000,
  SourceTextSpanOutOfBounds: 3001,
  SourceTextTooLarge: 3002,
  SourceTextCountMismatch: 3003,
  // Reserved; Phase 2 input validation throws TypeError instead.
  SourceTextUnpairedSurrogate: 3004,

  // Parse (4000..4999)
  ParseSourceTooLarge: 4000,
  ParseInvalidSourceId: 4001,
  ParseTooManySources: 4002,
  ParseTooManyNodes: 4003,
  ParseTooManyEdges: 4004,
  ParseTooManyTokens: 4005,
  ParseTooManyTrivia: 4006,
  ParseTooManyDiagnostics: 4007,
  ParseMissingRoot: 4008,

  // Initialization (10000..10999)
  InitializationWasmNotInitialized: 10_000,
  InitializationNativeBindingUnavailable: 10_001,

  // Binding validation (11000..11999)
  BindingValidationInvalidOptions: 11_000
} as const

/** Union of all known ox-mf2 API error code numeric values. */
export type OxMf2ErrorCodeValue = (typeof OxMf2ErrorCode)[keyof typeof OxMf2ErrorCode]

const OX_MF2_ERROR_CODE_NAMES: Record<number, string> = Object.fromEntries(
  Object.entries(OxMf2ErrorCode).map(([name, code]) => [code, name])
)

/**
 * Stable programmatic name for a known API error code.
 *
 * @param code - Numeric API error code from the unified namespace.
 * @returns Stable name string, or `"unknown"` when the code is unrecognized.
 */
export function oxMf2ErrorCodeName(code: number): string {
  return OX_MF2_ERROR_CODE_NAMES[code] ?? 'unknown'
}
