<script setup lang="ts">
import AstTree from './AstTree.vue'
import type { AstSelection, ParseView } from '../types/playground.ts'

defineProps<{
  parseView: ParseView
  revealPath: string
  revealVersion: number
  selectedPath: string
}>()

const emit = defineEmits<{
  select: [selection: AstSelection]
}>()

function handleSelect(selection: AstSelection): void {
  emit('select', selection)
}
</script>

<template>
  <section class="pane ast-pane">
    <div class="pane-title">
      <span>AST (Snapshot)</span>
      <strong>
        {{ parseView.counts.nodes }} nodes / {{ parseView.counts.tokens }} tokens /
        {{ parseView.snapshotBytes }} bytes
      </strong>
    </div>

    <div v-if="parseView.diagnostics.length > 0" class="diagnostics">
      <p v-for="(diagnostic, index) in parseView.diagnostics" :key="`${index}:${diagnostic}`">
        {{ diagnostic }}
      </p>
    </div>

    <AstTree
      :ast="parseView.ast"
      :reveal-path="revealPath"
      :reveal-version="revealVersion"
      :selected-path="selectedPath"
      @select="handleSelect"
    />
  </section>
</template>

<style scoped>
.ast-pane {
  position: relative;
}

.diagnostics {
  border-bottom: 1px solid var(--line);
  background: var(--diagnostic-bg);
  color: var(--diagnostic-ink);
  font: 13px/1.5 var(--mono);
}

.diagnostics p {
  margin: 0;
  padding: 9px 14px;
}
</style>
