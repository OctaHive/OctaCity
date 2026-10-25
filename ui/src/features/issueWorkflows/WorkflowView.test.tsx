// @vitest-environment jsdom
import { readFileSync } from 'node:fs';
import { cleanup, render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { App } from '../../App';
import type { IssueWorkflowsApi, WorkflowResource } from '../../api/issueWorkflows';
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
  defaultWorkflow = resource,
  listedWorkflow = resource,
}: {
  defaultError?: Error;
  defaultWorkflow?: WorkflowResource;
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
      .mockResolvedValue({ version: 7, workflow: defaultWorkflow }),
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

it.each(['default', 'catalog'])(
  'shows named status cards and explicit initial/terminal markers on the %s workflow canvas',
  async (entry) => {
    setup();
    const section =
      entry === 'default'
        ? await screen.findByRole('region', { name: 'Default for new tasks' })
        : screen.getByRole('region', { name: 'Published workflows' });
    if (entry === 'catalog') {
      await userEvent.click(await screen.findByRole('button', { name: 'Open Local workflow' }));
    }
    const graph = await within(section).findByRole('group', { name: 'Workflow graph' });
    expect(within(graph).getAllByRole('article')).toHaveLength(3);
    const initial = within(graph).getByRole('article', { name: 'New' });
    expect(within(initial).getByRole('heading', { name: 'New' })).toBeTruthy();
    expect(within(initial).getByText('Initial status')).toBeTruthy();
    expect(within(initial).queryByText('Terminal status')).toBeNull();
    for (const name of ['Done', 'Declined']) {
      const terminal = within(graph).getByRole('article', { name });
      expect(within(terminal).getByRole('heading', { name })).toBeTruthy();
      expect(within(terminal).getByText('Terminal status')).toBeTruthy();
      expect(within(terminal).queryByText('Initial status')).toBeNull();
    }
    expect(
      within(within(graph).getByRole('group', { name: 'Permitted transitions' })).queryAllByRole(
        'button',
      ),
    ).toHaveLength(0);
  },
);

it('draws only declared branches and return paths, with separate named arrows for parallel transitions', async () => {
  const configured: WorkflowResource = {
    ...resource,
    definition: {
      name: 'Review policy',
      statuses: [
        { id: '10000000-0000-4000-8000-000000000001', name: 'Inbox' },
        { id: '10000000-0000-4000-8000-000000000002', name: 'Review' },
        { id: '10000000-0000-4000-8000-000000000003', name: 'Done' },
        { id: '10000000-0000-4000-8000-000000000004', name: 'Archived' },
      ],
      initial_status_id: '10000000-0000-4000-8000-000000000001',
      terminal_status_ids: ['10000000-0000-4000-8000-000000000003'],
      transitions: [
        {
          id: '30000000-0000-4000-8000-000000000001',
          name: 'Submit',
          from: '10000000-0000-4000-8000-000000000001',
          to: '10000000-0000-4000-8000-000000000002',
          origins: ['manual'],
          evidence: [],
        },
        {
          id: '30000000-0000-4000-8000-000000000002',
          name: 'Complete directly',
          from: '10000000-0000-4000-8000-000000000001',
          to: '10000000-0000-4000-8000-000000000003',
          origins: ['manual'],
          evidence: [],
        },
        {
          id: '30000000-0000-4000-8000-000000000003',
          name: 'Return',
          from: '10000000-0000-4000-8000-000000000002',
          to: '10000000-0000-4000-8000-000000000001',
          origins: ['manual'],
          evidence: [],
        },
        {
          id: '30000000-0000-4000-8000-000000000004',
          name: 'Escalate',
          from: '10000000-0000-4000-8000-000000000001',
          to: '10000000-0000-4000-8000-000000000002',
          origins: ['pipeline'],
          evidence: [],
        },
      ],
    },
  };
  const { api } = setup({ defaultWorkflow: configured, listedWorkflow: configured });
  const defaultSection = await screen.findByRole('region', { name: 'Default for new tasks' });
  const graph = within(defaultSection).getByRole('group', { name: 'Workflow graph' });
  expect(within(graph).getAllByRole('article')).toHaveLength(4);
  const arrows = within(
    within(graph).getByRole('group', { name: 'Permitted transitions' }),
  ).getAllByRole('button');
  expect(arrows.map((arrow) => arrow.getAttribute('aria-label'))).toEqual([
    'Submit: Inbox → Review',
    'Complete directly: Inbox → Done',
    'Return: Review → Inbox',
    'Escalate: Inbox → Review',
  ]);
  const paths = arrows.map((arrow) => arrow.querySelector('path')!);
  expect(new Set(paths.map((path) => path.getAttribute('d'))).size).toBe(4);
  for (const [index, name] of ['Submit', 'Complete directly', 'Return', 'Escalate'].entries()) {
    expect(within(arrows[index]!).getByText(name, { selector: 'text' })).toBeTruthy();
    const marker = paths[index]!.getAttribute('marker-end')!;
    const markerId = marker.slice(5, -1);
    expect(document.getElementById(markerId)?.tagName).toBe('marker');
    expect(document.getElementById(markerId)?.getAttribute('orient')).toBe('auto');
  }
  await userEvent.click(screen.getByRole('button', { name: 'Open Review policy' }));
  const catalog = screen.getByRole('region', { name: 'Published workflows' });
  const catalogGraph = await within(catalog).findByRole('group', { name: 'Workflow graph' });
  expect(
    within(within(catalogGraph).getByRole('group', { name: 'Permitted transitions' }))
      .getAllByRole('button')
      .map((arrow) => arrow.getAttribute('aria-label')),
  ).toEqual(arrows.map((arrow) => arrow.getAttribute('aria-label')));
  const defaultMarker = paths[0]!.getAttribute('marker-end');
  const catalogMarker = within(
    within(catalogGraph).getByRole('group', { name: 'Permitted transitions' }),
  )
    .getAllByRole('button')[0]!
    .querySelector('path')!
    .getAttribute('marker-end');
  expect(catalogMarker).not.toBe(defaultMarker);
  expect(api.validate).not.toHaveBeenCalled();
  expect(api.create).not.toHaveBeenCalled();
  expect(api.publish).not.toHaveBeenCalled();
  expect(api.selectDefault).not.toHaveBeenCalled();
});

it('selects statuses by stable ID and shows their current read-only properties after reordering', async () => {
  const configured = {
    ...resource,
    definition: {
      ...definition,
      statuses: definition.statuses.map((status) => ({ ...status, name: 'Shared name' })),
    },
  };
  const { api, queryClient } = setup({ defaultWorkflow: configured });
  const section = await screen.findByRole('region', { name: 'Default for new tasks' });
  const graph = within(section).getByRole('group', { name: 'Workflow graph' });
  const nodes = within(graph).getAllByRole('button', { name: 'Select status Shared name' });
  await userEvent.click(nodes[1]!);
  const inspector = within(section).getByRole('complementary', { name: 'Workflow properties' });
  expect(within(inspector).getByText('Read only')).toBeTruthy();
  expect(within(inspector).getByRole('heading', { name: 'Shared name' })).toBeTruthy();
  const property = (label: string) =>
    within(inspector).getByText(label, { selector: 'dt' }).nextElementSibling?.textContent;
  expect(property('Status ID')).toBe('10000000-0000-4000-8000-000000000002');
  expect(property('Status order')).toBe('2');
  expect(property('Initial status')).toBe('No');
  expect(property('Terminal status')).toBe('Yes');
  expect(nodes[1]!.getAttribute('aria-pressed')).toBe('true');
  expect(nodes[0]!.getAttribute('aria-pressed')).toBe('false');
  expect(within(inspector).queryAllByRole('textbox')).toHaveLength(0);
  expect(within(inspector).queryAllByRole('checkbox')).toHaveLength(0);

  api.getDefault.mockResolvedValue({
    version: 7,
    workflow: {
      ...configured,
      definition: {
        ...configured.definition,
        statuses: [
          { ...definition.statuses[1]!, name: 'Renamed result' },
          definition.statuses[2]!,
          definition.statuses[0]!,
        ],
      },
    },
  });
  const { queryKeys } = await import('../../app/query');
  await queryClient.refetchQueries({ queryKey: queryKeys.issueWorkflowDefault('delivery') });
  await within(inspector).findByRole('heading', { name: 'Renamed result' });
  expect(property('Status ID')).toBe('10000000-0000-4000-8000-000000000002');
  expect(property('Status order')).toBe('1');
  const initial = within(graph).getByRole('button', { name: 'Select status New' });
  await userEvent.click(initial);
  expect(property('Initial status')).toBe('Yes');
  expect(property('Terminal status')).toBe('No');
  expect(
    within(graph)
      .getByRole('button', { name: 'Select status Renamed result' })
      .getAttribute('aria-pressed'),
  ).toBe('false');
  expect(api.validate).not.toHaveBeenCalled();
  expect(api.create).not.toHaveBeenCalled();
  expect(api.publish).not.toHaveBeenCalled();
  expect(api.selectDefault).not.toHaveBeenCalled();
});

it('inspects the exact parallel transition by stable ID with keyboard selection and no mutations', async () => {
  const first: Definition['transitions'][number] = {
    id: '30000000-0000-4000-8000-000000000001',
    name: 'Review',
    from: definition.statuses[0]!.id,
    to: definition.statuses[1]!.id,
    origins: ['manual'],
    evidence: [],
  };
  const second: Definition['transitions'][number] = {
    ...first,
    id: '30000000-0000-4000-8000-000000000002',
    origins: ['pipeline', 'source'],
    evidence: ['accepted_execution', 'passed_required_checks'],
  };
  const configured = { ...resource, definition: { ...definition, transitions: [first, second] } };
  const { api, queryClient } = setup({ defaultWorkflow: configured });
  const section = await screen.findByRole('region', { name: 'Default for new tasks' });
  const graph = within(section).getByRole('group', { name: 'Workflow graph' });
  const inspector = within(section).getByRole('complementary', { name: 'Workflow properties' });
  const property = (label: string) =>
    within(inspector).getByText(label, { selector: 'dt' }).nextElementSibling?.textContent;
  const arrows = within(graph).getAllByRole('button', { name: 'Review: New → Done' });
  arrows[1]!.focus();
  await userEvent.keyboard('{Enter}');
  expect(property('Transition ID')).toBe('30000000-0000-4000-8000-000000000002');
  expect(property('From status')).toBe('New · 10000000-0000-4000-8000-000000000001');
  expect(property('To status')).toBe('Done · 10000000-0000-4000-8000-000000000002');
  expect(property('Allowed origins')).toBe('Pipeline action, Source synchronization');
  expect(property('Required evidence')).toBe('Accepted execution, Required checks passed');
  expect(arrows[1]!.getAttribute('aria-pressed')).toBe('true');
  expect(arrows[0]!.getAttribute('aria-pressed')).toBe('false');
  expect(document.activeElement).toBe(arrows[1]);
  expect(within(inspector).queryAllByRole('textbox')).toHaveLength(0);
  expect(within(inspector).queryAllByRole('checkbox')).toHaveLength(0);
  const { queryKeys } = await import('../../app/query');
  api.getDefault.mockResolvedValue({
    version: 7,
    workflow: {
      ...configured,
      definition: {
        ...configured.definition,
        transitions: [{ ...second, name: 'Qualified review' }, first],
      },
    },
  });
  await queryClient.refetchQueries({ queryKey: queryKeys.issueWorkflowDefault('delivery') });
  await within(inspector).findByRole('heading', { name: 'Qualified review' });
  expect(property('Transition ID')).toBe('30000000-0000-4000-8000-000000000002');
  expect(
    within(graph)
      .getByRole('button', { name: 'Qualified review: New → Done' })
      .getAttribute('aria-pressed'),
  ).toBe('true');
  const manual = within(graph).getByRole('button', { name: 'Review: New → Done' });
  manual.focus();
  await userEvent.keyboard(' ');
  expect(property('Allowed origins')).toBe('Manual move');
  expect(property('Required evidence')).toBe('None');
  await userEvent.click(within(graph).getByRole('button', { name: 'Select status New' }));
  expect(within(inspector).queryByText('Transition ID')).toBeNull();
  expect(manual.getAttribute('aria-pressed')).toBe('false');
  expect(api.validate).not.toHaveBeenCalled();
  expect(api.create).not.toHaveBeenCalled();
  expect(api.publish).not.toHaveBeenCalled();
  expect(api.selectDefault).not.toHaveBeenCalled();
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
    definition: {
      name: 'Historical policy',
      statuses: [
        { id: definition.statuses[0]!.id, name: 'Earlier inbox' },
        { id: definition.statuses[1]!.id, name: 'Earlier result' },
      ],
      initial_status_id: definition.statuses[0]!.id,
      terminal_status_ids: [definition.statuses[1]!.id],
      transitions: [
        {
          id: '30000000-0000-4000-8000-000000000001',
          name: 'Earlier completion',
          from: definition.statuses[0]!.id,
          to: definition.statuses[1]!.id,
          origins: ['manual'],
          evidence: [],
        },
      ],
    },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Load version' }));
  await screen.findByText('Historical policy');
  expect(api.get).toHaveBeenLastCalledWith('delivery', resource.id, 2, expect.any(AbortSignal));
  const catalog = screen.getByRole('region', { name: 'Published workflows' });
  const historical = within(catalog).getByRole('group', { name: 'Workflow graph' });
  expect(
    within(historical)
      .getAllByRole('article')
      .map((node) => node.getAttribute('aria-label')),
  ).toEqual(['Earlier inbox', 'Earlier result']);
  expect(within(historical).getByRole('article', { name: 'Earlier inbox' }).textContent).toContain(
    'Initial status',
  );
  expect(within(historical).getByRole('article', { name: 'Earlier result' }).textContent).toContain(
    'Terminal status',
  );
  expect(
    within(within(historical).getByRole('group', { name: 'Permitted transitions' })).getAllByRole(
      'button',
    ),
  ).toHaveLength(1);
  expect(
    within(historical).getByRole('button', {
      name: 'Earlier completion: Earlier inbox → Earlier result',
    }),
  ).toBeTruthy();
  const current = within(screen.getByRole('region', { name: 'Default for new tasks' })).getByRole(
    'group',
    { name: 'Workflow graph' },
  );
  expect(within(current).getByRole('article', { name: 'New' })).toBeTruthy();
  expect(
    within(within(current).getByRole('group', { name: 'Permitted transitions' })).queryAllByRole(
      'button',
    ),
  ).toHaveLength(0);
  expect(api.validate).not.toHaveBeenCalled();
  expect(api.create).not.toHaveBeenCalled();
  expect(api.publish).not.toHaveBeenCalled();
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

it('creates a workflow from empty fields with only operator-defined statuses', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'Create workflow' }));
  const name = screen.getByRole('textbox', { name: 'Workflow name' });
  expect((name as HTMLInputElement).value).toBe('');
  expect(screen.queryByRole('textbox', { name: 'Status name 1' })).toBeNull();
  await userEvent.type(name, 'Support');
  await userEvent.click(screen.getByRole('button', { name: 'Add status' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Status name 1' }), 'Received');
  await userEvent.click(screen.getByRole('button', { name: 'Add status' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Status name 2' }), 'Answered');
  expect(screen.getByRole('combobox', { name: 'Initial status' }).textContent).toContain(
    'Received',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Publish version' }),
  );
  await waitFor(() =>
    expect(api.create).toHaveBeenCalledWith(
      'delivery',
      {
        id: expect.any(String),
        definition: {
          name: 'Support',
          statuses: [
            { id: expect.any(String), name: 'Received' },
            { id: expect.any(String), name: 'Answered' },
          ],
          initial_status_id: expect.any(String),
          terminal_status_ids: [],
          transitions: [],
        },
      },
      { 'Idempotency-Key': expect.any(String) },
    ),
  );
  const created = api.create.mock.calls[0]![1];
  expect(created.id).not.toBe(resource.id);
  expect(created.definition.initial_status_id).toBe(created.definition.statuses[0]!.id);
  for (const status of created.definition.statuses) {
    expect(definition.statuses.some((preset) => preset.id === status.id)).toBe(false);
  }
  expect(api.publish).not.toHaveBeenCalled();
  expect(api.selectDefault).not.toHaveBeenCalled();
});

it('can clear every draft status and start over, but cannot validate an empty workflow', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'Create workflow' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Workflow name' }), 'Support');
  await userEvent.click(screen.getByRole('button', { name: 'Add status' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Status name 1' }), 'Temporary');
  await userEvent.click(screen.getByRole('button', { name: 'Remove status 1' }));
  expect(screen.queryByRole('textbox', { name: 'Status name 1' })).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  expect(api.validate).not.toHaveBeenCalled();
  expect(
    (screen.getByRole('button', { name: 'Review publication' }) as HTMLButtonElement).disabled,
  ).toBe(true);
  await userEvent.click(screen.getByRole('button', { name: 'Add status' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Status name 1' }), 'Received');
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  expect(api.validate).toHaveBeenCalledWith('delivery', {
    name: 'Support',
    statuses: [{ id: expect.any(String), name: 'Received' }],
    initial_status_id: expect.any(String),
    terminal_status_ids: [],
    transitions: [],
  });
  const sent = api.validate.mock.calls[0]![1];
  expect(sent.initial_status_id).toBe(sent.statuses[0]!.id);
});

it.each(['default', 'catalog'])(
  'removes referenced statuses from a %s workflow draft without changing published versions',
  async (entry) => {
    const configured: WorkflowResource = {
      ...resource,
      definition: {
        ...definition,
        transitions: [
          {
            id: '30000000-0000-4000-8000-000000000001',
            name: 'Finish',
            from: definition.statuses[0]!.id,
            to: definition.statuses[1]!.id,
            origins: ['manual'],
            evidence: [],
          },
          {
            id: '30000000-0000-4000-8000-000000000002',
            name: 'Decline',
            from: definition.statuses[1]!.id,
            to: definition.statuses[2]!.id,
            origins: ['manual'],
            evidence: [],
          },
        ],
      },
    };
    const { api } = setup({ defaultWorkflow: configured, listedWorkflow: configured });
    if (entry === 'default') {
      await userEvent.click(await screen.findByRole('button', { name: 'Edit default workflow' }));
    } else {
      await userEvent.click(await screen.findByRole('button', { name: 'Open Local workflow' }));
      await userEvent.click(await screen.findByRole('button', { name: 'Edit this version' }));
    }
    await userEvent.click(screen.getByRole('button', { name: 'Remove status 1' }));
    expect(screen.queryByDisplayValue('New')).toBeNull();
    expect(screen.getByRole('combobox', { name: 'Initial status' }).textContent).toContain('Done');
    expect(screen.queryByDisplayValue('Finish')).toBeNull();
    expect(screen.getByDisplayValue('Decline')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Remove status 2' }));
    await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
    expect(api.validate).toHaveBeenLastCalledWith('delivery', {
      name: definition.name,
      statuses: [definition.statuses[1]],
      initial_status_id: definition.statuses[1]!.id,
      terminal_status_ids: [definition.statuses[1]!.id],
      transitions: [],
    });
    expect(api.publish).not.toHaveBeenCalled();
    expect(api.selectDefault).not.toHaveBeenCalled();
  },
);

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
