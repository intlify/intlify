import { execFileSync } from 'node:child_process'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { expect, test } from 'vite-plus/test'

import {
  detectDocsOnlyChange,
  isDocsOnlyChange,
  listChangedPaths,
  testedDesignDocuments
} from './detect-docs-only-change.mjs'

const rootDir = fileURLToPath(new URL('..', import.meta.url))
const overview = 'design/000-intlify-overview-design.md'
const figure = 'design/assets/000-intlify-architecture.svg'
const ignoreLog = () => {}

// Files that could read a design document. The two files of this check name
// design documents as data, so the scan leaves them out.
const scannedFile = /\.(?:rs|[cm]?[jt]sx?|vue|json|toml|ya?ml|sh|py)$/
const unscannedFiles = new Set([
  'scripts/detect-docs-only-change.mjs',
  'scripts/docs-only-change.test.mjs'
])
// A string literal that is a path to a design document, relative or not.
const designDocumentPath =
  /(["'])(?:\.{1,2}\/|\/)*(?<path>design\/(?:assets\/[\w./-]+|[\w./-]+\.md))\1/g

// Git in a scratch repository must not reach the repository that runs the tests.
const scratchGitEnv = Object.fromEntries(
  Object.entries(process.env).filter(([name]) => !name.startsWith('GIT_'))
)

test('needs only the lint for design documents', () => {
  expect(isDocsOnlyChange([overview])).toBe(true)
  expect(isDocsOnlyChange([overview, 'design/drafts/notes.md'])).toBe(true)
  expect(isDocsOnlyChange([overview, figure])).toBe(true)
  expect(isDocsOnlyChange([figure])).toBe(true)
})

test('needs every job once any other path changes', () => {
  for (const path of [
    'README.md',
    'crates/intlify_config/README.md',
    'design/schema.json',
    'designs/notes.md',
    'designs/assets/figure.svg',
    '.github/workflows/ci.yml',
    'crates/intlify_config/src/lib.rs'
  ]) {
    expect(isDocsOnlyChange([overview, path]), path).toBe(false)
  }
})

test('needs every job for a design document that a test reads', () => {
  for (const path of testedDesignDocuments) {
    expect(isDocsOnlyChange([path]), path).toBe(false)
  }
})

test('needs every job for an empty change', () => {
  expect(isDocsOnlyChange([])).toBe(false)
})

test('knows every design document that code names by path', async () => {
  const files = execFileSync('git', ['ls-files', '-z'], { cwd: rootDir, encoding: 'utf8' })
    .split('\0')
    .filter(file => scannedFile.test(file) && !/^(?:design|refers)\//.test(file))
    .filter(file => !unscannedFiles.has(file))
  const named = new Map()
  for (const file of files) {
    const source = await readFile(join(rootDir, file), 'utf8')
    for (const match of source.matchAll(designDocumentPath)) {
      named.set(match.groups.path, [...(named.get(match.groups.path) ?? []), file])
    }
  }

  const untested = [...named]
    .filter(([path]) => !testedDesignDocuments.has(path))
    .map(([path, readers]) => `${path} (named in ${readers.join(', ')})`)
  expect(untested, 'add these to testedDesignDocuments').toEqual([])
  const unnamed = [...testedDesignDocuments].filter(path => !named.has(path))
  expect(unnamed, 'no code names these by path').toEqual([])
})

test('compares from the merge base and lists both paths of a rename', async () => {
  await withScratchRepository(async repo => {
    await commit(repo, { 'crates/a.rs': 'fn a() {}\n', [overview]: '# Overview\n' })
    const base = git(repo, 'rev-parse', 'HEAD')

    git(repo, 'switch', '-q', '-c', 'docs')
    await commit(repo, { [overview]: '# Overview\n\nMore.\n' })
    const docs = git(repo, 'rev-parse', 'HEAD')

    // The base branch moves on with code after the branch point.
    git(repo, 'switch', '-q', 'main')
    await commit(repo, { 'crates/a.rs': 'fn a() { }\n' })
    const movedBase = git(repo, 'rev-parse', 'HEAD')

    expect(listChangedPaths(base, docs, repo)).toEqual([overview])
    expect(listChangedPaths(movedBase, docs, repo)).toEqual([overview])
    expect(detectDocsOnlyChange(movedBase, docs, repo, ignoreLog)).toBe(true)

    git(repo, 'switch', '-q', 'docs')
    git(repo, 'mv', 'crates/a.rs', 'design/a.md')
    await commit(repo, {})
    const renamed = git(repo, 'rev-parse', 'HEAD')

    expect(listChangedPaths(docs, renamed, repo)).toEqual(['crates/a.rs', 'design/a.md'])
    expect(detectDocsOnlyChange(docs, renamed, repo, ignoreLog)).toBe(false)
  })
})

test('runs every job without a base commit that Git can compare', async () => {
  await withScratchRepository(async repo => {
    await commit(repo, { [overview]: '# Overview\n' })
    const head = git(repo, 'rev-parse', 'HEAD')

    for (const [base, reason] of [
      ['', 'There is no base commit to compare with, so every job runs.'],
      ['0'.repeat(40), 'There is no base commit to compare with, so every job runs.'],
      ['f'.repeat(40), `Git cannot compare ${'f'.repeat(40)}...${head}, so every job runs.`]
    ]) {
      const lines = []
      expect(
        detectDocsOnlyChange(base, head, repo, line => lines.push(line)),
        base
      ).toBe(false)
      expect(lines[0], base).toBe(reason)
    }
  })
})

async function withScratchRepository(run) {
  const repo = await mkdtemp(join(tmpdir(), 'intlify-docs-only-'))
  try {
    git(repo, 'init', '-q', '-b', 'main')
    await run(repo)
  } finally {
    await rm(repo, { recursive: true, force: true })
  }
}

async function commit(repo, files) {
  for (const [path, content] of Object.entries(files)) {
    await mkdir(dirname(join(repo, path)), { recursive: true })
    await writeFile(join(repo, path), content)
  }
  git(repo, 'add', '-A')
  git(
    repo,
    '-c',
    'user.name=Intlify CI',
    '-c',
    'user.email=ci@example.invalid',
    '-c',
    'commit.gpgsign=false',
    'commit',
    '-q',
    '--no-verify',
    '-m',
    'change'
  )
}

function git(cwd, ...args) {
  return execFileSync('git', args, { cwd, encoding: 'utf8', env: scratchGitEnv }).trim()
}
