import { readFileSync } from 'node:fs'

import {
  MESSAGE_BENCHMARK_BOUNDARIES,
  MESSAGE_BENCHMARK_NON_INTERVALS,
  MESSAGE_BENCHMARK_PHASES,
  MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY,
  assertValidBoundaryRegistry,
  resource_host_parse_and_entry_extraction
} from './benchmark-phases.mjs'
import {
  MESSAGE_BENCHMARK_E2E_EXTRACTION_COMPANIONS,
  MESSAGE_BENCHMARK_PROFILE_REVISION,
  MESSAGE_BENCHMARK_REQUIRED_CASES,
  assertValidCompanionMappings,
  benchmarkCaseKey
} from './benchmark-profile.mjs'
import { assertValidChecksumObservationRegistry } from './checksum-observations.mjs'

export const MESSAGE_BENCH_RESULT_SCHEMA_VERSION = '3'

const fixtureSelection = JSON.parse(
  readFileSync(new URL('./fixture-selection.json', import.meta.url), 'utf8')
)
const descriptorByPair = new Map(
  [...MESSAGE_BENCHMARK_BOUNDARIES, ...MESSAGE_BENCHMARK_NON_INTERVALS].map(entry => [
    `${entry.descriptor.phase}\0${entry.descriptor.cost}`,
    entry.descriptor
  ])
)
const ESM_ASSOCIATION_CONTRACT = Object.freeze({
  'dev.intlify/esm-module': {
    localeKind: 'locale',
    deliveryUnitKind: 'unit',
    deliveryUnitSegments: ['main']
  },
  'dev.intlify/loader-map': {
    localeKind: 'shared',
    deliveryUnitKind: 'unit',
    deliveryUnitSegments: ['main']
  },
  'dev.intlify/typescript-accessor': {
    localeKind: 'shared',
    deliveryUnitKind: 'shared'
  }
})

/**
 * Validate one aggregate messages benchmark result.
 *
 * Performance magnitudes remain observational. This gate checks only the
 * closed profile, record structure, required-case coverage, and companion
 * integrity.
 *
 * @param value - Parsed benchmark result.
 */
export function assertValidMessageBenchmarkResult(value) {
  assertValidBoundaryRegistry()
  assertValidChecksumObservationRegistry()
  assertValidCompanionMappings()
  assertObject(value, 'result')
  assertExactFields(value, '/', [
    'schemaVersion',
    'benchmarkProfileRevision',
    'tool',
    'version',
    'generatedAt',
    'iterations',
    'warmupIterations',
    'phases',
    'fixtures',
    'results'
  ])
  assertEqual(value.schemaVersion, MESSAGE_BENCH_RESULT_SCHEMA_VERSION, '/schemaVersion')
  assertEqual(
    value.benchmarkProfileRevision,
    MESSAGE_BENCHMARK_PROFILE_REVISION,
    '/benchmarkProfileRevision'
  )
  assertEqual(value.tool, 'intlify-messages-bench', '/tool')
  assertString(value.version, '/version')
  assertString(value.generatedAt, '/generatedAt')
  assertPositiveInteger(value.iterations, '/iterations')
  assertNonNegativeInteger(value.warmupIterations, '/warmupIterations')
  assertEqual(JSON.stringify(value.phases), JSON.stringify(MESSAGE_BENCHMARK_PHASES), '/phases')
  const fixtures = assertFixtures(value.fixtures)
  const results = assertResults(value.results, fixtures, value.iterations)
  assertRequiredCases(results)
  assertExtractionCompanions(results)
  assertArtifactSizeComparisons(results, fixtures)
}

function assertFixtures(value) {
  assertObject(value, '/fixtures')
  assertEqual(value.revision, fixtureSelection.revision, '/fixtures/revision')
  assertEqual(JSON.stringify(value), JSON.stringify(fixtureSelection), '/fixtures')
  const fixtures = new Map()
  for (const profile of fixtureSelection.profiles) {
    if (fixtures.has(profile.name)) {
      throw new Error('/fixtures contains duplicate names')
    }
    fixtures.set(profile.name, profile)
  }
  if (fixtures.has(fixtureSelection.projectFixture.name)) {
    throw new Error('/fixtures contains duplicate names')
  }
  fixtures.set(fixtureSelection.projectFixture.name, fixtureSelection.projectFixture)
  return fixtures
}

