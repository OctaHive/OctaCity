import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { FactoryConfigurationDetails, FactoryRun } from '../../src/features/factory/api';

/** Uses the canonical management fixture verified by the server's OpenAPI tests. */
export function configurationFixture(): FactoryConfigurationDetails {
  return JSON.parse(
    readFileSync(
      resolve(
        '..',
        'server/api/octacity-server-api-rest/fixtures/v1/factory-configuration-details.json',
      ),
      'utf8',
    ),
  );
}

export function runFixture(): FactoryRun {
  return {
    id: '44444444-4444-4444-8444-444444444444',
    project_id: '11111111-1111-4111-8111-111111111111',
    configuration_id: '22222222-2222-4222-8222-222222222222',
    configuration_version: 1,
    external_identity: 'Requested change',
    state: 'waiting',
    source: 'manual',
    version: 2,
    work_id: 'work-a',
    admitted_at_unix_ms: 1,
    updated_at_unix_ms: 2,
    diagnostics: {
      active_node_count: 1,
      base_revision: 'revision-a',
      candidate: null,
      current_nodes: [
        {
          id: 'node-a',
          flow_run_id: 'flow-a',
          cycle_id: 'cycle-a',
          node: 'operator-selected-node',
          attempt: 1,
        },
      ],
      delivery: null,
      escalation: null,
      latest_decision: null,
      budget: {
        limit: {
          max_attempts: 2,
          max_tokens: 20,
          max_cost_micro_units: 20,
          max_elapsed_millis: 100,
          max_output_bytes: 100,
        },
        usage: { attempts: 1, tokens: 0, cost_micro_units: 0, elapsed_millis: 0, output_bytes: 0 },
      },
    },
  };
}
