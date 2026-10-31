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
import { FlowGraphEditor } from './FlowGraphEditor';
import { FactoryPublication } from './FactoryPublication';
import { configurationFixture } from '../../../tests/fixtures/factory';
afterEach(cleanup);
it.each([
  ['budget_exceeds_ceiling', 'Budget exceeds its enclosing limit'],
  ['concurrency_exceeds_ceiling', 'Concurrency exceeds its enclosing limit'],
] as const)(
  'shows the authoritative %s blocker without permitting publication',
  async (code, label) => {
    const { request } = setup();
    const api = {
      ...factoryApi,
      validateFlow: vi.fn(async () => {
        throw new ManagementApiError({
          code: 'invalid_request',
          message: 'Limit rejected',
          requestId: null,
          retryAfterMilliseconds: null,
          status: 400,
          validationIssues: [{ field: '/execution/budget', code }],
        });
      }),
    };
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
    expect(await screen.findByText(label, { exact: false })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
      'disabled',
      true,
    );
  },
);
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
  expect(dialog.textContent).toContain(
    'Source effects are delivered independently. Queuing does not confirm delivery; other effects may remain pending or fail.',
  );
  expect(dialog.textContent).toContain('selected-label');
  expect(dialog.textContent).toContain('retired-label');
  expect(dialog.textContent).toContain(writer.binding.configuration.digest);
  expect(createFlow).not.toHaveBeenCalled();
});

it('reviews local Issue pins separately and requires fresh review after a stale Flow publication', async () => {
  const { config, request } = setup();
  const node = request.definition.root.nodes.find((node) => node.action)!;
  const parameters = {
    workflow_id: '20000000-0000-4000-8000-000000000001',
    workflow_version: 3,
    transition_id: '30000000-0000-4000-8000-000000000001',
    outcome: node.outcomes[0]!.key,
  };
  node.action = { ...node.action!, action: 'transition_issue', parameters };
  config.flow.closure.definitions[0]!.nodes = request.definition.root.nodes;
  const replaceFlow = vi.fn<FactoryApi['replaceFlow']>().mockRejectedValue(
    new ManagementApiError({
      code: 'precondition_failed',
      message: 'Stale',
      requestId: null,
      retryAfterMilliseconds: null,
      status: 412,
    }),
  );
  const api = {
    ...factoryApi,
    validateFlow: vi.fn(async () => config.flow),
    replaceFlow,
    currentFlow: vi.fn(async () => ({
      flow: {
        ...config.flow,
        closure: { ...config.flow.closure, root: { ...config.flow.closure.root, version: 2 } },
      },
      project_id: request.project_id,
      published_at_unix_ms: 1,
    })),
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
  const dialog = await screen.findByRole('dialog');
  expect(within(dialog).getByText(new RegExp(parameters.transition_id))).toBeTruthy();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publish' }));
  expect(replaceFlow.mock.calls[0]?.[2]['If-Match']).toBe('"1"');
  expect(within(dialog).queryByRole('button', { name: 'Retry same command' })).toBeNull();
  expect(await within(dialog).findByRole('button', { name: /Refresh/ })).toBeTruthy();
});

it('validates a role-backed reasoning stage through the server and reviews exact candidates and failure policy before publication', async () => {
  const { factoryProfileConfigurationFixture } =
    await import('../../../tests/fixtures/factoryRoles');
  const config = factoryProfileConfigurationFixture().configuration;
  const { reference, ...root } = config.flow.closure.definitions[0]!;
  const request = {
    id: reference.id,
    project_id: config.reference.project_id,
    definition: { root, limits: config.flow.limits, data_schemas: config.flow.data_schemas },
  };
  const validateFlow = vi
    .fn<FactoryApi['validateFlow']>()
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'invalid_request',
        message: 'Incompatible selection',
        requestId: null,
        status: 400,
        retryAfterMilliseconds: null,
      }),
    )
    .mockResolvedValue(config.flow);
  const createFlow = vi.fn<FactoryApi['createFlow']>();
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <FactoryPublication
          api={{ ...factoryApi, validateFlow, createFlow }}
          subject={{ kind: 'flow', request }}
        />
      ),
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  const validate = screen.getByRole('button', { name: 'Validate draft' });
  expect(validate).toHaveProperty('disabled', false);
  await userEvent.click(validate);
  await screen.findByText('Incompatible selection');
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(validate);
  await userEvent.click(await screen.findByRole('button', { name: 'Review publication' }));
  const dialog = await screen.findByRole('dialog');
  const candidate = root.nodes[0]!.profile_selection!.candidates.default!.profile;
  expect(dialog.textContent).toContain(candidate.reference.id);
  expect(dialog.textContent).toContain('manual_selection');
  expect(createFlow).not.toHaveBeenCalled();
});

