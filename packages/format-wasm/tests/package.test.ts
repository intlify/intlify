import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'vite-plus/test'

const packageRoot = fileURLToPath(new URL('..', import.meta.url))

test('package metadata describes the formatter WASM package', async () => {
  const packageJson = JSON.parse(await readFile(`${packageRoot}/package.json`, 'utf8')) as {
    name: string
    files: string[]
    scripts: Record<string, string>
    publishConfig: { access: string }
  }

  expect(packageJson.name).toBe('@intlify/format-wasm')
  expect(packageJson.files).toEqual(['README.md', 'dist'])
  expect(packageJson.publishConfig.access).toBe('public')
  expect(packageJson.scripts['build:wasm']).toContain('../../crates/intlify_format_wasm')
  expect(packageJson.scripts['build:wasm']).toContain('--out-name intlify_format_wasm')
})
