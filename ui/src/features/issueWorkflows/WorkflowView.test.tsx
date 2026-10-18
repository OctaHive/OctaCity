// @vitest-environment jsdom
import { readFileSync } from 'node:fs';
import { cleanup, render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { App } from '../../App';
import type { IssueWorkflowsApi, WorkflowResource } from './api';
import { createConsoleQueryClient } from '../../app/query';
import { createConsoleRoutes } from '../../app/router';
import { fakeProjectsApi, project } from '../projects/ProjectTestSupport';

type Definition = components['schemas']['IssueWorkflowDefinition'];
const definition: Definition = JSON.parse(
  readFileSync(
    '../server/api/octacity-server-api-rest/fixtures/v1/issue-workflow-definition.json',
    'utf8',
  ),
);
const resource = {
  id: '20000000-0000-4000-8000-000000000001',
  project_id: 'delivery',
  version: 3,
  definition,
  published_at_unix_ms: 1700000000000,
};
function setup({
  defaultError,
  listedWorkflow = resource,
}: {
  defaultError?: Error;
  listedWorkflow?: WorkflowResource;
} = {}) {
  const projects = fakeProjectsApi();
  projects.getProject.mockResolvedValue({
    ancestors: [],
    project: project('delivery', 'Delivery'),
  });
  const api = {
    list: vi.fn<IssueWorkflowsApi['list']>().mockResolvedValue({
      items: [{ ...listedWorkflow, name: listedWorkflow.definition.name }],
      next_cursor: null,
    }),
    get: vi.fn<IssueWorkflowsApi['get']>().mockResolvedValue(listedWorkflow),
    getDefault: vi
      .fn<IssueWorkflowsApi['getDefault']>()
      .mockResolvedValue({ version: 7, workflow: resource }),
    validate: vi
      .fn<IssueWorkflowsApi['validate']>()
      .mockImplementation(async (_project: string, draft: Definition) => draft),
    create: vi
      .fn<IssueWorkflowsApi['create']>()
      .mockResolvedValue({ disposition: 'applied', resource: { ...resource, version: 1 } }),
    publish: vi
      .fn<IssueWorkflowsApi['publish']>()
      .mockResolvedValue({ disposition: 'applied', resource: { ...resource, version: 4 } }),
    selectDefault: vi
      .fn<IssueWorkflowsApi['selectDefault']>()
      .mockResolvedValue({ disposition: 'applied', resource: { version: 8, workflow: resource } }),
  };
  if (defaultError) api.getDefault.mockRejectedValue(defaultError);
  const queryClient = createConsoleQueryClient();
  const router = createMemoryRouter(
    createConsoleRoutes({
      projectsApi: projects,
      issueWorkflowsApi: api,
      readinessProbe: async () => 'ready',
    }),
    { initialEntries: ['/projects/delivery/issue-workflows'] },
  );
  render(<App router={router} queryClient={queryClient} />);
  return { api, queryClient };
}
afterEach(() => {
  cleanup();
  localStorage.clear();
});

it('edits ordered ID/name-only statuses and workflow-level selections before reviewed immutable publication', async () => {
  const { api } = setup();
  await screen.findByRole('heading', { name: 'Task workflow', level: 1 });
  await userEvent.click(await screen.findByRole('button', { name: 'Edit default workflow' }));
  const first = screen.getByRole('textbox', { name: 'Status name 1' });
  await userEvent.clear(first);
  await userEvent.type(first, 'Inbox');
  await userEvent.click(screen.getByRole('button', { name: 'Move status 2 up' }));
  expect((screen.getByRole('textbox', { name: 'Status name 2' }) as HTMLInputElement).value).toBe(
    'Inbox',
  );
  expect(screen.getByRole('combobox', { name: 'Initial status' }).textContent).toContain('Inbox');
  const terminals = screen.getByRole('group', { name: 'Terminal statuses' });
  await userEvent.click(within(terminals).getByRole('checkbox', { name: 'Declined' }));
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  expect(api.validate).toHaveBeenCalledWith(
    'delivery',
    expect.objectContaining({
      initial_status_id: definition.initial_status_id,
      terminal_status_ids: [definition.statuses[1]!.id],
      statuses: [
        { ...definition.statuses[1] },
        { ...definition.statuses[0], name: 'Inbox' },
        definition.statuses[2],
      ],
    }),
  );
  const review = screen.getByRole('button', { name: 'Review publication' });
  await userEvent.click(review);
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain('Local workflow');
  expect(dialog.textContent).toContain('Based on version 3');
  expect(dialog.textContent).toContain('Done → Inbox → Declined');
  expect(dialog.textContent).toContain('Initial status: Inbox');
  expect(dialog.textContent).toContain('Terminal statuses: Done');
  expect(dialog.textContent).toContain('Existing tasks keep their pinned workflow');
  expect(api.publish).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish version' }));
  await waitFor(() =>
    expect(api.publish).toHaveBeenCalledWith(
      'delivery',
      resource.id,
      expect.objectContaining({ name: definition.name }),
      expect.objectContaining({ 'If-Match': '"3"', 'Idempotency-Key': expect.any(String) }),
    ),
  );
  expect(api.selectDefault).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
  expect(document.activeElement).toBe(review);
});

it('authors finite transitions and reviews origins and evidence separately from status rows', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'Edit default workflow' }));
  expect(
    (screen.getByRole('button', { name: 'Remove status 1' }) as HTMLButtonElement).disabled,
  ).toBe(true);
  await userEvent.click(screen.getByRole('button', { name: 'Add status' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Status name 4' }), 'In review');
  await userEvent.click(screen.getByRole('button', { name: 'Add transition' }));
  const edge = screen.getByRole('group', { name: 'Transition 1' });
  await userEvent.type(within(edge).getByRole('textbox', { name: 'Transition name' }), 'Accept');
  await userEvent.click(within(edge).getByRole('combobox', { name: 'To status' }));
  await userEvent.click(screen.getByRole('option', { name: 'In review' }));
  await userEvent.click(within(edge).getByRole('checkbox', { name: 'Pipeline action' }));
  await userEvent.click(within(edge).getByRole('checkbox', { name: 'Source synchronization' }));
  await userEvent.click(within(edge).getByRole('checkbox', { name: 'Accepted execution' }));
  await userEvent.click(within(edge).getByRole('checkbox', { name: 'Required checks passed' }));
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  const sent = api.validate.mock.calls[0]![1];
  expect(sent.statuses[3]).toEqual({ id: expect.any(String), name: 'In review' });
  expect(sent.transitions).toEqual([
    {
      id: expect.any(String),
      name: 'Accept',
      from: definition.initial_status_id,
      to: sent.statuses[3]!.id,
      origins: ['manual', 'pipeline', 'source'],
      evidence: ['accepted_execution', 'passed_required_checks'],
    },
  ]);
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  const review = screen.getByRole('dialog');
  expect(review.textContent).toContain('Accept: New → In review');
  expect(review.textContent).toContain('Manual move, Pipeline action, Source synchronization');
  expect(review.textContent).toContain('Accepted execution, Required checks passed');
  await userEvent.keyboard('{Escape}');
  expect(screen.queryByRole('dialog')).toBeNull();
  await userEvent.click(within(edge).getByRole('button', { name: 'Remove transition' }));
  await userEvent.click(screen.getByRole('button', { name: 'Remove status 4' }));
  expect(screen.queryByRole('textbox', { name: 'Status name 4' })).toBeNull();
  expect(
    (screen.getByRole('button', { name: 'Review publication' }) as HTMLButtonElement).disabled,
  ).toBe(true);
});

it('rejects a late validation receipt after editing and enforces the generated UTF-8 byte bound', async () => {
  const { api } = setup();
  let finish!: (value: Definition) => void;
  api.validate.mockReturnValueOnce(
    new Promise<Definition>((resolve) => {
      finish = resolve;
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Edit default workflow' }));
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  const name = screen.getByRole('textbox', { name: 'Workflow name' });
  await userEvent.type(name, ' changed');
  finish(definition);
  await waitFor(() =>
    expect(
      (screen.getByRole('button', { name: 'Validate draft' }) as HTMLButtonElement).disabled,
    ).toBe(false),
  );
  expect(
    (screen.getByRole('button', { name: 'Review publication' }) as HTMLButtonElement).disabled,
  ).toBe(true);
  await userEvent.clear(name);
  await userEvent.type(name, 'я'.repeat(65));
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  expect(await screen.findByRole('alert')).toHaveProperty(
    'textContent',
    expect.stringContaining('128 UTF-8 bytes'),
  );
  expect(api.validate).toHaveBeenCalledTimes(1);
});

it('refreshes a stale publication baseline, preserves the draft and requires validation and a new review', async () => {
  const { api } = setup();
  const { ManagementApiError } = await import('../../api/client');
  api.publish.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'conflict',
      message: 'Stale workflow.',
      requestId: null,
      status: 409,
      retryAfterMilliseconds: null,
    }),
  );
  api.get.mockResolvedValue({ ...resource, version: 5 });
  await userEvent.click(await screen.findByRole('button', { name: 'Edit default workflow' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Workflow name' }), ' revised');
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish version' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect((screen.getByRole('textbox', { name: 'Workflow name' }) as HTMLInputElement).value).toBe(
    'Local workflow revised',
  );
  expect(screen.getByText('Based on version 5')).toBeTruthy();
  expect(
    (screen.getByRole('button', { name: 'Review publication' }) as HTMLButtonElement).disabled,
  ).toBe(true);
  expect(api.publish).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish version' }),
  );
  await waitFor(() => expect(api.publish).toHaveBeenCalledTimes(2));
  expect(api.publish.mock.calls[1]![3]['If-Match']).toBe('"5"');
  expect(api.publish.mock.calls[1]![3]['Idempotency-Key']).not.toBe(
    api.publish.mock.calls[0]![3]['Idempotency-Key'],
  );
});

it('lists immutable workflows, reads an exact historical version and selects it using the separate default revision', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'Open Local workflow' }));
  const version = screen.getByRole('spinbutton', { name: 'Workflow version' });
  await userEvent.clear(version);
  await userEvent.type(version, '2');
  api.get.mockResolvedValue({
    ...resource,
    version: 2,
    definition: { ...definition, name: 'Historical policy' },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Load version' }));
  await screen.findByText('Historical policy');
  expect(api.get).toHaveBeenLastCalledWith('delivery', resource.id, 2, expect.any(AbortSignal));
  await userEvent.click(screen.getByRole('button', { name: 'Use for new tasks' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain('Historical policy');
  expect(dialog.textContent).toContain('Version 2');
  expect(dialog.textContent).toContain('Existing tasks keep their pinned workflow');
  expect(api.selectDefault).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Select default' }));
  await waitFor(() =>
    expect(api.selectDefault).toHaveBeenCalledWith(
      'delivery',
      { workflow_id: resource.id, workflow_version: 2 },
      expect.objectContaining({ 'If-Match': '"7"' }),
    ),
  );
  expect(api.publish).not.toHaveBeenCalled();
});

it('creates an independent workflow from the editable default and keeps its choice separate', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'Create workflow' }));
  const name = screen.getByRole('textbox', { name: 'Workflow name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'Support');
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish version' }),
  );
  await waitFor(() =>
    expect(api.create).toHaveBeenCalledWith(
      'delivery',
      { id: expect.any(String), definition: { ...definition, name: 'Support' } },
      { 'Idempotency-Key': expect.any(String) },
    ),
  );
  expect(api.publish).not.toHaveBeenCalled();
  expect(api.selectDefault).not.toHaveBeenCalled();
});