function assertResults(value, fixtures, defaultIterations) {
  assertArray(value, '/results')
  if (value.length === 0) {
    throw new Error('/results must not be empty')
  }
  const byCase = new Map()
  for (const [index, result] of value.entries()) {
    const pointer = `/results/${index}`
    assertObject(result, pointer)
    const commonFields = [
      'status',
      'phase',
      'cost',
      'fixture',
      'fixtureRevision',
      'variant',
      'scale',
      'runtime',
      'executionModel',
      'operation',
      'metric',
      'inputCount',
      'outputCount'
    ]
    assertEqual(result.status, 'measured', `${pointer}/status`)
    for (const field of [
      'phase',
      'cost',
      'fixture',
      'variant',
      'runtime',
      'executionModel',
      'operation',
      'metric'
    ]) {
      assertString(result[field], `${pointer}/${field}`)
    }
    assertEqual(result.executionModel, 'in_process', `${pointer}/executionModel`)
    assertPositiveInteger(result.fixtureRevision, `${pointer}/fixtureRevision`)
    assertPositiveInteger(result.scale, `${pointer}/scale`)
    assertNonNegativeInteger(result.inputCount, `${pointer}/inputCount`)
    assertNonNegativeInteger(result.outputCount, `${pointer}/outputCount`)
    const fixture = fixtures.get(result.fixture)
    if (!fixture) {
      throw new Error(`${pointer}/fixture is undeclared`)
    }
    assertEqual(result.fixtureRevision, fixture.revision, `${pointer}/fixtureRevision`)
    if (!fixture.scales.includes(result.scale)) {
      throw new Error(`${pointer}/scale is undeclared by its fixture`)
    }
    if (!fixture.variants.includes(result.variant)) {
      throw new Error(`${pointer}/variant is undeclared by its fixture`)
    }

    const descriptor = descriptorByPair.get(`${result.phase}\0${result.cost}`)
    if (!descriptor) {
      throw new Error(`${pointer} uses an inactive phase/cost`)
    }
    const interval = Object.hasOwn(descriptor, 'boundaryId')
    if (interval) {
      assertString(result.boundaryId, `${pointer}/boundaryId`)
      assertEqual(result.boundaryId, descriptor.boundaryId, `${pointer}/boundaryId`)
      assertEqual(result.operation, descriptor.boundaryId, `${pointer}/operation`)
    } else {
      if (Object.hasOwn(result, 'boundaryId')) {
        throw new Error(`${pointer}/boundaryId is forbidden for a non-interval metric`)
      }
      assertEqual(result.operation, descriptor.cost, `${pointer}/operation`)
    }
    assertEqual(result.metric, descriptor.metric, `${pointer}/metric`)
    if (interval && /_v\d+$/.test(result.boundaryId)) {
      throw new Error(`${pointer}/boundaryId must not carry a version suffix`)
    }

    if (result.metric === 'duration') {
      const durationFields = ['iterations', 'elapsedMs', 'checksum']
      const resourceFields =
        descriptor === resource_host_parse_and_entry_extraction
          ? ['physicalGroupCount', 'hostByteCount', 'entryCount']
          : []
      const artifactFields =
        result.phase === 'message_export_esm' && result.cost === 'export_artifact_set_construction'
          ? ['artifacts']
          : []
      assertExactFields(result, pointer, [
        ...commonFields,
        'boundaryId',
        ...durationFields,
        ...resourceFields,
        ...artifactFields
      ])
      assertPositiveInteger(result.iterations, `${pointer}/iterations`)
      assertEqual(result.iterations, defaultIterations, `${pointer}/iterations`)
      assertNonNegativeNumber(result.elapsedMs, `${pointer}/elapsedMs`)
      assertU32(result.checksum, `${pointer}/checksum`)
      if (artifactFields.length !== 0) {
        assertArtifactObservations(result.artifacts, `${pointer}/artifacts`)
      }
    } else if (result.metric === 'peak_live_memory') {
      assertEqual(result.metric, 'peak_live_memory', `${pointer}/metric`)
      assertExactFields(result, pointer, [
        ...commonFields,
        'boundaryId',
        'peakLiveBytes',
        'retainedLiveBytes',
        'allocationCount'
      ])
      assertNonNegativeInteger(result.peakLiveBytes, `${pointer}/peakLiveBytes`)
      assertNonNegativeInteger(result.retainedLiveBytes, `${pointer}/retainedLiveBytes`)
      assertNonNegativeInteger(result.allocationCount, `${pointer}/allocationCount`)
    } else {
      assertEqual(result.metric, 'artifact_payload_size', `${pointer}/metric`)
      assertExactFields(result, pointer, [
        ...commonFields,
        'baselineArtifacts',
        'linkedArtifacts',
        'baselineAssociations',
        'linkedAssociations',
        'entryRoots',
        'buckets'
      ])
      assertArtifactObservations(result.baselineArtifacts, `${pointer}/baselineArtifacts`)
      assertArtifactObservations(result.linkedArtifacts, `${pointer}/linkedArtifacts`)
      assertArtifactAssociations(
        result.baselineAssociations,
        result.baselineArtifacts,
        `${pointer}/baselineAssociations`
      )
      assertArtifactAssociations(
        result.linkedAssociations,
        result.linkedArtifacts,
        `${pointer}/linkedAssociations`
      )
      assertPathVector(result.entryRoots, `${pointer}/entryRoots`)
      assertArray(result.buckets, `${pointer}/buckets`)
    }

    if (descriptor === resource_host_parse_and_entry_extraction) {
      assertPositiveInteger(result.physicalGroupCount, `${pointer}/physicalGroupCount`)
      assertPositiveInteger(result.hostByteCount, `${pointer}/hostByteCount`)
      assertNonNegativeInteger(result.entryCount, `${pointer}/entryCount`)
    }

    const key = benchmarkCaseKey(result)
    const records = byCase.get(key) ?? []
    records.push(result)
    byCase.set(key, records)
  }
  return byCase
}

