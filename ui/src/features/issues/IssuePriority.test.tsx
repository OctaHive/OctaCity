// @vitest-environment jsdom
import { act, cleanup, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { issue, setup } from '../../../tests/fixtures/issueTestSupport';
import { issuePropertyChoicesFixture } from '../../../tests/fixtures/issues';
import { queryKeys } from '../../app/query';

afterEach(() => {
  cleanup();
  localStorage.clear();
  sessionStorage.clear();
});

it('blocks an open priority confirmation when the field becomes source-owned', async () => {
  const user = userEvent.setup();
  const { queryClient, api } = setup('/projects/delivery/issues/' + issue.id);
  await user.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  await user.click(screen.getByRole('combobox', { name: 'Priority' }));
  await user.click(screen.getByRole('option', { name: 'High' }));
  await user.click(screen.getByRole('button', { name: 'Review task' }));
  await act(async () =>
    queryClient.setQueryData(queryKeys.issueTransitions('delivery', issue.id), {
      issue_version: 1,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: issue.status_id,
      field_policy: { source_owned: ['priority'], status_mapping_workflow: null, push: null },
      items: [],
    }),
  );
  await waitFor(() =>
    expect((screen.getByRole('combobox', { name: 'Priority' }) as HTMLButtonElement).disabled).toBe(
      true,
    ),
  );
  expect(
    (
      within(screen.getByRole('dialog')).getByRole('button', {
        name: 'Save task',
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  expect(api.edit).not.toHaveBeenCalled();
});

it('keeps the reviewed priority label frozen when a new Project catalog arrives', async () => {
  const user = userEvent.setup();
  const catalog = issuePropertyChoicesFixture();
  catalog.definitions.definition.priorities = [
    { id: 'urgent', name: 'Urgent incident', retired: false },
  ];
  const { queryClient, api } = setup('/projects/delivery/issues/' + issue.id, (api) =>
    api.planningChoices.mockResolvedValue(catalog),
  );
  await user.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  await user.click(screen.getByRole('combobox', { name: 'Priority' }));
  await user.click(screen.getByRole('option', { name: 'Urgent incident' }));
  await user.click(screen.getByRole('button', { name: 'Review task' }));
  const changed = structuredClone(catalog);
  changed.definitions.version = 8;
  changed.definitions.definition.priorities![0]!.name = 'Emergency';
  await act(async () =>
    queryClient.setQueryData(queryKeys.issuePropertyChoices('delivery'), changed),
  );
  const dialog = screen.getByRole('dialog');
  await waitFor(() =>
    expect(
      (within(dialog).getByRole('button', { name: 'Save task' }) as HTMLButtonElement).disabled,
    ).toBe(true),
  );
  expect(dialog.textContent).toContain('Priority: Urgent incident');
  expect(dialog.textContent).not.toContain('Emergency');
  expect(
    (within(dialog).getByRole('button', { name: 'Save task' }) as HTMLButtonElement).disabled,
  ).toBe(true);
  expect(api.edit).not.toHaveBeenCalled();
});

it.each([
  ['Blocker', 'blocker'],
  ['High', 'high'],
  ['Medium', 'medium'],
  ['Low', 'low'],
] as const)(
  'creates a task with %s priority and displays the same name in its card',
  async (label, priority) => {
    const created = { ...issue, content: { ...issue.content, priority } };
    const { api } = setup('/projects/delivery/issues', (api) => {
      api.create.mockResolvedValue({ disposition: 'applied', resource: created });
      api.get.mockResolvedValue(created);
    });
    await userEvent.click(await screen.findByRole('button', { name: 'Create task' }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Title' }), issue.content.title);
    const select = screen.getByRole('combobox', { name: 'Priority' });
    expect(screen.queryByRole('spinbutton', { name: 'Priority' })).toBeNull();
    await userEvent.click(select);
    expect(screen.getAllByRole('option').map((option) => option.textContent)).toEqual([
      'Not set',
      'Blocker',
      'High',
      'Medium',
      'Low',
    ]);
    await userEvent.click(screen.getByRole('option', { name: label }));
    await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
    const dialog = screen.getByRole('dialog');
    expect(dialog.textContent).toContain(`Priority: ${label}`);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create task' }));
    await waitFor(() =>
      expect(api.create).toHaveBeenCalledWith(
        'delivery',
        { title: issue.content.title, description: '', acceptance: null, priority },
        { 'Idempotency-Key': expect.any(String) },
      ),
    );
    await userEvent.click(within(dialog).getByRole('link', { name: 'Open ISS-42' }));
    await screen.findByRole('heading', { name: `ISS-42 · ${issue.content.title}` });
    const properties = screen.getByRole('complementary', { name: 'Properties' });
    expect(
      within(properties).getByText('Priority', { exact: true }).nextElementSibling?.textContent,
    ).toBe(label);
  },
);

it('edits priority using the Project catalog rather than legacy numeric levels', async () => {
  const user = userEvent.setup();
  const choices = issuePropertyChoicesFixture();
  choices.definitions.definition.priorities = [
    { id: 'critical', name: 'Critical incident', retired: false },
  ];
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.planningChoices.mockResolvedValue(choices);
  });
  await user.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  await user.click(screen.getByRole('combobox', { name: 'Priority' }));
  expect(screen.getAllByRole('option').map((el) => el.textContent)).toEqual([
    'Not set',
    'Critical incident',
  ]);
  await user.click(screen.getByRole('option', { name: 'Critical incident' }));
  await user.click(screen.getByRole('button', { name: 'Review task' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Save task' }));
  await waitFor(() =>
    expect(api.edit).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      { ...issue.content, priority: 'critical' },
      expect.objectContaining({ 'If-Match': '"1"' }),
    ),
  );
});

it.each([8, 255])('preserves legacy priority %i through an unrelated edit', async (priority) => {
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.get.mockResolvedValue({ ...issue, content: { ...issue.content, priority } });
  });
  await screen.findByRole('heading', { name: `ISS-42 · ${issue.content.title}` });
  const properties = screen.getByRole('complementary', { name: 'Properties' });
  expect(
    within(properties).getByText('Priority', { exact: true }).nextElementSibling?.textContent,
  ).toBe(`Existing priority (${priority})`);
  await userEvent.click(screen.getByRole('button', { name: 'Edit requirements' }));
  const select = screen.getByRole('combobox', { name: 'Priority' });
  expect(select.textContent).toBe(`Existing priority (${priority})`);
  await userEvent.click(select);
  await userEvent.click(screen.getByRole('option', { name: `Existing priority (${priority})` }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Description' }), 'Updated text');
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain(`Priority: Existing priority (${priority})`);
  await userEvent.click(within(dialog).getByRole('button', { name: 'Save task' }));
  await waitFor(() =>
    expect(api.edit).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      { ...issue.content, description: 'Updated text', priority },
      expect.objectContaining({ 'If-Match': '"1"' }),
    ),
  );
});

it.each(['urgent', '2'])(
  'preserves Project priority choice %s through an unrelated edit',
  async (priority) => {
    const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
      api.get.mockResolvedValue({ ...issue, content: { ...issue.content, priority } });
    });
    await screen.findByRole('heading', { name: `ISS-42 · ${issue.content.title}` });
    await userEvent.click(screen.getByRole('button', { name: 'Edit requirements' }));
    const select = screen.getByRole('combobox', { name: 'Priority' });
    expect(select.textContent).toBe(priority);
    await userEvent.click(select);
    await userEvent.click(screen.getByRole('option', { name: priority }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Description' }), 'Updated text');
    await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
    await userEvent.click(
      within(screen.getByRole('dialog')).getByRole('button', { name: 'Save task' }),
    );
    await waitFor(() =>
      expect(api.edit).toHaveBeenCalledWith(
        'delivery',
        issue.id,
        { ...issue.content, description: 'Updated text', priority },
        expect.objectContaining({ 'If-Match': '"1"' }),
      ),
    );
  },
);
