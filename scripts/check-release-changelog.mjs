import { readFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { isDirectRun } from './lib/is-direct-run.mjs'

const defaultRootDir = fileURLToPath(new URL('..', import.meta.url))

/**
 * Validate the release entry that is already part of a tagged source tree.
 *
 * This check deliberately does not call GitHub. The tag's source tree must be
 * self-contained before any package publication starts.
 *
 * @param source - Complete CHANGELOG.md text.
 * @param tag - Release tag that must be represented by the top entry.
 */
export function validateReleaseChangelog(source, tag) {
  assertValidTag(tag)

  const lines = source.split(/\r?\n/)
  const firstContentIndex = lines.findIndex(line => line.trim() !== '')
  const firstContent = firstContentIndex === -1 ? undefined : lines[firstContentIndex]
  const expectedHeading = new RegExp(
    `^# ${escapeRegExp(tag)} \\(\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}(?:\\.\\d{3})?Z\\)$`
  )
  if (!firstContent || !expectedHeading.test(firstContent)) {
    throw new Error(
      `CHANGELOG.md must start with "# ${tag} (<ISO-8601 UTC timestamp>)" (observed: ${firstContent ?? '<empty>'})`
    )
  }

  const exactTagHeadingPattern = new RegExp(`^# ${escapeRegExp(tag)}(?:\\s.*)?$`, 'gm')
  const exactTagHeadings = [...source.matchAll(exactTagHeadingPattern)]
  if (exactTagHeadings.length !== 1) {
    throw new Error(
      `CHANGELOG.md must contain exactly one heading for ${tag} (observed: ${exactTagHeadings.length})`
    )
  }

  const releaseUrl = `https://github.com/intlify/intlify/releases/tag/${tag}`
  const releaseUrlPattern = new RegExp(`${escapeRegExp(releaseUrl)}(?=[)\\s]|$)`, 'g')
  const releaseUrlCount = [...source.matchAll(releaseUrlPattern)].length
  const nextHeadingIndex = lines.findIndex(
    (line, index) => index > firstContentIndex && line.startsWith('# ')
  )
  const firstEntry = lines
    .slice(firstContentIndex, nextHeadingIndex === -1 ? lines.length : nextHeadingIndex)
    .join('\n')
  const firstEntryReleaseUrlCount = [...firstEntry.matchAll(releaseUrlPattern)].length
  if (releaseUrlCount !== 1 || firstEntryReleaseUrlCount !== 1) {
    throw new Error(
      `CHANGELOG.md must contain exactly one Release URL for ${tag} in its first entry (observed: total=${releaseUrlCount}, first-entry=${firstEntryReleaseUrlCount})`
    )
  }
}

if (isDirectRun(import.meta.url)) {
  const rootDir = process.env.INTLIFY_RELEASE_ROOT
    ? resolve(process.env.INTLIFY_RELEASE_ROOT)
    : defaultRootDir
  const tag = firstNonEmpty(process.argv[2], process.env.TAG, process.env.GITHUB_REF_NAME)

  if (!tag) {
    throw new Error('release tag is required')
  }

  const source = await readFile(join(rootDir, 'CHANGELOG.md'), 'utf8')
  validateReleaseChangelog(source, tag)
}

function assertValidTag(tag) {
  if (!isStrictReleaseTag(tag)) {
    throw new Error(`release tag must contain a strict semver version: ${tag}`)
  }
}

function isStrictReleaseTag(tag) {
  if (typeof tag !== 'string' || !tag.startsWith('v')) {
    return false
  }

  let version = tag.slice(1)
  const buildSeparator = version.indexOf('+')
  if (buildSeparator !== -1) {
    const build = version.slice(buildSeparator + 1)
    if (!build || !build.split('.').every(isAsciiIdentifier)) {
      return false
    }
    version = version.slice(0, buildSeparator)
  }

  let prerelease
  const prereleaseSeparator = version.indexOf('-')
  if (prereleaseSeparator !== -1) {
    prerelease = version.slice(prereleaseSeparator + 1)
    if (
      !prerelease ||
      !prerelease.split('.').every(identifier => {
        return (
          isAsciiIdentifier(identifier) &&
          (isStrictNumericIdentifier(identifier) || /[A-Z-]/i.test(identifier))
        )
      })
    ) {
      return false
    }
    version = version.slice(0, prereleaseSeparator)
  }

  const core = version.split('.')
  return core.length === 3 && core.every(isStrictNumericIdentifier)
}

function isStrictNumericIdentifier(identifier) {
  return /^(?:0|[1-9]\d*)$/.test(identifier)
}

function isAsciiIdentifier(identifier) {
  return /^[0-9A-Z-]+$/i.test(identifier)
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function firstNonEmpty(...values) {
  return values.find(value => value != null && value !== '')
}
