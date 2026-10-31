// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { createConsoleQueryClient, queryKeys } from '../../app/query';
import { FlowGraphEditor } from './FlowGraphEditor';
import { factorySourcesApi } from './sourceApi';
import { emptyFlowDraft } from './emptyFlowDraft';
afterEach(cleanup);
it('authors exact source effects on an ordinary intermediate action without changing the routes or storing its draft', async () => {
  const connection = integrationFixture('IntegrationConnection');
  const choices = integrationFixture('WorkSourceInputChoices', 1);
  const writer = choices.items.find((item) => item.purpose === 'task_update')!;
  const read = choices.items.find((item) => item.purpose === 'task_read')!;
  const original = configurationFixture().configuration.flow;
  let observed = emptyFlowDraft();
  const root = original.closure.definitions.find(
    (d) => d.reference.id === original.closure.root.id,
  )!;
  observed.root = structuredClone(root);
  const action = observed.root.nodes.find((node) => node.action)!;
  action.action = {
    plugin: writer.binding.capability,
    action: 'set_status',
    parameters: { status: 'needs-human-eyes', outbox: { max_attempts: 2, retry_delay_millis: 5 } },
  };
  observed.root.entry = action.key;
  const routes = structuredClone(observed.root.transitions);
  const api = {
    ...factorySourcesApi,
    connections: vi.fn(async () => ({ items: [connection], next_cursor: null })),
    choices: vi.fn(async () => choices),
  };
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        sources={{ api, projectId: connection.id }}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  const client = createConsoleQueryClient();
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <Harness />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  await userEvent.click(await screen.findByRole('combobox', { name: 'Source connection' }));
  await userEvent.click(await screen.findByRole('option', { name: connection.definition.name }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Original task source' }));
  await userEvent.click(
    await screen.findByRole('option', {
      name: `${read.locator} · ${read.binding.configuration.identity}`,
    }),
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Reporter mapping' }));
  await userEvent.click(
    screen.getByRole('option', { name: writer.binding.configuration.identity }),
  );
  await userEvent.click(screen.getByRole('checkbox', { name: 'Customize status mapping' }));
  const status = screen.getByRole('textbox', { name: 'Logical status' });
  await userEvent.clear(status);
  await userEvent.type(status, 'investigate-next');
  const sibling = observed.root.nodes.find((item) => item.key !== action.key)!;
  await userEvent.click(screen.getByRole('button', { name: `Edit step: ${sibling.key}` }));
  await userEvent.click(screen.getByRole('button', { name: `Edit step: ${action.key}` }));
  expect(screen.getByRole('textbox', { name: 'Logical status' })).toHaveProperty(
    'value',
    'investigate-next',
  );
  await act(async () =>
    client.setQueryData(
      queryKeys.factorySourceChoices(connection.id, connection.id, connection.version),
      { ...choices, items: [...choices.items].reverse() },
    ),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Use reviewed source effects' }));
  const parameters = observed.root.nodes.find((node) => node.key === action.key)!.action!
    .parameters;
  expect(parameters).toEqual({
    status: 'investigate-next',
    outbox: { max_attempts: 2, retry_delay_millis: 5 },
    source_update: {
      source: read.binding,
      reporter: writer.binding,
      connection: {
        id: connection.id,
        version: connection.version,
        access_version: choices.access_version,
        source_configuration: read.connection_configuration,
        reporter_configuration: writer.connection_configuration,
      },
      effects: { ...writer.reporting!.mapping.statuses[0], status: 'investigate-next' },
    },
  });
  expect(observed.root.nodes.map((node) => node.key)).toEqual(root.nodes.map((node) => node.key));
  expect(observed.root.transitions).toEqual(routes);
  const apply = screen.getByRole('button', { name: 'Use reviewed source effects' });
  await userEvent.click(screen.getByRole('checkbox', { name: 'Add label: needs context' }));
  expect(apply).toHaveProperty('disabled', true);
  await userEvent.click(screen.getByRole('checkbox', { name: 'Add label: needs context' }));
  expect(apply).toHaveProperty('disabled', false);
  expect(screen.queryByRole('button', { name: 'Add status mapping' })).toBeNull();
  expect(
    JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
  ).not.toContain('investigate-next');
});

it('keeps arbitrary installed action parameters usable without assuming a deferred source profile', () => {
  const flow = configurationFixture().configuration.flow;
  const root = flow.closure.definitions.find((item) => item.reference.id === flow.closure.root.id)!;
  const value = emptyFlowDraft();
  value.root = structuredClone(root);
  const node = value.root.nodes.find((node) => node.action)!;
  value.root.entry = node.key;
  node.action = { ...node.action!, action: 'set_status', parameters: null };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <MemoryRouter>
        <FlowGraphEditor
          value={value}
          choices={[]}
          subflows={[]}
          sources={{ api: factorySourcesApi, projectId: 'project' }}
          onChange={() => {}}
        />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  expect(
    screen.getByText(
      'Select a published deferred status action profile to configure source effects.',
    ),
  ).toBeTruthy();
  expect(screen.queryByRole('combobox', { name: 'Source connection' })).toBeNull();
});

it('creates the first authorized Reporter action from installed choices without an existing Flow', async () => {
  const connection = integrationFixture('IntegrationConnection');
  const choices = integrationFixture('WorkSourceInputChoices', 1);
  const writer = choices.items.find((item) => item.purpose === 'task_update')!;
  const read = choices.items.find((item) => item.purpose === 'task_read')!;
  const output = {
    reference: { identity: 'delivery-receipt', version: 'v1', digest: 'd'.repeat(64) },
    shape: { type: 'boolean' as const },
  };
  const contract = {
    action: 'set_status' as const,
    plugin: writer.binding.capability,
    output,
    outcome: { key: 'delivery_recorded', kind: 'success' as const, schema: output.reference },
    parameters: {
      contract: 'work_status_v1' as const,
      max_attempts: 5,
      max_retry_delay_millis: 900,
      max_status_bytes: 128,
      outbox_required: true as const,
      source_update_required: true as const,
    },
  };
  writer.reporting!.action = contract;
  let observed = emptyFlowDraft();
  observed.root.entry = 'report';
  observed.root.nodes = [
    {
      key: 'report',
      kind: 'trusted_action',
      required: true,
      subflow: null,
      input_schema: null,
      outcomes: [],
      budget: observed.root.execution.budget,
      permissions: observed.root.execution.permissions,
    },
  ];
  const api = {
    ...factorySourcesApi,
    connections: vi.fn(async () => ({ items: [connection], next_cursor: null })),
    choices: vi.fn(async () => choices),
  };
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        sources={{ api, projectId: connection.id }}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <MemoryRouter>
        <Harness />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  await userEvent.click(await screen.findByRole('combobox', { name: 'Source connection' }));
  await userEvent.click(await screen.findByRole('option', { name: connection.definition.name }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Original task source' }));
  await userEvent.click(
    await screen.findByRole('option', {
      name: `${read.locator} · ${read.binding.configuration.identity}`,
    }),
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Reporter mapping' }));
  await userEvent.click(
    screen.getByRole('option', { name: writer.binding.configuration.identity }),
  );
  expect(observed.root.nodes[0]!.action).toBeUndefined();
  await userEvent.click(screen.getByRole('button', { name: 'Use reviewed source effects' }));
  expect(observed.root.nodes[0]!.action).toEqual({
    plugin: contract.plugin,
    action: 'set_status',
    parameters: {
      status: writer.reporting!.mapping.statuses[0]!.status,
      outbox: { max_attempts: 1, retry_delay_millis: 0 },
      source_update: {
        source: read.binding,
        reporter: writer.binding,
        effects: writer.reporting!.mapping.statuses[0],
        connection: {
          id: connection.id,
          version: connection.version,
          access_version: choices.access_version,
          source_configuration: read.connection_configuration,
          reporter_configuration: writer.connection_configuration,
        },
      },
    },
  });
  expect(observed.root.nodes[0]!.outcomes).toEqual([contract.outcome]);
  expect(observed.data_schemas).toEqual([output]);
});

it('does not silently adopt changed Reporter effects after catalogue refresh', async () => {
  const connection = integrationFixture('IntegrationConnection');
  const choices = integrationFixture('WorkSourceInputChoices', 1);
  const writer = choices.items.find((item) => item.purpose === 'task_update')!;
  const read = choices.items.find((item) => item.purpose === 'task_read')!;
  const flow = configurationFixture().configuration.flow;
  let observed = emptyFlowDraft();
  observed.root = structuredClone(flow.closure.definitions[0]!);
  const node = observed.root.nodes.find((item) => item.action)!;
  observed.root.entry = node.key;
  node.action = {
    plugin: writer.binding.capability,
    action: 'set_status',
    parameters: { status: 'reviewed', outbox: { max_attempts: 2, retry_delay_millis: 5 } },
  };
  const api = {
    ...factorySourcesApi,
    connections: vi.fn(async () => ({ items: [connection], next_cursor: null })),
    choices: vi.fn(async () => choices),
  };
  const client = createConsoleQueryClient();
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        sources={{ api, projectId: connection.id }}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <Harness />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  await userEvent.click(await screen.findByRole('combobox', { name: 'Source connection' }));
  await userEvent.click(screen.getByRole('option', { name: connection.definition.name }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Original task source' }));
  await userEvent.click(
    screen.getByRole('option', {
      name: `${read.locator} · ${read.binding.configuration.identity}`,
    }),
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Reporter mapping' }));
  await userEvent.click(
    screen.getByRole('option', { name: writer.binding.configuration.identity }),
  );
  const refreshed = structuredClone(choices);
  refreshed.items.find(
    (item) => item.purpose === 'task_update',
  )!.reporting!.mapping.statuses[0]!.status = 'changed-by-operator';
  await act(async () => {
    client.setQueryData(
      queryKeys.factorySourceChoices(connection.id, connection.id, connection.version),
      refreshed,
    );
  });
  await userEvent.click(screen.getByRole('button', { name: 'Use reviewed source effects' }));
  expect(observed.root.nodes.find((item) => item.key === node.key)!.action!.parameters).toEqual({
    status: 'reviewed',
    outbox: { max_attempts: 2, retry_delay_millis: 5 },
  });
  expect(screen.getByText(/Reporter choices changed/)).toBeTruthy();
});
