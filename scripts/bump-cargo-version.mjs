import { readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

import { isDirectRun } from './lib/is-direct-run.mjs'
import { releaseCargoLockPackages, releaseCargoTomlFiles } from './lib/release-crates.mjs'

const rootDir = fileURLToPath(new URL('..', import.meta.url))

/**
 * Propagate the selected release version into the publish-related Cargo files.
 *
 * The function is intentionally independent from package.json so bumpp can pass
 * its selected version before it creates the release commit and tag.
 *
 * @param nextVersion - Version selected by bumpp.
 * @param options - Optional workspace root used by tests.
 */
export async function bumpCargoVersion(nextVersion, options = {}) {
  const workspaceRoot = options.rootDir ?? rootDir
  for (const relativePath of releaseCargoTomlFiles) {
    await replacePackageVersion(workspaceRoot, relativePath, nextVersion)
  }

  await replaceHtmlRootUrl(
    workspaceRoot,
    'crates/ox_mf2_parser/src/lib.rs',
    'ox_mf2_parser',
    nextVersion
  )
  await replaceCargoLockVersions(workspaceRoot, 'Cargo.lock', releaseCargoLockPackages, nextVersion)
}

if (isDirectRun(import.meta.url)) {
  const version = JSON.parse(
    await readFile(new URL('../package.json', import.meta.url), 'utf8')
  ).version
  await bumpCargoVersion(version)
}

async function replacePackageVersion(workspaceRoot, relativePath, nextVersion) {
  const file = pathToFileURL(join(workspaceRoot, relativePath))
  const source = await readFile(file, 'utf8')
  let matched = false
  const updated = source.replace(
    /(\[package\]\n(?:[^\n]*\n)*?version\s*=\s*")([^"]+)(")/,
    (_, prefix, _currentVersion, suffix) => {
      matched = true
      return `${prefix}${nextVersion}${suffix}`
    }
  )
  if (!matched) {
    throw new Error(`failed to update version in ${relativePath}`)
  }
  await writeFile(file, updated)
}

async function replaceCargoLockVersions(workspaceRoot, relativePath, packageNames, nextVersion) {
  const file = pathToFileURL(join(workspaceRoot, relativePath))
  const source = await readFile(file, 'utf8')
  const seen = new Set()
  const blocks = source.split('\n[[package]]\n')
  const updatedBlocks = blocks.map((block, index) => {
    const fullBlock = index === 0 ? block : `[[package]]\n${block}`
    const name = block.match(/^name = "([^"]+)"$/m)?.[1]
    if (!name || !packageNames.includes(name)) {
      return fullBlock
    }
    seen.add(name)
    return fullBlock.replace(/^version = "[^"]+"$/m, `version = "${nextVersion}"`)
  })
  const updated = updatedBlocks.join('\n')
  const missing = packageNames.filter(packageName => !seen.has(packageName))
  if (missing.length > 0) {
    throw new Error(`failed to update ${missing.join(', ')} in ${relativePath}`)
  }
  await writeFile(file, updated)
}

async function replaceHtmlRootUrl(workspaceRoot, relativePath, crateName, nextVersion) {
  const file = pathToFileURL(join(workspaceRoot, relativePath))
  const source = await readFile(file, 'utf8')
  const expected = `https://docs.rs/${crateName}/${nextVersion}`
  let matched = false
  const updated = source.replace(
    /(#!\[doc\(html_root_url\s*=\s*")([^"]+)("\)\])/,
    (_, prefix, _currentUrl, suffix) => {
      matched = true
      return `${prefix}${expected}${suffix}`
    }
  )
  if (!matched) {
    throw new Error(`failed to update html_root_url in ${relativePath}`)
  }
  await writeFile(file, updated)
}
