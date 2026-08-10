import { readFile } from 'node:fs/promises'

import { beforeAll, expect, test } from 'vite-plus/test'

let workflow

beforeAll(async () => {
  workflow = await readFile(new URL('../.github/workflows/release.yml', import.meta.url), 'utf8')
})

test('validates the tagged changelog before publishing', () => {
  expect(workflow).toContain('run: node scripts/check-release-changelog.mjs')
  expect(workflow).toContain('git merge-base --is-ancestor HEAD origin/main')
})

test('creates the GitHub Release from the existing tag without mutating main', () => {
  const releaseJob = getReleaseJob(workflow)

  expect(releaseJob).toContain('The only write capability is GitHub Release creation')
  expect(releaseJob).toContain('ref: ${{ env.TAG }}')
  expect(releaseJob).toContain('persist-credentials: false')
  expect(releaseJob).toContain('--verify-tag')
  expect(releaseJob).toContain('--generate-notes')
  expect(releaseJob).toContain('release_args+=(--prerelease --latest=false)')
  expect(releaseJob).not.toContain('Sync changelog from GitHub releases')
  expect(releaseJob).not.toContain('git-auto-commit-action')
  expect(releaseJob).not.toContain('file_pattern: CHANGELOG.md')
  expect(releaseJob).not.toContain('branch: main')
  expect(releaseJob).not.toContain('vp exec gh-changelogen')
})

test('keeps the release-notes dispatch as a publish-free retry path', () => {
  const releaseJob = getReleaseJob(workflow)

  expect(releaseJob).toContain("inputs.job == 'release-notes'")
  expect(releaseJob).toContain('gh release view "$TAG"')
  expect(releaseJob).toContain('Release $TAG already exists, skipping creation')
})

function getReleaseJob(source) {
  const jobIndex = source.indexOf('  release-notes:')
  expect(jobIndex).toBeGreaterThan(-1)
  return source.slice(jobIndex)
}
