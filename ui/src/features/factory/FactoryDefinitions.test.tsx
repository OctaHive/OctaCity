// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { FactoryDefinitions } from './FactoryDefinitions';
import type { FactoryApi } from './api';
import { factoryApi } from './api';
import { ManagementApiError } from '../../api/client';
import { configurationFixture } from '../../../tests/fixtures/factory';

import { factoryPolicyApi } from './policyApi';

beforeEach(() => {
  vi.spyOn(factoryPolicyApi, 'resolveLegacy').mockResolvedValue({ mapping: null });
  vi.spyOn(factoryPolicyApi, 'list').mockResolvedValue({ items: [], next_cursor: null });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it('presents a readable configuration summary and retains exact policies and budget values under technical details', async () => {
  const configuration = configurationFixture();
  configuration.configuration.hard_budget = {
    max_attempts: 8,
    max_elapsed_millis: 600000,
    max_tokens: 300000,
    max_cost_micro_units: 2000000,
    max_output_bytes: 134217728,
  };
  const reference = configuration.configuration.reference;
  const api: FactoryApi = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({
      items: [{ ...reference, enabled: true, created_at_unix_ms: 1, published_at_unix_ms: 1 }],
      total: 1,
      next_cursor: null,
    })),
    getConfiguration: vi.fn(async () => configuration),
    listFlows: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider
        router={createMemoryRouter([
          { path: '*', element: <FactoryDefinitions api={api} projectId={reference.project_id} /> },
        ])}
      />
    </QueryClientProvider>,
  );
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Factory Configuration · v1' }),
  );
  const summary = await screen.findByRole('region', { name: 'Factory Configuration · v1' });
  const content = within(summary);
  expect(content.getByText('600 s')).toBeTruthy();
  expect(content.getByText('128.0 MiB')).toBeTruthy();
  expect(content.getByText('Maximum attempts').tagName).toBe('DT');
  const details = content.getByText('Technical details', { exact: true });
  expect(details.closest('details')).toHaveProperty('open', false);
  expect(content.getByText('max_elapsed_millis').closest('details')).toHaveProperty('open', false);
  await userEvent.click(details);
  expect(content.getByText('manual.medium · v1')).toBeTruthy();
  expect(content.getByText('max_elapsed_millis').tagName).toBe('DT');
  expect(content.getByText('600000')).toBeTruthy();
  expect(content.getByRole('button', { name: 'Edit configuration draft' })).toBeTruthy();
  expect(content.getByRole('button', { name: 'Configure automatic intake' })).toBeTruthy();
});

it('distinguishes two pipelines with the same version by their saved names and opens the selected identity', async () => {
  const configuration = configurationFixture().configuration;
  const projectId = configuration.reference.project_id;
  const flows = ['Триаж задач', 'Проверка документации'].map((name, index) => {
    const flow = structuredClone(configuration.flow);
    const root = flow.closure.definitions.find(
      (item) => item.reference.id === flow.closure.root.id,
    )!;
    const id = index ? '20000000-0000-4000-8000-000000000002' : flow.closure.root.id;
    flow.closure.root.id = id;
    root.reference.id = id;
    root.name = name;
    return { flow, project_id: projectId, published_at_unix_ms: 1 };
  });
  const api: FactoryApi = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: flows, total: 2, next_cursor: null })),
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    getFlow: vi.fn(async (id) => flows.find((item) => item.flow.closure.root.id === id)!),
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider
        router={createMemoryRouter([
          { path: '*', element: <FactoryDefinitions api={api} projectId={projectId} /> },
        ])}
      />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('button', { name: 'Триаж задач · v1' })).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Проверка документации · v1' }));
  expect(await screen.findByRole('heading', { name: 'Проверка документации' })).toBeTruthy();
  expect(api.getFlow).toHaveBeenCalledWith(
    '20000000-0000-4000-8000-000000000002',
    1,
    expect.any(AbortSignal),
  );
});

it('opens Add pipeline as an empty graph without a template, prior Flow or publication', async () => {
  const api: FactoryApi = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    createFlow: vi.fn(),
    createConfiguration: vi.fn(),
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider
        router={createMemoryRouter([
          { path: '*', element: <FactoryDefinitions api={api} projectId="delivery" /> },
        ])}
      />
    </QueryClientProvider>,
  );
  const add = await screen.findByRole('button', { name: 'Add pipeline' });
  await waitFor(() => expect(add).toHaveProperty('disabled', false));
  expect(screen.getByRole('button', { name: 'Create configuration draft' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(
    screen.getByText('Publish a step graph before configuring how the pipeline starts.'),
  ).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Set up triage' })).toBeNull();
  await userEvent.click(add);
  expect(await screen.findByRole('img', { name: 'Flow graph' })).toBeTruthy();
  expect(screen.queryByRole('textbox', { name: 'Step name' })).toBeNull();
  expect(
    screen.getByText(/Publish the step graph.*configure how the pipeline starts/i),
  ).toBeTruthy();
  expect(api.createFlow).not.toHaveBeenCalled();
  expect(api.createConfiguration).not.toHaveBeenCalled();
});

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
  expect(screen.getByRole('button', { name: 'Add pipeline' })).toHaveProperty('disabled', true);
  expect(screen.getByRole('button', { name: 'Admit Work' })).toHaveProperty('disabled', true);
  expect(screen.getByRole('link', { name: 'AI agents' }).getAttribute('href')).toBe(
    `/ai-settings?project_id=${projectId}&view=agents`,
  );
  expect(screen.queryByRole('button', { name: 'Enroll agent' })).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Create configuration draft' }));
  expect(screen.queryByRole('button', { name: 'Discard draft' })).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'implement · v1' }));
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
  await userEvent.click(await screen.findByRole('button', { name: 'implement · v1' }));
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
  expect(await screen.findByRole('combobox', { name: 'Launch policy' })).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Discard draft' }));
  expect(screen.queryByRole('combobox', { name: 'Launch policy' })).toBeNull();
  await userEvent.click(await screen.findByRole('button', { name: 'implement · v1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Flow draft' }));
  const name = await screen.findByRole('textbox', { name: 'Step name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'draft_only_name');
  expect(router.state.location.search).not.toContain('draft_only_name');
  expect(JSON.stringify(localStorage)).not.toContain('draft_only_name');
  await userEvent.click(screen.getByRole('button', { name: 'Discard draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Edit Flow draft' }));
  expect(await screen.findByRole('textbox', { name: 'Step name' })).toHaveProperty(
    'value',
    'implement',
  );
});

