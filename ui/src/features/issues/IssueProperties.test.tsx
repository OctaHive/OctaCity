// @vitest-environment jsdom
import { act, cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { ManagementApiError } from '../../api/client';
import { queryKeys } from '../../app/query';
import type { IssuePlanning, IssuePropertyChoices } from './api';
import { issuePropertyChoicesFixture } from '../../../tests/fixtures/issues';
import userEvent from '@testing-library/user-event';
import { issue, setup } from '../../../tests/fixtures/issueTestSupport';
afterEach(() => {
  cleanup();
  localStorage.clear();
  sessionStorage.clear();
});
async function choose(user: ReturnType<typeof userEvent.setup>, label: string, option: string) {
  const trigger = screen.getByRole('combobox', { name: label });
  await waitFor(() => expect((trigger as HTMLButtonElement).disabled).toBe(false));
  await user.click(trigger);
  await user.click(
    within(screen.getByRole('listbox', { name: label })).getByRole('option', { name: option }),
  );
}
async function openProperties(user: ReturnType<typeof userEvent.setup>) {
  const button = await screen.findByRole('button', { name: 'Edit properties' });
  await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(false));
  await user.click(button);
}

it('shows useful optional properties and Unassigned without a directory or provider', async () => {
  const { api } = setup('/projects/delivery/issues/' + issue.id);
  await screen.findByRole('heading', { name: 'ISS-42 · Investigate checkout' });
  const panel = screen.getByRole('complementary', { name: 'Properties' });
  expect(await within(panel).findByText('Unassigned')).toBeTruthy();
  expect(
    within(panel).getByText('No person directory is available. Tasks can remain unassigned.'),
  ).toBeTruthy();
  expect(within(panel).getByText('Due date')).toBeTruthy();
  expect(within(panel).getByText('Labels')).toBeTruthy();
  expect(within(panel).getByText('No configurable properties in this project.')).toBeTruthy();
  expect(api.create).not.toHaveBeenCalled();
  expect(api.edit).not.toHaveBeenCalled();
  expect(localStorage.length).toBe(0);
  expect(sessionStorage.length).toBe(0);
});

it('reviews optional assignment, labels and date then saves only planning with exact task version', async () => {
  const user = userEvent.setup();
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue({
      definitions: {
        project_id: 'delivery',
        version: 7,
        definition: { labels: [{ id: 'urgent', name: 'Urgent', retired: false }], properties: [] },
        published_at_unix_ms: 1,
      },
      assignees: { available: true, items: [{ id: 'person-1', name: 'Alice' }] },
    });
  });
  await openProperties(user);
  await choose(user, 'Responsible party', 'Alice');
  await user.click(screen.getByLabelText('Urgent'));
  fireEvent.change(screen.getByLabelText('Due date'), { target: { value: '2026-11-20' } });
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  const dialog = screen.getByRole('dialog', { name: 'Review properties' });
  expect(within(dialog).getByText(/Alice/)).toBeTruthy();
  expect(api.editPlanning).not.toHaveBeenCalled();
  await user.click(within(dialog).getByRole('button', { name: 'Save properties' }));
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(1));
  expect(api.editPlanning.mock.calls[0]).toEqual([
    'delivery',
    issue.id,
    {
      definition_version: 7,
      assignee: 'person-1',
      labels: ['urgent'],
      due_date: '2026-11-20',
      values: {},
      parent_id: null,
    },
    { 'If-Match': '"1"', 'Idempotency-Key': expect.any(String) },
  ]);
  expect(api.move).not.toHaveBeenCalled();
  expect(api.edit).not.toHaveBeenCalled();
  expect(api.adopt).not.toHaveBeenCalled();
  expect(api.create).not.toHaveBeenCalled();
  expect(localStorage.length).toBe(0);
  expect(sessionStorage.length).toBe(0);
});

