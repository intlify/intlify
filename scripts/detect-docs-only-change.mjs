import { execFileSync } from 'node:child_process'
import { appendFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { isDirectRun } from './lib/is-direct-run.mjs'

const defaultRootDir = fileURLToPath(new URL('..', import.meta.url))

/**
 * Design documents that tests read.
 *
 * A change to one of them can fail a test, so it runs every CI job, as a change
 * to code does. `docs-only-change.test.mjs` checks this set against the design
 * documents that code names by path.
 */
export const testedDesignDocuments = new Set([
  // `changelog_documents_v01` in crates/ox_mf2_parser/tests/snapshot_compat.rs
  'design/003-ox-mf2-binary-ast-format-changelog.md'
])

/**
 * Check whether a change touches design documents alone.
 *
 * A design document is a Markdown file under `design/` or a file under
 * `design/assets/`, such as a figure. A change to design documents alone needs
 * the formatting check and none of the builds or tests. An empty change is not
 * one.
 *
 * @param paths - Repository-relative paths that the change adds, modifies, or deletes.
 * @returns Whether every path is a design document that no test reads.
 */
export function isDocsOnlyChange(paths) {
  return paths.length > 0 && paths.every(isUntestedDesignDocument)
}

/**
 * List the paths that a change adds, modifies, or deletes.
 *
 * The change runs from the merge base of the two commits to the head commit,
 * the comparison GitHub's path filters make for a pull request. A rename lists
 * both of its paths, so moving code under `design/` still changes code.
 *
 * @param base - Commit the change starts from.
 * @param head - Commit the change ends at.
 * @param cwd - Directory of the repository.
 * @returns Repository-relative paths, in Git's order.
 */
export function listChangedPaths(base, head, cwd) {
  const output = execFileSync(
    'git',
    ['diff', '--name-only', '--no-renames', '-z', `${base}...${head}`, '--'],
    { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }
  )
  return output.split('\0').filter(path => path !== '')
}

/**
 * Decide whether CI runs the design document lint alone for a change.
 *
 * Any doubt runs every job: no base commit (a manual run, or a push that
 * creates a branch), or two commits that Git cannot compare.
 *
 * @param base - Commit the change starts from; empty or all zeros when there is none.
 * @param head - Commit the change ends at.
 * @param cwd - Directory of the repository.
 * @param log - Receives each line that explains the decision.
 * @returns Whether the change touches design documents that no test reads, and nothing else.
 */
export function detectDocsOnlyChange(base, head, cwd, log = console.log) {
  if (!isCommitId(base)) {
    log('There is no base commit to compare with, so every job runs.')
    return false
  }

  let paths
  try {
    paths = listChangedPaths(base, head, cwd)
  } catch (error) {
    log(`Git cannot compare ${base}...${head}, so every job runs.`)
    log(String(error?.message ?? error))
    return false
  }

  log(`${base}...${head} changes ${paths.length} path(s):`)
  for (const path of paths) {
    log(`  ${path}`)
  }
  const docsOnly = isDocsOnlyChange(paths)
  log(
    docsOnly
      ? 'Only design documents change, so the design document lint runs alone.'
      : 'Every job runs.'
  )
  return docsOnly
}

if (isDirectRun(import.meta.url)) {
  const docsOnly = detectDocsOnlyChange(
    process.env.BASE_SHA ?? '',
    process.env.HEAD_SHA ?? '',
    defaultRootDir
  )
  const output = `docs_only=${docsOnly}\n`
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(process.env.GITHUB_OUTPUT, output)
  } else {
    process.stdout.write(output)
  }
}

function isUntestedDesignDocument(path) {
  return isDesignDocument(path) && !testedDesignDocuments.has(path)
}

function isDesignDocument(path) {
  return path.startsWith('design/assets/') || (path.startsWith('design/') && path.endsWith('.md'))
}

function isCommitId(value) {
  return /^[0-9a-f]{40}$/.test(value) && !/^0+$/.test(value)
}