it('requires fresh default-selection review after a conflict without publishing or moving a task', async () => {
  const { api } = setup();
  const { ManagementApiError } = await import('../../api/client');
  api.selectDefault.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'conflict',
      message: 'Stale default.',
      requestId: null,
      status: 409,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Open Local workflow' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Use for new tasks' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Select default' }),
  );
  api.getDefault.mockResolvedValue({ version: 9, workflow: resource });
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(api.selectDefault).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole('button', { name: 'Use for new tasks' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Select default' }),
  );
  await waitFor(() => expect(api.selectDefault).toHaveBeenCalledTimes(2));
  expect(api.selectDefault.mock.calls[1]![2]['If-Match']).toBe('"9"');
  expect(api.publish).not.toHaveBeenCalled();
});

it('replays the frozen publication after transport failure and does not store drafts in preferences', async () => {
  const { api } = setup();
  const { ManagementApiError } = await import('../../api/client');
  api.publish.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'transport_failure',
      message: 'Disconnected.',
      requestId: null,
      status: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Edit default workflow' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Workflow name' }), ' private draft');
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish version' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Retry same command' }));
  await screen.findByText('Published version 4.');
  expect(api.publish.mock.calls[1]).toEqual(api.publish.mock.calls[0]);
  expect(JSON.stringify(localStorage)).not.toContain('private draft');
  expect(JSON.stringify(sessionStorage)).not.toContain('private draft');
});

