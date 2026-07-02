import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'

const tag = firstNonEmpty(process.argv[2], process.env.TAG, process.env.GITHUB_REF_NAME)

if (!tag?.startsWith('v')) {
  throw new Error(`release tag must start with "v": ${String(tag)}`)
}

const version = tag.slice(1)
if (!version) {
  throw new Error(`release tag does not contain a version: ${tag}`)
}

const attempts = Number(process.env.CRATE_SMOKE_ATTEMPTS ?? 10)
const delayMs = Number(process.env.CRATE_SMOKE_DELAY_MS ?? 10_000)
let passed = false

for (let attempt = 1; attempt <= attempts; attempt++) {
  const workspace = await mkdtemp(join(tmpdir(), 'ox-mf2-parser-crate-smoke-'))
  try {
    await prepareProject(workspace, version)
    const result = spawnSync('cargo', ['run', '--quiet'], {
      cwd: workspace,
      stdio: 'inherit'
    })
    if (result.status === 0) {
      console.log(`ox_mf2_parser@${version} smoke test passed`)
      passed = true
      break
    }
    console.error(`ox_mf2_parser@${version} smoke test failed on attempt ${attempt}/${attempts}`)
  } finally {
    await rm(workspace, { recursive: true, force: true })
  }

  if (attempt < attempts) {
    await sleep(delayMs)
  }
}

if (!passed) {
  throw new Error(`ox_mf2_parser@${version} did not pass smoke test`)
}

async function prepareProject(workspace, version) {
  await mkdir(join(workspace, 'src'), { recursive: true })
  await writeFile(
    join(workspace, 'Cargo.toml'),
    `[package]
name = "ox-mf2-parser-smoke"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
ox_mf2_parser = "=${version}"
`
  )
  await writeFile(
    join(workspace, 'src', 'main.rs'),
    `use ox_mf2_parser::parse_message;

fn main() {
    let result = parse_message("Hello, {$name}!");
    assert!(result.diagnostics.is_empty());
    assert!(result.cst.node_count() > 0);
}
`
  )
}

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms))
}

function firstNonEmpty(...values) {
  return values.find(value => value != null && value !== '')
}
