<script setup lang="ts">
import AstTreeNode from './AstTreeNode.vue'

import type { AstSelection } from '../types/playground.ts'

defineProps<{
  ast: unknown
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
  <div v-if="ast" class="ast-tree" role="tree" aria-label="MessageFormat AST tree">
    <AstTreeNode
      name="root"
      path="root"
      root
      :reveal-path="revealPath"
      :reveal-version="revealVersion"
      :selected-path="selectedPath"
      :value="ast"
      @select="handleSelect"
    />
  </div>
  <div v-else class="ast-empty">No snapshot available.</div>
</template>

<style scoped>
.ast-tree,
.ast-empty {
  width: 100%;
  height: 100%;
  min-height: 0;
  background: var(--editor-bg);
  color: var(--ink);
  font-family: var(--mono);
  font-size: 13px;
  line-height: 1.55;
}

.ast-tree {
  overflow: auto;
  padding: 20px 22px 28px;
}

.ast-empty {
  display: grid;
  place-items: center;
  color: var(--muted);
}
</style>
