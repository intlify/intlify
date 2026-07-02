import { expect, test } from 'vite-plus/test'

import { distTagForVersion } from '../../../scripts/release-npm.mjs'

test('npm release dist-tag uses latest for stable versions', () => {
  expect(distTagForVersion('0.14.0')).toBe('latest')
})

test('npm release dist-tag uses the prerelease identifier', () => {
  expect(distTagForVersion('0.14.0-alpha.0')).toBe('alpha')
  expect(distTagForVersion('0.14.0-beta.2')).toBe('beta')
  expect(distTagForVersion('0.14.0-rc.1')).toBe('rc')
})

test('npm release dist-tag falls back to next for numeric prereleases', () => {
  expect(distTagForVersion('0.14.0-0')).toBe('next')
})