function assertRequiredCases(byCase) {
  const requiredKeys = new Set()
  for (const required of MESSAGE_BENCHMARK_REQUIRED_CASES) {
    const key = benchmarkCaseKey(required)
    if (requiredKeys.has(key)) {
      throw new Error(`required-case matrix duplicates ${key}`)
    }
    requiredKeys.add(key)
    const records = byCase.get(key) ?? []
    if (records.length !== 1) {
      throw new Error(`/results must contain required case exactly once: ${key}`)
    }
  }
  for (const [key, records] of byCase) {
    if (records.length !== 1) {
      throw new Error(`/results duplicates case: ${key}`)
    }
  }
}

function assertExtractionCompanions(byCase) {
  const usedExtraction = new Set()
  for (const [index, mapping] of MESSAGE_BENCHMARK_E2E_EXTRACTION_COMPANIONS.entries()) {
    const e2eKey = benchmarkCaseKey(mapping.e2e)
    const extractionKey = benchmarkCaseKey(mapping.extraction)
    const e2e = byCase.get(e2eKey) ?? []
    const extraction = byCase.get(extractionKey) ?? []
    if (e2e.length !== 1 || extraction.length !== 1) {
      throw new Error(`/companions/${index} is missing or duplicated`)
    }
    if (usedExtraction.has(extractionKey)) {
      throw new Error(`/companions/${index} shares an extraction record`)
    }
    usedExtraction.add(extractionKey)
    for (const field of ['fixture', 'variant', 'scale', 'runtime', 'executionModel']) {
      if (e2e[0][field] !== extraction[0][field]) {
        throw new Error(`/companions/${index} mismatches ${field}`)
      }
    }
  }
}

