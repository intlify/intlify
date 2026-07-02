/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

/**
 * Stable numeric constants shared by the N-API and WASM packages.
 *
 * These values mirror the Rust parser and Binary AST snapshot wire contract.
 */

export const NONE_U32 = 0xffff_ffff
export const SNAPSHOT_MAGIC = 'OXMF2AST'
export const SNAPSHOT_MAJOR_VERSION = 0
export const SNAPSHOT_MINOR_VERSION = 1
export const SNAPSHOT_FEATURE_FLAGS = 0
export const HEADER_SIZE = 32
export const SECTION_RECORD_SIZE = 20
export const ROOT_RECORD_SIZE = 16
export const STRING_OFFSET_RECORD_SIZE = 8
export const SOURCE_RECORD_SIZE = 32
export const NODE_RECORD_SIZE = 24
export const EDGE_RECORD_SIZE = 8
export const TOKEN_RECORD_SIZE = 36
export const TRIVIA_RECORD_SIZE = 16
export const DIAGNOSTIC_RECORD_SIZE = 28
export const DIAGNOSTIC_LABEL_RECORD_SIZE = 16
export const SECTION_ALIGNMENT = 8
export const SECTION_FLAG_REQUIRED = 1
export const EDGE_KIND_NODE = 0
export const EDGE_KIND_TOKEN = 1

export const SyntaxKind = {
  Tombstone: 0,
  Root: 1,
  SimpleMessage: 2,
  ComplexMessage: 3,
  Pattern: 10,
  Text: 11,
  QuotedPattern: 12,
  Placeholder: 13,
  LiteralExpression: 20,
  VariableExpression: 21,
  FunctionExpression: 22,
  Function: 23,
  Option: 24,
  Attribute: 25,
  LocalDeclaration: 30,
  InputDeclaration: 31,
  ComplexBody: 32,
  Matcher: 33,
  Selector: 34,
  Variant: 35,
  VariantKey: 36,
  CatchAllKey: 37,
  Markup: 50,
  MarkupOpen: 51,
  MarkupStandalone: 52,
  MarkupClose: 53,
  QuotedLiteral: 60,
  UnquotedLiteral: 61,
  Name: 62,
  Identifier: 63,
  Variable: 64,
  LeftBraceToken: 100,
  RightBraceToken: 101,
  LeftDoubleBraceToken: 102,
  RightDoubleBraceToken: 103,
  DotToken: 104,
  AtToken: 105,
  PipeToken: 106,
  EqualsToken: 107,
  ColonToken: 108,
  DollarToken: 109,
  SlashToken: 110,
  StarToken: 111,
  HashToken: 112,
  InputKeyword: 150,
  LocalKeyword: 151,
  MatchKeyword: 152,
  NameToken: 170,
  TextToken: 171,
  QuotedTextToken: 172,
  EscapeToken: 173,
  WhitespaceTrivia: 200,
  BidiTrivia: 201,
  Error: 300,
  Missing: 301,
  Unknown: 302
} as const

/** Numeric syntax kind value used in node, token, and trivia records. */
export type SyntaxKindValue = (typeof SyntaxKind)[keyof typeof SyntaxKind]

export const DiagnosticSeverity = {
  Error: 0,
  Warning: 1,
  Information: 2,
  Hint: 3
} as const

/** Numeric diagnostic severity value used in diagnostic records. */
export type DiagnosticSeverityValue = (typeof DiagnosticSeverity)[keyof typeof DiagnosticSeverity]

export const DiagnosticCode = {
  Unspecified: 0,
  UnexpectedEndOfInput: 1,
  UnclosedExpression: 2,
  UnclosedQuotedLiteral: 3,
  UnclosedQuotedPattern: 4,
  InvalidDeclarationStart: 5,
  InvalidMatcherSyntax: 6,
  InvalidVariantBoundary: 7,
  InvalidMarkupBoundary: 8,
  MissingComplexBody: 9,
  UnexpectedToken: 10,
  SpanOverflow: 11,
  InvalidEscape: 12,
  AmbiguousMessageMode: 13,
  MissingRequiredWhitespace: 14,
  MissingIdentifierName: 15,
  InvalidInputDeclaration: 16
} as const