it('accepts an empty terminal selection and removes terminal references explicitly with a status', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'Edit default workflow' }));
  const terminals = screen.getByRole('group', { name: 'Terminal statuses' });
  await userEvent.click(within(terminals).getByRole('checkbox', { name: 'Done' }));
  await userEvent.click(screen.getByRole('button', { name: 'Remove status 3' }));
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  expect(api.validate).toHaveBeenCalledWith('delivery', {
    ...definition,
    statuses: definition.statuses.slice(0, 2),
    terminal_status_ids: [],
  });
});

it('uses the successful publication receipt as the next editing baseline without a spurious conflict', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'Edit default workflow' }));
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish version' }),
  );
  await userEvent.click(
    await within(screen.getByRole('dialog')).findByRole('button', { name: 'Close' }),
  );
  await userEvent.type(screen.getByRole('textbox', { name: 'Workflow name' }), ' next edit');
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish version' }),
  );
  await waitFor(() => expect(api.publish).toHaveBeenCalledTimes(2));
  expect(api.publish.mock.calls[1]![3]['If-Match']).toBe('"4"');
});

it('keeps an indeterminate default-selection intent and its review through a background head refresh', async () => {
  const { api, queryClient } = setup();
  const { ManagementApiError } = await import('../../api/client');
  const { queryKeys } = await import('../../app/query');
  api.selectDefault.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'transport_failure',
      message: 'Disconnected.',
      requestId: null,
      status: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Open Local workflow' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Use for new tasks' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Select default' }),
  );
  await screen.findByRole('button', { name: 'Retry same command' });
  api.get.mockResolvedValue({
    ...resource,
    version: 4,
    definition: {
      ...definition,
      name: 'Later policy',
      statuses: definition.statuses.map((status) => ({ ...status, name: 'Later status' })),
    },
  });
  await queryClient.refetchQueries({ queryKey: queryKeys.issueWorkflow('delivery', resource.id) });
  await screen.findByRole('heading', { name: 'Later policy' });
  const review = screen.getByRole('dialog');
  expect(review.textContent).toContain('Local workflow');
  expect(review.textContent).toContain('Version 3');
  expect(review.textContent).not.toContain('Later status');
  await userEvent.click(within(review).getByRole('button', { name: 'Retry same command' }));
  await waitFor(() => expect(api.selectDefault).toHaveBeenCalledTimes(2));
  expect(api.selectDefault.mock.calls[1]).toEqual(api.selectDefault.mock.calls[0]);
});

