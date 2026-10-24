// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { FactoryDefinitions } from './FactoryDefinitions';
import type { FactoryApi } from './api';
import { factoryApi } from './api';
import { ManagementApiError } from '../../api/client';
import { configurationFixture } from '../../../tests/fixtures/factory';

afterEach(cleanup);

it('blocks draft creation when the server has no Factory capabilities while keeping published Flows readable', async () => {
  const configuration = configurationFixture().configuration;
  const projectId = configuration.reference.project_id;
  const flow = { flow: configuration.flow, project_id: projectId, published_at_unix_ms: 1 };
  const api: FactoryApi = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [flow], total: 1, next_cursor: null })),
    getFlow: vi.fn(async () => flow),
    listChoices: vi.fn(async () => {
      throw new ManagementApiError({
        code: 'capability_unavailable',
        message: 'the selected adapter does not support this operation',
        status: 422,
        requestId: 'factory-disabled-request',
        retryAfterMilliseconds: null,
      });
    }),
  };
  const router = createMemoryRouter([
    { path: '*', element: <FactoryDefinitions api={api} projectId={projectId} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(
    await screen.findByText(
      'Factory authoring is unavailable on this server. Ordinary CI/CD remains available.',
    ),
  ).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Create configuration draft' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.getByRole('button', { name: 'Create Flow draft' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.getByRole('button', { name: 'Admit Work' })).toHaveProperty('disabled', true);
  expect(screen.getByRole('button', { name: 'AI agents' })).toHaveProperty('disabled', false);
  expect(screen.queryByRole('button', { name: 'Enroll agent' })).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Create configuration draft' }));
  expect(screen.queryByRole('button', { name: 'Discard draft' })).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Inspect Flow Definition · v1' }));
  expect(await screen.findByRole('heading', { name: 'Immutable Flow Definition' })).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Edit Flow draft' })).toHaveProperty('disabled', true);
});

it('discovers visible configurations and Flows, then reads exact immutable versions without typing identifiers', async () => {
  const configuration = configurationFixture();
  const projectId = configuration.configuration.reference.project_id;
  const flow = {
    flow: configuration.configuration.flow,
    project_id: projectId,
    published_at_unix_ms: 1,
  };
  const api = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({
      items: [
        {
          ...configuration.configuration.reference,
          enabled: true,
          created_at_unix_ms: 1,
          published_at_unix_ms: 1,
        },
      ],
      total: 1,
      next_cursor: null,
    })),
    getConfiguration: vi.fn(async () => configuration),
    listFlows: vi.fn(async () => ({ items: [flow], total: 1, next_cursor: null })),
    getFlow: vi.fn(async () => flow),
  };
  const router = createMemoryRouter([
    { path: '*', element: <FactoryDefinitions api={api} projectId={projectId} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Factory Configuration · v1' }),
  );
  expect(await screen.findByText('manual.medium · v1')).toBeTruthy();
  expect(api.getConfiguration).toHaveBeenCalledWith(
    configuration.configuration.reference.id,
    1,
    expect.any(AbortSignal),
  );
  expect(router.state.location.search).toContain(
    'configuration_id=' + configuration.configuration.reference.id,
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Flow Definition · v1' }),
  );
  expect(await screen.findByRole('heading', { name: 'Immutable Flow Definition' })).toBeTruthy();
  expect(api.getFlow).toHaveBeenCalledWith(flow.flow.closure.root.id, 1, expect.any(AbortSignal));
  expect(screen.queryByRole('textbox', { name: 'Resource identifier' })).toBeNull();
});

it('opens memory-only configuration and Flow drafts from visible resources and discards them on leaving', async () => {
  const configuration = configurationFixture();
  const projectId = configuration.configuration.reference.project_id;
  const flow = {
    flow: configuration.configuration.flow,
    project_id: projectId,
    published_at_unix_ms: 1,
  };
  const api = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({
      items: [
        {
          ...configuration.configuration.reference,
          enabled: true,
          created_at_unix_ms: 1,
          published_at_unix_ms: 1,
        },
      ],
      total: 1,
      next_cursor: null,
    })),
    getConfiguration: vi.fn(async () => configuration),
    listFlows: vi.fn(async () => ({ items: [flow], total: 1, next_cursor: null })),
    getFlow: vi.fn(async () => flow),
    listChoices: vi.fn(async () => ({
      items: [
        {
          alias: 'Visible admission',
          identity: configuration.configuration.admission_policy.identity,
          kind: 'admission_policy' as const,
          reference: {
            kind: 'immutable' as const,
            reference: configuration.configuration.admission_policy,
          },
        },
        {
          alias: 'Visible ceiling',
          identity: configuration.configuration.permission_ceiling.identity,
          kind: 'permission_ceiling' as const,
          reference: {
            kind: 'immutable' as const,
            reference: configuration.configuration.permission_ceiling,
          },
        },
      ],
      total: 2,
      next_cursor: null,
    })),
  };
  const router = createMemoryRouter([
    { path: '*', element: <FactoryDefinitions api={api} projectId={projectId} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Factory Configuration · v1' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Edit configuration draft' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Admission policy' }));
  expect(await screen.findByRole('option', { name: 'Visible admission · v1' })).toBeTruthy();
  await userEvent.click(screen.getByRole('option', { name: 'Visible admission · v1' }));
  await userEvent.click(screen.getByRole('button', { name: 'Discard draft' }));
  expect(screen.queryByRole('combobox', { name: 'Admission policy' })).toBeNull();
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Flow Definition · v1' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Flow draft' }));
  const name = await screen.findByRole('textbox', { name: 'Node name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'draft_only_name');
  expect(router.state.location.search).not.toContain('draft_only_name');
  expect(JSON.stringify(localStorage)).not.toContain('draft_only_name');
  await userEvent.click(screen.getByRole('button', { name: 'Discard draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Edit Flow draft' }));
  expect(await screen.findByRole('textbox', { name: 'Node name' })).toHaveProperty(
    'value',
    'implement',
  );
});

it('keeps the graph editable without installed capabilities and loads older immutable versions in bounded windows', async () => {
  const configuration = configurationFixture();
  const projectId = configuration.configuration.reference.project_id;
  const flow = {
    flow: configuration.configuration.flow,
    project_id: projectId,
    published_at_unix_ms: 1,
  };
  const api = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({
      items: [
        {
          ...configuration.configuration.reference,
          version: 205,
          enabled: true,
          created_at_unix_ms: 1,
          published_at_unix_ms: 1,
        },
      ],
      total: 1,
      next_cursor: null,
    })),
    getConfiguration: vi.fn(async (_id: string, version: number) => ({
      ...configuration,
      configuration: {
        ...configuration.configuration,
        reference: { ...configuration.configuration.reference, version },
      },
    })),
    listFlows: vi.fn(async () => ({ items: [flow], total: 1, next_cursor: null })),
    getFlow: vi.fn(async () => flow),
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
  };
  const router = createMemoryRouter([
    { path: '*', element: <FactoryDefinitions api={api} projectId={projectId} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Factory Configuration · v205' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Earlier versions' }));
  await userEvent.click(screen.getByRole('button', { name: 'Earlier versions' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Version' }));
  await userEvent.click(screen.getByRole('option', { name: 'Version 1' }));
  expect(api.getConfiguration).toHaveBeenLastCalledWith(
    configuration.configuration.reference.id,
    1,
    expect.any(AbortSignal),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Inspect Flow Definition · v1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Flow draft' }));
  expect(await screen.findByRole('textbox', { name: 'Node name' })).toBeTruthy();
  expect(screen.getByText('No resource choices are visible.')).toBeTruthy();
});

it('starts first-publication drafts without typing identifiers and exposes validation and review on the real workbench', async () => {
  const configuration = configurationFixture();
  const projectId = configuration.configuration.reference.project_id;
  const flow = {
    flow: configuration.configuration.flow,
    project_id: projectId,
    published_at_unix_ms: 1,
  };
  const api = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [flow], total: 1, next_cursor: null })),
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
  };
  const router = createMemoryRouter([
    { path: '*', element: <FactoryDefinitions api={api} projectId={projectId} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Create Flow draft' }));
  expect(await screen.findByRole('button', { name: 'Validate draft' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.getByRole('button', { name: 'Add node' })).toBeTruthy();
  expect(screen.queryByRole('textbox', { name: 'Resource identifier' })).toBeNull();
  expect(router.state.location.search).toBe('');
});

it('owns only one authoring draft at a time and discards it when another immutable resource is selected', async () => {
  const configuration = configurationFixture();
  const projectId = configuration.configuration.reference.project_id;
  const flow = {
    flow: configuration.configuration.flow,
    project_id: projectId,
    published_at_unix_ms: 1,
  };
  const api = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({
      items: [
        {
          ...configuration.configuration.reference,
          enabled: true,
          created_at_unix_ms: 1,
          published_at_unix_ms: 1,
        },
      ],
      total: 1,
      next_cursor: null,
    })),
    getConfiguration: vi.fn(async () => configuration),
    listFlows: vi.fn(async () => ({ items: [flow], total: 1, next_cursor: null })),
    getFlow: vi.fn(async () => flow),
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
  };
  const router = createMemoryRouter([
    { path: '*', element: <FactoryDefinitions api={api} projectId={projectId} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Factory Configuration · v1' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Edit configuration draft' }));
  expect(await screen.findByRole('combobox', { name: 'Admission policy' })).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Inspect Flow Definition · v1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Flow draft' }));
  expect(await screen.findByRole('textbox', { name: 'Node name' })).toBeTruthy();
  expect(screen.queryByRole('combobox', { name: 'Admission policy' })).toBeNull();
  expect(screen.getAllByRole('button', { name: 'Discard draft' })).toHaveLength(1);
});

it('links a field error to its existing control without copying draft values into the URL', async () => {
  const configuration = configurationFixture();
  const projectId = configuration.configuration.reference.project_id;
  const flow = {
    flow: configuration.configuration.flow,
    project_id: projectId,
    published_at_unix_ms: 1,
  };
  const api = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [flow], total: 1, next_cursor: null })),
    getFlow: vi.fn(async () => flow),
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
  };
  const router = createMemoryRouter([
    { path: '*', element: <FactoryDefinitions api={api} projectId={projectId} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Flow Definition · v1' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Flow draft' }));
  const input = await screen.findByRole('textbox', { name: 'Node name' });
  await userEvent.clear(input);
  await userEvent.type(input, 'Bad name');
  await userEvent.click(screen.getByRole('button', { name: '/root/nodes/1/key' }));
  expect(document.activeElement).toBe(input);
  expect(router.state.location.hash).toBe('');
  expect(router.state.location.search).not.toContain('Bad');
});

