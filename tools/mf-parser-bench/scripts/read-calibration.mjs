import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'

const rootDir = resolve(import.meta.dirname, '..')
const [target, corpus] = process.argv.slice(2)

if (!target || !corpus) {
  throw new Error('Usage: node scripts/read-calibration.mjs <target> <corpus>')
}

try {
  const calibration = JSON.parse(await readFile(resolve(rootDir, '.tmp/calibration.json'), 'utf8'))
  const value = calibration.results?.[`${target}__${corpus}`]?.iterations
  process.stdout.write(`${Number.isInteger(value) && value > 0 ? value : 1000}\n`)
} catch {
  process.stdout.write('1000\n')
}
