// oxlint-disable-next-line import/default -- Vite ?url imports expose a default URL string.
import editorWorkerUrl from 'monaco-editor/esm/vs/editor/editor.worker?url'
import * as monaco from 'monaco-editor/esm/vs/editor/editor.api.js'
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'

import type { PlaygroundTheme, SourceRange } from '../types/playground'
import type { Ref } from 'vue'

const globalSelf = globalThis as typeof globalThis & {
  MonacoEnvironment?: {
    getWorker(): Worker
  }
}
const utf8Encoder = new TextEncoder()

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

    const startOffset = byteOffsetToStringOffset(source.value, range.start)
    const endOffset = byteOffsetToStringOffset(source.value, range.end)
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
      onSourceOffsetClick(stringOffsetToByteOffset(source.value, stringOffset))
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
      { token: 'keyword', foreground: '126b72', fontStyle: 'bold' },
      { token: 'delimiter.bracket', foreground: '7f5539' },
      { token: 'variable', foreground: '1f5f99', fontStyle: 'bold' },
      { token: 'type.identifier', foreground: '8a4f0a' },
      { token: 'annotation', foreground: '8463a9' },
      { token: 'operator', foreground: 'a34b2a' },
      { token: 'number', foreground: '2f6f4e' },
      { token: 'string', foreground: '2f3f46' }
    ],
    colors: {
      'editor.background': '#fbfaf7',
      'editor.foreground': '#182022',
      'editor.lineHighlightBackground': '#eef1ee',
      'editorLineNumber.activeForeground': '#126b72',
      'editorLineNumber.foreground': '#8b9699',
      'editor.selectionBackground': '#c9ebe7',
      'editorCursor.foreground': '#126b72'
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
      'editor.lineHighlightBackground': '#1a2224',
      'editorLineNumber.activeForeground': '#67d5cb',
      'editorLineNumber.foreground': '#6c797d',
      'editor.selectionBackground': '#264c5a',
      'editorCursor.foreground': '#67d5cb'
    }
  })
}

function stringOffsetToByteOffset(source: string, offset: number): number {
  return utf8Encoder.encode(source.slice(0, offset)).length
}

function byteOffsetToStringOffset(source: string, byteOffset: number): number {
  let currentBytes = 0
  let currentOffset = 0

  while (currentOffset < source.length && currentBytes < byteOffset) {
    const codePoint = source.codePointAt(currentOffset)

    if (codePoint === undefined) {
      break
    }

    const chunk = String.fromCodePoint(codePoint)
    const nextBytes = currentBytes + utf8Encoder.encode(chunk).length

    if (nextBytes > byteOffset) {
      break
    }

    currentBytes = nextBytes
    currentOffset += chunk.length
  }

  return currentOffset
}