const catalog = issuePropertyChoicesFixture();
it('creates a label inside planning without losing the task draft or requiring a reload', async () => {
  const user = userEvent.setup();
  const { api } = setup('/projects/delivery/issues/' + issue.id);
  api.publishProperties.mockImplementation(async (_project, request) => {
    const definitions = {
      project_id: 'delivery',
      version: 1,
      published_at_unix_ms: 2,
      definition: request.definition,
    };
    api.planningChoices.mockResolvedValue({
      definitions,
      assignees: { available: false, items: [] },
    });
    return { disposition: 'applied', resource: definitions };
  });
  await openProperties(user);
  fireEvent.change(screen.getByLabelText('Due date'), { target: { value: '2026-11-20' } });
  const labels = screen.getByRole('group', { name: 'Labels' });
  const create = within(labels).getByRole('button', { name: 'Create label' });
  await user.click(create);
  await user.type(screen.getByRole('textbox', { name: 'Label name' }), 'Срочно');
  await user.click(screen.getByRole('button', { name: 'Review label' }));
  const review = screen.getByRole('dialog', { name: 'Review label' });
  expect(review.textContent).toContain('Срочно');
  expect(review.textContent).toContain('Available to all tasks in this project');
  expect(api.publishProperties).not.toHaveBeenCalled();
  await user.click(within(review).getByRole('button', { name: 'Create label' }));
  await within(review).findByText('Label created. Select it in the list and save properties.');
  expect(api.publishProperties.mock.calls[0]).toEqual([
    'delivery',
    {
      expected_version: 0,
      definition: {
        labels: [
          { id: expect.stringMatching(/^[a-z0-9_]{1,64}$/), name: 'Срочно', retired: false },
        ],
        properties: [],
      },
    },
    { 'Idempotency-Key': expect.any(String) },
  ]);
  expect(api.editPlanning).not.toHaveBeenCalled();
  await user.click(within(review).getByRole('button', { name: 'Close' }));
  expect((screen.getByLabelText('Due date') as HTMLInputElement).value).toBe('2026-11-20');
  expect(screen.queryByText(/Planning changed\. Reload/)).toBeNull();
  await user.click(screen.getByRole('checkbox', { name: 'Срочно' }));
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save properties' }),
  );
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(1));
  expect(api.editPlanning.mock.calls[0]).toEqual([
    'delivery',
    issue.id,
    {
      definition_version: 1,
      labels: [api.publishProperties.mock.calls[0]![1].definition.labels[0].id],
      assignee: null,
      due_date: '2026-11-20',
      parent_id: null,
      values: {},
    },
    { 'If-Match': '"1"', 'Idempotency-Key': expect.any(String) },
  ]);
  expect(api.edit).not.toHaveBeenCalled();
  expect(api.create).not.toHaveBeenCalled();
  expect(localStorage.length).toBe(0);
  expect(sessionStorage.length).toBe(0);
});

it('requires a new label review when the catalog changes and preserves existing and retired definitions', async () => {
  const user = userEvent.setup();
  const initial = structuredClone(catalog);
  initial.definitions.definition.labels = [{ id: 'legacy', name: 'Legacy', retired: true }];
  const { api, queryClient } = setup('/projects/delivery/issues/' + issue.id, (api) =>
    api.planningChoices.mockResolvedValue(initial),
  );
  await openProperties(user);
  const create = await screen.findByRole('button', { name: 'Create label' });
  await waitFor(() => expect((create as HTMLButtonElement).disabled).toBe(false));
  await user.click(create);
  await user.type(screen.getByRole('textbox', { name: 'Label name' }), 'UI');
  await user.click(screen.getByRole('button', { name: 'Review label' }));
  const current = structuredClone(initial);
  current.definitions.version = 8;
  current.definitions.definition.labels.push({ id: 'external', name: 'External', retired: false });
  await act(async () =>
    queryClient.setQueryData(queryKeys.issuePropertyChoices('delivery'), current),
  );
  await waitFor(() =>
    expect(
      (
        within(screen.getByRole('dialog')).getByRole('button', {
          name: 'Create label',
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true),
  );
  expect(
    screen
      .getAllByRole('alert')
      .map((alert) => alert.textContent)
      .join(' '),
  ).toContain('The label catalog changed. Close this confirmation and review again.');
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Abandon' }));
  await user.click(screen.getByRole('button', { name: 'Review label' }));
  api.planningChoices.mockResolvedValue(current);
  api.publishProperties.mockResolvedValue({
    disposition: 'applied',
    resource: current.definitions,
  });
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Create label' }),
  );
  await waitFor(() => expect(api.publishProperties).toHaveBeenCalledTimes(1));
  expect(api.publishProperties.mock.calls[0]![1]).toEqual({
    expected_version: 8,
    definition: {
      labels: [
        { id: 'legacy', name: 'Legacy', retired: true },
        { id: 'external', name: 'External', retired: false },
        { id: expect.any(String), name: 'UI', retired: false },
      ],
      properties: initial.definitions.definition.properties,
    },
  });
  expect(
    screen
      .getAllByRole('alert')
      .map((alert) => alert.textContent)
      .join(' '),
  ).toContain('Planning changed. Reload properties and discard this draft before reviewing again.');
});

it('keeps a concurrent task change stale when label publication finishes without discarding the draft', async () => {
  const user = userEvent.setup();
  let finish!: () => void;
  const pending = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const { api, queryClient } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(catalog);
    api.publishProperties.mockImplementation(async (_project, body) => {
      await pending;
      const published = {
        ...catalog,
        definitions: { ...catalog.definitions, version: 8, definition: body.definition },
      };
      api.planningChoices.mockResolvedValue(published);
      return { disposition: 'applied', resource: published.definitions };
    });
  });
  await openProperties(user);
  fireEvent.change(screen.getByLabelText('Component'), { target: { value: 'draft' } });
  await user.click(screen.getByRole('button', { name: 'Create label' }));
  await user.type(screen.getByRole('textbox', { name: 'Label name' }), 'UI');
  await user.click(screen.getByRole('button', { name: 'Review label' }));
  const review = screen.getByRole('dialog');
  await user.click(within(review).getByRole('button', { name: 'Create label' }));
  await waitFor(() => expect(api.publishProperties).toHaveBeenCalledTimes(1));
  await act(async () => {
    queryClient.setQueryData(queryKeys.issue('delivery', issue.id), { ...issue, version: 2 });
  });
  await act(async () => finish());
  await within(review).findByText('Label created. Select it in the list and save properties.');
  await user.click(within(review).getByRole('button', { name: 'Close' }));
  expect((screen.getByLabelText('Component') as HTMLInputElement).value).toBe('draft');
  expect(screen.getByRole('alert').textContent).toContain('Planning changed.');
  expect(
    (screen.getByRole('button', { name: 'Review properties' }) as HTMLButtonElement).disabled,
  ).toBe(true);
  expect(api.editPlanning).not.toHaveBeenCalled();
});