it('replaces from a historical draft using the discovered current version rather than the historical version as the precondition', async () => {
  const configuration = configurationFixture();
  const projectId = configuration.configuration.reference.project_id;
  const historical = {
    flow: configuration.configuration.flow,
    project_id: projectId,
    published_at_unix_ms: 1,
  };
  const current = {
    ...historical,
    flow: {
      ...historical.flow,
      closure: {
        ...historical.flow.closure,
        root: { ...historical.flow.closure.root, version: 7 },
      },
    },
  };
  const replaceFlow = vi.fn<FactoryApi['replaceFlow']>(async () => ({
    disposition: 'applied' as const,
    resource: current,
  }));
  const api = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [current], total: 1, next_cursor: null })),
    getFlow: vi.fn(async () => historical),
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    validateFlow: vi.fn(async () => historical.flow),
    replaceFlow,
  };
  const router = createMemoryRouter([
    { path: '*', element: <FactoryDefinitions api={api} projectId={projectId} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Flow Definition · v7' }),
  );
  await userEvent.click(await screen.findByRole('combobox', { name: 'Version' }));
  await userEvent.click(screen.getByRole('option', { name: 'Version 1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Flow draft' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(screen.getByRole('button', { name: 'Confirm publish' }));
  expect(await screen.findByText('Immutable version published.')).toBeTruthy();
  expect(replaceFlow.mock.calls[0]?.[2]).toEqual(expect.objectContaining({ 'If-Match': '"7"' }));
});

it('opens a copied immutable Flow version outside the loaded current-definition page', async () => {
  const configuration = configurationFixture();
  const projectId = configuration.configuration.reference.project_id;
  const flow = {
    flow: configuration.configuration.flow,
    project_id: projectId,
    published_at_unix_ms: 1,
  };
  const id = flow.flow.closure.root.id;
  const api = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    currentFlow: vi.fn(async () => flow),
    getFlow: vi.fn(async () => flow),
  };
  const router = createMemoryRouter(
    [{ path: '*', element: <FactoryDefinitions api={api} projectId={projectId} /> }],
    {
      initialEntries: [`/factory?project_id=${projectId}&flow_definition_id=${id}&flow_version=1`],
    },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('heading', { name: 'Immutable Flow Definition' })).toBeTruthy();
  expect(api.currentFlow).toHaveBeenCalledWith(id, expect.any(AbortSignal));
  expect(api.getFlow).toHaveBeenCalledWith(id, 1, expect.any(AbortSignal));
});
