export const RESOURCE_BENCHMARK_PHASES = [
  {
    name: 'resource_extract',
    costs: ['host_parse_and_entry_extraction']
  },
  {
    name: 'resource_extract_peak_memory',
    costs: ['original_extraction', 'candidate_reextraction']
  },
  {
    name: 'resource_write_back',
    costs: [
      'reescape_measurement',
      'raw_materialization_and_edit_composition',
      'candidate_reparse_and_validation'
    ]
  },
  {
    name: 'fmt_catalog_output_admission_peak_memory',
    costs: [
      'message_parse_and_format',
      'raw_order_candidate_admission',
      'combined_formatter_output_and_admission'
    ]
  },
  {
    name: 'fmt_catalog_check_e2e',
    costs: ['catalog_file_read_and_cli_pipeline']
  },
  {
    name: 'fmt_catalog_write_e2e',
    costs: ['catalog_file_read_write_and_cli_pipeline']
  },
  {
    name: 'sequential_physical_group_aggregation',
    costs: ['ordered_cli_aggregation']
  }
]

/**
 * Resource-owned interval descriptor imported unchanged by messages-bench.
 *
 * The marker vocabulary is checked harness state and is intentionally not
 * serialized into benchmark result documents.
 */
export const resource_host_parse_and_entry_extraction = Object.freeze({
  phase: 'resource_extract',
  cost: 'host_parse_and_entry_extraction',
  boundaryId: 'resource_host_parse_and_entry_extraction',
  metric: 'duration',
  occurrencePolicy: 'per_planned_physical_source_group_sequential',
  firstMarker: 'before_host_format_registry_extract',
  finalMarker: 'complete_checked_extracted_catalog_retained',
  includedMarkers: Object.freeze([
    'host_format_registry_extract',
    'checked_extracted_catalog_construction'
  ]),
  excludedMarkers: Object.freeze([
    'inventory_discovery',
    'physical_grouping',
    'source_snapshot_read',
    'pre_extraction_admission',
    'definition_projection',
    'checksum_observation',
    'result_aggregation'
  ])
})

export const RESOURCE_BENCHMARK_PHASE_NAMES = RESOURCE_BENCHMARK_PHASES.map(phase => phase.name)

export const RESOURCE_BENCHMARK_CORE_PHASE_NAMES = RESOURCE_BENCHMARK_PHASE_NAMES.slice(0, 4)
export const RESOURCE_BENCHMARK_CLI_PHASE_NAMES = RESOURCE_BENCHMARK_PHASE_NAMES.slice(4)