it('uses project-defined typed optional fields, preserves false, clears values and enforces UTF-8 bounds', async () => {
  const user = userEvent.setup();
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) =>
    api.planningChoices.mockResolvedValue(catalog),
  );
  await openProperties(user);
  await choose(user, 'Task type', 'Defect');
  fireEvent.change(screen.getByLabelText('Component'), { target: { value: '🎉🎉🎉' } });
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(screen.getByRole('alert').textContent).toContain('Component: maximum 8 UTF-8 bytes.');
  fireEvent.change(screen.getByLabelText('Component'), { target: { value: 'ui' } });
  fireEvent.change(screen.getByLabelText('Size estimate'), { target: { value: '5' } });
  await choose(user, 'Verified', 'No');
  fireEvent.change(screen.getByLabelText('Scheduled date'), { target: { value: '2026-11-25' } });
  // Optional values can be explicitly cleared before review.
  fireEvent.change(screen.getByLabelText('Size estimate'), { target: { value: '' } });
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain('Task type: Defect');
  expect(dialog.textContent).toContain('Verified: No');
  await user.click(within(dialog).getByRole('button', { name: 'Save properties' }));
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(1));
  expect(api.editPlanning.mock.calls[0]![2].values).toEqual({
    type: { type: 'choice', value: 'defect' },
    component: { type: 'text', value: 'ui' },
    verified: { type: 'boolean', value: false },
    scheduled: { type: 'date', value: '2026-11-25' },
  });
});

it('explains retired choices, preserves them unchanged and never offers them as new selections', async () => {
  const user = userEvent.setup();
  const retired: IssuePropertyChoices = structuredClone(catalog);
  retired.definitions.definition.labels = [
    { id: 'legacy', name: 'Legacy label', retired: true },
    { id: 'unused', name: 'Unused label', retired: true },
  ];
  retired.definitions.definition.properties[0]!.kind = {
    type: 'choice',
    choices: [
      { id: 'defect', name: 'Defect', retired: true },
      { id: 'unused', name: 'Unused type', retired: true },
    ],
  };
  retired.definitions.definition.properties[1]!.retired = true;
  const plan: IssuePlanning = {
    definition_version: 7,
    assignee: 'unavailable-person',
    labels: ['legacy'],
    due_date: null,
    parent_id: null,
    values: {
      type: { type: 'choice', value: 'defect' },
      component: { type: 'text', value: 'historical component longer than current bound' },
    },
  };
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(retired);
    api.get.mockResolvedValue({ ...issue, planning: plan });
  });
  await openProperties(user);
  expect(
    screen.getByText('Retired values may be kept or removed; they cannot be selected again.'),
  ).toBeTruthy();
  expect((screen.getByLabelText('Unused label') as HTMLInputElement).disabled).toBe(true);
  await user.click(screen.getByRole('combobox', { name: 'Task type' }));
  expect(screen.queryByRole('option', { name: 'Unused type' })).toBeNull();
  await user.keyboard('{Escape}');
  expect((screen.getByLabelText('Component') as HTMLInputElement).disabled).toBe(true);
  expect(screen.getByRole('combobox', { name: 'Responsible party' }).textContent).toContain(
    'Assigned person unavailable',
  );
  // Missing directories must still allow removing an existing assignment.
  await choose(user, 'Responsible party', 'Unassigned');
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save properties' }),
  );
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(1));
  expect(api.editPlanning.mock.calls[0]![2]).toEqual({ ...plan, assignee: null });
});

