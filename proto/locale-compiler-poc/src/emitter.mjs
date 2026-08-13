/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

const SOURCE_LOCALE = 'en'
const TARGET_LOCALE = 'ja'

const RUNTIME_SOURCE = `import { messages as en } from './locale-en.mjs'
import { messages as ja } from './locale-ja.mjs'

const capsules = { en, ja }

const PLACEHOLDER_PATTERN = /\\{\\$([A-Za-z_][A-Za-z0-9_]*)\\}/g

export function message(intentId, parameters) {
  const locale = globalThis.__INTLIFY_LOCALE__ ?? 'en'

  if (!Object.hasOwn(capsules, locale)) {
    throw new RangeError(\`unsupported locale: \${locale}\`)
  }

  const value = capsules[locale][intentId]

  if (value === undefined) {
    throw new RangeError(\`missing localized message: \${locale}/\${intentId}\`)
  }

  if (arguments.length < 2) {
    return value
  }

  const source = capsules.en[intentId]
  const parameterNames = placeholderNames(source)
  const parameterObject =
    parameters !== null && typeof parameters === 'object' ? parameters : Object.create(null)

  for (const name of parameterNames) {
    if (!Object.hasOwn(parameterObject, name)) {
      throw new TypeError(\`missing message parameter: \${intentId}/\${name}\`)
    }
  }

  const expected = new Set(parameterNames)
  for (const name of Object.keys(parameterObject)) {
    if (!expected.has(name)) {
      throw new TypeError(\`unexpected message parameter: \${intentId}/\${name}\`)
    }
  }

  for (const name of parameterNames) {
    const parameter = parameterObject[name]
    if (
      typeof parameter !== 'string' &&
      (typeof parameter !== 'number' || !Number.isFinite(parameter))
    ) {
      throw new TypeError(
        \`invalid message parameter: \${intentId}/\${name} must be a string or finite number\`
      )
    }
  }

  return value.replace(PLACEHOLDER_PATTERN, (_placeholder, name) => String(parameterObject[name]))
}

function placeholderNames(message) {
  const names = []
  const seen = new Set()

  for (const match of message.matchAll(PLACEHOLDER_PATTERN)) {
    const name = match[1]
    if (!seen.has(name)) {
      seen.add(name)
      names.push(name)
    }
  }

  return names
}
`

/**
 * Emit all deployable Locale Compiler artifacts in their canonical order.
 *
 * @param options - Transformed source, validated intents and candidates, and Provider metadata.
 * @returns A filename-to-UTF-8-text map in write order.
 */
export function emitArtifacts({ transformedSource, intents, candidates, provider }) {
  const sourceMessages = intents.map(intent => ({ id: intent.id, message: intent.sourceText }))
  const targetMessages = candidates
    .filter(candidate => candidate.locale === TARGET_LOCALE)
    .map(candidate => ({ id: candidate.intentId, message: candidate.message }))

  const report = {
    schemaVersion: 1,
    sourceLocale: SOURCE_LOCALE,
    targetLocales: [TARGET_LOCALE],
    validationLevel: 'poc-contract-only',
    intentCount: intents.length,
    messageCountByLocale: {
      en: sourceMessages.length,
      ja: targetMessages.length
    },
    provider: {
      kind: provider.kind,
      revision: provider.revision
    }
  }

  return new Map([
    ['app.js', transformedSource],
    ['intlify-runtime.mjs', RUNTIME_SOURCE],
    ['locale-en.mjs', emitLocaleModule(SOURCE_LOCALE, sourceMessages)],
    ['locale-ja.mjs', emitLocaleModule(TARGET_LOCALE, targetMessages)],
    ['localization-report.json', `${JSON.stringify(report, null, 2)}\n`]
  ])
}

function emitLocaleModule(locale, entries) {
  const messages = Object.fromEntries(
    [...entries]
      .sort((left, right) => compareStrings(left.id, right.id))
      .map(entry => [entry.id, entry.message])
  )
  return `export const locale = ${JSON.stringify(locale)}\n\nexport const messages = ${JSON.stringify(messages, null, 2)}\n`
}

function compareStrings(left, right) {
  if (left < right) {
    return -1
  }
  if (left > right) {
    return 1
  }
  return 0
}
