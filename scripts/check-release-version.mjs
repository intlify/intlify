import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

const rootDir = fileURLToPath(new URL('..', import.meta.url))
const tag = process.argv[2] ?? process.env.TAG ?? process.env.GITHUB_REF_NAME

if (!tag) {
  throw new Error('release tag is required')
}
if (!tag.startsWith('v')) {
  throw new Error(`release tag must start with "v": ${tag}`)
}

const expectedVersion = tag.slice(1)
if (!expectedVersion) {
  throw new Error(`release tag does not contain a version: ${tag}`)
}

const packageFiles = [
  'package.json',
  'packages/ox-mf2-napi/package.json',
  'packages/ox-mf2-wasm/package.json',
  'packages/ox-mf2-shared/package.json'
]

const cargoTomlFiles = [
  'crates/ox_mf2_parser/Cargo.toml',
  'crates/ox_mf2_napi/Cargo.toml',
  'crates/ox_mf2_wasm/Cargo.toml'
]

for (const relativePath of packageFiles) {
  const pkg = await readJson(relativePath)
  assertEqual(`${relativePath} version`, pkg.version, expectedVersion)
}

for (const relativePath of cargoTomlFiles) {
  const source = await readText(relativePath)
  const version = source.match(/^version = "([^"]+)"/m)?.[1]
  assertEqual(`${relativePath} version`, version, expectedVersion)
}

await assertHtmlRootUrl('crates/ox_mf2_parser/src/lib.rs', 'ox_mf2_parser', expectedVersion)
await assertCargoLockVersions(['ox_mf2_parser', 'ox_mf2_napi', 'ox_mf2_wasm'], expectedVersion)
await assertPublicPackageMetadata('packages/ox-mf2-napi/package.json')
await assertPublicPackageMetadata('packages/ox-mf2-wasm/package.json')

async function assertPublicPackageMetadata(relativePath) {
  const pkg = await readJson(relativePath)
  if (pkg.private === true) {
    throw new Error(`${relativePath} must not be private for npm publish`)
  }
  assertEqual(`${relativePath} publishConfig.access`, pkg.publishConfig?.access, 'public')
  if (!pkg.repository?.url || !pkg.repository?.directory) {
    throw new Error(`${relativePath} must include repository.url and repository.directory`)
  }
  if (!pkg.bugs?.url) {
    throw new Error(`${relativePath} must include bugs.url`)
  }
  if (!pkg.homepage) {
    throw new Error(`${relativePath} must include homepage`)
  }
}

async function assertCargoLockVersions(packageNames, expected) {
  const source = await readText('Cargo.lock')
  for (const packageName of packageNames) {
    const block = source.match(
      new RegExp(
        `\\[\\[package\\]\\]\\nname = "${escapeRegExp(packageName)}"\\nversion = "([^"]+)"`
      )
    )
    assertEqual(`Cargo.lock ${packageName} version`, block?.[1], expected)
  }
}

async function assertHtmlRootUrl(relativePath, crateName, expected) {
  const source = await readText(relativePath)
  const actual = source.match(/#!\[doc\(html_root_url\s*=\s*"([^"]+)"\)\]/)?.[1]
  assertEqual(`${relativePath} html_root_url`, actual, `https://docs.rs/${crateName}/${expected}`)
}

async function readJson(relativePath) {
  return JSON.parse(await readText(relativePath))
}

async function readText(relativePath) {
  return readFile(new URL(relativePath, `file://${rootDir}/`), 'utf8')
}

function assertEqual(label, actual, expected) {
  if (actual !== expected) {
    throw new Error(`${label} must be ${expected}, got ${String(actual)}`)
  }
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}
