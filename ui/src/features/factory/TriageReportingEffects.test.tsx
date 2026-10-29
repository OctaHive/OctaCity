// @vitest-environment jsdom
import { useState } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { createConsoleQueryClient } from '../../app/query';
import { factoryApi } from './api';
import { TriageReportingEffects } from './TriageReportingEffects';
import type { TriageSourceDraft } from './TriageSourceSelection';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
function fixture(shape?: components['schemas']['FactoryFlowValueSchema']) {
  const configuration = configurationFixture().configuration;
  const choices = integrationFixture('WorkSourceInputChoices', 1);
  const read = choices.items.find((item) => item.purpose === 'task_read')!;
  const writer = choices.items.find((item) => item.purpose === 'task_update')!;
  writer.reporting = {
    mapping: { statuses: [] },
    editable: { labels: ['reviewed'], states: ['open'] },
  };
  const original = structuredClone(configuration.flow);
  const root = original.closure.definitions[0]!;
  const node = root.nodes[0]!;
  node.action = {
    action: 'set_status',
    plugin: writer.binding.capability,
    parameters: { status: 'reviewed', outbox: { max_attempts: 2, retry_delay_millis: 5 } },
  };
  node.outcomes = [
    {
      key: 'queued',
      kind: 'success',
      schema: original.data_schemas[0]!.reference,
      public_result: false,
    },
  ];
  const schema = original.data_schemas[0]!;
  schema.shape = {
    type: 'object',
    fields: {
      receipt: { required: true, value: { type: 'string', min_bytes: 64, max_bytes: 64 } },
    },
  };
  writer.reporting.action = {
    plugin: writer.binding.capability,
    action: 'set_status',
    output: schema,
    outcome: node.outcomes[0]!,
  };
  const result = shape
    ? { reference: { identity: 'classified.result', version: 'v1', digest: 'f'.repeat(64) }, shape }
    : schema;
  const gate = {
    ...node,
    kind: 'deterministic_gate' as const,
    key: 'accept_gate',
    action: null,
    outcomes: [
      { key: 'accepted', kind: 'success' as const, schema: result.reference, public_result: true },
      { key: 'rejected', kind: 'success' as const, schema: result.reference, public_result: true },
    ],
  };
  const selection: TriageSourceDraft = {
    input: {
      id: 'input',
      source: read.binding,
      reporter: writer.binding,
      connection: {
        id: choices.connection_id,
        version: choices.connection_version,
        access_version: choices.access_version,
        source_configuration: read.connection_configuration,
        reporter_configuration: writer.connection_configuration,
      },
    },
    repository: {
      id: 'repo',
      version: 1,
      definition_digest: 'f'.repeat(64),
      project_id: configuration.reference.project_id,
    },
    writer,
  };
  const listFlows = vi
    .spyOn(factoryApi, 'listFlows')
    .mockResolvedValue({ items: [], total: 0, next_cursor: null });
  const observed = vi.fn();
  function Harness() {
    const [value, setValue] = useState<
      NonNullable<components['schemas']['FactoryPresetResolutionRequest']['effects']>
    >({});
    return (
      <TriageReportingEffects
        flow={{
          root: { ...root, nodes: [gate] },
          data_schemas: [...original.data_schemas, ...(shape ? [result] : [])],
          limits: original.limits,
        }}
        selection={selection}
        value={value}
        onChange={(next) => {
          observed(next);
          setValue(next);
        }}
      />
    );
  }
  const queryClient = createConsoleQueryClient();
  const tree = () => (
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <Harness />
      </MemoryRouter>
    </QueryClientProvider>
  );
  const view = render(tree());
  return { observed, read, writer, schema, listFlows, refresh: () => view.rerender(tree()) };
}