function assertArtifactSizeComparisons(byCase, fixtures) {
  for (const required of MESSAGE_BENCHMARK_REQUIRED_CASES.filter(
    record => record.metric === 'artifact_payload_size'
  )) {
    const comparison = (byCase.get(benchmarkCaseKey(required)) ?? [])[0]
    if (!comparison) {
      continue
    }
    const sharedIdentity = {
      fixture: comparison.fixture,
      scale: comparison.scale,
      runtime: comparison.runtime,
      executionModel: comparison.executionModel
    }
    const exporterDescriptor = descriptorByPair.get(
      'message_export_esm\0export_artifact_set_construction'
    )
    if (!exporterDescriptor) {
      throw new Error('/descriptors omits the ESM artifact-set construction record')
    }
    const exporterRecord = variant => {
      const key = benchmarkCaseKey({
        phase: 'message_export_esm',
        cost: 'export_artifact_set_construction',
        ...sharedIdentity,
        variant,
        operation: exporterDescriptor.boundaryId,
        metric: exporterDescriptor.metric
      })
      const records = byCase.get(key) ?? []
      if (records.length !== 1) {
        throw new Error(
          `/results must contain one exporter artifact record for ${comparison.fixture}/${comparison.scale}/${variant}`
        )
      }
      return records[0]
    }
    const baseline = exporterRecord('all_definitions_baseline')
    const linked = exporterRecord('linked_output')
    assertJsonEqual(
      comparison.baselineArtifacts,
      baseline.artifacts,
      '/artifactSize/baselineArtifacts'
    )
    assertJsonEqual(comparison.linkedArtifacts, linked.artifacts, '/artifactSize/linkedArtifacts')

    const fixture = fixtures.get(comparison.fixture)
    if (!fixture?.exporter || !Array.isArray(fixture.entryRoots)) {
      throw new Error('/artifactSize fixture metadata is incomplete')
    }
    assertJsonEqual(comparison.entryRoots, fixture.entryRoots, '/artifactSize/entryRoots')
    assertAcceptedArtifactKinds(comparison.baselineArtifacts, fixture, '/artifactSize/baseline')
    assertAcceptedArtifactKinds(comparison.linkedArtifacts, fixture, '/artifactSize/linked')
    assertAssociationSemantics(
      comparison.baselineAssociations,
      comparison.baselineArtifacts,
      fixture,
      '/artifactSize/baselineAssociations'
    )
    assertAssociationSemantics(
      comparison.linkedAssociations,
      comparison.linkedArtifacts,
      fixture,
      '/artifactSize/linkedAssociations'
    )
    const expectedBuckets = deriveArtifactSizeBuckets(
      comparison.baselineArtifacts,
      comparison.baselineAssociations,
      comparison.linkedArtifacts,
      comparison.linkedAssociations,
      comparison.entryRoots
    )
    assertJsonEqual(comparison.buckets, expectedBuckets, '/artifactSize/buckets')
  }
}

