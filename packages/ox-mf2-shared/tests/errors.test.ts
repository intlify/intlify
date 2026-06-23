import { expect, test } from 'vite-plus/test'
import {
  OxMf2ErrorCode,
  OxMf2InitializationError,
  OxMf2ParseError,
  OxMf2SnapshotError,
  OxMf2SourceTextError,
  SectionKind
} from '../src/index.ts'

test('dedicated error classes preserve shape', () => {
  const error = new OxMf2SnapshotError({
    code: OxMf2ErrorCode.DecodeInvalidMagic,
    message: 'invalid magic',
    sectionKind: SectionKind.Nodes,
    offset: 32,
    recordIndex: 1
  })

  expect(error).toBeInstanceOf(Error)
  expect(error.name).toBe('OxMf2SnapshotError')
  expect(error.code).toBe(OxMf2ErrorCode.DecodeInvalidMagic)
  expect(error.sectionKind).toBe(SectionKind.Nodes)
  expect(error.offset).toBe(32)
  expect(error.recordIndex).toBe(1)
  expect(error.toJSON()).toEqual({
    code: OxMf2ErrorCode.DecodeInvalidMagic,
    message: 'invalid magic',
    sectionKind: SectionKind.Nodes,
    offset: 32,
    recordIndex: 1
  })
})

test('all dedicated error classes have stable names', () => {
  expect(new OxMf2ParseError(baseShape()).name).toBe('OxMf2ParseError')
  expect(new OxMf2SourceTextError(baseShape()).name).toBe('OxMf2SourceTextError')
  expect(new OxMf2InitializationError(baseShape()).name).toBe('OxMf2InitializationError')
})

function baseShape() {
  return {
    code: OxMf2ErrorCode.BindingValidationInvalidOptions,
    message: 'invalid options'
  }
}
