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
it.each(['rejection', 'diagnostics'] as const)(
  'does not present %s for an older draft as the current validation result',
  async (response) => {
    const { config, request } = setup();
    const api = {
      ...factoryApi,
      validateFlow: vi.fn<FactoryApi['validateFlow']>(async () => {
        if (response === 'diagnostics')
          return {
            ...config.flow,
            diagnostics: {
              errors: [{ field: '/root/nodes/0', code: 'invalid_contract' }],
              effective_permissions: [],
              required_node_capabilities: [],
              required_agent_capabilities: [],
              required_backend_capabilities: [],
              unavailable_references: [],
              warnings: [],
            },
          };
        throw new ManagementApiError({
          code: 'invalid_request',
          message: 'Old rejected draft',
          requestId: null,
          status: 400,
          retryAfterMilliseconds: null,
          validationIssues: [{ field: '/root/nodes/0', code: 'invalid_contract' }],
        });
      }),
    };
    function Harness() {
      const [current, setCurrent] = useState(request);
      return (
        <>
          <button
            onClick={() =>
              setCurrent({
                ...current,
                definition: {
                  ...current.definition,
                  root: {
                    ...current.definition.root,
                    nodes: current.definition.root.nodes.map((node) => ({
                      ...node,
                      required: !node.required,
                    })),
                  },
                },
              })
            }
          >
            Change draft
          </button>
          <FactoryPublication api={api} subject={{ kind: 'flow', request: current }} />
        </>
      );
    }
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <RouterProvider router={createMemoryRouter([{ path: '*', element: <Harness /> }])} />
      </QueryClientProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
    await screen.findByText(/Invalid contract/);
    expect(screen.queryByText('Authoritative validation passed.')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Change draft' }));
    expect(screen.queryByText('Old rejected draft')).toBeNull();
    expect(screen.queryByText(/Invalid contract/)).toBeNull();
    expect(api.validateFlow).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
      'disabled',
      true,
    );
  },
);
it('explains the exact locally exceeded budget with requested value and enclosing limit', () => {
  const { request } = setup();
  request.definition.root.execution.budget.max_tokens = 100000;
  request.definition.limits.budget.max_tokens = 300000;
  request.definition.root.nodes[0]!.budget.max_tokens = 150000;
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FactoryPublication api={factoryApi} subject={{ kind: 'flow', request }} />
    </QueryClientProvider>,
  );
  expect(
    screen.getByText('Tokens: requested 150,000; maximum 100,000. Limit: Flow execution policy.'),
  ).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Step 1 · Maximum tokens' })).toBeTruthy();
});
it.each([
  [
    'launch_policy_denied',
    'The selected launch policy does not allow this way of starting the pipeline. Choose an appropriate policy in the Configuration.',
  ],
  ['budget_exceeds_ceiling', 'Budget exceeds its enclosing limit'],
  ['concurrency_exceeds_ceiling', 'Concurrency exceeds its enclosing limit'],
  [
    'unmapped_legacy_policy',
    'Select a Project launch policy and access profile; the saved installation reference has no verified mapping.',
  ],
  [
    'permission_exceeds_ceiling',
    'The selected access profile exceeds current installation or runtime limits. Narrow the profile or restore the required installed capability.',
  ],
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
it.each([
  ['/root/input_schema', 'Input schema'],
  ['/context_binding/work_task', 'Task from admitted Work'],
  ['/gate/parameters', 'Condition check'],
])(
  'associates authoritative field %s with the draft and retains unrelated edits without allowing publication',
  async (field, label) => {
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
          validationIssues: [{ field, code: 'unavailable_reference' }],
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
    expect(await screen.findByText(field)).toBeTruthy();
    expect(screen.getByRole('button', { name: label })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
      'disabled',
      true,
    );
    expect(request.definition.root.nodes[0]?.budget.max_tokens).toBe(321);
  },
);

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

it('reviews each exact candidate profile ceiling alongside step access and unresolved runtime limits', async () => {
  const { config, request } = setup();
  const permissions = {
    ...config.flow.limits.permissions,
    plugins: [],
    tools: [],
    network_hosts: ['api.allowed.example'],
    resources: {
      cpu_millis: 1000,
      elapsed_millis: 10000,
      memory_bytes: 4096,
      disk_bytes: 8192,
      process_count: 4,
    },
  };
  const profile = {
    project_id: request.project_id,
    id: '77777777-7777-4777-8777-777777777777',
    version: 2,
    digest: '7'.repeat(64),
  };
  const api = {
    ...factoryApi,
    validateFlow: vi.fn<FactoryApi['validateFlow']>(async () => ({
      ...config.flow,
      diagnostics: {
        errors: [],
        unavailable_references: [],
        required_node_capabilities: [],
        required_agent_capabilities: [],
        required_backend_capabilities: [],
        warnings: [],
        effective_permissions: [
          {
            definition: config.flow.closure.root,
            node: 'operator_step',
            permissions,
            sources: ['flow_admission', 'flow_definition', 'node'],
            profiles: [
              {
                profile,
                permissions: {
                  ...permissions,
                  network_hosts: [],
                  resources: { ...permissions.resources, elapsed_millis: 0 },
                },
                sources: ['role_profile', 'profile_budget'],
              },
              {
                profile: { ...profile, version: 3, digest: '8'.repeat(64) },
                permissions: {
                  ...permissions,
                  network_hosts: ['one.example', 'two.example'],
                  resources: { ...permissions.resources, elapsed_millis: 2000 },
                },
                sources: ['role_profile', 'profile_budget'],
              },
            ],
            unresolved_sources: ['agent_local', 'backend_placement'],
          },
        ],
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
  expect(review.getByText('Plugins: 0; tools: 0; network hosts: 1')).toBeTruthy();
  const first = within(review.getByRole('group', { name: 'Profile candidate 1 · v2' }));
  expect(first.getByText('Plugins: 0; tools: 0; network hosts: 0')).toBeTruthy();
  expect(first.getByText('Elapsed time (ms)')).toBeTruthy();
  expect(first.getByText('0', { selector: 'dd' })).toBeTruthy();
  expect(first.getByText('Constraints from: Role profile, Profile budget')).toBeTruthy();
  const second = within(review.getByRole('group', { name: 'Profile candidate 2 · v3' }));
  expect(second.getByText('Plugins: 0; tools: 0; network hosts: 2')).toBeTruthy();
  expect(second.getByText('2,000', { selector: 'dd' })).toBeTruthy();
  expect(
    review.getByText(
      'Each candidate has its own ceiling. Flow and step limits also apply; the profile is selected at execution.',
    ),
  ).toBeTruthy();
  expect(review.getByRole('alert').textContent).toBe(
    'Access still requires enforcement by: Agent limits, Execution backend.',
  );
  await userEvent.click(first.getByText('Details', { selector: 'summary' }));
  expect(first.getByText(profile.id)).toBeTruthy();
  expect(first.getByText(profile.digest)).toBeTruthy();
  await userEvent.click(second.getByText('Details', { selector: 'summary' }));
  expect(second.getByText('8'.repeat(64))).toBeTruthy();
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
    await userEvent.click(screen.getByRole('button', { name: 'Maximum tokens' }));
    const input = screen.getByRole('spinbutton', { name: label });
    if (field.startsWith('/limits')) expect(input.closest('details')).toHaveProperty('open', true);
    expect(document.activeElement).toBe(input);
  },
);

it('reviews the normalized Issue card slots and only the fields that will be shown', async () => {
  const { config, request } = setup();
  const schema = config.flow.data_schemas[0]!;
  schema.shape = {
    type: 'object',
    fields: {
      conclusion: { required: true, value: { type: 'string', min_bytes: 1, max_bytes: 200 } },
      decision: { required: true, value: { type: 'enum', values: ['eligible'] } },
      priority: { required: false, value: { type: 'integer', minimum: 0, maximum: 4 } },
      private_notes: { required: true, value: { type: 'string', min_bytes: 1, max_bytes: 200 } },
    },
  };
  const root = config.flow.closure.definitions[0]!;
  root.nodes[0]!.outcomes[0] = {
    ...root.nodes[0]!.outcomes[0]!,
    schema: schema.reference,
    issue_presentation: { kind: 'triage_v1' },
  };
  root.nodes[0]!.outcomes[1] = {
    ...root.nodes[0]!.outcomes[1]!,
    schema: schema.reference,
    issue_presentation: {
      kind: 'custom_v1',
      title: 'Project assessment',
      conclusion_field: 'conclusion',
    },
  };
  const api = { ...factoryApi, validateFlow: vi.fn(async () => config.flow) };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FactoryPublication api={api} subject={{ kind: 'flow', request }} />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  const dialog = within(await screen.findByRole('dialog'));
  const triage = within(dialog.getByRole('article', { name: 'Triage' }));
  expect(triage.getByText('Card slot: triage')).toBeTruthy();
  expect(triage.getByText('Conclusion')).toBeTruthy();
  expect(triage.getByText('Decision')).toBeTruthy();
  expect(triage.getByText('Recommended priority')).toBeTruthy();
  expect(triage.queryByText('Size')).toBeNull();
  const custom = within(dialog.getByRole('article', { name: 'Project assessment' }));
  const outcome = root.nodes[0]!.outcomes[1]!;
  expect(
    custom.getByText(
      `Automatic slot · Flow ${root.reference.id} · Step ${root.nodes[0]!.key} · Outcome ${outcome.key}`,
    ),
  ).toBeTruthy();
  expect(custom.getByText('conclusion', { exact: true })).toBeTruthy();
  expect(dialog.queryByText('private_notes', { exact: false })).toBeNull();
});

it('reviews active Project label names for the exact classification before publication', async () => {
  const { config, request } = setup();
  const node = request.definition.root.nodes.find((node) => node.action)!;
  const plugin = node.action!.plugin;
  for (const permissions of [
    node.permissions,
    request.definition.root.execution.permissions,
    request.definition.limits.permissions,
  ]) {
    if (!permissions.plugins.some(({ digest }) => digest === plugin.digest))
      permissions.plugins.push(plugin);
  }
  node.action = {
    ...node.action!,
    action: 'apply_issue_classification',
    parameters: {
      source_node: 'triage',
      source_outcome: 'accepted',
      outcome: node.outcomes[0]!.key,
      policy: 'team-triage',
      size_labels: { small: 'team-small' },
    },
  };
  config.flow.closure.definitions[0]!.nodes = request.definition.root.nodes;
  const api = {
    ...factoryApi,
    validateFlow: vi.fn(async () => config.flow),
    getAuthoringCapabilities: vi.fn(async () => ({
      commands: [],
      human_gates: [],
      build_resource_modes: ['none' as const],
      source_free_incompatibility: null,
      deterministic_gate: {
        policy: plugin,
        parameter_contract: 'declarative_gate_v1' as const,
        max_parameter_bytes: 65536,
        resource_modes: ['none' as const],
      },
      issue_classification: {
        plugin,
        receipt_schema: config.flow.data_schemas[0]!,
        receipt_output_bytes: 317,
        policy: 'team-triage',
        priority_maximum: 100,
        size_labels: [{ size: 'small', label: 'team-small', name: 'Small work' }],
      },
    })),
    createFlow: vi.fn<FactoryApi['createFlow']>(),
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
  await userEvent.click(await screen.findByRole('button', { name: 'Review publication' }));
  const dialog = within(await screen.findByRole('dialog'));
  expect(await dialog.findByText('Small work')).toBeTruthy();
  expect(dialog.getByText('Priority · 0–100')).toBeTruthy();
  expect(api.createFlow).not.toHaveBeenCalled();
});
