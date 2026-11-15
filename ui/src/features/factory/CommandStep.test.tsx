// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { CommandStep } from './CommandStep';

afterEach(cleanup);

it('binds the installed command request and gives it sufficient time and output budget', async () => {
  const { configuration } = configurationFixture();
  const node = structuredClone(configuration.flow.closure.definitions[0]!.nodes[0]!);
  node.kind = 'build_command';
  node.budget = {
    max_attempts: 2,
    max_elapsed_millis: 60_000,
    max_tokens: 100,
    max_cost_micro_units: 200,
    max_output_bytes: 1_048_576,
  };
  node.permissions.network_hosts = ['already-reviewed.example'];
  const report = configuration.flow.data_schemas[0]!;
  node.outcomes = [{ key: 'checked', kind: 'success', schema: report.reference }];
  const ref = { identity: 'installed.checker', version: 'v1', digest: '4'.repeat(64) };
  const command: components['schemas']['FactoryAuthoringCapabilities']['commands'][number] = {
    alias: 'Repository checks',
    execution: {
      build_configuration: configuration.reference,
      plugin: ref,
      tool: ref,
      model_or_tool: ref,
      task_digest: '5'.repeat(64),
      result_output: 'checks.json',
      result_output_kind: 'report',
    },
    output_definition: report,
    permissions: {
      ...structuredClone(node.permissions),
      network_hosts: [],
      plugins: [ref],
      resources: { ...node.permissions.resources, elapsed_millis: 180_000 },
      outputs: {
        kinds: ['artifact', 'report'],
        max_artifact_count: 1,
        max_artifact_bytes: 4_194_304,
        max_report_count: 1,
        max_report_bytes: 1_048_576,
      },
    },
  };
  const onChange = vi.fn();
  const user = userEvent.setup();
  render(<CommandStep node={node} commands={[command]} onChange={onChange} />);
  await user.click(screen.getByRole('combobox', { name: 'Installed command' }));
  await user.click(screen.getByRole('option', { name: /Repository checks/ }));
  await user.click(screen.getByRole('button', { name: 'Use command' }));
  expect(onChange).toHaveBeenCalledWith(
    expect.objectContaining({
      permissions: expect.objectContaining({
        plugins: [ref],
        network_hosts: ['already-reviewed.example'],
      }),
      budget: {
        max_attempts: 2,
        max_elapsed_millis: 180_000,
        max_tokens: 100,
        max_cost_micro_units: 200,
        max_output_bytes: 5_242_880,
      },
    }),
    [report],
  );
});
