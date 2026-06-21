#!/usr/bin/env node
/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const SKIP_DIRS = new Set(['refers', 'node_modules', 'target'])

function walkFiles(dir, files) {
  const entries = fs.readdirSync(dir, { withFileTypes: true })

  for (const entry of entries) {
    if (entry.name.startsWith('.')) {
      continue
    }

    const fullPath = path.join(dir, entry.name)
    const relPath = path.relative(ROOT_DIR, fullPath)

    if (entry.isDirectory()) {
      const top = entry.name
      if (SKIP_DIRS.has(top)) {
        continue
      }
      walkFiles(fullPath, files)
      continue
    }

    if (entry.isFile() && relPath.endsWith('.rs')) {
      files.push(fullPath)
    }
  }
}

function hasRequiredHeaders(filePath) {
  const data = fs.readFileSync(filePath, 'utf8')
  const lines = data.split(/\r?\n/)
  let hasAuthor = false
  let hasLicense = false
  const missing = []

  for (const rawLine of lines) {
    const line = rawLine.replace(/^\uFEFF/, '').trimEnd()
    if (/^\s*\/\//.test(line)) {
      if (/@author\s+/.test(line)) {
        hasAuthor = true
      }
      if (/@license\s+/.test(line)) {
        hasLicense = true
      }
      continue
    }

    break
  }

  if (!hasAuthor) {
    missing.push('@author')
  }
  if (!hasLicense) {
    missing.push('@license')
  }

  if (missing.length > 0) {
    const rel = path.relative(ROOT_DIR, filePath).replace(/\\/g, '/')
    console.error(`missing ${missing.join(' ')} in ${rel}`)
    return false
  }

  return true
}

function main() {
  const files = []
  walkFiles(ROOT_DIR, files)
  let failures = 0

  for (const file of files) {
    if (!hasRequiredHeaders(file)) {
      failures += 1
    }
  }

  if (failures > 0) {
    console.error(`error: rust header check failed in ${failures} file(s)`)
    process.exitCode = 1
    return
  }

  console.log('rust header check passed')
}

main()