it('opens the newly published current policy and selects its exact version without changing the default automatically', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'Open Local workflow' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit this version' }));
  const published = {
    ...resource,
    version: 4,
    definition: { ...definition, name: 'Published policy' },
  };
  api.get.mockResolvedValue(published);
  api.publish.mockResolvedValue({ disposition: 'applied', resource: published });
  api.list.mockResolvedValue({
    items: [{ ...published, name: published.definition.name }],
    next_cursor: null,
  });
  const name = screen.getByRole('textbox', { name: 'Workflow name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'Published policy');
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish version' }),
  );
  await userEvent.click(
    await within(screen.getByRole('dialog')).findByRole('button', { name: 'Close' }),
  );
  expect(api.selectDefault).not.toHaveBeenCalled();
  await userEvent.click(await screen.findByRole('button', { name: 'Open Published policy' }));
  expect(await screen.findByRole('heading', { name: 'Published policy' })).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Use for new tasks' }));
  const review = screen.getByRole('dialog');
  expect(review.textContent).toContain('Version 4');
  await userEvent.click(within(review).getByRole('button', { name: 'Select default' }));
  await waitFor(() =>
    expect(api.selectDefault).toHaveBeenCalledWith(
      'delivery',
      { workflow_id: resource.id, workflow_version: 4 },
      expect.objectContaining({ 'If-Match': '"7"' }),
    ),
  );
});

