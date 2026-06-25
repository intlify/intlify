<script setup lang="ts">
defineProps<{
  collectTrivia: boolean
  includeSourceText: boolean
  includeTrivia: boolean
}>()

const emit = defineEmits<{
  'update:collectTrivia': [value: boolean]
  'update:includeSourceText': [value: boolean]
  'update:includeTrivia': [value: boolean]
}>()

function readChecked(event: Event): boolean {
  return event.target instanceof HTMLInputElement && event.target.checked
}
</script>

<template>
  <section class="toolbar" aria-label="Parser options">
    <label class="check-control">
      <input
        type="checkbox"
        :checked="collectTrivia"
        @change="emit('update:collectTrivia', readChecked($event))"
      />
      <span>Collect trivia</span>
    </label>

    <label class="check-control">
      <input
        type="checkbox"
        :checked="includeTrivia"
        @change="emit('update:includeTrivia', readChecked($event))"
      />
      <span>Include trivia</span>
    </label>

    <label class="check-control">
      <input
        type="checkbox"
        :checked="includeSourceText"
        @change="emit('update:includeSourceText', readChecked($event))"
      />
      <span>Embed source text</span>
    </label>
  </section>
</template>

<style scoped>
.toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 10px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}

.check-control {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-height: 34px;
  padding: 7px 10px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel-strong);
  color: var(--ink);
  font: 700 12px/1 var(--sans);
}

.check-control input {
  width: 15px;
  height: 15px;
  accent-color: var(--accent);
}
</style>
