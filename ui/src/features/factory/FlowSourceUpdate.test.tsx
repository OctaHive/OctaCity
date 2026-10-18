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
