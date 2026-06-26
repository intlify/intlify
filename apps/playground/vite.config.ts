import vue from '@vitejs/plugin-vue'
import { copyFileSync, createReadStream, existsSync, mkdirSync } from 'node:fs'
import type { IncomingMessage, ServerResponse } from 'node:http'
import { createRequire } from 'node:module'
import { basename, join } from 'node:path'
import { defineConfig } from 'vite-plus'
import { voidPlugin } from 'void'

const require = createRequire(import.meta.url)
const oxMf2WasmPackage = require('../../packages/ox-mf2-wasm/package.json') as {
  version: string
}
const oxMf2WasmAssetNames = ['ox_mf2_wasm.js', 'ox_mf2_wasm_bg.wasm']

type DevServer = {
  middlewares: {
    use(
      path: string,
      handler: (request: IncomingMessage, response: ServerResponse, next: () => void) => void
    ): void
  }
}

function getContentType(filename: string): string {
  return filename.endsWith('.wasm') ? 'application/wasm' : 'text/javascript; charset=utf-8'
}

function oxMf2WasmAssets() {
  const assetDir = join(process.cwd(), '..', '..', 'packages', 'ox-mf2-wasm', 'dist')

  return {
    name: 'ox-mf2-wasm-assets',
    configureServer(server: DevServer) {
      server.middlewares.use('/dist', (request, response, next) => {
        const pathname = decodeURIComponent((request.url ?? '').split('?')[0] ?? '')
        const filename = basename(pathname)

        if (!oxMf2WasmAssetNames.includes(filename)) {
          next()
          return
        }

        const file = join(assetDir, filename)
        if (!existsSync(file)) {
          next()
          return
        }

        response.setHeader('Content-Type', getContentType(filename))
        const stream = createReadStream(file)
        stream.on('error', () => {
          if (response.headersSent) {
            response.destroy()
            return
          }

          response.removeHeader('Content-Type')
          next()
        })
        stream.pipe(response)
      })
    },
    closeBundle() {
      for (const filename of oxMf2WasmAssetNames) {
        for (const outputDirectory of ['client', 'ssr']) {
          const targetDir = join(process.cwd(), 'dist', outputDirectory, 'dist')

          mkdirSync(targetDir, { recursive: true })
          copyFileSync(join(assetDir, filename), join(targetDir, filename))
        }
      }
    }
  }
}

export default defineConfig({
  define: {
    __OX_MF2_WASM_VERSION__: JSON.stringify(oxMf2WasmPackage.version)
  },
  optimizeDeps: {
    exclude: ['@intlify/ox-mf2-wasm']
  },
  build: {
    chunkSizeWarningLimit: 4000
  },
  plugins: [oxMf2WasmAssets(), voidPlugin(), vue()]
})
