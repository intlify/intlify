/**
 * Options for redirecting Cargo version updates to a fixture workspace.
 */
export interface BumpCargoVersionOptions {
  /** Workspace root containing the Cargo manifests and lockfile. */
  rootDir?: string
}

export function bumpCargoVersion(
  nextVersion: string,
  options?: BumpCargoVersionOptions
): Promise<void>
