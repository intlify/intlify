/**
 * Classify one completed synchronous core process without hiding executed failures.
 *
 * @param result - Result returned by `spawnSync`.
 * @param executable - Executable label used in failure messages.
 * @returns Parsed core output or an unavailable-executable record.
 */
export function decodeCoreProcessResult(result, executable) {
  if (result.error) {
    if (result.error.code === 'ENOENT' || result.error.code === 'EACCES') {
      return { kind: 'unavailable', reason: result.error.message }
    }
    throw new Error(`failed to execute ${executable}: ${result.error.message}`)
  }
  if (result.status !== 0) {
    const reason = result.stderr.trim() || `${executable} failed with status ${result.status}`
    throw new Error(reason)
  }

  let output
  try {
    output = JSON.parse(result.stdout)
  } catch (error) {
    throw new Error(`resource benchmark core emitted malformed JSON: ${error.message}`)
  }
  if (!Array.isArray(output.results) || !Array.isArray(output.memoryGrowthChecks)) {
    throw new Error('resource benchmark core output is missing result arrays')
  }
  return { kind: 'measured', output }
}
