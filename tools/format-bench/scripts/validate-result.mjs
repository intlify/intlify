import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'

import { assertValidFormatBenchmarkResult } from '../result-schema.mjs'

const resultPath = process.argv[2]
if (!resultPath) {
  throw new Error('usage: node scripts/validate-result.mjs <result.json>')
}

const result = JSON.parse(await readFile(resolve(resultPath), 'utf8'))
assertValidFormatBenchmarkResult(result)
console.log(`${resultPath} is a valid formatter benchmark result`)
