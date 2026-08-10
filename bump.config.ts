import { defineConfig } from 'bumpp'
import { updateChangelog } from 'gh-changelogen'

import { bumpCargoVersion } from './scripts/bump-cargo-version.mjs'

/**
 * Dependencies used by the local release preparation hook.
 */
interface ReleasePreparationDependencies {
  /** Propagates the selected version into Cargo metadata. */
  bumpCargoVersion?: typeof bumpCargoVersion
  /** Writes generated notes for the future release tag. */
  updateChangelog?: typeof updateChangelog
}

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
}

export default defineConfig({
  all: true,
  commit: 'release: v%s',
  push: true,
  tag: true,
  execute: async operation => {
    await prepareRelease(operation.state.newVersion)
  }
})
