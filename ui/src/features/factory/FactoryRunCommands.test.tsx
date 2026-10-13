// @vitest-environment jsdom
import { useState } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { ManagementApiError } from '../../api/client';
import { createConsoleQueryClient } from '../../app/query';
import { configurationFixture, runFixture } from '../../../tests/fixtures/factory';
import { factoryApi, type FactoryApi } from './api';
import { FactoryRunCommands } from './FactoryRunCommands';

afterEach(cleanup);

it.each([
  { code: 'precondition_failed', status: 412 },
  { code: 'conflict', status: 409 },
] as const)(
  'offers only server-eligible retry and refreshes a $code conflict before fresh confirmation',
  async ({ code, status }) => {
    const run = runFixture();
    run.diagnostics.current_nodes = [];
    const retryNode = vi
      .fn<FactoryApi['retryNode']>()
      .mockRejectedValueOnce(
        new ManagementApiError({
          code,
          message: 'stale version',
          requestId: 'request-correlation',
          status,
          retryAfterMilliseconds: null,
        }),
      )
      .mockResolvedValue({
        run_id: run.id,
        state: 'waiting',
        version: 10,
        disposition: 'applied',
        cancellation_requested: false,
      });
    const attempt = {
      id: 'retryable-attempt',
      factory_run_id: run.id,
      flow_run_id: 'flow-a',
      cycle_id: 'cycle-a',
      node_key: 'arbitrary_retry_target',
      node_kind: 'reasoning' as const,
      number: 1,
      input_digest: 'a'.repeat(64),
      execution: { kind: 'built_in' as const },
      started_at: 1,
      deadline: 100,
      retry_of: null,
      retryable: true,
      active: false,
    };
    const refresh = vi.fn(async () => {});
    function Commands() {
      const [version, setVersion] = useState(run.version);
      return (
        <FactoryRunCommands
          api={{ ...factoryApi, retryNode }}
          run={{ ...run, version }}
          attempts={[attempt]}
          flowRuns={[]}
          onRefresh={async () => {
            await refresh();
            setVersion(9);
          }}
        />
      );
    }
    const router = createMemoryRouter([{ path: '*', element: <Commands /> }]);
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Request node retry' }));
    await userEvent.click(
      within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm command' }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(retryNode).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('dialog')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Request node retry' }));
    await userEvent.click(
      within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm command' }),
    );
    expect(await screen.findByText('Command recorded.')).toBeTruthy();
    expect(retryNode.mock.calls[0]?.[2]['If-Match']).toBe('"2"');
    expect(retryNode.mock.calls[1]?.[2]['If-Match']).toBe('"9"');
    expect(retryNode.mock.calls[1]?.[2]['Idempotency-Key']).not.toBe(
      retryNode.mock.calls[0]?.[2]['Idempotency-Key'],
    );
  },
);

