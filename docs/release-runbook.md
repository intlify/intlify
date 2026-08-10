# Release Runbook

## Normal Monorepo Release

The release commit and tag are prepared locally. GitHub Actions only validates the tagged tree, publishes artifacts, runs published smoke tests, and creates the GitHub Release.

Before starting, confirm all of the following:

- the working tree is clean and the current branch is `main`;
- local `main` is fast-forwarded to `origin/main`, and its `HEAD` is already pushed;
- the future `vX.Y.Z` tag does not exist locally or remotely;
- `CHANGELOG.md` is tracked;
- the GitHub CLI is authenticated with a token that has repository `Contents: write` permission.

Run the release with the GitHub CLI token passed through an environment variable:

```sh
GH_TOKEN="$(gh auth token --hostname github.com)" vp run release
```

`bumpp` updates package versions, then its execute hook updates Cargo metadata and generates the future tag's CHANGELOG entry with `gh-changelogen --generate-notes --target=HEAD`. Only after both operations succeed does `bumpp` create the release commit, tag, and push them.

Do not pass the token through `--token` or commit generated temporary output. If the hook fails, inspect the dirty working tree, correct the cause, and rerun only after confirming the selected version and changelog entry are still correct. Never move or force-push a tag after an Actions validation failure; publish a corrected version instead.

The tag workflow validates the tagged version and changelog before publication. It then publishes npm and crates.io artifacts, runs the installed-package smoke tests, and creates the GitHub Release. It does not push a `chore: generate changelog` commit to `main`.

Prerelease tags such as `v0.14.0-alpha.12` are published with the npm prerelease dist-tag and as GitHub prereleases; they must not become the Latest Release. Stable tags use the normal stable Release behavior.

## Release Retry

If publication and smoke tests succeeded but GitHub Release creation failed, use `workflow_dispatch` with `job: release-notes` and the existing tag. The retry verifies that the remote tag exists and skips cleanly when the Release already exists. It does not republish packages or modify `main`.

Partial npm / crates.io publication failures use the existing job-specific retry policy. Resolve registry state before retrying a publish job.

## Formatter Packages

The formatter release flow publishes generated N-API native packages before the wrapper package, then publishes the WASM package:

1. `@intlify/format-napi-*` platform packages
2. `@intlify/format-napi`
3. `@intlify/format-wasm`

New `@intlify/format-*` npm packages may need token-based bootstrap publishing for the first release because npm Trusted Publishing can only be configured after the packages exist. After bootstrap, configure the `npm-release` trusted publisher for this repository workflow and use the normal trusted publishing release path.

Published release smoke tests install `@intlify/format-napi` and `@intlify/format-wasm` for the release tag and verify `formatMessage` / `checkFormat` behavior.