it('maps finite accepted outcomes through installed Reporter choices and preserves other outcomes explicitly', async () => {
  const { observed, read, writer, schema, listFlows } = fixture();
  const accepted = within(await screen.findByRole('group', { name: 'accepted' }));
  await userEvent.clear(screen.getByRole('spinbutton', { name: 'Reporter attempts' }));
  await userEvent.type(screen.getByRole('spinbutton', { name: 'Reporter attempts' }), '2');
  await userEvent.clear(screen.getByRole('spinbutton', { name: 'Reporter retry delay (ms)' }));
  await userEvent.type(screen.getByRole('spinbutton', { name: 'Reporter retry delay (ms)' }), '5');
  await userEvent.click(accepted.getByRole('checkbox', { name: 'Report this outcome' }));
  await userEvent.click(accepted.getByRole('checkbox', { name: 'Add label: reviewed' }));
  await userEvent.click(screen.getByRole('button', { name: 'Use reviewed reporting effects' }));
  expect(observed).toHaveBeenLastCalledWith({
    accept_gate: {
      id: expect.any(String),
      definition: expect.objectContaining({
        target: expect.objectContaining({
          source: read.binding,
          reporter: writer.binding,
          policy: writer.reporting?.editable,
        }),
        outcomes: {
          accepted: { status: 'accepted', add: ['reviewed'], remove: [], state: null },
          rejected: null,
        },
        dimensions: [],
        action_outcome: 'queued',
        retry: { max_attempts: 2, retry_delay_millis: 5 },
        receipt: schema,
      }),
    },
  });
  expect(screen.getByRole('link', { name: 'Review label scope' }).getAttribute('href')).toContain(
    '/integrations/',
  );
  expect(screen.queryByRole('textbox', { name: /label|json|digest/i })).toBeNull();
  expect(listFlows).not.toHaveBeenCalled();
});

it('uses one explicit effect switch so disabling a configured outcome leaves no hidden label write', async () => {
  const { observed } = fixture();
  const accepted = within(await screen.findByRole('group', { name: 'accepted' }));
  await userEvent.click(accepted.getByRole('checkbox', { name: 'Report this outcome' }));
  expect(accepted.queryByRole('checkbox', { name: 'Customize status mapping' })).toBeNull();
  await userEvent.click(accepted.getByRole('checkbox', { name: 'Add label: reviewed' }));
  await userEvent.click(accepted.getByRole('checkbox', { name: 'Report this outcome' }));
  await userEvent.click(screen.getByRole('button', { name: 'Use reviewed reporting effects' }));
  expect(observed).toHaveBeenLastCalledWith({
    accept_gate: {
      id: expect.any(String),
      definition: expect.objectContaining({ outcomes: { accepted: null, rejected: null } }),
    },
  });
});