it('explains source ownership and disables only the owned planning controls', async () => {
  const user = userEvent.setup();
  setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(catalog);
    api.transitions.mockResolvedValue({
      issue_version: 1,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: issue.status_id,
      field_policy: {
        source_owned: ['assignee', 'labels', 'due_date', 'properties', 'parent'],
        status_mapping_workflow: null,
        push: null,
      },
      items: [],
    });
  });
  await openProperties(user);
  expect(
    screen.getByText(
      'Managed by source: Responsible party, Labels, Due date, Custom properties, Parent task',
    ),
  ).toBeTruthy();
  expect((screen.getByLabelText('Component') as HTMLInputElement).disabled).toBe(true);
  expect((screen.getByLabelText('Due date') as HTMLInputElement).disabled).toBe(true);
});

it('requires explicit fresh review after task, definitions or ownership changes and retains the draft in memory', async () => {
  const user = userEvent.setup();
  const { queryClient, api } = setup('/projects/delivery/issues/' + issue.id, (api) =>
    api.planningChoices.mockResolvedValue(catalog),
  );
  await openProperties(user);
  fireEvent.change(screen.getByLabelText('Component'), { target: { value: 'draft' } });
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  await act(async () =>
    queryClient.setQueryData(queryKeys.issuePropertyChoices('delivery'), {
      ...catalog,
      definitions: { ...catalog.definitions, version: 8 },
    }),
  );
  await waitFor(() =>
    expect(
      (
        within(screen.getByRole('dialog')).getByRole('button', {
          name: 'Save properties',
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true),
  );
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Abandon' }));
  expect(screen.getByRole('alert').textContent).toContain(
    'Planning changed. Reload properties and discard this draft before reviewing again.',
  );
  expect((screen.getByLabelText('Component') as HTMLInputElement).value).toBe('draft');
  expect(
    (screen.getByRole('button', { name: 'Review properties' }) as HTMLButtonElement).disabled,
  ).toBe(true);
  await user.click(screen.getByRole('button', { name: 'Reload properties and discard draft' }));
  expect((screen.getByLabelText('Component') as HTMLInputElement).value).toBe('');
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save properties' }),
  );
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(1));
  expect(api.editPlanning.mock.calls[0]![2].definition_version).toBe(8);
  expect(localStorage.length).toBe(0);
  expect(sessionStorage.length).toBe(0);
});

it('reads saved values using their retained definition version while editing current choices', async () => {
  const user = userEvent.setup();
  const old = structuredClone(catalog.definitions);
  old.version = 6;
  old.definition.properties[0]!.name = 'Original type';
  const plan: IssuePlanning = {
    definition_version: 6,
    assignee: null,
    labels: [],
    due_date: null,
    parent_id: null,
    values: { type: { type: 'choice', value: 'defect' } },
  };
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(catalog);
    api.get.mockResolvedValue({ ...issue, planning: plan });
    api.propertyDefinitions.mockResolvedValue(old);
  });
  expect(await screen.findByText('Original type')).toBeTruthy();
  expect(screen.getByText('Saved definitions · version 6')).toBeTruthy();
  expect(api.propertyDefinitions).toHaveBeenCalledWith('delivery', 6, expect.any(AbortSignal));
  await user.click(screen.getByRole('button', { name: 'Edit properties' }));
  expect(screen.getByLabelText('Task type')).toBeTruthy();
});

const parent = {
  ...issue,
  id: '40000000-0000-4000-8000-000000000002',
  display_key: 'ISS-40',
  content: { ...issue.content, title: 'Parent task title' },
};
const child = {
  id: '40000000-0000-4000-8000-000000000003',
  project_id: 'delivery',
  number: 43,
  display_key: 'ISS-43',
  title: 'Child task title',
  version: 1,
  status_id: issue.status_id,
  workflow_id: issue.workflow_id,
  workflow_version: 3,
  applicability_version: 1,
  updated_at_unix_ms: 1,
};
it('navigates visible parent and bounded sub-issue pages by human names', async () => {
  const user = userEvent.setup();
  const plan: IssuePlanning = {
    definition_version: 0,
    assignee: null,
    labels: [],
    due_date: null,
    values: {},
    parent_id: parent.id,
  };
  const { api, router } = setup(
    '/projects/delivery/issues/' + issue.id + '?section=activity&revision=1',
    (api) => {
      api.get.mockImplementation(async (_project: string, id: string) =>
        id === parent.id ? parent : { ...issue, planning: plan },
      );
      api.children
        .mockResolvedValueOnce({ items: [child], next_cursor: child.id })
        .mockResolvedValue({
          items: [
            {
              ...child,
              id: '40000000-0000-4000-8000-000000000004',
              display_key: 'ISS-44',
              title: 'Next child',
            },
          ],
          next_cursor: null,
        });
    },
  );
  expect(await screen.findByRole('link', { name: 'ISS-40 · Parent task title' })).toBeTruthy();
  expect(await screen.findByRole('link', { name: 'ISS-43 · Child task title' })).toBeTruthy();
  expect(router.state.location.search).toBe('?section=activity&revision=1');
  await user.click(screen.getByRole('button', { name: 'Next sub-issues' }));
  expect(await screen.findByRole('link', { name: 'ISS-44 · Next child' })).toBeTruthy();
  expect(screen.queryByRole('link', { name: 'ISS-43 · Child task title' })).toBeNull();
  expect(api.children).toHaveBeenLastCalledWith(
    'delivery',
    issue.id,
    child.id,
    expect.any(AbortSignal),
  );
  await user.click(screen.getByRole('link', { name: 'ISS-40 · Parent task title' }));
  expect(router.state.location.pathname).toBe('/projects/delivery/issues/' + parent.id);
});