it('keeps the graph editable and chooses any historical version inside one bounded searchable dropdown', async () => {
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
  const version = await screen.findByRole('combobox', { name: 'Version' });
  expect(screen.queryByRole('button', { name: 'Earlier versions' })).toBeNull();
  expect(screen.queryByRole('button', { name: 'Later versions' })).toBeNull();
  expect(screen.queryByRole('searchbox', { name: 'Find version by number' })).toBeNull();
  await userEvent.click(version);
  expect(screen.getAllByRole('option').length).toBeLessThanOrEqual(101);
  await userEvent.type(screen.getByRole('searchbox', { name: 'Find version by number' }), '1');
  await userEvent.click(screen.getByRole('option', { name: 'Version 1' }));
  expect(api.getConfiguration).toHaveBeenLastCalledWith(
    configuration.configuration.reference.id,
    1,
    expect.any(AbortSignal),
  );
  await userEvent.click(await screen.findByRole('combobox', { name: 'Version' }));
  await userEvent.type(screen.getByRole('searchbox', { name: 'Find version by number' }), '205');
  await userEvent.click(screen.getByRole('option', { name: 'Version 205' }));
  expect(api.getConfiguration).toHaveBeenLastCalledWith(
    configuration.configuration.reference.id,
    205,
    expect.any(AbortSignal),
  );
  await userEvent.click(screen.getByRole('button', { name: 'implement · v1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Flow draft' }));
  expect(await screen.findByRole('textbox', { name: 'Step name' })).toBeTruthy();
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
  await userEvent.click(await screen.findByRole('button', { name: 'Add pipeline' }));
  expect(await screen.findByRole('button', { name: 'Validate draft' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.getByRole('button', { name: 'Add step' })).toBeTruthy();
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
  expect(await screen.findByRole('combobox', { name: 'Launch policy' })).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'implement · v1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Flow draft' }));
  expect(await screen.findByRole('textbox', { name: 'Step name' })).toBeTruthy();
  expect(screen.queryByRole('combobox', { name: 'Launch policy' })).toBeNull();
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
  await userEvent.click(await screen.findByRole('button', { name: 'implement · v1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Flow draft' }));
  const input = await screen.findByRole('textbox', { name: 'Step name' });
  await userEvent.clear(input);
  await userEvent.type(input, 'Bad name');
  await userEvent.click(screen.getByRole('button', { name: 'Step 2 · Step name' }));
  expect(document.activeElement).toBe(input);
  expect(router.state.location.hash).toBe('');
  expect(router.state.location.search).not.toContain('Bad');
});

it('publishes a historical Flow draft against the current version and reopens the saved criteria', async () => {
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
        definitions: historical.flow.closure.definitions.map((definition) => ({
          ...definition,
          reference: { ...definition.reference, version: 7 },
        })),
      },
    },
  };
  let latest = current;
  const replaceFlow = vi.fn<FactoryApi['replaceFlow']>(async (_id, { definition }) => {
    const reference = { ...current.flow.closure.root, version: 8 };
    latest = {
      ...current,
      flow: {
        ...current.flow,
        closure: { root: reference, definitions: [{ ...definition.root, reference }] },
      },
    };
    return { disposition: 'applied', resource: latest };
  });
  const api = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [latest], total: 1, next_cursor: null })),
    getFlow: vi.fn(async (_id: string, version: number) => (version === 1 ? historical : latest)),
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
  await userEvent.click(await screen.findByRole('button', { name: 'implement · v7' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Version' }));
  await userEvent.click(screen.getByRole('option', { name: 'Version 1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Flow draft' }));
  const task = await screen.findByRole('checkbox', { name: 'Task from admitted Work' });
  if (!(task as HTMLInputElement).checked) await userEvent.click(task);
  const criteriaLabel = { name: 'Acceptance criteria from admitted Work' };
  await userEvent.click(screen.getByRole('checkbox', criteriaLabel));
  await userEvent.click(await screen.findByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(screen.getByRole('button', { name: 'Confirm publish' }));
  await waitFor(() => expect(router.state.location.search).toContain('flow_version=8'));
  expect(replaceFlow.mock.calls[0]?.[2]).toEqual(expect.objectContaining({ 'If-Match': '"7"' }));
  expect((await screen.findByRole('combobox', { name: 'Version' })).textContent).toContain(
    'Version 8',
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Flow draft' }));
  expect(await screen.findByRole('checkbox', criteriaLabel)).toHaveProperty('checked', true);
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
