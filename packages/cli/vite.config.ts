import { defineConfig } from 'vite-plus'

export default defineConfig({
  run: {
    tasks: {
      build: {
        command: 'node ../../scripts/cli-local-validation.mjs build',
        cache: false
      },
      schema: {
        command:
          'cargo run -p intlify_cli --example generate_config_schema -- schema/config.schema.json',
        cache: false
      },
      'schema:check': {
        command:
          'cargo run -p intlify_cli --example generate_config_schema -- --check schema/config.schema.json',
        cache: false
      },
      'pack:check': {
        command: 'node ../../scripts/cli-local-validation.mjs pack-check',
        dependsOn: ['build'],
        cache: false
      },
      smoke: {
        command: 'node ../../scripts/cli-local-validation.mjs smoke',
        dependsOn: ['build'],
        cache: false
      },
      'bench:startup': {
        command: 'node ../../scripts/cli-local-validation.mjs bench-startup',
        dependsOn: ['build'],
        cache: false
      }
    }
  }
})