it.each(['escalation', 'delivery', 'promotion'] as const)(
  'requests only the pinned %s disposition and keeps the private reason out of navigation',
  async (operation) => {
    const run = runFixture();
    const subject = {
      exact: { project_id: run.project_id, repository_id: 'repository', base_revision: 'base' },
      candidate_revision: 'verified-candidate',
      changeset_digest: 'a'.repeat(64),
    };
    run.diagnostics.candidate = subject;
    run.diagnostics.latest_decision = { id: 'decision', outcome: 'accept', subject };
    const configuration = configurationFixture();
    const root = configuration.configuration.flow.closure.definitions[0]!;
    const node = root.nodes[0]!;
    node.key = `operator_gate_${operation}`;
    node.kind = 'human_gate';
    node.operator_requests =
      operation === 'escalation'
        ? { operation, outcomes: { resume: 'operator_resumed' } }
        : { operation, outcomes: { confirm: 'operator_confirmed' } };
    const attempt = {
      id: 'node-a',
      factory_run_id: run.id,
      flow_run_id: 'flow-a',
      cycle_id: 'cycle-a',
      node_key: node.key,
      node_kind: 'human_gate' as const,
      number: 1,
      input_digest: 'a'.repeat(64),
      execution: { kind: 'built_in' as const },
      started_at: 1,
      deadline: 100,
      retry_of: null,
      retryable: false,
      active: true,
    };
    const requestNodeOperation = vi.fn<FactoryApi['requestNodeOperation']>(async () => ({
      run_id: run.id,
      version: 3,
      state: 'waiting',
      cancellation_requested: false,
      disposition: 'applied',
    }));
    const router = createMemoryRouter([
      {
        path: '*',
        element: (
          <FactoryRunCommands
            api={{ ...factoryApi, requestNodeOperation }}
            run={run}
            configuration={configuration}
            attempts={[attempt]}
            flowRuns={[
              { id: 'flow-a', factory_run_id: run.id, definition: root.reference, parent: null },
            ]}
            onRefresh={async () => {}}
          />
        ),
      },
    ]);
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );
    await userEvent.type(
      screen.getByRole('textbox', { name: 'Operator reason' }),
      'private operator reason',
    );
    await userEvent.click(screen.getByRole('button', { name: `Review ${operation} request` }));
    const dialog = await screen.findByRole('dialog');
    expect(dialog.textContent).toContain('verified-candidate');
    expect(dialog.textContent).toContain('repository');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm command' }));
    expect(await within(dialog).findByText('Command recorded.')).toBeTruthy();
    expect(requestNodeOperation.mock.calls[0]?.slice(0, 3)).toEqual([
      run.id,
      'node-a',
      {
        operation,
        disposition: operation === 'escalation' ? 'resume' : 'confirm',
        reason: 'private operator reason',
      },
    ]);
    expect(router.state.location.search).toBe('');
    expect(JSON.stringify(localStorage)).not.toContain('private operator reason');
  },
);

