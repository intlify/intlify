import {
  diagnosticCodeName,
  diagnosticSeverityName,
  init,
  normalizeResult,
  parseMessage,
  sectionKindName,
  syntaxKindName
} from '@intlify/ox-mf2-wasm'
import { computed, onMounted, ref } from 'vue'

import type {
  ChildHandle,
  DiagnosticLabelView,
  DiagnosticView,
  NodeHandle,
  ParseMessageResult,
  RootHandle,
  SourceView,
  Span,
  TokenHandle,
  TriviaHandle
} from '@intlify/ox-mf2-wasm'
import type { ParserOptions, ParseView } from '../types/playground'
import type { Ref } from 'vue'

const emptyCounts = {
  diagnostics: 0,
  nodes: 0,
  roots: 0,
  sources: 0,
  tokens: 0,
  trivia: 0
} as const

type UseMf2ParserOptions = {
  options: ParserOptions
  source: Ref<string>
}

/**
 * Initialize the WASM parser and expose a reactive parse view.
 *
 * @param options - Parser option state.
 * @returns Reactive parse view for the current source text.
 */
export function useMf2Parser({ options, source }: UseMf2ParserOptions) {
  const wasmReady = ref(false)
  const wasmError = ref<string | null>(null)

  onMounted(async () => {
    try {
      await init()
      wasmReady.value = true
    } catch (error) {
      wasmError.value = error instanceof Error ? error.message : String(error)
    }
  })

  const parseView = computed<ParseView>(() => {
    if (!wasmReady.value) {
      if (wasmError.value) {
        return {
          status: 'error',
          ast: createErrorAst(wasmError.value),
          diagnostics: [wasmError.value],
          duration: null,
          snapshotBytes: 0,
          counts: emptyCounts
        }
      }

      return {
        status: 'loading',
        ast: null,
        diagnostics: ['Initializing WASM runtime...'],
        duration: null,
        snapshotBytes: 0,
        counts: emptyCounts
      }
    }

    const start = performance.now()

    try {
      const result = parseMessage(
        {
          source: source.value,
          path: 'playground.mf2',
          messageId: 'playground'
        },
        {
          collectTrivia: options.collectTrivia,
          includeDiagnostics: true,
          includeSourceText: options.includeSourceText,
          includeTrivia: options.includeTrivia
        }
      )
      const summary = normalizeResult(result)
      const diagnostics = result.diagnostics.map(formatDiagnostic)

      return {
        status: diagnostics.length === 0 ? 'ok' : 'invalid',
        ast: serializeParseResult(result),
        diagnostics,
        duration: performance.now() - start,
        snapshotBytes: result.snapshot.toBytes().byteLength,
        counts: summary.counts
      }
    } catch (error) {
      return {
        status: 'error',
        ast: createErrorAst(error instanceof Error ? error.message : String(error)),
        diagnostics: [error instanceof Error ? error.message : String(error)],
        duration: performance.now() - start,
        snapshotBytes: 0,
        counts: emptyCounts
      }
    }
  })

  return {
    parseView
  }
}

function createErrorAst(message: string): unknown {
  return {
    type: 'OxMf2Error',
    message
  }
}

function serializeParseResult(result: ParseMessageResult): unknown {
  const summary = normalizeResult(result)

  return {
    type: 'OxMf2ParseResult',
    snapshotBytes: result.snapshot.toBytes().byteLength,
    counts: summary.counts,
    roots: result.roots.map(root => serializeRoot(root, result.source)),
    sources: result.sources.map(serializeSource),
    diagnostics: result.diagnostics.map(serializeDiagnostic),
    sections: summary.sections.map(section => ({
      ...section,
      kindName: sectionKindName(section.kind)
    }))
  }
}

function serializeRoot(root: RootHandle, source: SourceView): unknown {
  return {
    type: 'Root',
    id: root.id,
    node: serializeNode(root.node(), source),
    diagnostics: root.diagnostics().map(serializeDiagnostic)
  }
}

function serializeNode(node: NodeHandle, source: SourceView): unknown {
  const kind = node.kind()

  return {
    type: 'Node',
    id: node.id,
    kind: syntaxKindName(kind),
    kindValue: kind,
    span: copySpan(node.span()),
    children: node.children().map(child => serializeChild(child, source))
  }
}

function serializeChild(child: ChildHandle, source: SourceView): unknown {
  return isNodeHandle(child) ? serializeNode(child, source) : serializeToken(child, source)
}

function serializeToken(token: TokenHandle, source: SourceView): unknown {
  const span = token.span()
  const kind = token.kind()

  return {
    type: 'Token',
    id: token.id,
    kind: syntaxKindName(kind),
    kindValue: kind,
    span: copySpan(span),
    text: source.sourceSlice(span),
    leadingTrivia: token.leadingTrivia().map(trivia => serializeTrivia(trivia, source)),
    trailingTrivia: token.trailingTrivia().map(trivia => serializeTrivia(trivia, source))
  }
}

function serializeTrivia(trivia: TriviaHandle, source: SourceView): unknown {
  const span = trivia.span()
  const kind = trivia.kind()

  return {
    type: 'Trivia',
    id: trivia.id,
    kind: syntaxKindName(kind),
    kindValue: kind,
    span: copySpan(span),
    text: source.sourceSlice(span)
  }
}

function serializeSource(source: SourceView): unknown {
  return {
    type: 'Source',
    id: source.id,
    path: source.path(),
    locale: source.locale(),
    messageId: source.messageId(),
    baseOffset: source.baseOffset()
  }
}

function serializeDiagnostic(diagnostic: DiagnosticView): unknown {
  return {
    type: 'Diagnostic',
    rootId: diagnostic.rootId,
    sourceId: diagnostic.sourceId,
    severity: diagnosticSeverityName(diagnostic.severity),
    severityValue: diagnostic.severity,
    code: diagnosticCodeName(diagnostic.code),
    codeValue: diagnostic.code,
    message: diagnostic.message,
    span: copySpan(diagnostic.span),
    location: diagnostic.location,
    labels: diagnostic.labels.map(serializeDiagnosticLabel)
  }
}

function serializeDiagnosticLabel(label: DiagnosticLabelView): unknown {
  return {
    span: copySpan(label.span),
    message: label.message
  }
}

function formatDiagnostic(diagnostic: DiagnosticView): string {
  const severity = diagnosticSeverityName(diagnostic.severity)
  const code = diagnosticCodeName(diagnostic.code)
  const location = diagnostic.location
    ? `${diagnostic.location.line}:${diagnostic.location.column}`
    : `${diagnostic.span.start}..${diagnostic.span.end}`
  const message = diagnostic.message ?? 'No diagnostic message'

  return `${severity} ${code} at ${location}: ${message}`
}

function copySpan(span: Span): Span {
  return {
    start: span.start,
    end: span.end
  }
}

function isNodeHandle(child: ChildHandle): child is NodeHandle {
  return typeof (child as NodeHandle).childCount === 'function'
}
