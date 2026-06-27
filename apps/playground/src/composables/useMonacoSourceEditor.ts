// oxlint-disable-next-line import/default -- Vite ?url imports expose a default URL string.
import editorWorkerUrl from 'monaco-editor/esm/vs/editor/editor.worker?url'
import { utf16OffsetToUtf8ByteOffset, utf8ByteOffsetToUtf16Offset } from '@intlify/ox-mf2-wasm'
import * as monaco from 'monaco-editor/esm/vs/editor/editor.api.js'
import { logger } from 'void/log'
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'

import type { PlaygroundTheme, SourceRange } from '../types/playground'
import type { Ref } from 'vue'

const globalSelf = globalThis as typeof globalThis & {
  MonacoEnvironment?: {
    getWorker(): Worker
  }
}
globalSelf.MonacoEnvironment = {
  getWorker() {
    return new Worker(editorWorkerUrl, { type: 'module' })
  }
}

let editorLanguageDefined = false
let editorThemesDefined = false

type UseMonacoSourceEditorOptions = {
  onSourceOffsetClick: (offset: number) => void
  source: Ref<string>
  theme: Ref<PlaygroundTheme>
}

/**
 * Create and synchronize the Monaco source editor.
 *
 * @param options - Source, theme, and click callback bindings.
 * @returns Editor host setter and source highlight command.
 */
export function useMonacoSourceEditor({
  onSourceOffsetClick,
  source,
  theme
}: UseMonacoSourceEditorOptions) {
  const editorHost = ref<HTMLElement | null>(null)
  const sourceEditor = shallowRef<monaco.editor.IStandaloneCodeEditor | null>(null)
  const sourceHighlightDecorations = shallowRef<monaco.editor.IEditorDecorationsCollection | null>(
    null
  )
  let resizeObserver: ResizeObserver | null = null

  const editorTheme = computed(() => `ox-mf2-${theme.value}`)

  const setEditorHost = (element: Element | null): void => {
    editorHost.value = element instanceof HTMLElement ? element : null
  }

  const highlightSourceRange = (range: SourceRange): void => {
    const editor = sourceEditor.value
    const model = editor?.getModel()

    if (!editor || !model) {
      return
    }

    let startOffset: number
    let endOffset: number
    try {
      startOffset = utf8ByteOffsetToUtf16Offset(source.value, range.start)
      endOffset = utf8ByteOffsetToUtf16Offset(source.value, range.end)
    } catch (error) {
      logOffsetConversionError('highlight', error)
      return
    }

    const start = model.getPositionAt(startOffset)
    const end = model.getPositionAt(endOffset)

    sourceHighlightDecorations.value?.clear()
    sourceHighlightDecorations.value = editor.createDecorationsCollection([
      {
        range: new monaco.Range(start.lineNumber, start.column, end.lineNumber, end.column),
        options: {
          inlineClassName: 'source-range-highlight',
          stickiness: monaco.editor.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges
        }
      }
    ])
  }

  onMounted(() => {
    defineEditorLanguage()
    defineEditorThemes()
    document.documentElement.dataset.theme = theme.value

    if (!editorHost.value) {
      return
    }

    sourceEditor.value = monaco.editor.create(editorHost.value, {
      automaticLayout: true,
      fontFamily: 'Menlo, Monaco, "SFMono-Regular", Consolas, "Liberation Mono", monospace',
      fontSize: 14,
      language: 'messageformat2',
      lineNumbersMinChars: 3,
      minimap: { enabled: false },
      padding: { bottom: 18, top: 18 },
      scrollBeyondLastLine: false,
      smoothScrolling: true,
      tabSize: 2,
      theme: editorTheme.value,
      value: source.value,
      wordWrap: 'on'
    })

    sourceEditor.value.onDidChangeModelContent(() => {
      const value = sourceEditor.value?.getValue() ?? ''

      if (value !== source.value) {
        source.value = value
      }
    })

    sourceEditor.value.onMouseDown((event: monaco.editor.IEditorMouseEvent) => {
      const model = sourceEditor.value?.getModel()
      const position = event.target.position

      if (!model || !position) {
        return
      }

      const stringOffset = model.getOffsetAt(position)
      let byteOffset: number
      try {
        byteOffset = utf16OffsetToUtf8ByteOffset(source.value, stringOffset)
      } catch (error) {
        logOffsetConversionError('click', error)
        return
      }

      onSourceOffsetClick(byteOffset)
    })

    resizeObserver = new ResizeObserver(() => {
      sourceEditor.value?.layout()
    })
    resizeObserver.observe(editorHost.value)
  })

  onBeforeUnmount(() => {
    resizeObserver?.disconnect()
    sourceHighlightDecorations.value?.clear()
    sourceEditor.value?.dispose()
  })

  watch(source, value => {
    const editor = sourceEditor.value

    if (editor && editor.getValue() !== value) {
      editor.setValue(value)
    }
  })

  watch(theme, value => {
    document.documentElement.dataset.theme = value
    monaco.editor.setTheme(editorTheme.value)
  })

  return {
    highlightSourceRange,
    setEditorHost
  }
}

