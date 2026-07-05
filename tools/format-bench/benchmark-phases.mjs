export const FORMAT_BENCHMARK_PHASE_NAMES = [
  'format_standard',
  'format_preserve',
  'format_check_cli_e2e',
  'format_check_json',
  'e2e_format'
]

export const FORMAT_BENCHMARK_COST_NAMES = [
  'parse',
  'snapshot_encode',
  'snapshot_decode_access',
  'syntax_traversal_layout_construction',
  'rendering',
  'napi_binding_call',
  'wasm_binding_call',
  'cli_e2e',
  'cli_json_reporter'
]

export const FORMAT_BENCHMARK_PHASES = [
  {
    name: 'format_standard',
    costs: [
      'parse',
      'snapshot_encode',
      'snapshot_decode_access',
      'syntax_traversal_layout_construction',
      'rendering',
      'napi_binding_call',
      'wasm_binding_call'
    ]
  },
  {
    name: 'format_preserve',
    costs: [
      'parse',
      'snapshot_encode',
      'snapshot_decode_access',
      'syntax_traversal_layout_construction',
      'rendering',
      'napi_binding_call',
      'wasm_binding_call'
    ]
  },
  {
    name: 'format_check_cli_e2e',
    costs: ['cli_e2e']
  },
  {
    name: 'format_check_json',
    costs: ['cli_e2e', 'cli_json_reporter']
  },
  {
    name: 'e2e_format',
    costs: ['cli_e2e']
  }
]
