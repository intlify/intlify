/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { lstat, mkdir, readFile, writeFile } from 'node:fs/promises'
import { extname, isAbsolute, join, relative, sep } from 'node:path'

import { emitArtifacts } from './emitter.mjs'
import { transformJavaScript } from './frontend.mjs'
import { runLocalizationProvider } from './provider.mjs'

const ARTIFACT_FILENAMES = [
  'app.js',
  'intlify-runtime.mjs',
  'locale-en.mjs',
  'locale-ja.mjs',
  'localization-report.json'
]

/**
 * Compile one self-contained browser module into validated locale artifacts.
 *
 * @param options - Absolute input, Provider and output paths plus the diagnostic working directory.
 * @returns A success result with written artifacts, or collected diagnostics.
 */
export async function compile({ entryPath, outDir, providerPath, cwd }) {
  const entryDiagnosticPath = diagnosticPath(cwd, entryPath)

  if (extname(entryPath) !== '.js') {
    return invalidSource(entryDiagnosticPath, 'The PoC entry must use the .js extension.')
  }

  let sourceBytes
  try {
    sourceBytes = await readFile(entryPath)
  } catch {
    return invalidSource(entryDiagnosticPath, 'The PoC entry could not be read.')
  }

  if (hasUtf8Bom(sourceBytes)) {
    return invalidSource(entryDiagnosticPath, 'The PoC entry must be UTF-8 without a BOM.')
  }

  let sourceText
  try {
    sourceText = new TextDecoder('utf-8', { fatal: true }).decode(sourceBytes)
  } catch {
    return invalidSource(entryDiagnosticPath, 'The PoC entry must contain valid UTF-8.')
  }

  const frontend = transformJavaScript({
    sourceText,
    relativePath: entryDiagnosticPath
  })
  if (!frontend.ok) {
    return frontend
  }

  const localized = await runLocalizationProvider({
    providerPath,
    cwd,
    intents: frontend.intents
  })
  if (!localized.ok) {
    return {
      ok: false,
      diagnostics: [...frontend.diagnostics, ...localized.diagnostics]
    }
  }

  const artifacts = emitArtifacts({
    transformedSource: frontend.transformedSource,
    intents: frontend.intents,
    candidates: localized.candidates,
    provider: localized.provider
  })

  const outputSafetyDiagnostic = await validateOutputSafety({
    entryPath,
    providerPath,
    outDir,
    cwd
  })
  if (outputSafetyDiagnostic !== undefined) {
    return {
      ok: false,
      diagnostics: [...frontend.diagnostics, outputSafetyDiagnostic]
    }
  }

  try {
    await mkdir(outDir, { recursive: true })
    for (const filename of ARTIFACT_FILENAMES) {
      await writeFile(join(outDir, filename), artifacts.get(filename), 'utf8')
    }
  } catch {
    return {
      ok: false,
      diagnostics: [
        ...frontend.diagnostics,
        createDiagnostic(
          'LC008',
          'output_write_failed',
          'The generated artifacts could not be written.',
          diagnosticPath(cwd, outDir)
        )
      ]
    }
  }

  return {
    ok: true,
    diagnostics: frontend.diagnostics,
    artifacts
  }
}

async function validateOutputSafety({ entryPath, providerPath, outDir, cwd }) {
  const outputPath = diagnosticPath(cwd, outDir)

  if (isSameOrInside(outDir, entryPath) || isSameOrInside(outDir, providerPath)) {
    return createDiagnostic(
      'LC009',
      'unsafe_output_path',
      'The output directory must not contain the entry or provider.',
      outputPath
    )
  }

  try {
    if (await isSymbolicLink(outDir)) {
      return createDiagnostic(
        'LC009',
        'unsafe_output_path',
        'The output directory must not be a symbolic link.',
        outputPath
      )
    }

    for (const filename of ARTIFACT_FILENAMES) {
      if (await isSymbolicLink(join(outDir, filename))) {
        return createDiagnostic(
          'LC009',
          'unsafe_output_path',
          'Generated artifact paths must not be symbolic links.',
          diagnosticPath(cwd, join(outDir, filename))
        )
      }
    }
  } catch {
    return createDiagnostic(
      'LC008',
      'output_write_failed',
      'The output path could not be inspected.',
      outputPath
    )
  }

  return undefined
}

async function isSymbolicLink(path) {
  try {
    return (await lstat(path)).isSymbolicLink()
  } catch (error) {
    if (error?.code === 'ENOENT') {
      return false
    }
    throw error
  }
}

function isSameOrInside(parentPath, candidatePath) {
  const relativePath = relative(parentPath, candidatePath)
  return (
    relativePath === '' ||
    (relativePath !== '..' && !relativePath.startsWith(`..${sep}`) && !isAbsolute(relativePath))
  )
}

function hasUtf8Bom(bytes) {
  return bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf
}

function invalidSource(path, message) {
  return {
    ok: false,
    diagnostics: [createDiagnostic('LC005', 'invalid_source', message, path)]
  }
}

function createDiagnostic(code, name, message, path) {
  return { severity: 'error', code, name, message, path }
}

function diagnosticPath(cwd, path) {
  return normalizePath(relative(cwd, path)) || '.'
}

function normalizePath(path) {
  return path.replaceAll('\\', '/')
}
