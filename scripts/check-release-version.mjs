import { readFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const rootDir = process.env.INTLIFY_RELEASE_ROOT
  ? resolve(process.env.INTLIFY_RELEASE_ROOT)
  : fileURLToPath(new URL('..', import.meta.url))
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
  'packages/ox-mf2-shared/package.json',
  'packages/cli/package.json',
  'packages/cli-native/package.json'
]

const cargoTomlFiles = [
  'crates/ox_mf2_parser/Cargo.toml',
  'crates/ox_mf2_napi/Cargo.toml',
  'crates/ox_mf2_wasm/Cargo.toml',
  'crates/intlify_cli/Cargo.toml'
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
await assertCargoLockVersions(
  ['ox_mf2_parser', 'ox_mf2_napi', 'ox_mf2_wasm', 'intlify_cli'],
  expectedVersion
)
await assertPublicPackageMetadata('packages/ox-mf2-napi/package.json')
await assertPublicPackageMetadata('packages/ox-mf2-wasm/package.json')
await assertPublicPackageMetadata('packages/cli/package.json')
await assertPublicPackageMetadata('packages/cli-native/package.json')
await assertCliPackageConsistency(expectedVersion)
await assertInternalCliCrate()
await assertReleaseBumpTargets()

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

async function assertCliPackageConsistency(expected) {
  const cliPackage = await readJson('packages/cli/package.json')
  const nativePackage = await readJson('packages/cli-native/package.json')

  assertSemverAtLeast('packages/cli/package.json version', cliPackage.version, '0.14.0')
  assertEqual(
    'packages/cli/package.json dependencies.@intlify/cli-native',
    cliPackage.dependencies?.['@intlify/cli-native'],
    'workspace:*'
  )
  assertEqual('packages/cli-native/package.json version', nativePackage.version, cliPackage.version)
  assertEqual('CLI release version', cliPackage.version, expected)
}

async function assertInternalCliCrate() {
  const source = await readText('crates/intlify_cli/Cargo.toml')
  if (!/^publish\s*=\s*false$/m.test(source)) {
    throw new Error('crates/intlify_cli/Cargo.toml must set publish = false')
  }
}

async function assertReleaseBumpTargets() {
  const pkg = await readJson('package.json')
  const releaseScript = pkg.scripts?.release ?? ''
  for (const relativePath of ['packages/cli/package.json', 'packages/cli-native/package.json']) {
    if (!releaseScript.includes(`"${relativePath}"`)) {
      throw new Error(`package.json release script must bump ${relativePath}`)
    }
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
  return readFile(join(rootDir, relativePath), 'utf8')
}

function assertEqual(label, actual, expected) {
  if (actual !== expected) {
    throw new Error(`${label} must be ${expected}, got ${String(actual)}`)
  }
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

function assertSemverAtLeast(label, actual, minimum) {
  if (compareSemver(actual, minimum) < 0) {
    throw new Error(`${label} must be at least ${minimum}, got ${actual}`)
  }
}

function compareSemver(left, right) {
  const leftParts = semverCore(left)
  const rightParts = semverCore(right)
  for (let index = 0; index < leftParts.length; index++) {
    if (leftParts[index] !== rightParts[index]) {
      return leftParts[index] - rightParts[index]
    }
  }
  return 0
}

function semverCore(version) {
  const match = version.match(/^(\d+)\.(\d+)\.(\d+)/)
  if (!match) {
    throw new Error(`invalid semver version: ${version}`)
  }
  return match.slice(1).map(Number)
}