function assertArtifactObservations(value, pointer) {
  assertArray(value, pointer)
  let previousPath
  const paths = new Set()
  for (const [index, artifact] of value.entries()) {
    const itemPointer = `${pointer}/${index}`
    assertObject(artifact, itemPointer)
    assertExactFields(artifact, itemPointer, [
      'path',
      'kind',
      'formatVersion',
      'payloadBytes',
      'payloadFingerprint',
      'relationships'
    ])
    assertPath(artifact.path, `${itemPointer}/path`)
    const pathKey = artifactPathKey(artifact.path)
    if (paths.has(pathKey) || (previousPath && comparePath(previousPath, artifact.path) >= 0)) {
      throw new Error(`${itemPointer}/path must be unique and canonically ordered`)
    }
    paths.add(pathKey)
    previousPath = artifact.path
    assertString(artifact.kind, `${itemPointer}/kind`)
    assertObject(artifact.formatVersion, `${itemPointer}/formatVersion`)
    assertExactFields(artifact.formatVersion, `${itemPointer}/formatVersion`, ['major', 'minor'])
    assertNonNegativeInteger(artifact.formatVersion.major, `${itemPointer}/formatVersion/major`)
    assertNonNegativeInteger(artifact.formatVersion.minor, `${itemPointer}/formatVersion/minor`)
    assertNonNegativeInteger(artifact.payloadBytes, `${itemPointer}/payloadBytes`)
    assertObject(artifact.payloadFingerprint, `${itemPointer}/payloadFingerprint`)
    assertExactFields(artifact.payloadFingerprint, `${itemPointer}/payloadFingerprint`, [
      'algorithm',
      'digest'
    ])
    assertEqual(
      artifact.payloadFingerprint.algorithm,
      'blake3-256',
      `${itemPointer}/payloadFingerprint/algorithm`
    )
    if (!/^[0-9a-f]{64}$/.test(artifact.payloadFingerprint.digest)) {
      throw new Error(`${itemPointer}/payloadFingerprint/digest must be lowercase BLAKE3-256 hex`)
    }
    assertArray(artifact.relationships, `${itemPointer}/relationships`)
    let previousRelationship
    for (const [relationshipIndex, relationship] of artifact.relationships.entries()) {
      const relationshipPointer = `${itemPointer}/relationships/${relationshipIndex}`
      assertObject(relationship, relationshipPointer)
      assertExactFields(relationship, relationshipPointer, ['kind', 'target'])
      if (!['eager-load', 'lazy-load'].includes(relationship.kind)) {
        throw new Error(`${relationshipPointer}/kind is unknown`)
      }
      assertPath(relationship.target, `${relationshipPointer}/target`)
      if (previousRelationship && compareRelationship(previousRelationship, relationship) >= 0) {
        throw new Error(`${relationshipPointer} must be unique and canonically ordered`)
      }
      previousRelationship = relationship
    }
  }
  for (const [index, artifact] of value.entries()) {
    for (const [relationshipIndex, relationship] of artifact.relationships.entries()) {
      if (!paths.has(artifactPathKey(relationship.target))) {
        throw new Error(
          `${pointer}/${index}/relationships/${relationshipIndex}/target is unresolved`
        )
      }
    }
  }
}

function assertArtifactAssociations(value, artifacts, pointer) {
  assertArray(value, pointer)
  if (value.length !== artifacts.length) {
    throw new Error(`${pointer} must classify every artifact exactly once`)
  }
  for (const [index, association] of value.entries()) {
    const itemPointer = `${pointer}/${index}`
    assertObject(association, itemPointer)
    assertExactFields(association, itemPointer, ['path', 'locale', 'deliveryUnit'])
    assertPath(association.path, `${itemPointer}/path`)
    assertJsonEqual(association.path, artifacts[index].path, `${itemPointer}/path`)
    assertLocaleBucket(association.locale, `${itemPointer}/locale`)
    assertDeliveryUnitBucket(association.deliveryUnit, `${itemPointer}/deliveryUnit`)
  }
}

function assertLocaleBucket(value, pointer) {
  assertObject(value, pointer)
  if (value.kind === 'shared') {
    assertExactFields(value, pointer, ['kind'])
    return
  }
  assertEqual(value.kind, 'locale', `${pointer}/kind`)
  assertExactFields(value, pointer, ['kind', 'value'])
  assertString(value.value, `${pointer}/value`)
}

function assertDeliveryUnitBucket(value, pointer) {
  assertObject(value, pointer)
  if (value.kind === 'shared') {
    assertExactFields(value, pointer, ['kind'])
    return
  }
  assertEqual(value.kind, 'unit', `${pointer}/kind`)
  assertExactFields(value, pointer, ['kind', 'segments'])
  assertPath(value.segments, `${pointer}/segments`)
}

function assertPathVector(value, pointer) {
  assertArray(value, pointer)
  const seen = new Set()
  for (const [index, path] of value.entries()) {
    assertPath(path, `${pointer}/${index}`)
    const key = artifactPathKey(path)
    if (seen.has(key)) {
      throw new Error(`${pointer}/${index} duplicates an entry root`)
    }
    seen.add(key)
  }
}

function assertPath(value, pointer) {
  assertArray(value, pointer)
  if (value.length === 0) {
    throw new Error(`${pointer} must be a non-empty path`)
  }
  for (const [index, segment] of value.entries()) {
    assertString(segment, `${pointer}/${index}`)
  }
}

