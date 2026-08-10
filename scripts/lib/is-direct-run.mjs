import { pathToFileURL } from 'node:url'

/**
 * Check whether an ES module is being executed as the process entry point.
 *
 * @param moduleUrl - URL of the module asking for the check.
 * @param entryPath - Optional process entry path used by tests.
 * @returns Whether the module is the process entry point.
 */
export function isDirectRun(moduleUrl, entryPath = process.argv[1]) {
  return Boolean(entryPath) && moduleUrl === pathToFileURL(entryPath).href
}
