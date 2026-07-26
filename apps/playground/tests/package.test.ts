import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { expect, test } from 'vite-plus/test'

const packageRoot = fileURLToPath(new URL('..', import.meta.url))

test('workspace build reuses the binding dependency graph', async () => {
  const packageJson = JSON.parse(await readFile(`${packageRoot}/package.json`, 'utf8')) as {
    scripts: Record<string, string>
  }

  expect(packageJson.scripts.build).toBe('vp build --configLoader runner')
  expect(packageJson.scripts.build).not.toContain('build:bindings')
  expect(packageJson.scripts.dev).toContain('build:bindings')
})
