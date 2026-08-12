/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { relative } from 'node:path'
import { pathToFileURL } from 'node:url'

const TARGET_LOCALE = 'ja'

/**
 * Load a Localization Provider and obtain validated target-locale candidates.
 *
 * @param options - Provider path, compiler working directory, and extracted intents.
 * @returns Provider metadata and candidates, or one blocking diagnostic.
 */
export async function runLocalizationProvider({ providerPath, cwd, intents }) {
  const diagnosticPath = normalizePath(relative(cwd, providerPath))
  let providerModule

  try {
    providerModule = await import(pathToFileURL(providerPath).href)
  } catch {
    return providerFailure(
      'LC003',
      'invalid_localization_provider',
      'The provider module could not be loaded.',
      diagnosticPath
    )
  }

  if (!isValidProviderModule(providerModule)) {
    return providerFailure(
      'LC003',
      'invalid_localization_provider',
      'The provider must export kind, revision, and localize.',
      diagnosticPath
    )
  }

  const provider = {
    kind: providerModule.kind,
    revision: providerModule.revision
  }
  const requests = createLocalizationRequests(intents)

  if (requests.length === 0) {
    return { ok: true, provider, requests, candidates: [] }
  }

  const providerRequests = requests.map(request => ({ ...request }))
  let result

  try {
    result = await providerModule.localize(providerRequests)
  } catch {
    return providerFailure(
      'LC007',
      'localization_provider_failed',
      'The provider failed while localizing messages.',
      diagnosticPath
    )
  }

  const validated = validateLocalizationResult(requests, result, diagnosticPath)
  if (!validated.ok) {
    return validated
  }

  return {
    ok: true,
    provider,
    requests,
    candidates: validated.candidates
  }
}

function isValidProviderModule(providerModule) {
  return (
    typeof providerModule.kind === 'string' &&
    providerModule.kind.trim().length > 0 &&
    typeof providerModule.revision === 'string' &&
    providerModule.revision.trim().length > 0 &&
    typeof providerModule.localize === 'function'
  )
}

function createLocalizationRequests(intents) {
  return intents
    .map(intent => ({
      intentId: intent.id,
      sourceLocale: intent.sourceLocale,
      targetLocale: TARGET_LOCALE,
      sourceText: intent.sourceText,
      surface: intent.surface
    }))
    .sort(compareRequests)
}

function validateLocalizationResult(requests, result, diagnosticPath) {
  if (!Array.isArray(result)) {
    return invalidLocalizationResult(diagnosticPath)
  }

  const expectedKeys = new Set(
    requests.map(request => requestKey(request.targetLocale, request.intentId))
  )
  const seenKeys = new Set()
  const candidates = []

  for (const candidate of result) {
    if (
      candidate === null ||
      typeof candidate !== 'object' ||
      typeof candidate.intentId !== 'string' ||
      typeof candidate.locale !== 'string' ||
      typeof candidate.message !== 'string' ||
      candidate.message.trim().length === 0
    ) {
      return invalidLocalizationResult(diagnosticPath)
    }

    const key = requestKey(candidate.locale, candidate.intentId)
    if (!expectedKeys.has(key) || seenKeys.has(key)) {
      return invalidLocalizationResult(diagnosticPath)
    }

    seenKeys.add(key)
    candidates.push({
      intentId: candidate.intentId,
      locale: candidate.locale,
      message: candidate.message
    })
  }

  if (seenKeys.size !== expectedKeys.size) {
    return invalidLocalizationResult(diagnosticPath)
  }

  candidates.sort(compareCandidates)
  return { ok: true, candidates }
}

function invalidLocalizationResult(path) {
  return providerFailure(
    'LC004',
    'invalid_localization_result',
    'The provider result must contain exactly one non-empty message for every requested intent and locale.',
    path
  )
}

function providerFailure(code, name, message, path) {
  return {
    ok: false,
    diagnostics: [{ severity: 'error', code, name, message, path }]
  }
}

function requestKey(locale, intentId) {
  return JSON.stringify([locale, intentId])
}

function compareRequests(left, right) {
  return (
    left.targetLocale.localeCompare(right.targetLocale) ||
    left.intentId.localeCompare(right.intentId)
  )
}

function compareCandidates(left, right) {
  return left.locale.localeCompare(right.locale) || left.intentId.localeCompare(right.intentId)
}

function normalizePath(path) {
  return path.replaceAll('\\', '/')
}
