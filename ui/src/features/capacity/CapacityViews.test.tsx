// @vitest-environment jsdom

import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ManagementApiError } from '../../api/client';
import { createConsoleQueryClient, queryKeys } from '../../app/query';
import type { AgentPoolResource, AgentResource, CapacityApi } from './api';
import { AgentPoolView, AgentView } from './CapacityViews';

afterEach(cleanup);

describe('Capacity detail views', () => {
  it('renders idle Agent readiness, assignment, and inventory without compatibility inference', async () => {
    const api = fakeCapacityApi();
    api.getAgent.mockResolvedValue(agent('agent-a', 'Builder A', 'pool-a'));
    renderView('/agents/agent-a', <AgentView api={api} />);

    expect(await screen.findByRole('heading', { level: 1, name: 'Builder A' })).toBeTruthy();
    expect(screen.getByText('Online')).toBeTruthy();
    expect(screen.getByText('Idle — no current execution is published.')).toBeTruthy();
    expect(screen.getByRole('link', { name: 'pool-a' }).getAttribute('href')).toBe(
      '/agent-pools/pool-a',
    );
    expect(screen.getByText('Available')).toBeTruthy();
    expect(screen.queryByText(/compatible/i)).toBeNull();
  });

  it('links a current execution to its Build and exposes only safe lease facts', async () => {
    const api = fakeCapacityApi();
    const active = agent('agent-a', 'Builder A', 'pool-a');
    active.current_execution = {
      attempt_id: 'attempt-a',
      build_id: 'build-a',
      job_id: 'job-a',
      lease_id: 'lease-a',
      lease_state: 'cancellation_requested',
    };
    api.getAgent.mockResolvedValue(active);
    renderView('/agents/agent-a', <AgentView api={api} />);

    expect((await screen.findByRole('link', { name: 'build-a' })).getAttribute('href')).toBe(
      '/builds/build-a',
    );
    expect(screen.getByText('Cancellation requested')).toBeTruthy();
    expect(screen.getByText('lease-a')).toBeTruthy();
    expect(screen.queryByText(/fence|credential|bearer/i)).toBeNull();
  });

  it('renders restrictive Pool admission facts without deciding workload compatibility', async () => {
    const api = fakeCapacityApi();
    const restricted = pool('pool-a', 'Linux isolation');
    restricted.definition.admission_policy = {
      mode: 'execution_allowlist',
      platforms: [{ architecture: 'amd64', operating_system: 'linux' }],
      execution_targets: [
        {
          host_platform: { architecture: 'amd64', os: 'linux' },
          mode: 'isolation',
          required_guarantees: ['filesystem_isolation', 'network_isolation'],
          target_platform: { architecture: 'amd64', os: 'linux' },
        },
      ],
    };
    api.getAgentPool.mockResolvedValue(restricted);
    renderView('/agent-pools/pool-a', <AgentPoolView api={api} />);

    expect(await screen.findByRole('heading', { level: 1, name: 'Linux isolation' })).toBeTruthy();
    expect(screen.getByText('linux/amd64')).toBeTruthy();
    expect(screen.getByText(/Isolation: linux\/amd64 → linux\/amd64/)).toBeTruthy();
    expect(screen.getByText(/Filesystem isolation, Network isolation/)).toBeTruthy();
    expect(screen.queryByText(/compatible|incompatible/i)).toBeNull();
  });

  it('distinguishes unavailable and stale Agent reads while preserving prior data', async () => {
    const unavailableApi = fakeCapacityApi();
    unavailableApi.getAgent.mockRejectedValue(managementError('request-unavailable'));
    renderView('/agents/agent-a', <AgentView api={unavailableApi} />);
    const unavailable = await screen.findByRole('alert');
    expect(unavailable.textContent).toContain('Agent could not be loaded.');
    expect(unavailable.textContent).toContain('Error code: unavailable');
    expect(unavailable.textContent).toContain('Request ID: request-unavailable');
    cleanup();

    const staleApi = fakeCapacityApi();
    staleApi.getAgent
      .mockResolvedValueOnce(agent('agent-a', 'Builder A', 'pool-a'))
      .mockRejectedValueOnce(managementError('request-stale'));
    const { queryClient } = renderView('/agents/agent-a', <AgentView api={staleApi} />);
    await screen.findByRole('heading', { level: 1, name: 'Builder A' });
    await act(async () => {
      await queryClient.refetchQueries({ queryKey: queryKeys.agent('agent-a') });
    });
    expect(await screen.findByText('Refresh failed. Showing the last loaded Agent.')).toBeTruthy();
    expect(screen.getByRole('heading', { level: 1, name: 'Builder A' })).toBeTruthy();
  });

  it('confirms one versioned drain command and correlates its audit evidence', async () => {
    vi.stubGlobal('crypto', {
      randomUUID: vi.fn(() => '11111111-1111-4111-8111-111111111111'),
    });
    const api = fakeCapacityApi();
    const pending = deferred<Awaited<ReturnType<CapacityApi['drainAgent']>>>();
    api.getAgent.mockResolvedValue(agent('agent-a', 'Builder A', 'pool-a'));
    api.listAgentPools.mockResolvedValue({ items: [], next_cursor: null });
    api.drainAgent.mockReturnValue(pending.promise);
    const { queryClient } = renderView('/agents/agent-a', <AgentView api={api} />);
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');

    await userEvent.click(await screen.findByRole('button', { name: 'Drain Agent' }));
    const dialog = screen.getByRole('dialog', { name: 'Drain Agent Builder A?' });
    expect(within(dialog).getByText(/finish its current execution/)).toBeTruthy();
    const confirm = within(dialog).getByRole('button', { name: 'Start graceful drain' });
    await userEvent.click(confirm);
    await userEvent.click(confirm);

    expect(api.drainAgent).toHaveBeenCalledTimes(1);
    expect(api.drainAgent).toHaveBeenCalledWith(
      'agent-a',
      { mode: 'graceful' },
      {
        'Idempotency-Key': '11111111-1111-4111-8111-111111111111',
        'If-Match': '"3"',
      },
    );
    pending.resolve({
      requestId: 'request-drain',
      response: {
        disposition: 'applied',
        resource: { ...agent('agent-a', 'Builder A', 'pool-a'), status: 'draining', version: 4 },
      },
    });

    expect(await within(dialog).findByText('Drain accepted (Applied).')).toBeTruthy();
    expect(
      within(dialog).getByRole('link', { name: 'View drain audit evidence' }).getAttribute('href'),
    ).toBe('/audit?request_identity=request-drain');
    expect(invalidate).toHaveBeenCalledWith({ exact: true, queryKey: queryKeys.agent('agent-a') });
    expect(invalidate).toHaveBeenCalledWith({
      exact: false,
      queryKey: queryKeys.agentPoolAgents('pool-a'),
    });
    expect(invalidate).toHaveBeenCalledWith({ exact: false, queryKey: queryKeys.audit });
  });

  it('reassigns only an idle Agent to a selected Pool with a strong precondition', async () => {
    vi.stubGlobal('crypto', {
      randomUUID: vi.fn(() => '22222222-2222-4222-8222-222222222222'),
    });
    const api = fakeCapacityApi();
    api.getAgent.mockResolvedValue(agent('agent-a', 'Builder A', 'pool-a'));
    api.listAgentPools.mockResolvedValue({
      items: [pool('pool-a', 'Current Pool'), pool('pool-b', 'Target Pool')],
      next_cursor: null,
    });
    api.reassignAgentPool.mockResolvedValue({
      requestId: 'request-reassign',
      response: {
        disposition: 'applied',
        resource: { ...agent('agent-a', 'Builder A', 'pool-b'), version: 4 },
      },
    });
    const { queryClient } = renderView('/agents/agent-a', <AgentView api={api} />);
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');

    const target = await screen.findByRole('combobox', { name: 'Target Agent Pool' });
    expect(within(target).queryByRole('option', { name: 'Current Pool' })).toBeNull();
    await userEvent.click(target);
    await userEvent.click(screen.getByRole('option', { name: 'Target Pool' }));
    await userEvent.click(screen.getByRole('button', { name: 'Review pool reassignment' }));
    const dialog = screen.getByRole('dialog', { name: 'Move Builder A to Target Pool?' });
    expect(within(dialog).getByText(/stops assigning the Agent through Current Pool/)).toBeTruthy();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Move Agent' }));

    expect(api.reassignAgentPool).toHaveBeenCalledWith(
      'agent-a',
      { pool_id: 'pool-b' },
      {
        'Idempotency-Key': '22222222-2222-4222-8222-222222222222',
        'If-Match': '"3"',
      },
    );
    expect(await within(dialog).findByText('Pool reassigned (Applied).')).toBeTruthy();
    expect(
      within(dialog)
        .getByRole('link', { name: 'View reassignment audit evidence' })
        .getAttribute('href'),
    ).toBe('/audit?request_identity=request-reassign');
    expect(invalidate).toHaveBeenCalledWith({
      exact: false,
      queryKey: queryKeys.agentPoolAgents('pool-a'),
    });
    expect(invalidate).toHaveBeenCalledWith({
      exact: false,
      queryKey: queryKeys.agentPoolAgents('pool-b'),
    });
  });

  it('does not offer pool reassignment while an Agent has a current execution', async () => {
    const api = fakeCapacityApi();
    const active = agent('agent-a', 'Builder A', 'pool-a');
    active.current_execution = {
      attempt_id: 'attempt-a',
      build_id: 'build-a',
      job_id: 'job-a',
      lease_id: 'lease-a',
      lease_state: 'active',
    };
    api.getAgent.mockResolvedValue(active);
    api.listAgentPools.mockResolvedValue({
      items: [pool('pool-b', 'Target Pool')],
      next_cursor: null,
    });
    renderView('/agents/agent-a', <AgentView api={api} />);

    expect(
      await screen.findByText(/Pool reassignment is available only while the Agent is idle/),
    ).toBeTruthy();
    expect(screen.queryByRole('combobox', { name: 'Target Agent Pool' })).toBeNull();
  });

  it('surfaces an ineligible Pool conflict once and links its audit correlation', async () => {
    vi.stubGlobal('crypto', {
      randomUUID: vi.fn(() => '33333333-3333-4333-8333-333333333333'),
    });
    const api = fakeCapacityApi();
    api.getAgent.mockResolvedValue(agent('agent-a', 'Builder A', 'pool-a'));
    api.listAgentPools.mockResolvedValue({
      items: [pool('pool-a', 'Current Pool'), pool('pool-b', 'Ineligible Pool')],
      next_cursor: null,
    });
    api.reassignAgentPool.mockRejectedValue(
      new ManagementApiError({
        code: 'conflict',
        message: 'The target Agent Pool does not admit this Agent.',
        requestId: 'request-ineligible',
        retryAfterMilliseconds: null,
        status: 409,
      }),
    );
    renderView('/agents/agent-a', <AgentView api={api} />);

    await userEvent.click(await screen.findByRole('combobox', { name: 'Target Agent Pool' }));
    await userEvent.click(screen.getByRole('option', { name: 'Ineligible Pool' }));
    await userEvent.click(screen.getByRole('button', { name: 'Review pool reassignment' }));
    const dialog = screen.getByRole('dialog', { name: 'Move Builder A to Ineligible Pool?' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Move Agent' }));

    expect(
      await within(dialog).findByText('The target Agent Pool does not admit this Agent.'),
    ).toBeTruthy();
    expect(within(dialog).getByText(/Error code: conflict/)).toBeTruthy();
    expect(
      within(dialog).getByRole('link', { name: 'View audit evidence' }).getAttribute('href'),
    ).toBe('/audit?request_identity=request-ineligible');
    expect(within(dialog).queryByRole('button', { name: 'Retry same command' })).toBeNull();
    expect(api.reassignAgentPool).toHaveBeenCalledTimes(1);
  });
});

function renderView(path: string, element: ReactNode) {
  const route = path.startsWith('/agent-pools/') ? '/agent-pools/:poolId' : '/agents/:agentId';
  const router = createMemoryRouter([{ path: route, element }], { initialEntries: [path] });
  const queryClient = createConsoleQueryClient();
  render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { queryClient, router };
}

function managementError(requestId: string) {
  return new ManagementApiError({
    code: 'unavailable',
    message: 'Unavailable.',
    requestId,
    retryAfterMilliseconds: null,
    status: 503,
  });
}

function fakeCapacityApi() {
  return {
    getAgent: vi.fn<CapacityApi['getAgent']>(),
    getAgentPool: vi.fn<CapacityApi['getAgentPool']>(),
    drainAgent: vi.fn<CapacityApi['drainAgent']>(),
    listAgentPools: vi
      .fn<CapacityApi['listAgentPools']>()
      .mockResolvedValue({ items: [], next_cursor: null }),
    listAgents: vi
      .fn<CapacityApi['listAgents']>()
      .mockResolvedValue({ items: [], next_cursor: null }),
    reassignAgentPool: vi.fn<CapacityApi['reassignAgentPool']>(),
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((fulfill) => {
    resolve = fulfill;
  });
  return { promise, resolve };
}

function pool(id: string, name: string): AgentPoolResource {
  return {
    definition: {
      admission_policy: { mode: 'any' },
      concurrency_limit: 2,
      drain_state: 'accepting',
      enabled: true,
      fairness_policy: 'priority_fifo',
      static_capacity_limit: 4,
    },
    id,
    name,
    published_at_unix_ms: 1_700_000_000_000,
    version: 2,
  };
}

function agent(id: string, name: string, poolId: string): AgentResource {
  return {
    capacity: {
      logical_cpu_count: 8,
      state_disk_total_bytes: 53_687_091_200,
      total_memory_bytes: 17_179_869_184,
      virtualization_available: true,
      work_disk_total_bytes: 107_374_182_400,
    },
    current_execution: null,
    id,
    inventory: {
      agent_version: '0.1.0',
      cache: null,
      coordinator_protocols: [1],
      host_platform: { architecture: 'amd64', os: 'linux' },
      labels: { region: 'local' },
      octa: {
        build_commit: null,
        event_schemas: [1],
        features: [],
        octafile_versions: [1],
        plugin_protocols: [1],
        plugins: [],
        runner_protocols: [1],
        runner_sha256: 'a'.repeat(64),
        version: '0.4.0',
      },
      runtimes: [],
      source_plugins: [],
    },
    last_seen_at_unix_ms: 1_700_000_000_000,
    name,
    pool_id: poolId,
    pool_version: 2,
    status: 'online',
    version: 3,
  };
}