function assertAcceptedArtifactKinds(artifacts, fixture, pointer) {
  const accepted = new Set(
    fixture.exporter.acceptedKinds.map(
      item => `${item.kind}\0${item.formatVersion.major}\0${item.formatVersion.minor}`
    )
  )
  for (const [index, artifact] of artifacts.entries()) {
    const identity = `${artifact.kind}\0${artifact.formatVersion.major}\0${artifact.formatVersion.minor}`
    if (!accepted.has(identity)) {
      throw new Error(`${pointer}Artifacts/${index} uses an undeclared kind/version`)
    }
  }
}

function assertAssociationSemantics(associations, artifacts, fixture, pointer) {
  const productionLocales = new Set(fixture.exporter.options.productionLocales)
  for (const [index, association] of associations.entries()) {
    if (association.locale.kind === 'locale' && !productionLocales.has(association.locale.value)) {
      throw new Error(`${pointer}/${index}/locale is not a production locale`)
    }
    const artifactKind = artifacts[index].kind
    const expected = ESM_ASSOCIATION_CONTRACT[artifactKind]
    if (!expected) {
      throw new Error(`${pointer}/${index} has no built-in ESM association contract`)
    }
    if (
      association.locale.kind !== expected.localeKind ||
      association.deliveryUnit.kind !== expected.deliveryUnitKind ||
      (expected.deliveryUnitSegments &&
        JSON.stringify(association.deliveryUnit.segments) !==
          JSON.stringify(expected.deliveryUnitSegments))
    ) {
      throw new Error(`${pointer}/${index} violates the built-in ESM association contract`)
    }
  }
}

function deriveArtifactSizeBuckets(
  baselineArtifacts,
  baselineAssociations,
  linkedArtifacts,
  linkedAssociations,
  entryRoots
) {
  const baseline = artifactTotals(baselineArtifacts, baselineAssociations, entryRoots)
  const linked = artifactTotals(linkedArtifacts, linkedAssociations, entryRoots)
  const buckets = [
    sizeBucket('complete-set', { kind: 'all' }, baseline.complete, linked.complete),
    sizeBucket(
      'initial-eager-load',
      { kind: 'entry-root-closure' },
      baseline.initial,
      linked.initial
    )
  ]
  const localeKeys = unionSorted(baseline.locales, linked.locales, bucketKeyCompare)
  for (const key of localeKeys) {
    buckets.push(
      sizeBucket(
        'locale',
        JSON.parse(key),
        baseline.locales.get(key) ?? 0,
        linked.locales.get(key) ?? 0
      )
    )
  }
  const unitKeys = unionSorted(baseline.units, linked.units, bucketKeyCompare)
  for (const key of unitKeys) {
    buckets.push(
      sizeBucket(
        'delivery-unit',
        JSON.parse(key),
        baseline.units.get(key) ?? 0,
        linked.units.get(key) ?? 0
      )
    )
  }
  const kindKeys = unionSorted(baseline.kinds, linked.kinds, compareByteOrder)
  for (const key of kindKeys) {
    buckets.push(
      sizeBucket(
        'kind',
        { kind: 'artifact-kind', value: key },
        baseline.kinds.get(key) ?? 0,
        linked.kinds.get(key) ?? 0
      )
    )
  }
  return buckets
}

function artifactTotals(artifacts, associations, entryRoots) {
  const byPath = new Map(artifacts.map(artifact => [artifactPathKey(artifact.path), artifact]))
  const roots = entryRoots.map(artifactPathKey)
  for (const root of roots) {
    if (!byPath.has(root)) {
      throw new Error('/artifactSize/entryRoots contains an unresolved path')
    }
  }
  const eager = new Set()
  const pending = [...roots]
  while (pending.length !== 0) {
    const path = pending.pop()
    if (eager.has(path)) {
      continue
    }
    eager.add(path)
    for (const relationship of byPath.get(path).relationships) {
      if (relationship.kind === 'eager-load') {
        pending.push(artifactPathKey(relationship.target))
      }
    }
  }
  const locales = new Map()
  const units = new Map()
  const kinds = new Map()
  let complete = 0
  let initial = 0
  for (const [index, artifact] of artifacts.entries()) {
    const bytes = artifact.payloadBytes
    complete += bytes
    if (eager.has(artifactPathKey(artifact.path))) {
      initial += bytes
    }
    addBucket(locales, JSON.stringify(associations[index].locale), bytes)
    addBucket(units, JSON.stringify(associations[index].deliveryUnit), bytes)
    addBucket(kinds, artifact.kind, bytes)
  }
  assertSafeSum(complete, '/artifactSize/completeBytes')
  assertSafeSum(initial, '/artifactSize/initialBytes')
  return { complete, initial, locales, units, kinds }
}

