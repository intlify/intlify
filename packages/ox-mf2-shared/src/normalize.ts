/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { SectionKind } from './constants.ts'

import type {
  ChildHandle,
  DecodedSnapshotResult,
  DiagnosticLabelView,
  DiagnosticView,
  NodeHandle,
  ParseBatchResult,
  ParseMessageResult,
  RootHandle,
  SectionMetadata,
  SnapshotAccessor,
  SourceLocation,
  SourceView,
  Span,
  TokenHandle,
  TriviaHandle
} from './types.ts'

/** Plain diagnostic shape used by tests and JSON-friendly output. */
export type NormalizedDiagnostic = {
  /** Root id that owns the diagnostic. */
  readonly rootId: number
  /** Source id that owns the diagnostic span. */
  readonly sourceId: number
  /** Numeric diagnostic severity. */
  readonly severity: number
  /** Numeric diagnostic code. */
  readonly code: number
  /** Optional diagnostic message. */
  readonly message: string | null
  /** Primary diagnostic span. */
  readonly span: Span
  /** Optional line and column location. */
  readonly location: SourceLocation | null
  /** Additional labels associated with the diagnostic. */
  readonly labels: DiagnosticLabelView[]
}

/** Plain parse or decode result shape used for assertions and snapshots. */
export type NormalizedResult = {
  /** Root ids decoded from the snapshot. */
  readonly rootIds: number[]
  /** Source ids decoded from the snapshot. */
  readonly sourceIds: number[]
  /** Diagnostics decoded from the snapshot. */
  readonly diagnostics: NormalizedDiagnostic[]
  /** Counts of primary snapshot record types. */
  readonly counts: {
    /** Number of root records. */
    readonly roots: number
    /** Number of source records. */
    readonly sources: number
    /** Number of node records. */
    readonly nodes: number
    /** Number of token records. */
    readonly tokens: number
    /** Number of trivia records. */
    readonly trivia: number
    /** Number of diagnostic records. */
    readonly diagnostics: number
  }
  /** Section metadata decoded from the snapshot. */
  readonly sections: SectionMetadata[]
}

/**
 * Copy a span into a JSON-friendly object.
 *
 * @param span - Span to copy.
 * @returns Copied span.
 */
export function normalizeSpan(span: Span): Span {
  return { start: span.start, end: span.end }
}

/**
 * Convert a diagnostic view into a plain diagnostic object.
 *
 * @param diagnostic - Diagnostic view to normalize.
 * @returns Plain diagnostic object.
 */
export function normalizeDiagnostic(diagnostic: DiagnosticView): NormalizedDiagnostic {
  return {
    rootId: diagnostic.rootId,
    sourceId: diagnostic.sourceId,
    severity: diagnostic.severity,
    code: diagnostic.code,
    message: diagnostic.message,
    span: normalizeSpan(diagnostic.span),
    location: diagnostic.location
      ? { line: diagnostic.location.line, column: diagnostic.location.column }
      : null,
    labels: diagnostic.labels.map(normalizeDiagnosticLabel)
  }
}

/**
 * Convert a parse or decode result into a plain summary.
 *
 * @param result - Parse or decode result to normalize.
 * @returns Plain result summary.
 */
export function normalizeResult(
  result: ParseMessageResult | ParseBatchResult | DecodedSnapshotResult
): NormalizedResult {
  return {
    rootIds: result.roots.map(root => root.id),
    sourceIds: result.sources.map(source => source.id),
    diagnostics: result.diagnostics.map(normalizeDiagnostic),
    counts: normalizeSnapshotCounts(result.snapshot),
    sections: normalizeSections(result.snapshot)
  }
}

/**
 * Convert a root handle into a plain object.
 *
 * @param root - Root handle to normalize.
 * @returns Plain root object.
 */
export function normalizeRoot(root: RootHandle): unknown {
  return {
    id: root.id,
    node: normalizeNode(root.node()),
    diagnostics: root.diagnostics().map(normalizeDiagnostic)
  }
}

/**
 * Convert a node handle into a plain object.
 *
 * @param node - Node handle to normalize.
 * @returns Plain node object.
 */
export function normalizeNode(node: NodeHandle): unknown {
  return {
    id: node.id,
    kind: node.kind(),
    span: normalizeSpan(node.span()),
    children: node.children().map(normalizeChild)
  }
}

/**
 * Convert a token handle into a plain object.
 *
 * @param token - Token handle to normalize.
 * @returns Plain token object.
 */
export function normalizeToken(token: TokenHandle): unknown {
  return {
    id: token.id,
    kind: token.kind(),
    span: normalizeSpan(token.span()),
    leadingTrivia: token.leadingTrivia().map(normalizeTrivia),
    trailingTrivia: token.trailingTrivia().map(normalizeTrivia)
  }
}

/**
 * Convert a trivia handle into a plain object.
 *
 * @param trivia - Trivia handle to normalize.
 * @returns Plain trivia object.
 */
export function normalizeTrivia(trivia: TriviaHandle): unknown {
  return {
    id: trivia.id,
    kind: trivia.kind(),
    span: normalizeSpan(trivia.span())
  }
}

/**
 * Convert a source view into a plain object.
 *
 * @param source - Source view to normalize.
 * @returns Plain source object.
 */
export function normalizeSource(source: SourceView): unknown {
  return {
    id: source.id,
    path: source.path(),
    locale: source.locale(),
    messageId: source.messageId(),
    baseOffset: source.baseOffset()
  }
}

/**
 * Return primary record counts from a snapshot accessor.
 *
 * @param snapshot - Snapshot accessor to inspect.
 * @returns Plain count object.
 */
export function normalizeSnapshotCounts(snapshot: SnapshotAccessor): NormalizedResult['counts'] {
  return {
    roots: snapshot.rootCount(),
    sources: snapshot.sourceCount(),
    nodes: snapshot.nodeCount(),
    tokens: snapshot.tokenCount(),
    trivia: snapshot.triviaCount(),
    diagnostics: snapshot.diagnosticCount()
  }
}

/**
 * Return present snapshot sections in numeric section order.
 *
 * @param snapshot - Snapshot accessor to inspect.
 * @returns Section metadata entries.
 */
export function normalizeSections(snapshot: SnapshotAccessor): SectionMetadata[] {
  return Object.values(SectionKind)
    .map(kind => snapshot.section(kind))
    .filter((section): section is SectionMetadata => section !== null)
}

function normalizeChild(child: ChildHandle): unknown {
  return isNodeHandle(child) ? normalizeNode(child) : normalizeToken(child)
}

function isNodeHandle(child: ChildHandle): child is NodeHandle {
  return typeof (child as NodeHandle).childCount === 'function'
}

function normalizeDiagnosticLabel(label: DiagnosticLabelView): DiagnosticLabelView {
  return {
    span: normalizeSpan(label.span),
    message: label.message
  }
}