it('responds to an arbitrary pinned HumanGate using its declared typed outcome without raw JSON', async () => {
  const run = runFixture();
  const configuration = configurationFixture();
  const root = configuration.configuration.flow.closure.definitions[0]!;
  const node = root.nodes[0]!;
  node.key = 'operator_gate';
  node.kind = 'human_gate';
  node.operator_requests = null;
  node.outcomes = [
    {
      key: 'operator_outcome',
      kind: 'success',
      schema: configuration.configuration.flow.data_schemas[0]!.reference,
    },
  ];
  const attempt = {
    id: 'node-a',
    factory_run_id: run.id,
    flow_run_id: 'flow-a',
    cycle_id: 'cycle-a',
    node_key: node.key,
    node_kind: 'human_gate' as const,
    number: 1,
    input_digest: 'a'.repeat(64),
    execution: { kind: 'built_in' as const },
    started_at: 1,
    deadline: 100,
    retry_of: null,
    retryable: false,
    active: true,
  };
  run.diagnostics.current_nodes = [];
  run.diagnostics.active_node_count = 33;
  const respondNode = vi.fn<FactoryApi['respondNode']>(async () => ({
    run_id: run.id,
    version: 3,
    state: 'waiting',
    cancellation_requested: false,
    disposition: 'applied',
  }));
  const router = createMemoryRouter(
    [
      {
        path: '*',
        element: (
          <FactoryRunCommands
            api={{ ...factoryApi, respondNode }}
            run={run}
            configuration={configuration}
            attempts={[attempt, { ...attempt, id: 'node-selected' }]}
            flowRuns={[
              { id: 'flow-a', factory_run_id: run.id, definition: root.reference, parent: null },
            ]}
            onRefresh={async () => {}}
          />
        ),
      },
    ],
    { initialEntries: ['/factory?node_attempt_id=node-selected'] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole('checkbox', { name: 'Human response' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review HumanGate response' }));
  const dialog = await screen.findByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm command' }));
  expect(await within(dialog).findByText('Command recorded.')).toBeTruthy();
  expect(respondNode.mock.calls[0]?.slice(0, 3)).toEqual([
    run.id,
    'node-selected',
    { outcome: 'operator_outcome', value: true },
  ]);
  expect(respondNode.mock.calls[0]?.[3]['If-Match']).toBe('"2"');
  expect(screen.queryByRole('textbox', { name: /JSON/ })).toBeNull();
});
it('replays one frozen cancellation intent after a lost response with the original strong precondition', async () => {
  const run = runFixture();
  const failure = new ManagementApiError({
    code: 'transport_failure',
    message: 'safe transport failure',
    status: null,
    requestId: null,
    retryAfterMilliseconds: null,
  });
  const cancelRun = vi
    .fn<FactoryApi['cancelRun']>()
    .mockRejectedValueOnce(failure)
    .mockResolvedValue({
      run_id: run.id,
      version: 3,
      state: 'waiting',
      cancellation_requested: true,
      disposition: 'replayed',
    });
  const queryClient = createConsoleQueryClient();
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <FactoryRunCommands
          api={{ ...factoryApi, cancelRun }}
          run={run}
          configuration={configurationFixture()}
          attempts={[]}
          flowRuns={[]}
          onRefresh={async () => {}}
        />
      ),
    },
  ]);
  render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Request cancellation' }));
  const dialog = await screen.findByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm command' }));
  await userEvent.click(await within(dialog).findByRole('button', { name: 'Retry same command' }));
  expect(await within(dialog).findByText('Cancellation requested.')).toBeTruthy();
  expect(
    within(dialog).getByRole('link', { name: 'View audit evidence' }).getAttribute('href'),
  ).toBe(`/factory/${run.id}?diagnostic=audit`);
  expect(cancelRun).toHaveBeenCalledTimes(2);
  expect(cancelRun.mock.calls[0]).toEqual(cancelRun.mock.calls[1]);
  expect(cancelRun.mock.calls[0]?.[1]).toMatchObject({
    'If-Match': '"2"',
    'Idempotency-Key': expect.any(String),
  });
  expect(invalidate.mock.calls.map(([target]) => target?.queryKey)).toContainEqual([
    'factory',
    'run',
    run.id,
  ]);
  expect(router.state.location.search).not.toContain('safe transport');
});

it('does not offer delivery for a candidate accepted by a different exact Decision subject', async () => {
  const run = runFixture();
  const configuration = configurationFixture();
  const root = configuration.configuration.flow.closure.definitions[0]!;
  const node = root.nodes[0]!;
  node.kind = 'human_gate';
  node.operator_requests = { operation: 'delivery', outcomes: { confirm: 'operator_confirmed' } };
  const subject = {
    exact: {
      project_id: run.project_id,
      repository_id: 'repository',
      base_revision: run.diagnostics.base_revision,
    },
    candidate_revision: 'accepted-a',
    changeset_digest: 'a'.repeat(64),
  };
  run.diagnostics.latest_decision = { id: 'decision', outcome: 'accept', subject };
  run.diagnostics.candidate = { ...subject, candidate_revision: 'unaccepted-b' };
  const attempt = {
    id: 'node-a',
    factory_run_id: run.id,
    flow_run_id: 'flow-a',
    cycle_id: 'cycle-a',
    node_key: node.key,
    node_kind: 'human_gate' as const,
    number: 1,
    input_digest: 'a'.repeat(64),
    execution: { kind: 'built_in' as const },
    started_at: 1,
    deadline: 100,
    retry_of: null,
    retryable: false,
    active: true,
  };
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <FactoryRunCommands
          api={factoryApi}
          run={run}
          configuration={configuration}
          attempts={[attempt]}
          flowRuns={[
            { id: 'flow-a', factory_run_id: run.id, definition: root.reference, parent: null },
          ]}
          onRefresh={async () => {}}
        />
      ),
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Operator reason' }),
    'Synthetic reason',
  );
  expect(
    (screen.getByRole('button', { name: 'Review delivery request' }) as HTMLButtonElement).disabled,
  ).toBe(true);
});
