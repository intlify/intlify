import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'

import { assertValidMessageBenchmarkResult } from '../result-schema.mjs'

const path = resolve(process.argv[2] ?? 'results/latest.json')
const result = JSON.parse(await readFile(path, 'utf8'))
assertValidMessageBenchmarkResult(result)
console.log(`${path} is a valid messages benchmark result`)
