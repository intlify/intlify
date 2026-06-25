import type { Span } from '@intlify/ox-mf2-wasm'

/** Available UI color themes. */
export type PlaygroundTheme = 'light' | 'dark'

/** UTF-8 byte range in the edited source text. */
export type SourceRange = Span

/** Parser options exposed by the playground controls. */
export type ParserOptions = {
  /** Whether the parser should collect whitespace and bidi trivia. */
  collectTrivia: boolean
  /** Whether collected trivia should be written into the snapshot. */
  includeTrivia: boolean
  /** Whether source text bytes should be embedded in the snapshot. */
  includeSourceText: boolean
}

/** Persisted playground settings. */
export type PlaygroundSettings = ParserOptions & {
  /** Current UI color theme. */
  theme: PlaygroundTheme
}

/** Primary snapshot record counts. */
export type SnapshotCounts = {
  /** Root record count. */
  roots: number
  /** Source record count. */
  sources: number
  /** Node record count. */
  nodes: number
  /** Token record count. */
  tokens: number
  /** Trivia record count. */
  trivia: number
  /** Diagnostic record count. */
  diagnostics: number
}

/** Reactive parser view consumed by the UI panes. */
export type ParseView =
  | {
      /** Parser lifecycle state. */
      status: 'loading'
      /** Materialized AST-like snapshot tree. */
      ast: null
      /** Human-readable diagnostics or runtime messages. */
      diagnostics: string[]
      /** Last parse duration in milliseconds. */
      duration: null
      /** Snapshot byte length. */
      snapshotBytes: 0
      /** Snapshot record counts. */
      counts: SnapshotCounts
    }
  | {
      /** Parser lifecycle state. */
      status: 'ok' | 'invalid' | 'error'
      /** Materialized AST-like snapshot tree. */
      ast: unknown
      /** Human-readable diagnostics or runtime messages. */
      diagnostics: string[]
      /** Last parse duration in milliseconds. */
      duration: number | null
      /** Snapshot byte length. */
      snapshotBytes: number
      /** Snapshot record counts. */
      counts: SnapshotCounts
    }

/** AST tree selection emitted by the snapshot pane. */
export type AstSelection = {
  /** Dot-separated tree path. */
  path: string
  /** Source range associated with the selected node, when available. */
  range: SourceRange | null
  /** Raw selected tree value. */
  value: unknown
}