/** Numeric diagnostic code value used in diagnostic records. */
export type DiagnosticCodeValue = (typeof DiagnosticCode)[keyof typeof DiagnosticCode]

export const SectionKind = {
  Roots: 1,
  Sources: 2,
  Nodes: 3,
  Edges: 4,
  Tokens: 5,
  Trivia: 6,
  Diagnostics: 7,
  DiagnosticLabels: 8,
  StringOffsets: 9,
  StringData: 10,
  SourceTextData: 11,
  ExtendedData: 12
} as const

/** Numeric section kind value used in the snapshot section table. */
export type SectionKindValue = (typeof SectionKind)[keyof typeof SectionKind]

export const CORE_SECTION_KINDS = [
  SectionKind.Roots,
  SectionKind.Sources,
  SectionKind.Nodes,
  SectionKind.Edges,
  SectionKind.Tokens,
  SectionKind.StringOffsets,
  SectionKind.StringData
] as const

const SYNTAX_KIND_NAMES = createNameMap(SyntaxKind)
const DIAGNOSTIC_SEVERITY_NAMES = createNameMap(DiagnosticSeverity)
const DIAGNOSTIC_CODE_NAMES = createNameMap(DiagnosticCode)
const SECTION_KIND_NAMES = createNameMap(SectionKind)

/**
 * Return the stable name for a syntax kind value.
 *
 * @param kind - Numeric syntax kind value.
 * @returns Stable syntax kind name, or `unknown` when unrecognized.
 */
export function syntaxKindName(kind: number): string {
  return nameFrom(SYNTAX_KIND_NAMES, kind)
}

/**
 * Return the stable name for a diagnostic severity value.
 *
 * @param severity - Numeric diagnostic severity value.
 * @returns Stable diagnostic severity name, or `unknown` when unrecognized.
 */
export function diagnosticSeverityName(severity: number): string {
  return nameFrom(DIAGNOSTIC_SEVERITY_NAMES, severity)
}

/**
 * Return the stable name for a diagnostic code value.
 *
 * @param code - Numeric diagnostic code value.
 * @returns Stable diagnostic code name, or `unknown` when unrecognized.
 */
export function diagnosticCodeName(code: number): string {
  return nameFrom(DIAGNOSTIC_CODE_NAMES, code)
}

/**
 * Return the stable name for a section kind value.
 *
 * @param kind - Numeric section kind value.
 * @returns Stable section kind name, or `unknown` when unrecognized.
 */
export function sectionKindName(kind: number): string {
  return nameFrom(SECTION_KIND_NAMES, kind)
}

/**
 * Return whether a section kind is part of the required snapshot core.
 *
 * @param kind - Section kind to inspect.
 * @returns True when the section is a required core section.
 */
export function isCoreSectionKind(kind: SectionKindValue): boolean {
  return CORE_SECTION_KINDS.includes(kind as (typeof CORE_SECTION_KINDS)[number])
}

/**
 * Return the fixed record size for a section kind.
 *
 * @param kind - Section kind to inspect.
 * @returns Record size in bytes, or zero for byte payload sections.
 */
export function sectionRecordSize(kind: SectionKindValue): number {
  switch (kind) {
    case SectionKind.Roots:
      return ROOT_RECORD_SIZE
    case SectionKind.Sources:
      return SOURCE_RECORD_SIZE
    case SectionKind.Nodes:
      return NODE_RECORD_SIZE
    case SectionKind.Edges:
      return EDGE_RECORD_SIZE
    case SectionKind.Tokens:
      return TOKEN_RECORD_SIZE
    case SectionKind.Trivia:
      return TRIVIA_RECORD_SIZE
    case SectionKind.Diagnostics:
      return DIAGNOSTIC_RECORD_SIZE
    case SectionKind.DiagnosticLabels:
      return DIAGNOSTIC_LABEL_RECORD_SIZE
    case SectionKind.StringOffsets:
      return STRING_OFFSET_RECORD_SIZE
    default:
      return 0
  }
}

function createNameMap(values: Record<string, number>): ReadonlyMap<number, string> {
  return new Map(Object.entries(values).map(([name, value]) => [value, name]))
}

function nameFrom(names: ReadonlyMap<number, string>, value: number): string {
  return names.get(value) ?? 'unknown'
}