it.each(['not_found', 'unavailable'] as const)(
  'keeps visible workflows editable when the default read returns %s',
  async (code) => {
    const { ManagementApiError } = await import('../../api/client');
    const visible = {
      ...resource,
      id: '20000000-0000-4000-8000-000000000002',
      definition: { ...definition, name: 'Visible policy' },
    };
    const { api } = setup({
      defaultError: new ManagementApiError({
        code,
        message: 'Default cannot be read.',
        requestId: null,
        status: code === 'not_found' ? 404 : 503,
        retryAfterMilliseconds: null,
      }),
      listedWorkflow: visible,
    });
    await screen.findByRole('alert');
    await userEvent.click(await screen.findByRole('button', { name: 'Open Visible policy' }));
    expect(await screen.findByRole('heading', { name: 'Visible policy' })).toBeTruthy();
    expect(
      (screen.getByRole('button', { name: 'Use for new tasks' }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(screen.queryByRole('button', { name: 'Create workflow' })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Edit this version' }));
    await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
    await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
    await userEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish version' }),
    );
    await waitFor(() =>
      expect(api.publish).toHaveBeenCalledWith(
        'delivery',
        visible.id,
        visible.definition,
        expect.objectContaining({ 'If-Match': '"3"' }),
      ),
    );
    expect(api.selectDefault).not.toHaveBeenCalled();
  },
);

it.each([true, false])(
  'restores editing focus after a stale publication refresh (read succeeds: %s)',
  async (succeeds) => {
    const { api } = setup();
    const { ManagementApiError } = await import('../../api/client');
    api.publish.mockRejectedValueOnce(
      new ManagementApiError({
        code: 'conflict',
        message: 'Stale workflow.',
        requestId: null,
        status: 409,
        retryAfterMilliseconds: null,
      }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Edit default workflow' }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Workflow name' }), ' retained');
    await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
    await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
    await userEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish version' }),
    );
    if (succeeds) api.get.mockResolvedValue({ ...resource, version: 5 });
    else
      api.get.mockRejectedValue(
        new ManagementApiError({
          code: 'unavailable',
          message: 'Refresh unavailable.',
          requestId: null,
          status: 503,
          retryAfterMilliseconds: null,
        }),
      );
    await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Validate draft' }));
    expect((screen.getByRole('textbox', { name: 'Workflow name' }) as HTMLInputElement).value).toBe(
      'Local workflow retained',
    );
    expect(
      (screen.getByRole('button', { name: 'Review publication' }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(api.publish).toHaveBeenCalledTimes(1);
  },
);
