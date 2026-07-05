import { readFile, writeFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

import { releaseCargoLockPackages, releaseCargoTomlFiles } from './lib/release-crates.mjs'

const rootDir = fileURLToPath(new URL('..', import.meta.url))
const version = JSON.parse(
  await readFile(new URL('../package.json', import.meta.url), 'utf8')
).version

for (const relativePath of releaseCargoTomlFiles) {
  await replacePackageVersion(relativePath, version)
}

await replaceHtmlRootUrl('crates/ox_mf2_parser/src/lib.rs', 'ox_mf2_parser', version)
await replaceCargoLockVersions('Cargo.lock', releaseCargoLockPackages, version)

async function replacePackageVersion(relativePath, nextVersion) {
  const file = new URL(relativePath, `file://${rootDir}/`)
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

async function replaceCargoLockVersions(relativePath, packageNames, nextVersion) {
  const file = new URL(relativePath, `file://${rootDir}/`)
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

async function replaceHtmlRootUrl(relativePath, crateName, nextVersion) {
  const file = new URL(relativePath, `file://${rootDir}/`)
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
