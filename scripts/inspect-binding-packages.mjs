import { spawnSync } from 'node:child_process'
import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

const rootDir = fileURLToPath(new URL('..', import.meta.url))

const optionalNapiPackages = [
  '@intlify/ox-mf2-napi-darwin-arm64',
  '@intlify/ox-mf2-napi-darwin-x64',
  '@intlify/ox-mf2-napi-linux-x64-gnu',
  '@intlify/ox-mf2-napi-linux-x64-musl',
  '@intlify/ox-mf2-napi-linux-arm64-gnu',
  '@intlify/ox-mf2-napi-win32-x64-msvc'
]

const optionalFormatNapiPackages = [
  '@intlify/format-napi-darwin-arm64',
  '@intlify/format-napi-darwin-x64',
  '@intlify/format-napi-linux-x64-gnu',
  '@intlify/format-napi-linux-x64-musl',
  '@intlify/format-napi-linux-arm64-gnu',
  '@intlify/format-napi-win32-x64-msvc'
]

await inspectPackage({
  packagePath: 'packages/ox-mf2-napi',
  requiredFiles: [
    'dist/index.js',
    'dist/index.d.ts',
    'dist/native-binding.js',
    'dist/native-binding.d.ts'
  ],
  extraChecks({ files, pkg }) {
    const hasLocalNative = [...files].some(
      file => file.startsWith('dist/') && file.endsWith('.node')
    )
    const optionalDependencies = pkg.optionalDependencies ?? {}
    const missingOptionalPackages = optionalNapiPackages.filter(
      packageName => optionalDependencies[packageName] !== pkg.version
    )
    if (!hasLocalNative && missingOptionalPackages.length > 0) {
      throw new Error(
        `${pkg.name} must include a local .node file or optionalDependencies for all N-API platform packages; missing ${missingOptionalPackages.join(', ')}`
      )
    }
  }
})

await inspectPackage({
  packagePath: 'packages/ox-mf2-wasm',
  requiredFiles: [
    'dist/index.js',
    'dist/index.d.ts',
    'dist/ox_mf2_wasm.js',
    'dist/ox_mf2_wasm_bg.wasm'
  ]
})

await inspectPackage({
  packagePath: 'packages/format-napi',
  requiredFiles: [
    'dist/index.js',
    'dist/index.d.ts',
    'dist/native-binding.js',
    'dist/native-binding.d.ts'
  ],
  extraChecks({ files, pkg }) {
    const hasLocalNative = [...files].some(
      file => file.startsWith('dist/') && file.endsWith('.node')
    )
    const optionalDependencies = pkg.optionalDependencies ?? {}
    const missingOptionalPackages = optionalFormatNapiPackages.filter(
      packageName => optionalDependencies[packageName] !== pkg.version
    )
    if (!hasLocalNative && missingOptionalPackages.length > 0) {
      throw new Error(
        `${pkg.name} must include a local .node file or optionalDependencies for all formatter N-API platform packages; missing ${missingOptionalPackages.join(', ')}`
      )
    }
  }
})

await inspectPackage({
  packagePath: 'packages/format-wasm',
  requiredFiles: [
    'dist/index.js',
    'dist/index.d.ts',
    'dist/intlify_format_wasm.js',
    'dist/intlify_format_wasm_bg.wasm'
  ]
})

async function inspectPackage({ packagePath, requiredFiles, extraChecks }) {
  const pkg = await readJson(`${packagePath}/package.json`)
  if (pkg.private === true) {
    throw new Error(`${pkg.name} must not be private`)
  }
  if (pkg.publishConfig?.access !== 'public') {
    throw new Error(`${pkg.name} must set publishConfig.access to public`)
  }

  assertNoPublishWorkspaceDependencies(pkg)
  assertNoSharedRuntimeDependency(pkg)

  const files = await packFiles(packagePath)
  for (const file of requiredFiles) {
    if (!files.has(file)) {
      throw new Error(`${pkg.name} package is missing ${file}`)
    }
  }
  await assertNoPackedSharedRuntimeImport(packagePath, files, pkg.name)

  extraChecks?.({ files, pkg })
  console.log(`${pkg.name} package contents ok`)
}

async function packFiles(packagePath) {
  const result = spawnSync('npm', ['pack', '--dry-run', '--json'], {
    cwd: new URL(packagePath, `file://${rootDir}/`),
    encoding: 'utf8'
  })
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || `npm pack failed in ${packagePath}`)
  }
  const [pack] = JSON.parse(result.stdout)
  return new Set(pack.files.map(file => file.path))
}

async function readJson(relativePath) {
  return JSON.parse(await readFile(new URL(relativePath, `file://${rootDir}/`), 'utf8'))
}

function assertNoPublishWorkspaceDependencies(pkg) {
  for (const field of ['dependencies', 'optionalDependencies', 'peerDependencies']) {
    for (const [name, specifier] of Object.entries(pkg[field] ?? {})) {
      if (typeof specifier === 'string' && specifier.startsWith('workspace:')) {
        throw new Error(`${pkg.name} ${field}.${name} must not use ${specifier}`)
      }
    }
  }
}

function assertNoSharedRuntimeDependency(pkg) {
  for (const field of ['dependencies', 'optionalDependencies', 'peerDependencies']) {
    if (pkg[field]?.['@intlify/ox-mf2-shared']) {
      throw new Error(`${pkg.name} must not publish a runtime dependency on @intlify/ox-mf2-shared`)
    }
  }
}

async function assertNoPackedSharedRuntimeImport(packagePath, files, packageName) {
  for (const file of files) {
    if (!file.startsWith('dist/') || !file.endsWith('.js')) {
      continue
    }

    const content = await readFile(new URL(`${packagePath}/${file}`, `file://${rootDir}/`), 'utf8')
    if (content.includes('@intlify/ox-mf2-shared')) {
      throw new Error(`${packageName} package must bundle @intlify/ox-mf2-shared in ${file}`)
    }
  }
}