it('chooses a parent from authorized pages, retains the selected human label and excludes self', async () => {
  const user = userEvent.setup();
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.list
      .mockResolvedValueOnce({
        items: [
          { ...child, id: issue.id, display_key: issue.display_key, title: issue.content.title },
          { ...child, id: parent.id, display_key: parent.display_key, title: parent.content.title },
        ],
        next_cursor: parent.id,
      })
      .mockResolvedValue({ items: [], next_cursor: null });
    api.get.mockImplementation(async (_project: string, id: string) =>
      id === parent.id ? parent : issue,
    );
  });
  await openProperties(user);
  const select = await screen.findByRole('combobox', { name: 'Parent task' });
  await waitFor(() => expect((select as HTMLButtonElement).disabled).toBe(false));
  await user.click(select);
  const options = screen.getByRole('listbox', { name: 'Parent task' });
  expect(
    within(options).queryByRole('option', { name: 'ISS-42 · Investigate checkout' }),
  ).toBeNull();
  await user.click(within(options).getByRole('option', { name: 'ISS-40 · Parent task title' }));
  await user.click(screen.getByRole('button', { name: 'Next parent choices' }));
  await waitFor(() =>
    expect(api.list).toHaveBeenLastCalledWith('delivery', parent.id, expect.any(AbortSignal)),
  );
  expect(select.textContent).toContain('ISS-40 · Parent task title');
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  expect(screen.getByRole('dialog').textContent).toContain('ISS-40 · Parent task title');
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save properties' }),
  );
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(1));
  expect(api.editPlanning.mock.calls[0]![2].parent_id).toBe(parent.id);
});

it('retains readable catalog data but blocks review when refreshing its saved definition fails', async () => {
  const old = { ...structuredClone(catalog.definitions), version: 6 };
  const plan: IssuePlanning = {
    definition_version: 6,
    assignee: null,
    labels: [],
    due_date: null,
    parent_id: null,
    values: {},
  };
  const { api, queryClient } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(catalog);
    api.propertyDefinitions.mockResolvedValue(old);
    api.get.mockResolvedValue({ ...issue, planning: plan });
  });
  await screen.findByText('Saved definitions · version 6');
  api.propertyDefinitions.mockRejectedValue(
    new ManagementApiError({
      code: 'unavailable',
      message: 'Unavailable.',
      status: 503,
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await act(async () =>
    queryClient.refetchQueries({ queryKey: queryKeys.issuePropertyDefinitions('delivery', 6) }),
  );
  expect(await screen.findByText('Unavailable.')).toBeTruthy();
  expect(screen.getByText('Task type')).toBeTruthy();
  expect(
    (screen.getByRole('button', { name: 'Edit properties' }) as HTMLButtonElement).disabled,
  ).toBe(true);
});

it('reviews the exact versions and an unavailable assignment without calling it Unassigned', async () => {
  const user = userEvent.setup();
  setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(catalog);
    api.get.mockResolvedValue({
      ...issue,
      planning: {
        definition_version: 7,
        assignee: 'hidden-person',
        labels: [],
        due_date: null,
        parent_id: null,
        values: {},
      },
    });
  });
  await openProperties(user);
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain('Responsible party: Assigned person unavailable');
  expect(dialog.textContent).toContain('Based on task version 1');
  expect(dialog.textContent).toContain('Planning definitions · version 7');
  expect(dialog.textContent).not.toContain('hidden-person');
});

