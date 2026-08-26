/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { createHash } from 'node:crypto'

import ts from 'typescript'

import { parsePlaceholders } from './placeholder.mjs'

const SOURCE_LOCALE = 'en'
const AUTOMATIC_SURFACE = 'dom-text-content'
const EXPLICIT_SURFACE = 'explicit-intent'
const MARKER_MODULE = '@intlify/locale'
const MARKER_NAME = 'intent'
const RUNTIME_IMPORT_PATH = './intlify-runtime.mjs'
const RUNTIME_ALIAS_BASE = '__intlify_message'

/**
 * Extract automatic and explicit Intents and rewrite them to runtime message lookups.
 *
 * @param options - Source text, normalized origin, and an optional test hash seam.
 * @returns The transformed source and intents, or blocking diagnostics.
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

  const diagnostics = []
  const marker = inspectImports({ sourceFile, path: normalizedRelativePath, diagnostics })
  const identifiers = collectIdentifiers(sourceFile)
  const explicitCalls = []
  const validExplicitCalls = new Map()

  if (marker !== undefined) {
    const bindingInspection = inspectMarkerBinding({
      sourceFile,
      path: normalizedRelativePath,
      diagnostics
    })
    explicitCalls.push(...bindingInspection.calls)

    const explicitCallSet = new Set(explicitCalls)
    for (const call of explicitCalls) {
      if (hasIntentCallAncestor(call, explicitCallSet)) {
        diagnostics.push(
          invalidIntentDiagnostic(
            sourceFile,
            normalizedRelativePath,
            call.getStart(sourceFile),
            'Nested intent() calls are not supported.'
          )
        )
        continue
      }

      const declaration = parseIntentCall(call, sourceFile, normalizedRelativePath)
      if (declaration.ok) {
        validExplicitCalls.set(call, declaration)
      } else {
        diagnostics.push(declaration.diagnostic)
      }
    }
  }

  const extracted = collectAutomaticAssignments({
    sourceFile,
    path: normalizedRelativePath,
    validExplicitCalls,
    diagnostics
  })
  const declarations = [
    ...[...validExplicitCalls.values()].map(declaration => ({
      ...declaration,
      surface: EXPLICIT_SURFACE
    })),
    ...extracted.map(declaration => ({
      ...declaration,
      parameters: [],
      surface: AUTOMATIC_SURFACE
    }))
  ].sort((left, right) => left.start - right.start)

  diagnostics.sort(compareSourceDiagnostics)
  if (diagnostics.some(diagnostic => diagnostic.severity === 'error')) {
    return { ok: false, diagnostics }
  }

  const built = buildIntents({
    declarations,
    sourceFile,
    path: normalizedRelativePath,
    hashPayload,
    diagnostics
  })
  if (!built.ok) {
    return built
  }

  if (built.intents.length === 0 && marker === undefined) {
    return {
      ok: true,
      diagnostics,
      intents: [],
      runtimeAlias: undefined,
      transformedSource: sourceText
    }
  }

  const runtimeAlias = built.intents.length === 0 ? undefined : chooseRuntimeAlias(identifiers)
  const transformedSource = rewriteSource({
    sourceText,
    replacements: built.replacements,
    runtimeAlias,
    marker
  })

  return {
    ok: true,
    diagnostics,
    intents: built.intents,
    runtimeAlias,
    transformedSource
  }
}

function inspectImports({ sourceFile, path, diagnostics }) {
  let marker

  for (const statement of sourceFile.statements) {
    if (ts.isExportDeclaration(statement) && statement.moduleSpecifier !== undefined) {
      diagnostics.push(unsupportedModuleImportDiagnostic(sourceFile, path, statement))
      continue
    }

    if (!ts.isImportDeclaration(statement)) {
      continue
    }

    if (
      !ts.isStringLiteral(statement.moduleSpecifier) ||
      statement.moduleSpecifier.text !== MARKER_MODULE
    ) {
      diagnostics.push(unsupportedModuleImportDiagnostic(sourceFile, path, statement))
      continue
    }

    if (!isExactMarkerImport(statement) || marker !== undefined) {
      diagnostics.push(
        invalidIntentDiagnostic(
          sourceFile,
          path,
          statement.getStart(sourceFile),
          "The marker import must be exactly: import { intent } from '@intlify/locale'."
        )
      )
      continue
    }

    marker = statement
  }

  visit(sourceFile, node => {
    if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword) {
      diagnostics.push(unsupportedModuleImportDiagnostic(sourceFile, path, node))
    }
  })

  return marker
}

function unsupportedModuleImportDiagnostic(sourceFile, path, node) {
  return createSourceDiagnostic({
    severity: 'error',
    code: 'LC002',
    name: 'unsupported_module_import',
    message: 'The PoC entry must be a self-contained browser module.',
    path,
    sourceFile,
    position: node.getStart(sourceFile)
  })
}

function isExactMarkerImport(statement) {
  const importClause = statement.importClause
  if (
    statement.attributes !== undefined ||
    importClause === undefined ||
    importClause.isTypeOnly ||
    importClause.name !== undefined ||
    !ts.isNamedImports(importClause.namedBindings) ||
    importClause.namedBindings.elements.length !== 1
  ) {
    return false
  }

  const specifier = importClause.namedBindings.elements[0]
  return (
    !specifier.isTypeOnly &&
    specifier.propertyName === undefined &&
    specifier.name.text === MARKER_NAME
  )
}

function inspectMarkerBinding({ sourceFile, path, diagnostics }) {
  const calls = []
  const shadowingBindings = collectShadowingBindings(sourceFile)
  const shadowingIdentifiers = new Set(shadowingBindings.map(binding => binding.identifier))

  visit(sourceFile, node => {
    if (!ts.isIdentifier(node) || node.text !== MARKER_NAME) {
      return
    }

    if (isImportDeclarationIdentifier(node)) {
      return
    }

    if (shadowingIdentifiers.has(node)) {
      diagnostics.push(
        invalidIntentDiagnostic(
          sourceFile,
          path,
          node.getStart(sourceFile),
          'The imported intent binding must not be shadowed.'
        )
      )
      return
    }

    if (isInsideShadowingScope(node, shadowingBindings)) {
      return
    }

    if (isDirectIntentCallIdentifier(node)) {
      calls.push(node.parent)
      return
    }

    if (isNonReferenceIdentifier(node)) {
      return
    }

    diagnostics.push(
      invalidIntentDiagnostic(
        sourceFile,
        path,
        node.getStart(sourceFile),
        'The imported intent binding may only be used as a direct intent() call.'
      )
    )
  })

  calls.sort((left, right) => left.getStart(sourceFile) - right.getStart(sourceFile))
  return { calls }
}

function collectShadowingBindings(sourceFile) {
  const bindings = []

  visit(sourceFile, node => {
    if (ts.isVariableDeclaration(node)) {
      collectBindingIdentifiers(node.name, variableScope(node), bindings)
      return
    }

    if (ts.isParameter(node)) {
      collectBindingIdentifiers(node.name, node.parent, bindings)
      return
    }

    if (
      (ts.isFunctionDeclaration(node) ||
        ts.isFunctionExpression(node) ||
        ts.isClassDeclaration(node) ||
        ts.isClassExpression(node)) &&
      node.name?.text === MARKER_NAME
    ) {
      bindings.push({
        identifier: node.name,
        scope: ts.isFunctionExpression(node) || ts.isClassExpression(node) ? node : node.parent
      })
      return
    }

    if (ts.isCatchClause(node) && node.variableDeclaration !== undefined) {
      collectBindingIdentifiers(node.variableDeclaration.name, node, bindings)
    }
  })

  return bindings
}

function collectBindingIdentifiers(name, scope, bindings) {
  if (ts.isIdentifier(name)) {
    if (name.text === MARKER_NAME) {
      bindings.push({ identifier: name, scope })
    }
    return
  }

  for (const element of name.elements) {
    if (ts.isBindingElement(element)) {
      collectBindingIdentifiers(element.name, scope, bindings)
    }
  }
}

function variableScope(declaration) {
  const declarationList = declaration.parent
  const blockScoped = (declarationList.flags & ts.NodeFlags.BlockScoped) !== 0
  let current = declarationList.parent

  while (current.parent !== undefined) {
    if (blockScoped && (ts.isBlock(current) || ts.isSourceFile(current))) {
      return current
    }
    if (!blockScoped && (ts.isFunctionLike(current) || ts.isSourceFile(current))) {
      return current
    }
    current = current.parent
  }

  return current
}

function isInsideShadowingScope(identifier, bindings) {
  for (const binding of bindings) {
    let current = identifier.parent
    while (current !== undefined) {
      if (current === binding.scope) {
        return true
      }
      current = current.parent
    }
  }
  return false
}

function isImportDeclarationIdentifier(node) {
  let current = node.parent
  while (current !== undefined && !ts.isStatement(current)) {
    current = current.parent
  }
  return current !== undefined && ts.isImportDeclaration(current)
}

function isDirectIntentCallIdentifier(node) {
  return (
    ts.isCallExpression(node.parent) &&
    node.parent.expression === node &&
    node.parent.questionDotToken === undefined
  )
}

function isNonReferenceIdentifier(node) {
  const parent = node.parent
  return (
    (ts.isPropertyAccessExpression(parent) && parent.name === node) ||
    (ts.isPropertyAssignment(parent) && parent.name === node) ||
    (ts.isBindingElement(parent) && parent.propertyName === node) ||
    (ts.isMethodDeclaration(parent) && parent.name === node) ||
    (ts.isGetAccessorDeclaration(parent) && parent.name === node) ||
    (ts.isSetAccessorDeclaration(parent) && parent.name === node) ||
    (ts.isPropertyDeclaration(parent) && parent.name === node) ||
    (ts.isLabeledStatement(parent) && parent.label === node) ||
    (ts.isBreakStatement(parent) && parent.label === node) ||
    (ts.isContinueStatement(parent) && parent.label === node) ||
    (ts.isExportSpecifier(parent) && parent.propertyName !== undefined && parent.name === node)
  )
}

function hasIntentCallAncestor(call, explicitCallSet) {
  let current = call.parent
  while (current !== undefined) {
    if (explicitCallSet.has(current)) {
      return true
    }
    current = current.parent
  }
  return false
}

function parseIntentCall(call, sourceFile, path) {
  if (call.arguments.length < 1 || call.arguments.length > 2) {
    return invalidIntentCall(
      sourceFile,
      path,
      call,
      'intent() requires one source argument and parameters only when placeholders are present.'
    )
  }

  const sourceExpression = call.arguments[0]
  const sourceText = getStaticValue(sourceExpression)
  if (sourceText === undefined) {
    return invalidIntentCall(
      sourceFile,
      path,
      sourceExpression,
      'The intent source must be a static string literal.'
    )
  }
  if (sourceText.trim().length === 0) {
    return invalidIntentCall(
      sourceFile,
      path,
      sourceExpression,
      'The intent source must not be empty or whitespace-only.'
    )
  }

  const placeholders = parsePlaceholders(sourceText)
  if (!placeholders.ok) {
    return invalidIntentCall(
      sourceFile,
      path,
      sourceExpression,
      'The intent source contains an invalid placeholder.'
    )
  }

  if (placeholders.names.length === 0) {
    if (call.arguments.length !== 1) {
      return invalidIntentCall(
        sourceFile,
        path,
        call,
        'An intent without placeholders must not have a parameters object.'
      )
    }
    return validIntentCall(call, sourceExpression, sourceText, [])
  }

  if (call.arguments.length !== 2) {
    return invalidIntentCall(
      sourceFile,
      path,
      call,
      'The intent parameters must exactly match the message placeholders.'
    )
  }

  const parametersExpression = call.arguments[1]
  if (!ts.isObjectLiteralExpression(parametersExpression)) {
    return invalidIntentCall(
      sourceFile,
      path,
      parametersExpression,
      'The intent parameters must be an object literal.'
    )
  }

  const parameterNames = []
  const parameterNameSet = new Set()
  for (const property of parametersExpression.properties) {
    if (
      (!ts.isPropertyAssignment(property) && !ts.isShorthandPropertyAssignment(property)) ||
      !ts.isIdentifier(property.name)
    ) {
      return invalidIntentCall(
        sourceFile,
        path,
        property,
        'Intent parameters only support identifier properties and shorthand properties.'
      )
    }

    const name = property.name.text
    if (name === '__proto__' || parameterNameSet.has(name)) {
      return invalidIntentCall(
        sourceFile,
        path,
        property,
        'Intent parameter names must be unique and must not be __proto__.'
      )
    }
    parameterNameSet.add(name)
    parameterNames.push(name)
  }

  const placeholderSet = new Set(placeholders.names)
  if (
    parameterNameSet.size !== placeholderSet.size ||
    parameterNames.some(name => !placeholderSet.has(name))
  ) {
    return invalidIntentCall(
      sourceFile,
      path,
      call,
      'The intent parameters must exactly match the message placeholders.'
    )
  }

  return validIntentCall(
    call,
    sourceExpression,
    sourceText,
    placeholders.names.map(name => ({ name, type: 'string-or-number' }))
  )
}

function validIntentCall(call, sourceExpression, sourceText, parameters) {
  return {
    ok: true,
    call,
    sourceExpression,
    sourceText,
    parameters,
    start: call.getStart(),
    end: call.getEnd()
  }
}

function invalidIntentCall(sourceFile, path, node, message) {
  return {
    ok: false,
    diagnostic: invalidIntentDiagnostic(sourceFile, path, node.getStart(sourceFile), message)
  }
}

function collectAutomaticAssignments({ sourceFile, path, validExplicitCalls, diagnostics }) {
  const assignments = []

  visit(sourceFile, node => {
    if (!isTextContentAssignment(node)) {
      return
    }

    const right = node.right
    const staticValue = getStaticValue(right)
    if (staticValue !== undefined) {
      if (staticValue.trim().length > 0) {
        assignments.push({
          sourceText: staticValue,
          start: right.getStart(sourceFile),
          end: right.getEnd(),
          automaticExpression: right
        })
      }
      return
    }

    if (ts.isCallExpression(right) && validExplicitCalls.has(right)) {
      return
    }

    diagnostics.push(
      createSourceDiagnostic({
        severity: 'warning',
        code: 'LC001',
        name: 'unsupported_dynamic_ui_text',
        message: 'Dynamic textContent was left unchanged.',
        path,
        sourceFile,
        position: right.getStart(sourceFile)
      })
    )
  })

  return assignments
}

function buildIntents({ declarations, sourceFile, path, hashPayload, diagnostics }) {
  const occurrences = new Map()
  const payloadById = new Map()
  const intents = []
  const replacements = []

  for (const declaration of declarations) {
    const occurrenceKey = JSON.stringify([path, declaration.surface, declaration.sourceText])
    const occurrence = occurrences.get(occurrenceKey) ?? 0
    occurrences.set(occurrenceKey, occurrence + 1)

    const payload = JSON.stringify([path, declaration.surface, declaration.sourceText, occurrence])
    const id = `m_${hashPayload(payload)}`
    const previousPayload = payloadById.get(id)

    if (previousPayload !== undefined && previousPayload !== payload) {
      const collision = createSourceDiagnostic({
        severity: 'error',
        code: 'LC006',
        name: 'intent_id_collision',
        message: `Different intent payloads produced the same ID: ${id}`,
        path,
        sourceFile,
        position: declaration.start
      })
      return {
        ok: false,
        diagnostics: [...diagnostics, collision].sort(compareSourceDiagnostics)
      }
    }

    payloadById.set(id, payload)
    intents.push({
      id,
      sourceLocale: SOURCE_LOCALE,
      sourceText: declaration.sourceText,
      surface: declaration.surface,
      parameters: declaration.parameters,
      origin: {
        path,
        start: declaration.start,
        end: declaration.end
      }
    })

    if (declaration.call !== undefined) {
      replacements.push({
        start: declaration.call.expression.getStart(sourceFile),
        end: declaration.call.expression.getEnd(),
        kind: 'callee'
      })
      replacements.push({
        start: declaration.sourceExpression.getStart(sourceFile),
        end: declaration.sourceExpression.getEnd(),
        text: JSON.stringify(id)
      })
    } else {
      replacements.push({
        start: declaration.start,
        end: declaration.end,
        kind: 'automatic',
        id
      })
    }
  }

  return { ok: true, intents, replacements }
}

function hashIntentPayload(payload) {
  return createHash('sha256').update(payload, 'utf8').digest().subarray(0, 12).toString('base64url')
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

function rewriteSource({ sourceText, replacements, runtimeAlias, marker }) {
  const edits = replacements.map(replacement => ({
    start: replacement.start,
    end: replacement.end,
    text:
      replacement.text ??
      (replacement.kind === 'callee'
        ? runtimeAlias
        : `${runtimeAlias}(${JSON.stringify(replacement.id)})`)
  }))

  if (marker !== undefined) {
    const markerHasSemicolon = sourceText[marker.getEnd() - 1] === ';'
    edits.push({
      start: marker.getStart(),
      end: marker.getEnd(),
      text:
        runtimeAlias === undefined
          ? ''
          : `import { message as ${runtimeAlias} } from ${JSON.stringify(RUNTIME_IMPORT_PATH)}` +
            (markerHasSemicolon ? ';' : '')
    })
  }

  let transformed = sourceText
  for (const edit of edits.sort((left, right) => right.start - left.start)) {
    transformed = transformed.slice(0, edit.start) + edit.text + transformed.slice(edit.end)
  }

  if (marker !== undefined || runtimeAlias === undefined) {
    return transformed
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

function invalidIntentDiagnostic(sourceFile, path, position, message) {
  return createSourceDiagnostic({
    severity: 'error',
    code: 'LC010',
    name: 'invalid_intent_declaration',
    message,
    path,
    sourceFile,
    position
  })
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
