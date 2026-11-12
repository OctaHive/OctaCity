// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, it, expect, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { FactoryAuthoring } from './FactoryAuthoring';
import { factoryApi, type FactoryApi } from './api';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { projectDocumentsApi } from '../../api/projectDocuments';
afterEach(cleanup);

it('blocks Flow publication while process text is being edited and restores the unchanged draft after cancellation', async () => {
  const original = configurationFixture().configuration.flow;
  const acceptedPredecessor = {
    definition: { ...original.closure.root, digest: '8'.repeat(64) },
    terminal: 'triaged',
    outcomes: ['eligible'],
  };
  const flow = { ...original, accepted_predecessor: acceptedPredecessor };
  const projectId = configurationFixture().configuration.reference.project_id;
  vi.spyOn(projectDocumentsApi, 'list').mockResolvedValue({ items: [], next_cursor: null });
  const api = {
    ...factoryApi,
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    validateFlow: vi.fn<FactoryApi['validateFlow']>(async () => flow),
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider
        router={createMemoryRouter([
          {
            path: '*',
            element: (
              <FactoryAuthoring
                api={api}
                baseline={{ flow, project_id: projectId, published_at_unix_ms: 1 }}
                onDiscard={() => {}}
              />
            ),
          },
        ])}
      />
    </QueryClientProvider>,
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Validate draft' }));
  expect(await screen.findByText('Authoritative validation passed.')).toBeTruthy();
  expect(api.validateFlow.mock.calls[0]?.[0].definition).toHaveProperty(
    'accepted_predecessor',
    acceptedPredecessor,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Edit completion criteria' }));
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Completion criteria' }),
    'Unpublished private draft',
  );
  expect(screen.getByRole('button', { name: 'Validate draft' })).toHaveProperty('disabled', true);
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Cancel process edit' }));
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    false,
  );
  expect(screen.queryByDisplayValue('Unpublished private draft')).toBeNull();
  expect(api.validateFlow).toHaveBeenCalledTimes(1);
  vi.restoreAllMocks();
});
it('authors and confirms a first Flow through guided controls, generating identities internally and invalidating only owning caches', async () => {
  const configuration = configurationFixture().configuration;
  const schema = configuration.flow.data_schemas[0]!;
  const projectId = configuration.reference.project_id;
  const createFlow = vi.fn<FactoryApi['createFlow']>(async (body) => ({
    disposition: 'applied',
    resource: {
      flow: {
        closure: {
          root: { id: body.id, version: 1, digest: '5'.repeat(64) },
          definitions: [
            {
              ...body.definition.root,
              reference: { id: body.id, version: 1, digest: '5'.repeat(64) },
            },
          ],
        },
        data_schemas: body.definition.data_schemas,
        limits: body.definition.limits,
      },
      project_id: projectId,
      published_at_unix_ms: 1,
    },
  }));
  const api = {
    ...factoryApi,
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async () => schema),
    validateFlow: vi.fn<FactoryApi['validateFlow']>(async (body) => ({
      closure: {
        root: { id: body.id, version: 1, digest: '5'.repeat(64) },
        definitions: [
          {
            ...body.definition.root,
            reference: { id: body.id, version: 1, digest: '5'.repeat(64) },
          },
        ],
      },
      data_schemas: body.definition.data_schemas,
      limits: body.definition.limits,
    })),
    createFlow,
  };
  const queryClient = createConsoleQueryClient();
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <FactoryAuthoring api={api} projectId={projectId} kind="flow" onDiscard={() => {}} />
      ),
    },
  ]);
  render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(screen.queryByRole('group', { name: 'Data schemas' })).toBeNull();
  await userEvent.click(await screen.findByRole('button', { name: 'Add step' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Step name' }), 'operator_decision');
  await userEvent.click(screen.getByRole('button', { name: 'Add result' }));
  expect(screen.getByRole('button', { name: 'Save result' })).toHaveProperty('disabled', false);
  expect(screen.queryByRole('textbox', { name: 'Schema name' })).toBeNull();
  expect(screen.queryByRole('textbox', { name: 'Schema version' })).toBeNull();
  expect(screen.getByRole('button', { name: 'Validate draft' })).toHaveProperty('disabled', true);
  await userEvent.click(screen.getByRole('button', { name: 'Save result' }));
  expect(api.normalizeSchema.mock.calls[0]?.[0]).toMatchObject({
    project_id: projectId,
    shape: { type: 'string', min_bytes: 0, max_bytes: 4096 },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Flow settings' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add terminal' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Terminal name' }), 'finished');
  await userEvent.click(screen.getByRole('combobox', { name: 'Completion result' }));
  await userEvent.click(screen.getByRole('option', { name: 'operator_decision.result' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add transition' }));
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  expect(await screen.findByText('Authoritative validation passed.')).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  expect(await screen.findByText('Immutable version published.')).toBeTruthy();
  expect(createFlow).toHaveBeenCalledTimes(1);
  const body = createFlow.mock.calls[0]?.[0];
  expect(body?.project_id).toBe(projectId);
  expect(body?.id).toMatch(/^[a-f0-9-]{36}$/);
  expect(body?.definition.root.entry).toBe('operator_decision');
  expect(invalidate.mock.calls.map(([target]) => target?.queryKey)).toContainEqual([
    'factory',
    'flows',
    projectId,
  ]);
  expect(invalidate.mock.calls.map(([target]) => target?.queryKey)).not.toContainEqual([
    'projects',
  ]);
  expect(router.state.location.search).toBe('');
});

it('retains concurrent graph edits after schema normalization', async () => {
  const configuration = configurationFixture().configuration;
  const flow = configuration.flow;
  let resolveSchema!: (schema: (typeof flow.data_schemas)[number]) => void;
  const normalization = new Promise<(typeof flow.data_schemas)[number]>((resolve) => {
    resolveSchema = resolve;
  });
  const api: FactoryApi = {
    ...factoryApi,
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    normalizeSchema: vi.fn(async () => normalization),
  };
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <FactoryAuthoring
          api={api}
          baseline={{
            flow,
            project_id: configuration.reference.project_id,
            published_at_unix_ms: 1,
          }}
          onDiscard={() => {}}
        />
      ),
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await screen.findByRole('button', { name: 'Add step' });
  await userEvent.click(screen.getByText('Advanced data contracts'));
  await userEvent.type(
    await screen.findByRole('textbox', { name: 'Schema name' }),
    'concurrent_schema',
  );
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema version' }), 'v1');
  await userEvent.click(screen.getByRole('button', { name: 'Add schema' }));
  const name = screen.getByRole('textbox', { name: 'Step name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'edited_while_pending');
  expect(screen.getByRole('textbox', { name: 'Step name' })).toHaveProperty(
    'value',
    'edited_while_pending',
  );
  await act(async () => resolveSchema(flow.data_schemas[0]!));
  expect(screen.getByRole('textbox', { name: 'Step name' })).toHaveProperty(
    'value',
    'edited_while_pending',
  );
});

it.each([true, false])('publishes an example (reproduction required=%s)', async (required) => {
  const { configuration } = configurationFixture();
  const projectId = configuration.reference.project_id;
  const original = configuration.flow;
  const { reference: _reference, ...root } = original.closure.definitions[0]!;
  void _reference;
  const preset = { identity: 'ticket-delivery', version: 'v1', digest: '3'.repeat(64) };
  const report = original.data_schemas[0]!;
  const command = {
    alias: 'Reviewed reproduction',
    execution: {
      build_configuration: {
        project_id: projectId,
        id: 'build',
        version: 1,
        definition_digest: '4'.repeat(64),
      },
      tool: preset,
      plugin: preset,
      model_or_tool: preset,
      task_digest: `aaaaaaaa${'5'.repeat(56)}`,
      result_output: 'reproduction.json',
      result_output_kind: 'report' as const,
    },
    output_schema: report.reference,
    output_definition: report,
  };
  const example = {
    root: { ...root, name: 'Ticket delivery' },
    limits: original.limits,
    data_schemas: original.data_schemas,
  };
  const alternateCommand = {
    ...command,
    execution: {
      ...command.execution,
      task_digest: 'a'.repeat(64),
    },
  };
  const legacyExample = {
    reference: preset,
    profile_aliases: [],
    required_node_kinds: ['build_command' as const],
    parameters: [{ path: '/commands/reproduction', kind: 'command' as const, required }],
    editor: {
      name: 'Ticket delivery',
      budget: original.limits.budget,
      max_active_nodes: 1,
      steps: [
        {
          alias: 'reproduction',
          kind: 'build_command' as const,
          outcome: 'observed',
          result: null,
        },
      ],
    },
  };
  const listPresets = vi.fn(async () => ({
    items: [
      legacyExample,
      {
        ...legacyExample,
        reference: { ...preset, version: 'v2' },
        parameters: [
          ...legacyExample.parameters,
          { path: '/issue_snapshot', kind: 'issue_snapshot' as const, required: true },
        ],
      },
    ],
    total: 2,
    next_cursor: null,
  }));
  const resolvePreset = vi.fn<FactoryApi['resolvePreset']>(async () => ({
    preset,
    diagnostics: [],
    flow: example,
    configuration: null,
    input: null,
    dependencies: [],
    completion: {
      kind: 'draft',
      project_id: projectId,
      preset,
      content: 'Retain the reviewed result.',
    },
  }));
  const published = {
    ...original,
    closure: {
      ...original.closure,
      definitions: [{ ...original.closure.definitions[0]!, name: 'Ticket delivery' }],
    },
  };
  const validateFlow = vi.fn<FactoryApi['validateFlow']>(async () => published);
  const createFlow = vi.fn<FactoryApi['createFlow']>(async () => ({
    disposition: 'applied',
    resource: { flow: published, project_id: projectId, published_at_unix_ms: 1 },
  }));
  const api = {
    ...factoryApi,
    listPresets,
    resolvePreset,
    validateFlow,
    createFlow,
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () => ({
      commands: required ? [command, alternateCommand] : [],
      candidate_publications: [],
      human_gates: [],
      build_resource_modes: [],
      source_free_incompatibility: null,
      deterministic_gate: {
        policy: preset,
        parameter_contract: 'declarative_gate_v1',
        max_parameter_bytes: 64,
        resource_modes: [],
      },
    })),
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider
        router={createMemoryRouter([
          {
            path: '*',
            element: (
              <FactoryAuthoring api={api} projectId={projectId} kind="flow" onDiscard={() => {}} />
            ),
          },
        ])}
      />
    </QueryClientProvider>,
  );
  expect(listPresets).not.toHaveBeenCalled();
  await userEvent.click(await screen.findByText('Start from an example'));
  await userEvent.click(await screen.findByRole('button', { name: 'Ticket delivery · v2' }));
  expect(screen.getByRole('button', { name: 'Open example draft' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(
    screen.getByText(
      'The required input settings for this version are not supported here. Choose another version.',
    ),
  ).toBeTruthy();
  expect(resolvePreset).not.toHaveBeenCalled();
  await userEvent.click(await screen.findByRole('button', { name: 'Ticket delivery · v1' }));
  expect(screen.getByRole('button', { name: 'Open example draft' })).toHaveProperty(
    'disabled',
    required,
  );
  if (!required) {
    expect(screen.getByRole('group', { name: 'Reproduction' }).textContent).toContain(
      'Optional. Without a command, tickets that need this check are sent for review.',
    );
  }
  if (required) {
    await userEvent.click(await screen.findByRole('combobox', { name: 'Reproduction' }));
    const options = screen.getAllByRole('option', { name: /Reviewed reproduction/ });
    expect(new Set(options.map((option) => option.textContent)).size).toBe(2);
    for (const option of options) {
      expect(option.textContent).toContain('Reviewed reproduction');
      expect(option.textContent).not.toMatch(/[0-9a-f]{20}/);
    }
    await userEvent.click(options[1]!);
  }
  await userEvent.click(screen.getByRole('button', { name: 'Open example draft' }));
  const name = await screen.findByDisplayValue('Ticket delivery');
  await userEvent.clear(name);
  await userEvent.type(name, 'Reviewed delivery');
  expect(resolvePreset).toHaveBeenCalledWith(
    projectId,
    expect.objectContaining({
      preset,
      flow_only: true,
      flow_id: expect.any(String),
      commands: required
        ? {
            reproduction: {
              build: { ...alternateCommand.execution, result_outcome: 'observed' },
              result: report,
              permissions: expect.any(Object),
            },
          }
        : {},
    }),
    expect.any(AbortSignal),
  );
  expect(createFlow).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await screen.findByText('Authoritative validation passed.');
  expect(validateFlow.mock.calls[0]?.[0].definition.root.name).toBe('Reviewed delivery');
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  expect(createFlow).not.toHaveBeenCalled();
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  expect(await screen.findByText('Immutable version published.')).toBeTruthy();
  expect(createFlow.mock.calls[0]?.[0].definition.root.name).toBe('Reviewed delivery');
});
