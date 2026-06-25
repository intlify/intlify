<script setup lang="ts">
import type { PlaygroundTheme } from '../types/playground'

defineProps<{
  duration: number | null
  statusLabel: string
  statusTone: Record<string, boolean>
  theme: PlaygroundTheme
  version: string
}>()

const emit = defineEmits<{
  toggleTheme: []
}>()

function handleToggleTheme(): void {
  emit('toggleTheme')
}
</script>

<template>
  <section class="topbar" aria-labelledby="title">
    <div class="brand">
      <img class="brand-logo" src="../assets/logo.svg" alt="" width="96" height="96" />
      <div class="brand-copy">
        <p class="eyebrow">@intlify/ox-mf2-wasm v{{ version }}</p>
        <h1 id="title">ox-mf2 Playground</h1>
      </div>
    </div>

    <div class="top-actions">
      <a
        class="icon-link"
        href="https://github.com/intlify/intlify"
        target="_blank"
        rel="noreferrer"
        aria-label="Open intlify on GitHub"
      >
        <svg aria-hidden="true" viewBox="0 0 16 16">
          <path
            fill="currentColor"
            d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.14-.28-.15-.68-.52-.01-.53.63-.01 1.08.57 1.23.81.72 1.18 1.87.84 2.33.64.07-.51.28-.84.51-1.03-1.78-.2-3.64-.86-3.64-3.87 0-.85.31-1.55.82-2.1-.08-.2-.36-.99.08-2.07 0 0 .67-.21 2.2.8A7.6 7.6 0 0 1 8 3.65c.68 0 1.36.09 2 .27 1.53-1.01 2.2-.8 2.2-.8.44 1.08.16 1.87.08 2.07.51.55.82 1.25.82 2.1 0 3.01-1.87 3.67-3.65 3.87.29.25.54.71.54 1.45 0 1.04-.01 1.88-.01 2.14 0 .21.15.46.55.38A8.02 8.02 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
          />
        </svg>
      </a>

      <button
        type="button"
        class="theme-toggle"
        :aria-pressed="theme === 'dark'"
        aria-label="Toggle color theme"
        @click="handleToggleTheme"
      >
        <span class="toggle-track">
          <span class="toggle-thumb" />
        </span>
        {{ theme === 'light' ? 'Light' : 'Dark' }}
      </button>

      <div class="status" :class="statusTone">
        <span>{{ statusLabel }}</span>
        <strong v-if="duration !== null">{{ duration.toFixed(2) }} ms</strong>
      </div>
    </div>
  </section>
</template>

<style scoped>
.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 18px;
  min-height: 88px;
  padding: 14px 16px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel);
}

.brand {
  display: flex;
  align-items: center;
  min-width: 0;
  gap: 14px;
}

.brand-logo {
  flex: 0 0 auto;
  width: 56px;
  height: 56px;
}

.brand-copy {
  min-width: 0;
}

.eyebrow {
  margin: 0 0 7px;
  color: var(--accent);
  font: 700 12px/1 var(--sans);
}

h1 {
  margin: 0;
  color: var(--ink);
  font-size: clamp(24px, 4vw, 42px);
  line-height: 1;
}

.top-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
}

.icon-link,
.theme-toggle,
.status {
  display: inline-flex;
  align-items: center;
  min-height: 36px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--panel-strong);
  color: var(--ink);
  font: 700 12px/1 var(--sans);
}

.icon-link {
  width: 38px;
  justify-content: center;
}

.icon-link:hover,
.theme-toggle:hover {
  border-color: var(--accent);
  color: var(--accent);
}

.icon-link svg {
  width: 17px;
  height: 17px;
}

.theme-toggle {
  gap: 9px;
  padding: 8px 10px;
}

.toggle-track {
  position: relative;
  width: 34px;
  height: 18px;
  border-radius: 999px;
  background: var(--line);
}

.toggle-thumb {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: var(--accent);
  transition: transform 0.18s ease;
}

.theme-toggle[aria-pressed='true'] .toggle-thumb {
  transform: translateX(16px);
}

.status {
  gap: 10px;
  padding: 8px 10px;
  color: var(--muted);
  white-space: nowrap;
}

.status::before {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  content: '';
}

.status.is-loading::before {
  background: var(--tone-loading);
}

.status.is-ok::before {
  background: var(--tone-ok);
}

.status.is-warn::before {
  background: var(--tone-warn);
}

.status.is-error::before {
  background: var(--tone-error);
}

.status strong {
  color: var(--ink);
}

@media (max-width: 760px) {
  .topbar {
    align-items: flex-start;
    flex-direction: column;
  }
}
</style>
