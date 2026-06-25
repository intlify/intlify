import { reactive, ref, watch } from 'vue'
import type { ParserOptions, PlaygroundSettings, PlaygroundTheme } from '../types/playground'

const settingsKey = 'ox-mf2.playground.settings'

const defaultSettings: PlaygroundSettings = {
  collectTrivia: true,
  includeSourceText: true,
  includeTrivia: true,
  theme: 'light'
}

function isPlaygroundTheme(value: unknown): value is PlaygroundTheme {
  return value === 'dark' || value === 'light'
}

function loadSettings(): PlaygroundSettings {
  if (typeof localStorage === 'undefined') {
    return { ...defaultSettings }
  }

  try {
    const raw = localStorage.getItem(settingsKey)

    if (!raw) {
      return { ...defaultSettings }
    }

    const parsed = JSON.parse(raw) as Partial<PlaygroundSettings>

    return {
      collectTrivia:
        typeof parsed.collectTrivia === 'boolean'
          ? parsed.collectTrivia
          : defaultSettings.collectTrivia,
      includeSourceText:
        typeof parsed.includeSourceText === 'boolean'
          ? parsed.includeSourceText
          : defaultSettings.includeSourceText,
      includeTrivia:
        typeof parsed.includeTrivia === 'boolean'
          ? parsed.includeTrivia
          : defaultSettings.includeTrivia,
      theme: isPlaygroundTheme(parsed.theme) ? parsed.theme : defaultSettings.theme
    }
  } catch {
    return { ...defaultSettings }
  }
}

function saveSettings(options: ParserOptions, theme: PlaygroundTheme): void {
  if (typeof localStorage === 'undefined') {
    return
  }

  try {
    localStorage.setItem(
      settingsKey,
      JSON.stringify({
        collectTrivia: options.collectTrivia,
        includeSourceText: options.includeSourceText,
        includeTrivia: options.includeTrivia,
        theme
      } satisfies PlaygroundSettings)
    )
  } catch {
    // Storage can be unavailable in private contexts; current-session settings still work.
  }
}

/**
 * Load, persist, and expose playground settings.
 *
 * @returns Reactive parser options and color theme controls.
 */
export function usePlaygroundSettings() {
  const settings = loadSettings()
  const theme = ref<PlaygroundTheme>(settings.theme)
  const options = reactive<ParserOptions>({
    collectTrivia: settings.collectTrivia,
    includeSourceText: settings.includeSourceText,
    includeTrivia: settings.includeTrivia
  })

  const toggleTheme = () => {
    theme.value = theme.value === 'light' ? 'dark' : 'light'
  }

  watch(
    [
      theme,
      () => options.collectTrivia,
      () => options.includeSourceText,
      () => options.includeTrivia
    ],
    () => saveSettings(options, theme.value)
  )

  return {
    options,
    theme,
    toggleTheme
  }
}
