import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'vite-plus/test'

const packageRoot = fileURLToPath(new URL('..', import.meta.url))

test('package metadata describes the formatter native package', async () => {
  const packageJson = JSON.parse(await readFile(`${packageRoot}/package.json`, 'utf8')) as {
    name: string
    files: string[]
    scripts: Record<string, string>
    napi: { binaryName: string; packageName: string; targets: string[] }
    publishConfig: { access: string }
  }

  expect(packageJson.name).toBe('@intlify/format-napi')
  expect(packageJson.files).toEqual(['README.md', 'dist'])
  expect(packageJson.publishConfig.access).toBe('public')
  expect(packageJson.scripts.build).toContain('ox-mf2-shared#build')
  expect(packageJson.scripts.test).toContain('ox-mf2-shared#build')
  expect(packageJson.napi.binaryName).toBe('intlify_format_napi')
  expect(packageJson.napi.packageName).toBe('@intlify/format-napi')
  expect(packageJson.napi.targets).toEqual([
    'aarch64-apple-darwin',
    'x86_64-apple-darwin',
    'x86_64-unknown-linux-gnu',
    'x86_64-unknown-linux-musl',
    'aarch64-unknown-linux-gnu',
    'x86_64-pc-windows-msvc'
  ])
})
