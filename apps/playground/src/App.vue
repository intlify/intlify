<script setup lang="ts">
import { computed, ref } from 'vue'
import AstPane from './components/AstPane.vue'
import ParserOptionsToolbar from './components/ParserOptionsToolbar.vue'
import PlaygroundHeader from './components/PlaygroundHeader.vue'
import SourcePane from './components/SourcePane.vue'
import { useMf2Parser } from './composables/useMf2Parser'
import { useMonacoSourceEditor } from './composables/useMonacoSourceEditor'
import { usePlaygroundSettings } from './composables/usePlaygroundSettings'
import type { AstSelection, SourceRange } from './types/playground'

const sample = `.input {$count :number}
.local $label = {$count :number}
.match $count
0 {{No items}}
1 {{One item}}
* {{{$label} items}}`

const oxMf2WasmVersion = __OX_MF2_WASM_VERSION__
const source = ref(sample)
const selectedAstPath = ref('')
const revealAstPath = ref('')
const revealAstVersion = ref(0)
const { options, theme, toggleTheme } = usePlaygroundSettings()
const { parseView } = useMf2Parser({ options, source })
const { highlightSourceRange, setEditorHost } = useMonacoSourceEditor({
  onSourceOffsetClick: revealAstNodeAtOffset,
  source,
  theme
})

function revealAstNodeAtOffset(offset: number): void {
  const path = findAstPathAtOffset(parseView.value.ast, offset)

  if (!path) {
    return
  }

  selectedAstPath.value = path
  revealAstPath.value = path
  revealAstVersion.value += 1
}

function findAstPathAtOffset(value: unknown, offset: number): string | null {
  let matchPath: string | null = null
  let matchDepth = -1
  let matchSpan = Number.POSITIVE_INFINITY

  function visit(item: unknown, path: string, depth: number): void {
    const range = getAstObjectRange(item)

    if (range && path !== 'root' && range.start <= offset && offset <= range.end) {
      const span = range.end - range.start

      if (matchPath === null || span < matchSpan || (span === matchSpan && depth > matchDepth)) {
        matchPath = path
        matchDepth = depth
        matchSpan = span
      }
    }

    if (Array.isArray(item)) {
      item.forEach((child, index) => visit(child, `${path}.${index}`, depth + 1))
      return
    }

    if (isAstRecord(item)) {
      Object.entries(item).forEach(([key, child]) => {
        visit(child, `${path}.${key}`, depth + 1)
      })
    }
  }

  visit(value, 'root', 0)

  return matchPath
}

function getAstObjectRange(value: unknown): SourceRange | null {
  if (!isAstRecord(value)) {
    return null
  }

  return readRange(value.span) ?? readRange(value.range)
}

function readRange(value: unknown): SourceRange | null {
  if (Array.isArray(value)) {
    return value.length === 2 && typeof value[0] === 'number' && typeof value[1] === 'number'
      ? { start: value[0], end: value[1] }
      : null
  }

  if (isAstRecord(value) && typeof value.start === 'number' && typeof value.end === 'number') {
    return { start: value.start, end: value.end }
  }

  return null
}

function isAstRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

const handleAstNodeSelect = (selection: AstSelection) => {
  selectedAstPath.value = selection.path

  if (selection.range) {
    highlightSourceRange(selection.range)
  }
}

const statusLabel = computed(() => {
  if (parseView.value.status === 'loading') {
    return 'Loading'
  }
  if (parseView.value.status === 'ok') {
    return 'Parsed'
  }
  if (parseView.value.status === 'invalid') {
    return 'Diagnostics'
  }
  return 'Error'
})

const statusTone = computed(() => ({
  'is-error': parseView.value.status === 'error',
  'is-loading': parseView.value.status === 'loading',
  'is-ok': parseView.value.status === 'ok',
  'is-warn': parseView.value.status === 'invalid'
}))

const resetSample = () => {
  source.value = sample
}
</script>

<template>
  <main class="playground" :data-theme="theme">
    <PlaygroundHeader
      :duration="parseView.duration"
      :status-label="statusLabel"
      :status-tone="statusTone"
      :theme="theme"
      :version="oxMf2WasmVersion"
      @toggle-theme="toggleTheme"
    />

    <ParserOptionsToolbar
      v-model:collect-trivia="options.collectTrivia"
      v-model:include-source-text="options.includeSourceText"
      v-model:include-trivia="options.includeTrivia"
    />

    <section class="workspace" aria-label="MessageFormat parser workspace">
      <SourcePane
        :set-editor-host="setEditorHost"
        :source-length="source.length"
        @reset="resetSample"
      />

      <AstPane
        :parse-view="parseView"
        :reveal-path="revealAstPath"
        :reveal-version="revealAstVersion"
        :selected-path="selectedAstPath"
        @select="handleAstNodeSelect"
      />
    </section>
  </main>
</template>

<style scoped>
.playground {
  display: grid;
  grid-template-rows: auto auto minmax(0, 1fr);
  gap: 10px;
  width: min(1480px, 100%);
  height: 100dvh;
  min-height: 0;
  margin: 0 auto;
  overflow: hidden;
  padding: 12px;
}

.workspace {
  display: grid;
  grid-template-columns: minmax(0, 0.95fr) minmax(0, 1.05fr);
  gap: 10px;
  min-height: 0;
  overflow: hidden;
}

@media (max-width: 980px) {
  .playground {
    height: auto;
    min-height: 100dvh;
    overflow: visible;
  }

  .workspace {
    grid-template-columns: 1fr;
    grid-template-rows: minmax(480px, 54dvh) minmax(520px, 60dvh);
    overflow: visible;
  }
}
</style>
