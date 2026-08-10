import { readFile } from 'node:fs/promises'

import { defineConfig } from 'bumpp'
import { updateChangelog } from 'gh-changelogen'

import { bumpCargoVersion } from './scripts/bump-cargo-version.mjs'
import { validateReleaseChangelog } from './scripts/check-release-changelog.mjs'

/**
 * Dependencies used by the local release preparation hook.
 */
interface ReleasePreparationDependencies {
  /** Propagates the selected version into Cargo metadata. */
  bumpCargoVersion?: typeof bumpCargoVersion
  /** Writes generated notes for the future release tag. */
  updateChangelog?: typeof updateChangelog
  /** Reads the generated changelog before release metadata is committed. */
  readChangelog?: (path: string) => Promise<string>
  /** Checks that generated notes satisfy the tagged changelog contract. */
  validateChangelog?: typeof validateReleaseChangelog
}

interface ReleaseOperation {
  state: {
    newVersion: string
  }
}

type ReleasePreparation = (nextVersion: string) => Promise<void>

/**
 * Prepare all non-package release metadata before bumpp commits and tags it.
 *
 * @param nextVersion - Version selected by bumpp.
 * @param dependencies - Optional test doubles for the two preparation steps.
 */
export async function prepareRelease(
  nextVersion: string,
  dependencies: ReleasePreparationDependencies = {}
) {
  await (dependencies.bumpCargoVersion ?? bumpCargoVersion)(nextVersion)
  await (dependencies.updateChangelog ?? updateChangelog)({
    repository: 'intlify/intlify',
    tagName: `v${nextVersion}`,
    source: 'generated-notes',
    targetCommitish: 'HEAD',
    output: 'CHANGELOG.md'
  })

  const readChangelog = dependencies.readChangelog ?? (path => readFile(path, 'utf8'))
  const changelog = await readChangelog('CHANGELOG.md')
  ;(dependencies.validateChangelog ?? validateReleaseChangelog)(changelog, `v${nextVersion}`)
}

/**
 * Adapt bumpp's operation state to the release preparation function.
 *
 * @param operation - Operation state supplied by bumpp.
 * @param prepare - Release preparation function, replaceable in tests.
 */
export async function executeRelease(
  operation: ReleaseOperation,
  prepare: ReleasePreparation = prepareRelease
) {
  await prepare(operation.state.newVersion)
}

export default defineConfig({
  all: true,
  commit: 'release: v%s',
  push: true,
  tag: true,
  execute: async operation => {
    await executeRelease(operation)
  }
})
