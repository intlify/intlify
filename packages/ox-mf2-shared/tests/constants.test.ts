import { expect, test } from 'vite-plus/test'
import {
  DiagnosticCode,
  DiagnosticSeverity,
  SectionKind,
  SyntaxKind,
  diagnosticCodeName,
  diagnosticSeverityName,
  sectionKindName,
  syntaxKindName
} from '../src/index.ts'

test('numeric const name helpers resolve exported constants', () => {
  for (const [name, kind] of Object.entries(SyntaxKind)) {
    expect(syntaxKindName(kind)).toBe(name)
  }
  for (const [name, code] of Object.entries(DiagnosticCode)) {
    expect(diagnosticCodeName(code)).toBe(name)
  }
  for (const [name, severity] of Object.entries(DiagnosticSeverity)) {
    expect(diagnosticSeverityName(severity)).toBe(name)
  }
  for (const [name, kind] of Object.entries(SectionKind)) {
    expect(sectionKindName(kind)).toBe(name)
  }
})

test('numeric const name helpers return unknown for unrecognized values', () => {
  expect(syntaxKindName(9999)).toBe('unknown')
  expect(diagnosticCodeName(9999)).toBe('unknown')
  expect(diagnosticSeverityName(99)).toBe('unknown')
  expect(sectionKindName(99)).toBe('unknown')
})
