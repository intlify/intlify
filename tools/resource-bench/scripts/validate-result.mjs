import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { assertValidResourceBenchmarkResult } from '../result-schema.mjs'

const defaultResultPath = fileURLToPath(new URL('../results/latest.json', import.meta.url))
const resultPath = resolve(process.argv[2] ?? defaultResultPath)
const result = JSON.parse(await readFile(resultPath, 'utf8'))
assertValidResourceBenchmarkResult(result)
console.log(resultPath)
