/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { createHash } from 'node:crypto'

import ts from 'typescript'

const SOURCE_LOCALE = 'en'
const SURFACE = 'dom-text-content'
const RUNTIME_IMPORT_PATH = './intlify-runtime.mjs'
const RUNTIME_ALIAS_BASE = '__intlify_message'

/**
 * Extract localizable UI text and rewrite it to runtime message lookups.
 *
 * @param options - Source text, normalized origin, and an optional test hash seam.
 * @returns The transformed source and intents, or a blocking diagnostic.
 */
export function transformJavaScript({ sourceText, relativePath, hashPayload = hashIntentPayload }) {
  const normalizedRelativePath = normalizePath(relativePath)
  const sourceFile = ts.createSourceFile(
    normalizedRelativePath,
    sourceText,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.JS
  )

  const parseDiagnostic = sourceFile.parseDiagnostics[0]
  if (parseDiagnostic !== undefined) {
    return {
      ok: false,
      diagnostics: [
        createSourceDiagnostic({
          severity: 'error',
          code: 'LC005',
          name: 'invalid_source',
          message: ts.flattenDiagnosticMessageText(parseDiagnostic.messageText, '\n'),
          path: normalizedRelativePath,
          sourceFile,
          position: parseDiagnostic.start ?? 0
        })
      ]
    }
  }

  const importDeclaration = sourceFile.statements.find(ts.isImportDeclaration)
  if (importDeclaration !== undefined) {
    return {
      ok: false,
      diagnostics: [
        createSourceDiagnostic({
          severity: 'error',
          code: 'LC002',
          name: 'unsupported_module_import',
          message: 'The PoC entry must be a self-contained browser module.',
          path: normalizedRelativePath,
          sourceFile,
          position: importDeclaration.getStart(sourceFile)
        })
      ]
    }
  }

  const identifiers = collectIdentifiers(sourceFile)
  const staticAssignments = []
  const diagnostics = []

  visit(sourceFile, node => {
    if (!isTextContentAssignment(node)) {
      return
    }

    const right = node.right
    const staticValue = getStaticValue(right)

    if (staticValue === undefined) {
      diagnostics.push(
        createSourceDiagnostic({
          severity: 'warning',
          code: 'LC001',
          name: 'unsupported_dynamic_ui_text',
          message: 'Dynamic textContent was left unchanged.',
          path: normalizedRelativePath,
          sourceFile,
          position: right.getStart(sourceFile)
        })
      )
      return
    }

    if (staticValue.trim().length === 0) {
      return
    }

    staticAssignments.push({
      sourceText: staticValue,
      start: right.getStart(sourceFile),
      end: right.getEnd()
    })
  })

  diagnostics.sort(compareSourceDiagnostics)
  staticAssignments.sort((left, right) => left.start - right.start)

  const occurrences = new Map()
  const payloadById = new Map()
  const intents = []
  const replacements = []

  for (const assignment of staticAssignments) {
    const occurrenceKey = JSON.stringify([normalizedRelativePath, SURFACE, assignment.sourceText])
    const occurrence = occurrences.get(occurrenceKey) ?? 0
    occurrences.set(occurrenceKey, occurrence + 1)

    const payload = JSON.stringify([
      normalizedRelativePath,
      SURFACE,
      assignment.sourceText,
      occurrence
    ])
    const id = `m_${hashPayload(payload)}`
    const previousPayload = payloadById.get(id)

    if (previousPayload !== undefined && previousPayload !== payload) {
      return {
        ok: false,
        diagnostics: [
          ...diagnostics,
          createSourceDiagnostic({
            severity: 'error',
            code: 'LC006',
            name: 'intent_id_collision',
            message: `Different intent payloads produced the same ID: ${id}`,
            path: normalizedRelativePath,
            sourceFile,
            position: assignment.start
          })
        ]
      }
    }

    payloadById.set(id, payload)
    intents.push({
      id,
      sourceLocale: SOURCE_LOCALE,
      sourceText: assignment.sourceText,
      surface: SURFACE,
      origin: {
        path: normalizedRelativePath,
        start: assignment.start,
        end: assignment.end
      }
    })
    replacements.push({ id, start: assignment.start, end: assignment.end })
  }

  if (intents.length === 0) {
    return {
      ok: true,
      diagnostics,
      intents,
      runtimeAlias: undefined,
      transformedSource: sourceText
    }
  }

  const runtimeAlias = chooseRuntimeAlias(identifiers)
  const transformedSource = rewriteSource(sourceText, replacements, runtimeAlias)

  return {
    ok: true,
    diagnostics,
    intents,
    runtimeAlias,
    transformedSource
  }
}

function hashIntentPayload(payload) {
  return createHash('sha256').update(payload, 'utf8').digest('hex').slice(0, 32)
}

function collectIdentifiers(sourceFile) {
  const identifiers = new Set()
  visit(sourceFile, node => {
    if (ts.isIdentifier(node)) {
      identifiers.add(node.text)
    }
  })
  return identifiers
}

function isTextContentAssignment(node) {
  return (
    ts.isBinaryExpression(node) &&
    node.operatorToken.kind === ts.SyntaxKind.EqualsToken &&
    ts.isPropertyAccessExpression(node.left) &&
    node.left.name.text === 'textContent'
  )
}

function getStaticValue(expression) {
  if (ts.isStringLiteral(expression) || ts.isNoSubstitutionTemplateLiteral(expression)) {
    return expression.text
  }
  return undefined
}

function chooseRuntimeAlias(identifiers) {
  if (!identifiers.has(RUNTIME_ALIAS_BASE)) {
    return RUNTIME_ALIAS_BASE
  }

  let suffix = 1
  while (identifiers.has(`${RUNTIME_ALIAS_BASE}_${suffix}`)) {
    suffix += 1
  }
  return `${RUNTIME_ALIAS_BASE}_${suffix}`
}

function rewriteSource(sourceText, replacements, runtimeAlias) {
  let transformed = sourceText
  const sortedReplacements = [...replacements].sort((left, right) => right.start - left.start)

  for (const replacement of sortedReplacements) {
    const runtimeCall = `${runtimeAlias}(${JSON.stringify(replacement.id)})`
    transformed =
      transformed.slice(0, replacement.start) + runtimeCall + transformed.slice(replacement.end)
  }

  const newline = detectNewline(sourceText)
  const runtimeImport =
    `import { message as ${runtimeAlias} } from ${JSON.stringify(RUNTIME_IMPORT_PATH)}` +
    newline +
    newline
  return runtimeImport + transformed
}

function detectNewline(sourceText) {
  const match = /\r\n|\n/.exec(sourceText)
  return match?.[0] ?? '\n'
}

function createSourceDiagnostic(options) {
  const location = options.sourceFile.getLineAndCharacterOfPosition(options.position)
  return {
    severity: options.severity,
    code: options.code,
    name: options.name,
    message: options.message,
    path: options.path,
    line: location.line + 1,
    column: location.character + 1
  }
}

function compareSourceDiagnostics(left, right) {
  return (left.line ?? 0) - (right.line ?? 0) || (left.column ?? 0) - (right.column ?? 0)
}

function visit(root, callback) {
  callback(root)
  root.forEachChild(node => visit(node, callback))
}

function normalizePath(path) {
  return path.replaceAll('\\', '/')
}