it.each(['task', 'ownership', 'directory'] as const)(
  'requires a new planning review when %s changes',
  async (change) => {
    const user = userEvent.setup();
    const { queryClient } = setup('/projects/delivery/issues/' + issue.id, (api) =>
      api.planningChoices.mockResolvedValue(catalog),
    );
    await openProperties(user);
    fireEvent.change(screen.getByLabelText('Component'), { target: { value: 'draft' } });
    await act(async () => {
      if (change === 'task')
        queryClient.setQueryData(queryKeys.issue('delivery', issue.id), { ...issue, version: 2 });
      else if (change === 'directory')
        queryClient.setQueryData(queryKeys.issuePropertyChoices('delivery'), {
          ...catalog,
          assignees: { available: true, items: [{ id: 'new-person', name: 'New person' }] },
        });
      else
        queryClient.setQueryData(queryKeys.issueTransitions('delivery', issue.id), {
          issue_version: 1,
          workflow_id: issue.workflow_id,
          workflow_version: 3,
          status_id: issue.status_id,
          field_policy: { source_owned: ['properties'], status_mapping_workflow: null, push: null },
          items: [],
        });
    });
    expect(
      await screen.findByText(
        'Planning changed. Reload properties and discard this draft before reviewing again.',
      ),
    ).toBeTruthy();
    expect((screen.getByLabelText('Component') as HTMLInputElement).value).toBe('draft');
    expect(
      (screen.getByRole('button', { name: 'Review properties' }) as HTMLButtonElement).disabled,
    ).toBe(true);
  },
);

it('retries an indeterminate planning command unchanged after a head and catalog refresh', async () => {
  const user = userEvent.setup();
  const { api, queryClient } = setup('/projects/delivery/issues/' + issue.id, (api) =>
    api.planningChoices.mockResolvedValue(catalog),
  );
  api.editPlanning.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'transport_failure',
      message: 'Disconnected.',
      status: null,
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await openProperties(user);
  fireEvent.change(screen.getByLabelText('Component'), { target: { value: 'frozen' } });
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save properties' }),
  );
  await screen.findByRole('button', { name: 'Retry same command' });
  await act(async () => {
    queryClient.setQueryData(queryKeys.issue('delivery', issue.id), { ...issue, version: 2 });
    queryClient.setQueryData(queryKeys.issuePropertyChoices('delivery'), {
      ...catalog,
      definitions: { ...catalog.definitions, version: 8 },
    });
  });
  await user.click(screen.getByRole('button', { name: 'Retry same command' }));
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(2));
  expect(api.editPlanning.mock.calls[1]).toEqual(api.editPlanning.mock.calls[0]);
  expect(api.editPlanning.mock.calls[1]![3]['If-Match']).toBe('"1"');
});

it('keeps local requirements usable when planning and relationship reads are forbidden', async () => {
  const denied = new ManagementApiError({
    code: 'forbidden',
    message: 'Access denied.',
    status: 403,
    requestId: null,
    retryAfterMilliseconds: null,
  });
  const user = userEvent.setup();
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockRejectedValue(denied);
    api.children.mockRejectedValue(denied);
  });
  await screen.findByRole('heading', { name: 'ISS-42 · Investigate checkout' });
  expect(await screen.findAllByText('Access denied.')).toHaveLength(2);
  expect(screen.queryByText('No visible sub-issues')).toBeNull();
  await user.click(screen.getByRole('button', { name: 'Edit requirements' }));
  expect(screen.getByRole('textbox', { name: 'Description' })).toBeTruthy();
  expect(api.create).not.toHaveBeenCalled();
  expect(api.editPlanning).not.toHaveBeenCalled();
});

it('discards the property draft on navigation and does not leak inaccessible parent names', async () => {
  const user = userEvent.setup();
  const { router } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(catalog);
    api.get.mockImplementation(async (_project: string, id: string) => {
      if (id !== issue.id)
        throw new ManagementApiError({
          code: 'not_found',
          message: 'Not found.',
          status: 404,
          requestId: null,
          retryAfterMilliseconds: null,
        });
      return {
        ...issue,
        planning: {
          definition_version: 7,
          assignee: null,
          labels: [],
          due_date: null,
          parent_id: parent.id,
          values: {},
        },
      };
    });
  });
  expect(await screen.findByText('Not found.')).toBeTruthy();
  expect(screen.queryByText(parent.content.title)).toBeNull();
  expect(screen.queryByText(parent.id)).toBeNull();
  await user.click(screen.getByRole('button', { name: 'Edit properties' }));
  fireEvent.change(screen.getByLabelText('Component'), { target: { value: 'draft' } });
  await act(async () => router.navigate('/projects/delivery/issues'));
  await screen.findByRole('heading', { name: 'Tasks' });
  await act(async () => router.navigate('/projects/delivery/issues/' + issue.id));
  await openProperties(user);
  expect((screen.getByLabelText('Component') as HTMLInputElement).value).toBe('');
  expect(localStorage.length).toBe(0);
  expect(sessionStorage.length).toBe(0);
});

