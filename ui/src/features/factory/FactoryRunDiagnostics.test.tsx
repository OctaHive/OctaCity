// @vitest-environment jsdom
import { triageCompletion } from '../../../tests/fixtures/factoryTriage';
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { configurationFixture, runFixture } from '../../../tests/fixtures/factory';
import { factoryApi, type FactoryApi } from './api';
import { ManagementApiError } from '../../api/client';
import { FactoryRunDiagnostics } from './FactoryRunDiagnostics';

afterEach(cleanup);
it('reads pinned arbitrary nodes and distinguishes a typed advisory signal from an authoritative Decision', async () => {
  const run = runFixture();
  const configuration = configurationFixture();
  const root = configuration.configuration.flow.closure.definitions[0]!;
  root.nodes[0]!.key = 'operator_arbitrary_step';
  const reference = configuration.configuration.admission_policy;
  const listDiagnostics = vi.fn<FactoryApi['listDiagnostics']>(async (_id, collection) => {
    if (collection === 'node-attempts')
      return {
        next_cursor: null,
        items: [
          {
            kind: 'node_attempt',
            record: {
              id: 'node-a',
              factory_run_id: run.id,
              flow_run_id: 'flow-a',
              cycle_id: 'cycle-a',
              node_key: 'operator_arbitrary_step',
              node_kind: 'reasoning',
              input_digest: 'a'.repeat(64),
              execution: { kind: 'built_in' },
              number: 1,
              retry_of: null,
              retryable: false,
              active: true,
              started_at: 1,
              deadline: 100,
            },
          },
          {
            kind: 'node_attempt',
            record: {
              id: 'retry-node',
              factory_run_id: run.id,
              flow_run_id: 'flow-a',
              cycle_id: 'cycle-a',
              node_key: 'another_arbitrary_node',
              node_kind: 'reasoning',
              input_digest: 'a'.repeat(64),
              execution: { kind: 'built_in' },
              number: 1,
              retry_of: null,
              retryable: true,
              active: false,
              started_at: 1,
              deadline: 100,
            },
          },
          {
            kind: 'node_attempt',
            record: {
              id: 'join-node',
              factory_run_id: run.id,
              flow_run_id: 'flow-a',
              cycle_id: 'cycle-a',
              node_key: 'wait_for_declared_inputs',
              node_kind: 'join',
              input_digest: 'a'.repeat(64),
              execution: { kind: 'built_in' },
              number: 1,
              retry_of: null,
              retryable: false,
              active: true,
              started_at: 1,
              deadline: 100,
            },
          },
        ],
      };
    if (collection === 'flow-runs')
      return {
        next_cursor: null,
        items: [
          {
            kind: 'flow_run',
            record: {
              id: 'flow-a',
              factory_run_id: run.id,
              definition: root.reference,
              parent: null,
            },
          },
          {
            kind: 'flow_run',
            record: {
              id: 'nested-flow',
              factory_run_id: run.id,
              definition: root.reference,
              parent: { flow_run_id: 'flow-a', node_attempt_id: 'node-a' },
            },
          },
        ],
      };
    if (collection === 'decision-signals')
      return {
        next_cursor: null,
        items: [
          {
            kind: 'domain_fact',
            record: {
              id: 'receipt-diagnostic',
              flow_run_id: 'flow-a',
              cycle_id: 'cycle-a',
              node_attempt_id: 'node-a',
              input_digest: 'b'.repeat(64),
              observed_at: 2,
              facts: Object.assign(
                {
                  kind: 'decision_signal' as const,
                  subject: {
                    project_id: run.project_id,
                    repository_id: 'repo',
                    base_revision: 'base',
                  },
                  id: 'receipt',
                  request_id: 'request',
                  receipt_digest: 'c'.repeat(64),
                  input_digest: 'd'.repeat(64),
                  frozen_input_digest: 'e'.repeat(64),
                  question_set: reference,
                  probability_semantics: reference,
                  provider: reference,
                  adapter: reference,
                  model: reference,
                  policy: reference,
                  state: 'completed' as const,
                  purpose: 'routing' as const,
                  mode: 'shadow' as const,
                  fallback: 'escalate' as const,
                  answers: [
                    {
                      question: reference,
                      value: {
                        FiniteChoice: {
                          selected: 'accept',
                          selected_probability: 900000,
                          runner_up_probability: 100000,
                        },
                      },
                    },
                  ],
                  consumption: {
                    baseline: { Route: 'declared_baseline' },
                    recommendation: { Route: 'accept' },
                    final_disposition: { Route: 'declared_baseline' },
                    kind: 'Shadow' as const,
                  },
                },
                { provider_payload: 'private provider prompt', reasoning: 'hidden reasoning' },
              ),
            },
          },
        ],
      };
    return { items: [], next_cursor: null };
  });
  const api = {
    ...factoryApi,
    getConfiguration: vi.fn(async () => configuration),
    listDiagnostics,
  };
  const router = createMemoryRouter(
    [{ path: '*', element: <FactoryRunDiagnostics api={api} run={run} /> }],
    { initialEntries: [`/factory/${run.id}`] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('table', { name: 'Node Attempts' })).toBeTruthy();
  expect(await screen.findByRole('region', { name: 'Run overview' })).toBeTruthy();
  expect(screen.getByRole('region', { name: 'Execution journal' })).toBeTruthy();
  expect(screen.queryByText('a'.repeat(64))).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Inspect record · node-a' }));
  expect(await screen.findByText('a'.repeat(64))).toBeTruthy();
  expect((await screen.findAllByText('Active attempt')).length).toBeGreaterThan(0);
  expect(screen.getByText('Retry eligible')).toBeTruthy();
  expect(screen.getByText('Pending join')).toBeTruthy();
  expect(await screen.findByRole('img', { name: 'Nested Flow Runs' })).toBeTruthy();
  const runtime = await screen.findByRole('table', { name: 'Nested Flow Runs' });
  expect(within(runtime).getByText('nested-flow')).toBeTruthy();
  expect(within(runtime).getByText('Root Flow')).toBeTruthy();
  expect(within(runtime).getByText('Nested Flow')).toBeTruthy();
  expect(within(runtime).getByText('flow-a → node-a')).toBeTruthy();
  expect(await screen.findAllByText('operator_arbitrary_step')).not.toHaveLength(0);
  expect(api.getConfiguration).toHaveBeenCalledWith(
    run.configuration_id,
    run.configuration_version,
    expect.any(AbortSignal),
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Diagnostic collection' }));
  await userEvent.click(screen.getByRole('option', { name: 'Decision Signals' }));
  expect(
    await screen.findByText(
      'Signals are observations. Only the recorded program disposition is authoritative.',
    ),
  ).toBeTruthy();
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect record · receipt-diagnostic' }),
  );
  expect(await screen.findByText('900000')).toBeTruthy();
  expect(screen.queryByText('private provider prompt')).toBeNull();
  expect(screen.queryByText('hidden reasoning')).toBeNull();
  expect(router.state.location.search).toContain('diagnostic=decision-signals');
  expect(router.state.location.search).not.toContain('accept');
  await userEvent.click(screen.getByText('Pinned definition closure', { selector: 'summary' }));
  expect(
    await screen.findByRole('heading', { name: 'Declared node permission ceilings' }),
  ).toBeTruthy();
});

it('reopens an exact diagnostic from a later bounded page without scanning earlier history', async () => {
  const run = runFixture();
  const listDiagnostics = vi.fn<FactoryApi['listDiagnostics']>(async (_run, collection, cursor) => {
    if (collection !== 'node-attempts') return { items: [], next_cursor: null };
    return {
      items: [
        {
          kind: 'node_attempt',
          record: {
            id: cursor === 'after-first' ? 'later-node' : 'earlier-node',
            factory_run_id: run.id,
            flow_run_id: 'flow-a',
            cycle_id: 'cycle-a',
            node_key: 'arbitrary',
            node_kind: 'reasoning',
            input_digest: cursor === 'after-first' ? 'f'.repeat(64) : 'a'.repeat(64),
            execution: { kind: 'built_in' },
            number: 1,
            retry_of: null,
            retryable: false,
            active: false,
            started_at: 1,
            deadline: 100,
          },
        },
      ],
      next_cursor: null,
    };
  });
  const api = {
    ...factoryApi,
    getConfiguration: vi.fn(async () => configurationFixture()),
    listDiagnostics,
  };
  const router = createMemoryRouter(
    [{ path: '*', element: <FactoryRunDiagnostics api={api} run={run} /> }],
    {
      initialEntries: [
        `/factory/${run.id}?diagnostic=node-attempts&diagnostic_after=after-first&diagnostic_id=later-node`,
      ],
    },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(await screen.findByText('f'.repeat(64))).toBeTruthy();
  expect(listDiagnostics).toHaveBeenCalledWith(
    run.id,
    'node-attempts',
    'after-first',
    expect.any(AbortSignal),
  );
});

it('identifies unavailable historical signal metadata without inventing purpose or confidence', async () => {
  const run = runFixture();
  const configuration = configurationFixture();
  const reference = configuration.configuration.admission_policy;
  const listDiagnostics = vi.fn<FactoryApi['listDiagnostics']>(async (_run, collection) => ({
    items:
      collection !== 'decision-signals'
        ? []
        : [
            {
              kind: 'domain_fact',
              record: {
                id: 'historical-signal',
                flow_run_id: 'flow-a',
                cycle_id: 'cycle-a',
                node_attempt_id: 'node-a',
                input_digest: 'a'.repeat(64),
                observed_at: 1,
                facts: {
                  kind: 'decision_signal',
                  subject: {
                    project_id: run.project_id,
                    repository_id: 'repository',
                    base_revision: 'base',
                  },
                  id: 'receipt',
                  request_id: 'request',
                  receipt_digest: 'b'.repeat(64),
                  input_digest: 'c'.repeat(64),
                  frozen_input_digest: 'd'.repeat(64),
                  question_set: reference,
                  probability_semantics: reference,
                  provider: reference,
                  adapter: reference,
                  model: reference,
                  policy: reference,
                  state: 'completed',
                  consumption: {
                    baseline: { Route: 'baseline' },
                    recommendation: null,
                    final_disposition: { Route: 'baseline' },
                    kind: 'Shadow',
                  },
                },
              },
            },
          ],
    next_cursor: null,
  }));
  const router = createMemoryRouter(
    [
      {
        path: '*',
        element: (
          <FactoryRunDiagnostics
            api={{ ...factoryApi, listDiagnostics, getConfiguration: async () => configuration }}
            run={run}
          />
        ),
      },
    ],
    {
      initialEntries: [
        `/factory/${run.id}?diagnostic=decision-signals&diagnostic_id=historical-signal`,
      ],
    },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(
    await screen.findByText('Some signal metadata is unavailable in this historical record.'),
  ).toBeTruthy();
  expect(screen.queryByText('routing')).toBeNull();
});

it.each(['node-attempts', 'node-completions', 'node-stops'] as const)(
  'reports a failed %s dependency even when another journal is selected, and retries it',
  async (failed) => {
    const run = runFixture();
    let unavailable = true;
    const api = {
      ...factoryApi,
      getConfiguration: vi.fn(async () => configurationFixture()),
      listDiagnostics: vi.fn<FactoryApi['listDiagnostics']>(async (_run, collection) => {
        if (collection === failed && unavailable)
          throw new ManagementApiError({
            code: 'unavailable',
            message: 'safe dependency failure',
            requestId: 'dependency-correlation',
            status: 503,
            retryAfterMilliseconds: null,
          });
        return { items: [], next_cursor: null };
      }),
    };
    const router = createMemoryRouter(
      [{ path: '*', element: <FactoryRunDiagnostics api={api} run={run} /> }],
      { initialEntries: [`/factory/${run.id}?diagnostic=audit`] },
    );
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );
    expect(await screen.findByText('safe dependency failure')).toBeTruthy();
    expect(screen.getByText('Request ID: dependency-correlation')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Request cancellation' })).toHaveProperty(
      'disabled',
      false,
    );
    unavailable = false;
    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
    await expect.poll(() => screen.queryByText('safe dependency failure')).toBeNull();
  },
);

it('does not label a stopped attempt as merely recorded when stop details are unavailable', async () => {
  const run = runFixture();
  const api = {
    ...factoryApi,
    getConfiguration: vi.fn(async () => configurationFixture()),
    listDiagnostics: vi.fn<FactoryApi['listDiagnostics']>(async (_run, collection) => {
      if (collection === 'node-stops')
        throw new ManagementApiError({
          code: 'unavailable',
          message: 'safe stop failure',
          requestId: 'stop-correlation',
          status: 503,
          retryAfterMilliseconds: null,
        });
      if (collection === 'node-attempts')
        return {
          items: [
            {
              kind: 'node_attempt',
              record: {
                id: 'stopped-node',
                factory_run_id: run.id,
                flow_run_id: 'flow-a',
                cycle_id: 'cycle-a',
                node_key: 'arbitrary',
                node_kind: 'reasoning',
                input_digest: 'a'.repeat(64),
                execution: { kind: 'built_in' },
                number: 1,
                retry_of: null,
                retryable: false,
                active: false,
                started_at: 1,
                deadline: 100,
              },
            },
          ],
          next_cursor: null,
        };
      return { items: [], next_cursor: null };
    }),
  };
  const router = createMemoryRouter([
    { path: '*', element: <FactoryRunDiagnostics api={api} run={run} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await screen.findByText('safe stop failure');
  const nodes = screen.getByRole('table', { name: 'Node Attempts' });
  expect(within(nodes).getByText('Execution details unavailable')).toBeTruthy();
  expect(within(nodes).queryByText('Recorded attempt')).toBeNull();
});

it('copies a later-page Flow link with its cursor and filters and reopens the selected record', async () => {
  const run = runFixture();
  const root = configurationFixture().configuration.flow.closure.definitions[0]!.reference;
  const api = {
    ...factoryApi,
    getConfiguration: vi.fn(async () => configurationFixture()),
    listDiagnostics: vi.fn<FactoryApi['listDiagnostics']>(async (_run, collection, cursor) =>
      collection === 'flow-runs'
        ? {
            items: [
              {
                kind: 'flow_run',
                record: {
                  id: cursor === 'after-root' ? 'later-flow' : 'root-flow',
                  factory_run_id: run.id,
                  definition: {
                    ...root,
                    digest: cursor === 'after-root' ? 'e'.repeat(64) : root.digest,
                  },
                  parent:
                    cursor === 'after-root'
                      ? { flow_run_id: 'root-flow', node_attempt_id: 'parent-node' }
                      : null,
                },
              },
            ],
            next_cursor: cursor === 'after-root' ? null : 'after-root',
          }
        : { items: [], next_cursor: null },
    ),
  };
  const mount = (url: string) => {
    const router = createMemoryRouter(
      [{ path: '*', element: <FactoryRunDiagnostics api={api} run={run} /> }],
      { initialEntries: [url] },
    );
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );
    return router;
  };
  const router = mount(`/factory/${run.id}?state=waiting&source=manual`);
  await screen.findByRole('link', { name: 'root-flow' });
  const hierarchy = screen.getByRole('heading', { name: 'Nested Flow Runs' }).closest('section')!;
  await userEvent.click(
    within(hierarchy).getByRole('button', { name: 'Load more diagnostic records' }),
  );
  const later = await screen.findByRole('link', { name: 'later-flow' });
  const copied = later.getAttribute('href')!;
  const search = new URL(copied, 'https://console.example').searchParams;
  expect(search.get('diagnostic_after')).toBe('after-root');
  expect(search.get('state')).toBe('waiting');
  expect(search.get('source')).toBe('manual');
  await userEvent.click(later);
  expect(await screen.findByText('e'.repeat(64))).toBeTruthy();
  expect(router.state.location.search).toContain('diagnostic_id=later-flow');
  cleanup();
  mount(copied);
  expect(await screen.findByText('e'.repeat(64))).toBeTruthy();
});

it('opens retained assessments directly and preserves exact provenance without running agents or writes', async () => {
  const run = runFixture();
  const acceptedProject = triageCompletion('eligibility');
  const acceptedAnalyst = triageCompletion('classification');
  const api = {
    ...factoryApi,
    getConfiguration: vi.fn(async () => configurationFixture()),
    respondNode: vi.fn(),
    retryNode: vi.fn(),
    cancelRun: vi.fn(),
    requestNodeOperation: vi.fn(),
    listDiagnostics: vi.fn<FactoryApi['listDiagnostics']>(async (_run, collection) => ({
      items:
        collection === 'node-completions'
          ? [
              { kind: 'node_completion', record: acceptedProject },
              { kind: 'node_completion', record: acceptedAnalyst },
            ]
          : collection === 'effect-delivery'
            ? [
                {
                  kind: 'effect_delivery',
                  record: {
                    id: 'd'.repeat(64),
                    record_id: 'e'.repeat(64),
                    kind: 'work.status.projection',
                    input_digest: 'b'.repeat(64),
                    state: 'failed',
                    failure: 'exhausted',
                    attempt: 2,
                    available_at: 3,
                    recorded_at: 4,
                  },
                },
              ]
            : [],
      next_cursor: null,
    })),
  };
  const router = createMemoryRouter(
    [{ path: '*', element: <FactoryRunDiagnostics api={api} run={run} /> }],
    { initialEntries: [`/factory/${run.id}`] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('heading', { name: 'Project assessment' })).toBeTruthy();
  expect(screen.getByRole('heading', { name: 'Systems analyst assessment' })).toBeTruthy();
  expect(
    await screen.findByText('Delivery budget exhausted; review the retained attempt'),
  ).toBeTruthy();
  expect(screen.getByRole('heading', { name: 'Reporter delivery' })).toBeTruthy();
  const result = screen.getByRole('region', { name: 'Retained triage results' });
  const provenance = within(result).getAllByRole('link', { name: 'Inspect stage provenance' });
  const url = new URL(provenance[0]!.getAttribute('href')!, 'http://console.test');
  expect(url.searchParams.get('node_attempt_id')).toBe('original-eligibility-agent-attempt');
  expect(url.searchParams.get('diagnostic_id')).toBe('original-eligibility-agent-attempt');
  expect(url.searchParams.get('flow_run_id')).toBe('source-flow');
  expect(url.searchParams.get('cycle_id')).toBe('source-cycle');
  expect(url.searchParams.get('diagnostic')).toBe('calls');
  expect(within(result).getByText('original-eligibility-context')).toBeTruthy();
  expect(within(result).getByText('original-classification-context')).toBeTruthy();
  expect(within(result).getByText('original-eligibility-profile')).toBeTruthy();
  expect(within(result).getByText('original-classification-profile')).toBeTruthy();
  expect(api.requestNodeOperation).not.toHaveBeenCalled();
  expect(api.respondNode).not.toHaveBeenCalled();
  expect(api.retryNode).not.toHaveBeenCalled();
  expect(api.cancelRun).not.toHaveBeenCalled();
});
