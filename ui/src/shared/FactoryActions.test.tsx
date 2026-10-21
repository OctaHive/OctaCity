// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, MemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { ManagementApiError } from '../api/client';
import { createConsoleQueryClient } from '../app/query';
import { FactoryActions } from './FactoryActions';
import type {
  FactoryActionsApi,
  FactoryActionItem,
  FactoryActionScope,
} from '../api/factoryActions';
afterEach(() => {
  cleanup();
  localStorage.clear();
  sessionStorage.clear();
});
function item(run: string, version: number): FactoryActionItem {
  return {
    project_id: 'project',
    association: null,
    action: {
      work_id: `work-${run}`,
      run_id: run,
      run_version: version,
      flow_run_id: `flow-${run}`,
      cycle_id: `cycle-${run}`,
      node_attempt_id: `gate-${run}`,
      contract: { id: 'definition', version: 1, digest: 'a'.repeat(64) },
      node_key: 'legal_review',
      input_digest: 'b'.repeat(64),
      candidate: null,
      actionable_at: 11,
      deadline: 100,
      kind: {
        kind: 'respond',
        outcomes: [
          {
            key: 'approved',
            kind: 'success',
            schema: { identity: 'operator.response', version: 'v1', digest: 'c'.repeat(64) },
          },
        ],
        binding: null,
        schemas: [
          {
            reference: { identity: 'operator.response', version: 'v1', digest: 'c'.repeat(64) },
            shape: { type: 'boolean' },
          },
        ],
      },
    },
  };
}
function actionsApi() {
  const first = item('run-a', 3),
    second = item('run-b', 9);
  return {
    list: vi.fn<FactoryActionsApi['list']>().mockResolvedValue({
      total: 2,
      coverage: 'complete',
      items: [first, second],
      next_cursor: null,
      observed_at: 12,
    }),
    observe: vi.fn<FactoryActionsApi['observe']>().mockImplementation(async (run, node) => ({
      run_id: run,
      node_attempt_id: node,
      run_version: run === 'run-b' ? 9 : 3,
      state: 'waiting',
      description: run === 'run-b' ? second.action : first.action,
      recorded_version: null,
      recorded_at: null,
      observed_at: 12,
      accepted_candidate: false,
    })),
    respondNode: vi.fn<FactoryActionsApi['respondNode']>().mockResolvedValue({
      run_id: 'run-b',
      version: 10,
      state: 'waiting',
      disposition: 'applied',
      cancellation_requested: false,
    }),
    retryNode: vi.fn<FactoryActionsApi['retryNode']>(),
    requestNodeOperation: vi.fn<FactoryActionsApi['requestNodeOperation']>(),
  };
}
function setup(configure?: (api: ReturnType<typeof actionsApi>) => void) {
  const api = actionsApi();
  configure?.(api);
  const router = createMemoryRouter(
    [
      {
        path: '*',
        element: <FactoryActions api={api} scope={{ projectId: 'project', issueId: 'task' }} />,
      },
    ],
    { initialEntries: ['/?section=actions'] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { api, router };
}
it('answers only the reviewed exact Run and retains recorded feedback without calling it completed', async () => {
  const { api, router } = setup();
  await screen.findByRole('heading', { name: 'Execution actions' });
  await userEvent.click(
    within(await screen.findByRole('article', { name: 'legal_review · run-b' })).getByRole(
      'button',
      { name: 'Review action' },
    ),
  );
  await userEvent.click(await screen.findByRole('checkbox', { name: 'Human response' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review HumanGate response' }));
  expect(api.respondNode).not.toHaveBeenCalled();
  api.observe.mockResolvedValue({
    run_id: 'run-b',
    node_attempt_id: 'gate-run-b',
    run_version: 10,
    state: 'recorded',
    description: null,
    accepted_candidate: false,
    recorded_version: 10,
    recorded_at: 13,
    observed_at: 14,
  });
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm command' }),
  );
  await waitFor(() => expect(api.respondNode).toHaveBeenCalledTimes(1));
  expect(await screen.findByText('Awaiting execution owner.')).toBeTruthy();
  expect(api.respondNode).toHaveBeenCalledWith(
    'run-b',
    'gate-run-b',
    { outcome: 'approved', value: true },
    { 'If-Match': '"9"', 'Idempotency-Key': expect.any(String) },
  );
  expect(await screen.findByText('Command recorded; awaiting execution owner.')).toBeTruthy();
  expect(api.retryNode).not.toHaveBeenCalled();
  expect(api.requestNodeOperation).not.toHaveBeenCalled();
  expect(router.state.location.search).not.toContain('approved');
  expect(localStorage.length).toBe(0);
  expect(sessionStorage.length).toBe(0);
});

it('requires a new review after a competing response instead of retrying stale intent', async () => {
  const { api } = setup();
  await userEvent.click(
    within(await screen.findByRole('article', { name: 'legal_review · run-b' })).getByRole(
      'button',
      { name: 'Review action' },
    ),
  );
  await userEvent.click(await screen.findByRole('checkbox', { name: 'Human response' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review HumanGate response' }));
  api.respondNode.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'precondition_failed',
      message: 'Changed',
      status: 412,
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm command' }),
  );
  await screen.findByRole('button', { name: 'Refresh current data' });
  expect(screen.queryByRole('button', { name: 'Retry same command' })).toBeNull();
  const winner = item('run-b', 11);
  api.observe.mockResolvedValue({
    run_id: 'run-b',
    node_attempt_id: 'gate-run-b',
    run_version: 11,
    state: 'waiting',
    description: winner.action,
    accepted_candidate: false,
    recorded_version: null,
    recorded_at: null,
    observed_at: 15,
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(api.respondNode).toHaveBeenCalledTimes(1);
  await userEvent.click(await screen.findByRole('button', { name: 'Review HumanGate response' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm command' }),
  );
  await waitFor(() => expect(api.respondNode).toHaveBeenCalledTimes(2));
  const first = api.respondNode.mock.calls[0]![3],
    second = api.respondNode.mock.calls[1]![3];
  expect(first['If-Match']).toBe('"9"');
  expect(second['If-Match']).toBe('"11"');
  expect(second['Idempotency-Key']).not.toBe(first['Idempotency-Key']);
});

it('keeps incomplete coverage explicit and disables retained actions after a read failure', async () => {
  const { api } = setup();
  await screen.findByRole('article', { name: 'legal_review · run-b' });
  api.list.mockResolvedValue({
    total: 0,
    coverage: 'incomplete',
    items: [],
    next_cursor: null,
    observed_at: 13,
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh actions' }));
  await screen.findByText(/coverage.*incomplete/i);
  expect(screen.queryByText('No unanswered actions.')).toBeNull();
  api.list.mockRejectedValue(new Error('offline'));
  await userEvent.click(screen.getByRole('button', { name: 'Refresh actions' }));
  await waitFor(() =>
    expect(
      screen.getByRole<HTMLButtonElement>('button', { name: 'Refresh actions' }).disabled,
    ).toBe(false),
  );
  expect(api.respondNode).not.toHaveBeenCalled();
});

it.each(['retry', 'delivery', 'escalation'] as const)(
  'uses the original %s control with no generic response bypass',
  async (kind) => {
    const selected = item('run-b', 9);
    selected.action.kind =
      kind === 'retry'
        ? { kind: 'retry' }
        : {
            kind: 'respond',
            outcomes: [],
            schemas: [],
            binding:
              kind === 'delivery'
                ? { operation: 'delivery', outcomes: { confirm: 'approved' } }
                : { operation: 'escalation', outcomes: { resume: 'approved' } },
          };
    const receipt = {
      run_id: 'run-b',
      version: 10,
      state: 'waiting',
      disposition: 'applied',
      cancellation_requested: false,
    } as const;
    const { api } = setup((api) => {
      api.observe.mockResolvedValue({
        run_id: 'run-b',
        node_attempt_id: 'gate-run-b',
        run_version: 9,
        state: 'waiting',
        description: selected.action,
        accepted_candidate: true,
        recorded_at: null,
        recorded_version: null,
        observed_at: 12,
      });
      api.retryNode.mockResolvedValue(receipt);
      api.requestNodeOperation.mockResolvedValue(receipt);
    });
    await userEvent.click(
      within(await screen.findByRole('article', { name: 'legal_review · run-b' })).getByRole(
        'button',
        { name: 'Review action' },
      ),
    );
    if (kind === 'retry')
      await userEvent.click(await screen.findByRole('button', { name: 'Request node retry' }));
    else {
      await userEvent.type(
        await screen.findByRole('textbox', { name: 'Operator reason' }),
        'Private explanation',
      );
      await userEvent.click(screen.getByRole('button', { name: `Review ${kind} request` }));
    }
    await userEvent.click(
      within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm command' }),
    );
    if (kind === 'retry')
      await waitFor(() =>
        expect(api.retryNode).toHaveBeenCalledWith(
          'run-b',
          'gate-run-b',
          expect.objectContaining({ 'If-Match': '"9"' }),
        ),
      );
    else
      await waitFor(() =>
        expect(api.requestNodeOperation).toHaveBeenCalledWith(
          'run-b',
          'gate-run-b',
          {
            operation: kind,
            disposition: kind === 'delivery' ? 'confirm' : 'resume',
            reason: 'Private explanation',
          },
          expect.objectContaining({ 'If-Match': '"9"' }),
        ),
      );
    expect(api.respondNode).not.toHaveBeenCalled();
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
  },
);

it('discards a private operation reason when selecting another exact execution', async () => {
  setup((api) =>
    api.observe.mockImplementation(async (run, node) => {
      const description = item(run, run === 'run-b' ? 9 : 3).action;
      description.kind = {
        kind: 'respond',
        outcomes: [],
        schemas: [],
        binding: { operation: 'escalation', outcomes: { resume: 'approved' } },
      };
      return {
        run_id: run,
        node_attempt_id: node,
        run_version: description.run_version,
        state: 'waiting',
        description,
        accepted_candidate: false,
        recorded_at: null,
        recorded_version: null,
        observed_at: 12,
      };
    }),
  );
  await userEvent.click(
    within(await screen.findByRole('article', { name: 'legal_review · run-a' })).getByRole(
      'button',
      { name: 'Review action' },
    ),
  );
  await userEvent.type(
    await screen.findByRole('textbox', { name: 'Operator reason' }),
    'Private first execution',
  );
  await userEvent.click(
    within(screen.getByRole('article', { name: 'legal_review · run-b' })).getByRole('button', {
      name: 'Review action',
    }),
  );
  await waitFor(() =>
    expect(
      screen.getByRole<HTMLTextAreaElement>('textbox', { name: 'Operator reason' }).value,
    ).toBe(''),
  );
});

it.each([
  { selection: 'Project', from: { projectId: 'project-a' }, to: { projectId: 'project-b' } },
  {
    selection: 'Issue',
    from: { projectId: 'project', issueId: 'task-a' },
    to: { projectId: 'project', issueId: 'task-b' },
  },
])(
  'does not retain another $selection observation after the same exact target is denied',
  async ({ from, to }) => {
    const api = actionsApi();
    api.list.mockResolvedValue({
      total: 0,
      coverage: 'complete',
      items: [],
      next_cursor: null,
      observed_at: 12,
    });
    const unavailable = new ManagementApiError({
      code: 'not_found',
      message: 'Unavailable in selected scope',
      status: 404,
      requestId: null,
      retryAfterMilliseconds: null,
    });
    api.observe.mockRejectedValue(unavailable).mockResolvedValueOnce({
      run_id: 'run-a',
      node_attempt_id: 'gate-run-a',
      run_version: 3,
      state: 'waiting',
      description: item('run-a', 3).action,
      accepted_candidate: false,
      recorded_version: null,
      recorded_at: null,
      observed_at: 12,
    });
    const client = createConsoleQueryClient();
    const view = (scope: FactoryActionScope) => (
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={['/?action_run=run-a&action_attempt=gate-run-a']}>
          <FactoryActions api={api} scope={scope} />
        </MemoryRouter>
      </QueryClientProvider>
    );
    const { rerender } = render(view(from));
    await screen.findByRole('checkbox', { name: 'Human response' });
    rerender(view(to));
    await screen.findByText('Unavailable in selected scope');
    expect(screen.queryByRole('checkbox', { name: 'Human response' })).toBeNull();
    expect(screen.queryByText('Waiting for a reviewed response.')).toBeNull();
    expect(api.respondNode).not.toHaveBeenCalled();
    expect(api.requestNodeOperation).not.toHaveBeenCalled();
    expect(api.retryNode).not.toHaveBeenCalled();
  },
);
