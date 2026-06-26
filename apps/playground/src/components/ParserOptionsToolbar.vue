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
    <div class="toolbar-copy">
      <h2 class="toolbar-title">Parser options</h2>
      <p class="toolbar-description">Parse / snapshot settings</p>
    </div>

    <div class="toolbar-controls">
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
    </div>
  </section>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 18px;
  padding: 10px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}

.toolbar-copy {
  flex: 0 0 auto;
  min-width: 210px;
  padding: 0 14px 0 2px;
}

.toolbar-title,
.toolbar-description {
  margin: 0;
  letter-spacing: 0.12em;
  line-height: 1;
  text-transform: uppercase;
}

.toolbar-title {
  color: var(--accent);
  font: 700 13px/1 var(--sans);
}

.toolbar-description {
  margin-top: 7px;
  color: var(--muted);
  font: 700 11px/1 var(--sans);
}

.toolbar-controls {
  display: flex;
  flex: 1 1 auto;
  flex-wrap: wrap;
  gap: 8px;
}

.check-control {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-height: 34px;
  padding: 7px 10px;
  border: 1px solid var(--line);
  border-radius: 999px;
  background: var(--panel-strong);
  color: var(--ink);
  font: 700 12px/1 var(--sans);
  letter-spacing: 0.12em;
  text-transform: uppercase;
}

.check-control input {
  width: 15px;
  height: 15px;
  accent-color: var(--accent);
}

@media (max-width: 760px) {
  .toolbar {
    align-items: flex-start;
    flex-direction: column;
  }

  .toolbar-copy {
    min-width: 0;
    padding: 0;
  }
}
</style>