it('shows server-reported requested and effective budget scopes without inventing unresolved concurrency', async () => {
  const { config, request } = setup();
  const budget = {
    max_attempts: 10,
    max_elapsed_millis: 120000,
    max_tokens: 50000,
    max_cost_micro_units: 3000000,
    max_output_bytes: 4096,
  };
  const api = {
    ...factoryApi,
    validateFlow: vi.fn<FactoryApi['validateFlow']>(async () => ({
      ...config.flow,
      diagnostics: {
        errors: [],
        effective_permissions: [],
        required_node_capabilities: [],
        required_agent_capabilities: [],
        required_backend_capabilities: [],
        unavailable_references: [],
        warnings: [],
        constraints: {
          run_budget: {
            definition: null,
            node: null,
            profile: null,
            scope: 'run_total',
            requested: budget,
            effective: { ...budget, max_tokens: 12000 },
            retry_charging: 'cumulative_in_run',
            sources: [
              {
                source: 'configuration',
                scope: 'run_total',
                limit: { ...budget, max_tokens: 12000 },
              },
            ],
          },
          node_budgets: [],
          concurrency: [
            {
              definition: null,
              scope: 'configuration_runs',
              source: 'configuration_wip',
              requested: null,
              effective: null,
              unresolved: 'configuration_required',
            },
          ],
        },
      },
    })),
  };
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
  const review = within(await screen.findByRole('region', { name: 'Effective constraints' }));
  expect(review.getByRole('columnheader', { name: 'Requested' })).toBeTruthy();
  expect(review.getByRole('columnheader', { name: 'Effective' })).toBeTruthy();
  expect(review.getByText('50,000')).toBeTruthy();
  expect(review.getByText('12,000')).toBeTruthy();
  expect(review.getAllByText(/^Configuration run budget ·/)).toHaveLength(5);
  expect(review.getByText('Choose a Configuration to resolve this limit.')).toBeTruthy();
  expect(
    review.getByText('Attempts and retries are charged cumulatively to this Run.'),
  ).toBeTruthy();
});

it('names the rejected resource and ceiling while keeping technical field paths under Details', async () => {
  const { request } = setup();
  const api = {
    ...factoryApi,
    validateFlow: vi.fn<FactoryApi['validateFlow']>(async () => {
      throw new ManagementApiError({
        code: 'invalid_request',
        message: 'Limit rejected',
        requestId: null,
        status: 400,
        retryAfterMilliseconds: null,
        validationIssues: [
          {
            field: '/execution/budget',
            code: 'budget_exceeds_ceiling',
            resource: 'tokens',
            ceiling: 'flow_definition',
            requested: 150000,
            maximum: 100000,
          },
        ],
      });
    }),
  };
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
  expect(
    await screen.findByText(
      'Tokens: requested 150,000; maximum 100,000. Limit: Flow execution policy.',
    ),
  ).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Execution limits' })).toBeTruthy();
  expect(screen.queryByRole('button', { name: '/execution/budget' })).toBeNull();
  expect(screen.getByText('/execution/budget', { exact: true }).closest('details')).toHaveProperty(
    'open',
    false,
  );
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    true,
  );
});

it.each([
  ['/limits/budget/max_tokens', 'Admission budget · Maximum tokens'],
  ['/execution/budget/max_tokens', 'Run budget · Maximum tokens'],
])(
  'opens the rejected budget control %s from a visible validation blocker',
  async (field, label) => {
    const { request } = setup();
    const api = {
      ...factoryApi,
      validateFlow: vi.fn<FactoryApi['validateFlow']>(async () => {
        throw new ManagementApiError({
          code: 'invalid_request',
          message: 'Rejected',
          requestId: null,
          status: 400,
          retryAfterMilliseconds: null,
          validationIssues: [{ field, code: 'budget_exceeds_ceiling' }],
        });
      }),
    };
    function Harness() {
      const [value, setValue] = useState(request.definition);
      return (
        <>
          <FlowGraphEditor value={value} choices={[]} subflows={[]} onChange={setValue} />
          <FactoryPublication
            api={api}
            subject={{ kind: 'flow', request: { ...request, definition: value } }}
          />
        </>
      );
    }
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <RouterProvider router={createMemoryRouter([{ path: '*', element: <Harness /> }])} />
      </QueryClientProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
    await screen.findByText('Rejected');
    await userEvent.click(screen.getByRole('button', { name: 'Execution limits' }));
    const input = screen.getByRole('spinbutton', { name: label });
    if (field.startsWith('/limits')) expect(input.closest('details')).toHaveProperty('open', true);
    expect(document.activeElement).toBe(input);
  },
);
