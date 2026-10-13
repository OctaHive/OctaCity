// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { FactoryExplorer } from './FactoryExplorer';
import type { FactoryRun } from './api';

afterEach(cleanup);

it('lazily opens bounded Configuration and Run branches, retaining filters and cached expansion', async () => {
  const projects = {
    listProjects: vi.fn(async () => ({
      items: [
        {
          id: 'project-a',
          name: 'Alpha',
          parent_id: null,
          has_children: false,
          version: 1,
          created_at_unix_ms: 1,
          updated_at_unix_ms: 1,
        },
      ],
      next_cursor: null,
      total: 1,
    })),
    getProject: vi.fn(),
  };
  const api = {
    listConfigurations: vi.fn(async () => ({
      items: [
        {
          id: 'configuration-a',
          project_id: 'project-a',
          version: 3,
          enabled: true,
          created_at_unix_ms: 1,
          published_at_unix_ms: 1,
        },
      ],
      next_cursor: null,
      total: 1,
    })),
    listRuns: vi.fn(async () => ({ items: [], next_cursor: 'runs-next', total: 0 })),
    getRun: vi.fn(),
    getConfiguration: vi.fn(),
  };
  const router = createMemoryRouter(
    [{ path: '*', element: <FactoryExplorer api={api} projects={projects} /> }],
    { initialEntries: ['/factory?state=waiting&decision=rework'] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(api.listConfigurations).not.toHaveBeenCalled();
  await userEvent.click(await screen.findByRole('button', { name: 'Expand Alpha' }));
  expect(api.listRuns).not.toHaveBeenCalled();
  await userEvent.click(
    await screen.findByRole('button', { name: 'Expand Factory Configuration · v3' }),
  );
  expect(await screen.findByText('No matching Factory Runs.')).toBeTruthy();
  expect(api.listRuns).toHaveBeenCalledWith(
    'project-a',
    { state: 'waiting', decision: 'rework', configuration_id: 'configuration-a' },
    null,
    expect.any(AbortSignal),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Load more Factory Runs' }));
  expect(api.listRuns).toHaveBeenLastCalledWith(
    'project-a',
    { state: 'waiting', decision: 'rework', configuration_id: 'configuration-a' },
    'runs-next',
    expect.any(AbortSignal),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Collapse Alpha' }));
  await userEvent.click(screen.getByRole('button', { name: 'Expand Alpha' }));
  expect(
    await screen.findByRole('button', { name: 'Collapse Factory Configuration · v3' }),
  ).toBeTruthy();
  expect(api.listConfigurations).toHaveBeenCalledTimes(1);
  expect(api.listRuns).toHaveBeenCalledTimes(2);
});

it('reveals an unloaded selected Run and its authoritative Project and Configuration from a copied link', async () => {
  const run: FactoryRun = {
    id: 'run-a',
    project_id: 'project-a',
    configuration_id: 'configuration-a',
    configuration_version: 3,
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
  const api = {
    listConfigurations: vi.fn(async () => ({ items: [], next_cursor: null, total: 0 })),
    listRuns: vi.fn(async () => ({ items: [], next_cursor: null, total: 0 })),
    getRun: vi.fn(async () => run),
    getConfiguration: vi.fn(async () => ({
      configuration: {
        enabled: true,
        reference: {
          id: 'configuration-a',
          project_id: 'project-a',
          version: 3,
          definition_digest: '1'.repeat(64),
        },
      },
      published_at_unix_ms: 1,
    })),
  };
  const project = {
    id: 'project-a',
    name: 'Selected Project',
    parent_id: null,
    created_at_unix_ms: 1,
    updated_at_unix_ms: 1,
    version: 1,
  };
  const projects = {
    listProjects: vi.fn(async () => ({ items: [], next_cursor: null, total: 0 })),
    getProject: vi.fn(async () => ({ project, ancestors: [] })),
  };
  const router = createMemoryRouter(
    [{ path: '*', element: <FactoryExplorer api={api} projects={projects} /> }],
    { initialEntries: ['/factory/run-a?state=waiting'] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('button', { name: 'Collapse Selected Project' })).toBeTruthy();
  const link = await screen.findByRole('link', { name: 'Requested change' });
  expect(link.getAttribute('aria-current')).toBe('page');
  expect(link.getAttribute('href')).toBe(
    '/factory/run-a?state=waiting&configuration_id=configuration-a',
  );
  expect(screen.getByText('waiting · operator-selected-node')).toBeTruthy();
  expect(api.getConfiguration).toHaveBeenCalledWith('configuration-a', 3, expect.any(AbortSignal));
});

it('reveals a selected Project outside the first page from a copied workbench link', async () => {
  const projectId = '11111111-1111-4111-8111-111111111111';
  const project = {
    id: projectId,
    name: 'Selected owner',
    parent_id: null,
    created_at_unix_ms: 1,
    updated_at_unix_ms: 1,
    version: 1,
  };
  const projects = {
    listProjects: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    getProject: vi.fn(async () => ({ project, ancestors: [] })),
  };
  const api = {
    listConfigurations: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listRuns: vi.fn(),
    getRun: vi.fn(),
    getConfiguration: vi.fn(),
  };
  const router = createMemoryRouter(
    [{ path: '*', element: <FactoryExplorer api={api} projects={projects} /> }],
    { initialEntries: [`/factory?project_id=${projectId}`] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('button', { name: 'Collapse Selected owner' })).toBeTruthy();
  expect(
    await screen.findByText(
      'Optional Factory mode is not configured. Ordinary CI/CD remains available.',
    ),
  ).toBeTruthy();
  expect(projects.getProject).toHaveBeenCalledWith(projectId, expect.any(AbortSignal));
});

it('discovers children of a revealed selected Project instead of assuming it is a leaf', async () => {
  const projectId = '11111111-1111-4111-8111-111111111111';
  const project = {
    id: projectId,
    name: 'Selected owner',
    parent_id: null,
    created_at_unix_ms: 1,
    updated_at_unix_ms: 1,
    version: 1,
  };
  const projects = {
    listProjects: vi.fn(async (parent: string | null) => ({
      items:
        parent === projectId
          ? [
              {
                ...project,
                id: '33333333-3333-4333-8333-333333333333',
                name: 'Visible child',
                parent_id: projectId,
                has_children: false,
              },
            ]
          : [],
      total: parent === projectId ? 1 : 0,
      next_cursor: null,
    })),
    getProject: vi.fn(async () => ({ project, ancestors: [] })),
  };
  const api = {
    listConfigurations: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listRuns: vi.fn(),
    getRun: vi.fn(),
    getConfiguration: vi.fn(),
  };
  const router = createMemoryRouter(
    [{ path: '*', element: <FactoryExplorer api={api} projects={projects} /> }],
    { initialEntries: [`/factory?project_id=${projectId}`] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('button', { name: 'Expand Visible child' })).toBeTruthy();
  expect(projects.listProjects).toHaveBeenCalledWith(projectId, null, expect.any(AbortSignal));
});