it('keeps an unchanged number after its definition bounds tighten and validates a changed number', async () => {
  const user = userEvent.setup();
  const plan: IssuePlanning = {
    definition_version: 7,
    assignee: null,
    labels: [],
    due_date: null,
    parent_id: null,
    values: { size: { type: 'number', value: 9 } },
  };
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(catalog);
    api.get.mockResolvedValue({ ...issue, planning: plan });
  });
  await openProperties(user);
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  expect(screen.getByRole('dialog').textContent).toContain('Size estimate: 9');
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Abandon' }));
  fireEvent.change(screen.getByLabelText('Size estimate'), { target: { value: '0' } });
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  expect(screen.queryByRole('dialog')).toBeNull();
  fireEvent.change(screen.getByLabelText('Size estimate'), { target: { value: '7' } });
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save properties' }),
  );
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(1));
  expect(api.editPlanning.mock.calls[0]![2].values.size).toEqual({ type: 'number', value: 7 });
});

it('does not claim planning ownership when only requirements are source-managed', async () => {
  const user = userEvent.setup();
  setup('/projects/delivery/issues/' + issue.id, (api) =>
    api.transitions.mockResolvedValue({
      issue_version: 1,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: issue.status_id,
      field_policy: { source_owned: ['title'], status_mapping_workflow: null, push: null },
      items: [],
    }),
  );
  await openProperties(user);
  expect(
    within(screen.getByRole('region', { name: 'Edit properties' })).queryByText(
      /^Managed by source:/u,
    ),
  ).toBeNull();
});

it.each(['__proto__', 'constructor'])(
  'round-trips the supported custom key %s in the saved planning body',
  async (id) => {
    const user = userEvent.setup();
    const choices = issuePropertyChoicesFixture();
    choices.definitions.definition.properties = [
      { id, name: 'Custom note', retired: false, kind: { type: 'text', max_bytes: 64 } },
    ];
    const { api } = setup('/projects/delivery/issues/' + issue.id, (api) =>
      api.planningChoices.mockResolvedValue(choices),
    );
    await openProperties(user);
    fireEvent.change(screen.getByLabelText('Custom note'), { target: { value: 'scope' } });
    await user.click(screen.getByRole('button', { name: 'Review properties' }));
    expect(screen.getByRole('dialog').textContent).toContain('Custom note: scope');
    await user.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Save properties' }),
    );
    await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(1));
    const body = JSON.parse(JSON.stringify(api.editPlanning.mock.calls[0]![2]));
    expect(body.values).toEqual(
      JSON.parse(
        id === '__proto__'
          ? '{"__proto__":{"type":"text","value":"scope"}}'
          : '{"constructor":{"type":"text","value":"scope"}}',
      ),
    );
    expect(api.move).not.toHaveBeenCalled();
    expect(api.create).not.toHaveBeenCalled();
  },
);

it.each(['__proto__', 'constructor'])(
  'treats an absent inherited key %s as Not set in display and review',
  async (id) => {
    const user = userEvent.setup();
    const choices = issuePropertyChoicesFixture();
    choices.definitions.definition.properties = [
      { id, name: 'Custom note', retired: true, kind: { type: 'text', max_bytes: 64 } },
    ];
    setup('/projects/delivery/issues/' + issue.id, (api) => {
      api.planningChoices.mockResolvedValue(choices);
      api.get.mockResolvedValue({
        ...issue,
        planning: {
          definition_version: 7,
          assignee: null,
          labels: [],
          due_date: null,
          parent_id: null,
          values: {},
        },
      });
    });
    const term = await screen.findByText('Custom note', { selector: 'dt' });
    expect(term.nextElementSibling?.textContent).toBe('Not set');
    await openProperties(user);
    expect(screen.queryByRole('button', { name: 'Clear Custom note' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Review properties' }));
    expect(screen.getByRole('dialog').textContent).toContain('Custom note: Not set');
    expect(screen.getByRole('dialog').textContent).not.toContain('undefined');
  },
);

it('keeps a saved task draft mounted while its retained catalog loads after publication', async () => {
  const user = userEvent.setup();
  const saved: IssuePlanning = {
    definition_version: 7,
    assignee: null,
    labels: [],
    due_date: null,
    parent_id: null,
    values: { component: { type: 'text', value: 'saved' } },
  };
  let release!: (value: IssuePropertyChoices['definitions']) => void;
  const retained = new Promise<IssuePropertyChoices['definitions']>((resolve) => {
    release = resolve;
  });
  const { api, queryClient } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(catalog);
    api.get.mockResolvedValue({ ...issue, planning: saved });
    api.propertyDefinitions.mockReturnValue(retained);
  });
  await openProperties(user);
  fireEvent.change(screen.getByLabelText('Component'), { target: { value: 'draft' } });
  const advanced = { ...catalog, definitions: { ...catalog.definitions, version: 8 } };
  api.planningChoices.mockResolvedValue(advanced);
  await act(async () =>
    queryClient.setQueryData(queryKeys.issuePropertyChoices('delivery'), advanced),
  );
  await waitFor(() =>
    expect(api.propertyDefinitions).toHaveBeenCalledWith('delivery', 7, expect.any(AbortSignal)),
  );
  expect((screen.getByLabelText('Component') as HTMLInputElement).value).toBe('draft');
  expect(
    (screen.getByRole('button', { name: 'Review properties' }) as HTMLButtonElement).disabled,
  ).toBe(true);
  await act(async () => release(catalog.definitions));
  await screen.findByText('Saved definitions · version 7');
  expect((screen.getByLabelText('Component') as HTMLInputElement).value).toBe('draft');
  expect(screen.getByRole('alert').textContent).toContain('Planning changed.');
  expect(api.editPlanning).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: 'Reload properties and discard draft' }));
  expect((screen.getByLabelText('Component') as HTMLInputElement).value).toBe('saved');
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save properties' }),
  );
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(1));
  expect(api.editPlanning.mock.calls[0]![2]).toEqual({ ...saved, definition_version: 8 });
  expect(api.move).not.toHaveBeenCalled();
  expect(localStorage.length).toBe(0);
  expect(sessionStorage.length).toBe(0);
});

