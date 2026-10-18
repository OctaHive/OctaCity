// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { ManagementApiError } from '../../api/client';
import type { FactoryApi } from './api';
import { factoryApi } from './api';
import { FactoryPublication } from './FactoryPublication';
import { configurationFixture } from '../../../tests/fixtures/factory';
afterEach(cleanup);
function setup() {
  const config = configurationFixture().configuration;
  const root = config.flow.closure.definitions.find(
    (item) => item.reference.id === config.flow.closure.root.id,
  )!;
  const { reference, ...definition } = root;
  void reference;
  return {
    config,
    request: {
      id: config.flow.closure.root.id,
      project_id: config.reference.project_id,
      definition: {
        root: definition,
        data_schemas: config.flow.data_schemas,
        limits: config.flow.limits,
      },
    },
  };
}
it('requires fresh authoritative validation and replays one frozen publication with its current-version precondition', async () => {
  const { config, request } = setup();
  const result = { flow: config.flow, project_id: request.project_id, published_at_unix_ms: 1 };
  const replaceFlow = vi
    .fn()
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'transport_failure',
        message: 'Unavailable',
        requestId: null,
        retryAfterMilliseconds: null,
        status: null,
      }),
    )
    .mockResolvedValueOnce({ disposition: 'replayed', resource: result });
  const api = { ...factoryApi, validateFlow: vi.fn(async () => config.flow), replaceFlow };
  function Harness() {
    const [definition, setDefinition] = useState(request.definition);
    return (
      <>
        <button
          onClick={() =>
            setDefinition({
              ...definition,
              root: {
                ...definition.root,
                execution: { ...definition.root.execution, max_active_nodes: 1 },
              },
            })
          }
        >
          Change bounds
        </button>
        <FactoryPublication
          api={api}
          subject={{ kind: 'flow', request: { ...request, definition } }}
          version={1}
        />
      </>
    );
  }
  const router = createMemoryRouter([{ path: '*', element: <Harness /> }]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  expect(await screen.findByText('Authoritative validation passed.')).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Change bounds' }));
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  const dialog = await screen.findByRole('dialog', { name: 'Publish Flow Definition?' });
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publish' }));
  await userEvent.click(await within(dialog).findByRole('button', { name: 'Retry same command' }));
  expect(replaceFlow).toHaveBeenCalledTimes(2);
  expect(replaceFlow.mock.calls[0]).toEqual(replaceFlow.mock.calls[1]);
  expect(replaceFlow.mock.calls[0]?.[2]['If-Match']).toBe('"1"');
  expect(await within(dialog).findByText('Immutable version published.')).toBeTruthy();
});
it('associates authoritative field errors with the draft and retains unrelated edits without allowing publication', async () => {
  const { request } = setup();
  request.definition.root.nodes[0]!.budget.max_tokens = 321;
  const api = {
    ...factoryApi,
    validateFlow: vi.fn(async () => {
      throw new ManagementApiError({
        code: 'invalid_request',
        message: 'Invalid contract',
        requestId: null,
        retryAfterMilliseconds: null,
        status: 400,
        validationIssues: [{ field: '/root/input_schema', code: 'unavailable_reference' }],
      });
    }),
  };
  const router = createMemoryRouter([
    {
      path: '*',
      element: <FactoryPublication api={api} subject={{ kind: 'flow', request }} version={1} />,
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  expect(await screen.findByText('/root/input_schema')).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(request.definition.root.nodes[0]?.budget.max_tokens).toBe(321);
});

it('creates a first immutable Flow after confirmation and abandons a reviewed intent without sending it', async () => {
  const { config, request } = setup();
  const createFlow = vi.fn<FactoryApi['createFlow']>(async () => ({
    disposition: 'applied' as const,
    resource: { flow: config.flow, project_id: request.project_id, published_at_unix_ms: 1 },
  }));
  const api = { ...factoryApi, validateFlow: vi.fn(async () => config.flow), createFlow };
  const router = createMemoryRouter([
    { path: '*', element: <FactoryPublication api={api} subject={{ kind: 'flow', request }} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  let dialog = await screen.findByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Abandon' }));
  expect(createFlow).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  dialog = await screen.findByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publish' }));
  expect(await within(dialog).findByText('Immutable version published.')).toBeTruthy();
  expect(createFlow).toHaveBeenCalledWith(
    request,
    expect.objectContaining({ 'Idempotency-Key': expect.stringMatching(/^[a-f0-9-]{36}$/) }),
  );
  expect(createFlow.mock.calls[0]?.[1]).not.toHaveProperty('If-Match');
});
it('refreshes a stale baseline explicitly, preserves draft values and requires a new validation before a new intent', async () => {
  const { config, request } = setup();
  const current = {
    flow: {
      ...config.flow,
      closure: { ...config.flow.closure, root: { ...config.flow.closure.root, version: 2 } },
    },
    project_id: request.project_id,
    published_at_unix_ms: 2,
  };
  const replaceFlow = vi
    .fn()
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'precondition_failed',
        message: 'Stale baseline',
        requestId: null,
        retryAfterMilliseconds: null,
        status: 412,
      }),
    )
    .mockResolvedValueOnce({ disposition: 'applied', resource: current });
  const api = {
    ...factoryApi,
    validateFlow: vi.fn(async () => config.flow),
    replaceFlow,
    currentFlow: vi.fn(async () => current),
  };
  const router = createMemoryRouter([
    {
      path: '*',
      element: <FactoryPublication api={api} subject={{ kind: 'flow', request }} version={1} />,
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(api.currentFlow).toHaveBeenCalledWith(request.id);
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  expect(await screen.findByText('Immutable version published.')).toBeTruthy();
  expect(replaceFlow.mock.calls[1]?.[1]).toEqual({ definition: request.definition });
  expect(replaceFlow.mock.calls[1]?.[2]['If-Match']).toBe('"2"');
  expect(replaceFlow.mock.calls[1]?.[2]['Idempotency-Key']).not.toBe(
    replaceFlow.mock.calls[0]?.[2]['Idempotency-Key'],
  );
});

it('reviews a disabled configuration with server-derived capability and permission diagnostics and replaces its immutable version', async () => {
  const { config } = setup();
  const request = {
    id: config.reference.id,
    project_id: config.reference.project_id,
    definition: {
      enabled: false,
      admission_policy: 'admission',
      permission_ceiling: 'permissions',
      flow: config.flow,
      hard_budget: config.hard_budget,
      wip_limits: config.wip_limits,
    },
  };
  const diagnostics = {
    errors: [],
    unavailable_references: [],
    required_node_capabilities: [
      { identity: 'installed.capability', version: 'v7', digest: '7'.repeat(64) },
    ],
    required_agent_capabilities: [],
    required_backend_capabilities: ['filesystem_paths'],
    effective_permissions: [
      {
        definition: config.flow.closure.root,
        node: 'operator_defined_step',
        permissions: config.flow.limits.permissions,
      },
    ],
    warnings: [],
  };
  const replaceConfiguration = vi.fn<FactoryApi['replaceConfiguration']>(async () => ({
    disposition: 'applied',
    resource: {
      configuration: { ...config, enabled: false, reference: { ...config.reference, version: 2 } },
      published_at_unix_ms: 2,
    },
  }));
  const api = {
    ...factoryApi,
    validateConfiguration: vi.fn(async () => ({ configuration: config, diagnostics })),
    replaceConfiguration,
  };
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <FactoryPublication api={api} subject={{ kind: 'configuration', request }} version={1} />
      ),
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  expect(await screen.findByText('installed.capability · v7')).toBeTruthy();
  expect(screen.getByRole('heading', { name: 'Effective permission narrowing' })).toBeTruthy();
  expect(screen.getByText('operator_defined_step')).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(await screen.findByRole('dialog', { name: 'Publish Factory Configuration?' })).getByRole(
      'button',
      { name: 'Confirm publish' },
    ),
  );
  expect(await screen.findByText('Immutable version published.')).toBeTruthy();
  expect(replaceConfiguration.mock.calls[0]?.[1]).toEqual({ definition: request.definition });
  expect(replaceConfiguration.mock.calls[0]?.[2]['If-Match']).toBe('"1"');
});

it('reviews exact immutable per-stage effects before publishing the validated Flow', async () => {
  const { config, request } = setup();
  const choice = (await import('../../../tests/fixtures/integrations')).integrationFixture(
    'WorkSourceInputChoices',
    1,
  );
  const writer = choice.items.find((item) => item.purpose === 'task_update')!;
  const source = choice.items.find((item) => item.purpose === 'task_read')!;
  const node = request.definition.root.nodes.find((node) => node.action)!;
  node.action!.action = 'set_status';
  node.action!.parameters = {
    status: 'review-next',
    outbox: { max_attempts: 2, retry_delay_millis: 5 },
    source_update: {
      source: source.binding,
      reporter: writer.binding,
      connection: null,
      effects: {
        status: 'review-next',
        add: ['selected-label'],
        remove: ['retired-label'],
        state: null,
      },
    },
  };
  const createFlow = vi.fn();
  const api = { ...factoryApi, validateFlow: vi.fn(async () => config.flow), createFlow };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider
        router={createMemoryRouter([
          {
            path: '*',
            element: <FactoryPublication api={api} subject={{ kind: 'flow', request }} />,
          },
        ])}
      />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Review publication' }));
  const dialog = await screen.findByRole('dialog', { name: 'Publish Flow Definition?' });
  expect(dialog.textContent).toContain('selected-label');
  expect(dialog.textContent).toContain('retired-label');
  expect(dialog.textContent).toContain(writer.binding.configuration.digest);
  expect(createFlow).not.toHaveBeenCalled();
});
