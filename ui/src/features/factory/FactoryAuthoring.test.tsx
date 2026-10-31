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
  const flow = configurationFixture().configuration.flow;
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
    normalizeSchema: vi.fn(async () => schema),
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
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Schema name' }),
    schema.reference.identity,
  );
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Schema version' }),
    schema.reference.version,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Add schema' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add step' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Step name' }), 'operator_decision');
  await userEvent.click(screen.getByRole('button', { name: 'Add outcome' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Outcome name' }), 'accepted');
  await userEvent.click(screen.getByRole('button', { name: 'Flow settings' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add terminal' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Terminal name' }), 'finished');
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
  await userEvent.type(screen.getByRole('textbox', { name: 'Schema name' }), 'concurrent_schema');
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
