// @vitest-environment jsdom

import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ManagementApiError } from '../../api/client';
import { createConsoleQueryClient } from '../../app/query';
import type { AgentPoolResource, AgentResource, CapacityApi } from './api';
import { CapacityExplorer } from './CapacityExplorer';

afterEach(cleanup);

describe('Capacity explorer', () => {
  it('paginates Pool and each expanded Agent branch independently', async () => {
    const api = fakeCapacityApi();
    api.listAgentPools
      .mockResolvedValueOnce(page([pool('pool-a', 'Linux'), pool('pool-b', 'macOS')], 'pool-next'))
      .mockResolvedValueOnce(page([pool('pool-c', 'Windows')], null));
    api.listAgents.mockImplementation(async (poolId, cursor) => {
      if (poolId === 'pool-a' && cursor === null) {
        return page([agent('agent-a', 'Builder A', poolId)], 'agent-a-next');
      }
      if (poolId === 'pool-a' && cursor === 'agent-a-next') {
        return page([agent('agent-a2', 'Builder A2', poolId)], null);
      }
      return page([agent('agent-b', 'Builder B', poolId)], null);
    });
    renderExplorer('/agents', api);

    await userEvent.click(await screen.findByRole('button', { name: 'Expand Linux' }));
    expect(await screen.findByRole('link', { name: 'Builder A' })).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Expand macOS' }));
    expect(await screen.findByRole('link', { name: 'Builder B' })).toBeTruthy();

    await userEvent.click(screen.getByRole('button', { name: 'Load more Agents in Linux' }));
    expect(await screen.findByRole('link', { name: 'Builder A2' })).toBeTruthy();
    expect(api.listAgents).toHaveBeenCalledWith('pool-a', 'agent-a-next', expect.any(AbortSignal));
    expect(api.listAgents).not.toHaveBeenCalledWith('pool-b', 'agent-a-next', expect.anything());

    await userEvent.click(screen.getByRole('button', { name: 'Load more Agent Pools' }));
    expect(await screen.findByRole('link', { name: 'Windows' })).toBeTruthy();
  });

  it('reveals a deep-linked Agent under its authoritative Pool without an unassigned bucket', async () => {
    const api = fakeCapacityApi();
    const selectedPool = pool('selected-pool', 'Selected Pool');
    const selectedAgent = agent('selected-agent', 'Selected Agent', selectedPool.id);
    api.getAgent.mockResolvedValue(selectedAgent);
    api.getAgentPool.mockResolvedValue(selectedPool);
    api.listAgentPools.mockResolvedValue(page([pool('other-pool', 'Other Pool')], null));
    api.listAgents.mockResolvedValue(
      page([agent('other-agent', 'Other Agent', selectedPool.id)], null),
    );
    renderExplorer('/agents/selected-agent', api);

    expect(await screen.findByRole('button', { name: 'Collapse Selected Pool' })).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Selected Pool' }).getAttribute('href')).toBe(
      '/agent-pools/selected-pool',
    );
    const selected = await screen.findByRole('link', { name: 'Selected Agent' });
    expect(selected.getAttribute('aria-current')).toBe('page');
    expect(selected.getAttribute('href')).toBe('/agents/selected-agent');
    expect(api.listAgents).toHaveBeenCalledWith('selected-pool', null, expect.any(AbortSignal));
    expect(screen.queryByText(/unassigned/i)).toBeNull();
  });

  it('distinguishes empty Agent Pools and stale branch pagination', async () => {
    const api = fakeCapacityApi();
    api.listAgentPools.mockResolvedValue(
      page([pool('empty-pool', 'Empty Pool'), pool('stale-pool', 'Stale Pool')], null),
    );
    api.listAgents.mockImplementation(async (poolId, cursor) => {
      if (poolId === 'empty-pool') return page([], null);
      if (cursor === null) return page([agent('agent-1', 'Agent One', poolId)], 'next');
      throw managementError('stale-agent-request');
    });
    renderExplorer('/agents', api);

    await userEvent.click(await screen.findByRole('button', { name: 'Expand Empty Pool' }));
    expect(await screen.findByText('No Agents are enrolled in Empty Pool.')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Expand Stale Pool' }));
    await screen.findByRole('link', { name: 'Agent One' });
    await userEvent.click(screen.getByRole('button', { name: 'Load more Agents in Stale Pool' }));
    expect(
      await screen.findByText('Refresh failed. Showing the last loaded Stale Pool Agents.'),
    ).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Agent One' })).toBeTruthy();
  });
});

export function fakeCapacityApi() {
  return {
    drainAgent: vi.fn<CapacityApi['drainAgent']>(),
    getAgent: vi.fn<CapacityApi['getAgent']>(),
    getAgentPool: vi.fn<CapacityApi['getAgentPool']>(),
    listAgentPools: vi.fn<CapacityApi['listAgentPools']>().mockResolvedValue(page([], null)),
    listAgents: vi.fn<CapacityApi['listAgents']>().mockResolvedValue(page([], null)),
    reassignAgentPool: vi.fn<CapacityApi['reassignAgentPool']>(),
  };
}

export function page<T>(items: T[], nextCursor: string | null) {
  return { items, next_cursor: nextCursor };
}

export function pool(id: string, name: string): AgentPoolResource {
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

export function agent(id: string, name: string, poolId: string): AgentResource {
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

function renderExplorer(path: string, api: CapacityApi) {
  const router = createMemoryRouter([{ path: '*', element: <CapacityExplorer api={api} /> }], {
    initialEntries: [path],
  });
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
