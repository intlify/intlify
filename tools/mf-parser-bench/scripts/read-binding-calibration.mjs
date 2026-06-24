import { resolve } from 'node:path'

import { readBindingIterations } from './binding-calibration.mjs'

const rootDir = resolve(import.meta.dirname, '..')
const [runtime, operation] = process.argv.slice(2)

if (!runtime || !operation) {
  throw new Error('Usage: node scripts/read-binding-calibration.mjs <runtime> <operation>')
}

const iterations = await readBindingIterations(rootDir, runtime, operation)
process.stdout.write(`${iterations}\n`)