function addBucket(map, key, bytes) {
  const total = (map.get(key) ?? 0) + bytes
  assertSafeSum(total, '/artifactSize/bucketBytes')
  map.set(key, total)
}

function sizeBucket(axis, identity, baselineBytes, linkedBytes) {
  const delta = linkedBytes - baselineBytes
  return {
    axis,
    identity,
    baselineBytes,
    linkedBytes,
    difference: {
      direction: delta === 0 ? 'equal' : delta > 0 ? 'linked-larger' : 'baseline-larger',
      bytes: Math.abs(delta)
    },
    ratio:
      baselineBytes === 0
        ? { state: 'undefined-zero-baseline' }
        : { state: 'defined', numerator: linkedBytes, denominator: baselineBytes }
  }
}

function unionSorted(left, right, compare) {
  return [...new Set([...left.keys(), ...right.keys()])].sort(compare)
}

function bucketKeyCompare(left, right) {
  const leftValue = JSON.parse(left)
  const rightValue = JSON.parse(right)
  if (leftValue.kind === 'shared') {
    return rightValue.kind === 'shared' ? 0 : 1
  }
  if (rightValue.kind === 'shared') {
    return -1
  }
  return compareByteOrder(JSON.stringify(leftValue), JSON.stringify(rightValue))
}

function compareRelationship(left, right) {
  const rank = { 'eager-load': 0, 'lazy-load': 1 }
  return rank[left.kind] - rank[right.kind] || comparePath(left.target, right.target)
}

function comparePath(left, right) {
  const length = Math.min(left.length, right.length)
  for (let index = 0; index < length; index += 1) {
    const compared = compareByteOrder(left[index], right[index])
    if (compared !== 0) {
      return compared
    }
  }
  return left.length - right.length
}

function compareByteOrder(left, right) {
  return Buffer.compare(Buffer.from(left), Buffer.from(right))
}

function artifactPathKey(path) {
  return JSON.stringify(path)
}

function assertJsonEqual(actual, expected, pointer) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${pointer} mismatch`)
  }
}

function assertSafeSum(value, pointer) {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${pointer} exceeds the safe serialized integer range`)
  }
}

function assertExactFields(value, pointer, allowedFields) {
  const allowed = new Set(allowedFields)
  for (const field of Object.keys(value)) {
    if (!allowed.has(field)) {
      const separator = pointer === '/' ? '' : '/'
      throw new Error(`${pointer}${separator}${field} is unknown`)
    }
  }
}

function assertObject(value, pointer) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${pointer} must be an object`)
  }
}

function assertArray(value, pointer) {
  if (!Array.isArray(value)) {
    throw new Error(`${pointer} must be an array`)
  }
}

function assertString(value, pointer) {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${pointer} must be a non-empty string`)
  }
}

function assertEqual(actual, expected, pointer) {
  if (actual !== expected) {
    throw new Error(`${pointer} mismatch: expected ${JSON.stringify(expected)}`)
  }
}

function assertPositiveInteger(value, pointer) {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new Error(`${pointer} must be a positive safe integer`)
  }
}

function assertNonNegativeInteger(value, pointer) {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${pointer} must be a non-negative safe integer`)
  }
}

function assertU32(value, pointer) {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
    throw new Error(`${pointer} must be a u32`)
  }
}

function assertNonNegativeNumber(value, pointer) {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    throw new Error(`${pointer} must be a non-negative finite number`)
  }
}

void MESSAGE_BENCHMARK_OVERLAP_TOPOLOGY