it('keeps an indeterminate command retryable when its saved catalog is loading or unavailable', async () => {
  const user = userEvent.setup();
  const saved: IssuePlanning = {
    definition_version: 7,
    assignee: null,
    labels: [],
    due_date: null,
    parent_id: null,
    values: { component: { type: 'text', value: 'saved' } },
  };
  let reject!: (error: Error) => void;
  const retained = new Promise<IssuePropertyChoices['definitions']>((_resolve, fail) => {
    reject = fail;
  });
  const { api, queryClient } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(catalog);
    api.get.mockResolvedValue({ ...issue, planning: saved });
    api.propertyDefinitions.mockReturnValue(retained);
    api.editPlanning.mockRejectedValueOnce(
      new ManagementApiError({
        code: 'transport_failure',
        message: 'Disconnected.',
        status: null,
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    );
  });
  await openProperties(user);
  fireEvent.change(screen.getByLabelText('Component'), { target: { value: 'frozen' } });
  await user.click(screen.getByRole('button', { name: 'Review properties' }));
  await user.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save properties' }),
  );
  await screen.findByRole('button', { name: 'Retry same command' });
  const advanced = { ...catalog, definitions: { ...catalog.definitions, version: 8 } };
  api.planningChoices.mockResolvedValue(advanced);
  await act(async () =>
    queryClient.setQueryData(queryKeys.issuePropertyChoices('delivery'), advanced),
  );
  await waitFor(() => expect(api.propertyDefinitions).toHaveBeenCalled());
  expect(
    (screen.getByRole('button', { name: 'Retry same command' }) as HTMLButtonElement).disabled,
  ).toBe(false);
  await act(async () =>
    reject(
      new ManagementApiError({
        code: 'unavailable',
        message: 'Saved definitions unavailable.',
        status: 503,
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    ),
  );
  await screen.findByText('Saved definitions unavailable.');
  expect(screen.getByRole('dialog').textContent).toContain('Component: frozen');
  await user.click(screen.getByRole('button', { name: 'Retry same command' }));
  await waitFor(() => expect(api.editPlanning).toHaveBeenCalledTimes(2));
  expect(api.editPlanning.mock.calls[1]).toEqual(api.editPlanning.mock.calls[0]);
  expect(api.editPlanning.mock.calls[1]![3]['If-Match']).toBe('"1"');
  expect(api.editPlanning.mock.calls[1]![2].definition_version).toBe(7);
  expect(localStorage.length).toBe(0);
  expect(sessionStorage.length).toBe(0);
});

it('returns to the first sub-issue page after its continuation becomes invalid', async () => {
  const user = userEvent.setup();
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.children
      .mockResolvedValueOnce({ items: [child], next_cursor: child.id })
      .mockRejectedValueOnce(
        new ManagementApiError({
          code: 'invalid_request',
          message: 'Invalid cursor.',
          status: 400,
          requestId: null,
          retryAfterMilliseconds: null,
        }),
      )
      .mockResolvedValue({
        items: [{ ...child, display_key: 'ISS-44', title: 'Updated sub-issue' }],
        next_cursor: null,
      });
  });
  await screen.findByRole('link', { name: 'ISS-43 · Child task title' });
  await user.click(screen.getByRole('button', { name: 'Next sub-issues' }));
  await screen.findByText('Invalid cursor.');
  const first = screen.getByRole('button', { name: 'First sub-issues' });
  expect((first as HTMLButtonElement).disabled).toBe(false);
  await user.click(first);
  expect(await screen.findByRole('link', { name: 'ISS-44 · Updated sub-issue' })).toBeTruthy();
  expect(api.children).toHaveBeenLastCalledWith(
    'delivery',
    issue.id,
    null,
    expect.any(AbortSignal),
  );
  expect(screen.queryByText('Invalid cursor.')).toBeNull();
  expect(api.editPlanning).not.toHaveBeenCalled();
  expect(api.move).not.toHaveBeenCalled();
});