function logOffsetConversionError(action: string, error: unknown): void {
  logger.warn('Failed to convert MessageFormat source offset', {
    action,
    error: error instanceof Error ? error.message : String(error)
  })
}

function defineEditorLanguage(): void {
  if (editorLanguageDefined) {
    return
  }

  editorLanguageDefined = true
  monaco.languages.register({ id: 'messageformat2' })
  monaco.languages.setMonarchTokensProvider('messageformat2', {
    tokenizer: {
      root: [
        [/\.(input|local|match)\b/, 'keyword'],
        [/\{\{|\}\}|\{|\}/, 'delimiter.bracket'],
        [/\$[a-z_][\w-]*/i, 'variable'],
        [/:[a-z_][\w-]*/i, 'type.identifier'],
        [/@[a-z_][\w-]*/i, 'annotation'],
        [/\*/, 'operator'],
        [/\d+(?:\.\d+)?/, 'number'],
        [/\s+/, 'white'],
        [/[^{}$:.@\s*]+/, 'string']
      ]
    }
  })
}

function defineEditorThemes(): void {
  if (editorThemesDefined) {
    return
  }

  editorThemesDefined = true
  monaco.editor.defineTheme('ox-mf2-light', {
    base: 'vs',
    inherit: true,
    rules: [
      { token: 'keyword', foreground: '6e4b20', fontStyle: 'bold' },
      { token: 'delimiter.bracket', foreground: '7f5539' },
      { token: 'variable', foreground: '315a95', fontStyle: 'bold' },
      { token: 'type.identifier', foreground: '8a4f0a' },
      { token: 'annotation', foreground: '8463a9' },
      { token: 'operator', foreground: 'b85d22' },
      { token: 'number', foreground: '2f6f4e' },
      { token: 'string', foreground: '2f3f46' }
    ],
    colors: {
      'editor.background': '#fffaf2',
      'editor.foreground': '#1d1710',
      'editor.lineHighlightBackground': '#f3eadc',
      'editorLineNumber.activeForeground': '#7a4a18',
      'editorLineNumber.foreground': '#9b8d80',
      'editor.selectionBackground': '#f7d8a8',
      'editorCursor.foreground': '#7a4a18'
    }
  })

  monaco.editor.defineTheme('ox-mf2-dark', {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: 'keyword', foreground: '67d5cb', fontStyle: 'bold' },
      { token: 'delimiter.bracket', foreground: 'e3b261' },
      { token: 'variable', foreground: '89b8ff', fontStyle: 'bold' },
      { token: 'type.identifier', foreground: 'f3c06a' },
      { token: 'annotation', foreground: 'c9a7ff' },
      { token: 'operator', foreground: 'ff8a80' },
      { token: 'number', foreground: 'a7d28d' },
      { token: 'string', foreground: 'e9eef0' }
    ],
    colors: {
      'editor.background': '#121719',
      'editor.foreground': '#e9eef0',
      'editor.lineHighlightBackground': '#201b15',
      'editorLineNumber.activeForeground': '#ffad33',
      'editorLineNumber.foreground': '#6c797d',
      'editor.selectionBackground': '#5a3a1a',
      'editorCursor.foreground': '#ffad33'
    }
  })
}