it('maps a selected finite classification field independently and preserves its other choices', async () => {
  const { observed } = fixture({
    type: 'object',
    fields: {
      assessment: {
        required: true,
        value: {
          type: 'object',
          fields: { size: { required: true, value: { type: 'enum', values: ['small', 'large'] } } },
        },
      },
    },
  });
  await userEvent.click(await screen.findByRole('combobox', { name: 'Classification field' }));
  await userEvent.click(screen.getByRole('option', { name: 'assessment / size' }));
  const outcomes = within(screen.getByRole('group', { name: 'Report dimension for outcomes' }));
  await userEvent.click(outcomes.getByRole('checkbox', { name: 'accepted' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add classification dimension' }));
  const dimension = within(screen.getByRole('region', { name: 'Dimension assessment / size' }));
  const small = within(dimension.getByRole('group', { name: 'small' }));
  await userEvent.click(small.getByRole('checkbox', { name: 'Report this choice' }));
  await userEvent.click(small.getByRole('checkbox', { name: 'Add label: reviewed' }));
  await userEvent.click(screen.getByRole('button', { name: 'Use reviewed reporting effects' }));
  expect(observed).toHaveBeenLastCalledWith({
    accept_gate: {
      id: expect.any(String),
      definition: expect.objectContaining({
        outcomes: { accepted: null, rejected: null },
        dimensions: [
          {
            key: expect.any(String),
            path: ['assessment', 'size'],
            outcomes: ['accepted'],
            cases: [
              {
                selection: { selection: 'literal', value: 'small' },
                effects: { status: 'size', add: ['reviewed'], remove: [], state: null },
              },
              { selection: { selection: 'literal', value: 'large' }, effects: null },
            ],
          },
        ],
      }),
    },
  });
});

it('partitions the declared priority range into complete independent bands without changing existing effects on split', async () => {
  const { observed } = fixture({
    type: 'object',
    fields: {
      assessment: {
        required: true,
        value: {
          type: 'object',
          fields: {
            priority: { required: true, value: { type: 'integer', minimum: 0, maximum: 100 } },
          },
        },
      },
    },
  });
  await userEvent.click(await screen.findByRole('combobox', { name: 'Classification field' }));
  await userEvent.click(screen.getByRole('option', { name: 'assessment / priority' }));
  await userEvent.click(
    within(screen.getByRole('group', { name: 'Report dimension for outcomes' })).getByRole(
      'checkbox',
      { name: 'accepted' },
    ),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Add classification dimension' }));
  const dimension = within(screen.getByRole('region', { name: 'Dimension assessment / priority' }));
  const full = within(dimension.getByRole('group', { name: '0–100' }));
  await userEvent.clear(full.getByRole('spinbutton', { name: 'Split at' }));
  await userEvent.type(full.getByRole('spinbutton', { name: 'Split at' }), '49');
  await userEvent.click(full.getByRole('button', { name: 'Split band' }));
  const upper = within(dimension.getByRole('group', { name: '50–100' }));
  await userEvent.click(upper.getByRole('checkbox', { name: 'Report this choice' }));
  await userEvent.click(upper.getByRole('checkbox', { name: 'Add label: reviewed' }));
  await userEvent.clear(upper.getByRole('spinbutton', { name: 'Split at' }));
  await userEvent.type(upper.getByRole('spinbutton', { name: 'Split at' }), '100');
  expect(upper.getByRole('button', { name: 'Split band' })).toHaveProperty('disabled', true);
  await userEvent.clear(upper.getByRole('spinbutton', { name: 'Split at' }));
  await userEvent.type(upper.getByRole('spinbutton', { name: 'Split at' }), '74');
  await userEvent.click(upper.getByRole('button', { name: 'Split band' }));
  const middle = within(dimension.getByRole('group', { name: '50–74' }));
  expect(middle.getByRole('checkbox', { name: 'Add label: reviewed' })).toHaveProperty(
    'checked',
    true,
  );
  expect(
    within(dimension.getByRole('group', { name: '75–100' })).getByRole('checkbox', {
      name: 'Add label: reviewed',
    }),
  ).toHaveProperty('checked', true);
  await userEvent.click(middle.getByRole('checkbox', { name: 'Report this choice' }));
  await userEvent.click(screen.getByRole('button', { name: 'Use reviewed reporting effects' }));
  expect(observed).toHaveBeenLastCalledWith({
    accept_gate: {
      id: expect.any(String),
      definition: expect.objectContaining({
        dimensions: [
          {
            key: expect.any(String),
            path: ['assessment', 'priority'],
            outcomes: ['accepted'],
            cases: [
              { selection: { selection: 'integer_range', minimum: 0, maximum: 49 }, effects: null },
              {
                selection: { selection: 'integer_range', minimum: 50, maximum: 74 },
                effects: null,
              },
              {
                selection: { selection: 'integer_range', minimum: 75, maximum: 100 },
                effects: { status: 'priority', add: ['reviewed'], remove: [], state: null },
              },
            ],
          },
        ],
      }),
    },
  });
});

it('keeps retained effects removable when the native Reporter contract becomes unavailable', async () => {
  const { observed, writer, refresh } = fixture();
  await userEvent.click(
    await screen.findByRole('button', { name: 'Use reviewed reporting effects' }),
  );
  expect(observed).toHaveBeenCalledTimes(1);
  if (!writer.reporting) throw new Error('fixture requires reporting');
  delete writer.reporting.action;
  refresh();
  await userEvent.click(
    screen.getByRole('button', { name: 'Remove reporting effects for accept_gate' }),
  );
  expect(observed).toHaveBeenLastCalledWith({});
  expect(screen.getByText(/Check the server installation of this Reporter action/)).toBeTruthy();
});
