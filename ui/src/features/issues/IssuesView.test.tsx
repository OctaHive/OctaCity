// @vitest-environment jsdom
import { cleanup, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { issue, workflow, setup } from '../../../tests/fixtures/issueTestSupport';
afterEach(() => {
  cleanup();
  localStorage.clear();
  sessionStorage.clear();
});
it('creates an incomplete manual task without integration, repository or Factory setup and opens its human key', async () => {
  const { api, router } = setup();
  expect(await screen.findByText('No tasks yet. Create your first task here.')).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Create task' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Title' }), 'Investigate checkout');
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  const dialog = screen.getByRole('dialog');
  expect(api.create).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Create task' }));
  await waitFor(() =>
    expect(api.create).toHaveBeenCalledWith(
      'delivery',
      { title: 'Investigate checkout', description: '', acceptance: null, priority: null },
      { 'Idempotency-Key': expect.any(String) },
    ),
  );
  await userEvent.click(within(dialog).getByRole('link', { name: 'Open ISS-42' }));
  expect(
    await screen.findByRole('heading', { name: 'ISS-42 · Investigate checkout', level: 1 }),
  ).toBeTruthy();
  expect(router.state.location.pathname).toBe('/projects/delivery/issues/' + issue.id);
  expect(screen.getByRole('heading', { name: 'Description' })).toBeTruthy();
  expect(screen.getByRole('heading', { name: 'Activity preview' })).toBeTruthy();
});
it('renders the pinned workflow status, base properties and a real activity preview without treating Done as execution success', async () => {
  const { workflows } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.get.mockResolvedValue({ ...issue, status_id: workflow.definition.statuses[1].id });
    api.history.mockResolvedValue({
      items: [
        {
          issue_id: issue.id,
          version: 1,
          workflow_id: issue.workflow_id,
          workflow_version: 3,
          from: null,
          to: workflow.definition.statuses[1].id,
          transition_id: null,
          previous_workflow: null,
          origin: 'manual',
          reason: null,
          actor: { kind: 'unauthenticated_management', identity: null },
          occurred_at: 1700000000000,
        },
      ],
      next_cursor: null,
    });
  });
  await screen.findByRole('heading', { name: 'ISS-42 · Investigate checkout', level: 1 });
  expect(await screen.findByText('Business status: Done')).toBeTruthy();
  expect(workflows.get).toHaveBeenCalledWith(
    'delivery',
    issue.workflow_id,
    3,
    expect.any(AbortSignal),
  );
  expect(screen.getByRole('button', { name: 'Start pipeline' })).toHaveProperty('disabled', false);
  expect(
    screen.getByText('Task status does not approve a result or permit delivery.'),
  ).toBeTruthy();
  expect(screen.getByRole('link', { name: 'Overview', current: 'page' })).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Properties' }));
  expect(screen.getByRole('complementary', { name: 'Properties' }).textContent).toContain(
    'Not set',
  );
  const preview = screen.getByRole('region', { name: 'Activity preview' });
  expect(await within(preview).findByText('Unauthenticated management')).toBeTruthy();
  await userEvent.click(screen.getByRole('link', { name: 'Activity' }));
  expect(screen.getByRole('region', { name: 'Status history' })).toBeTruthy();
  expect(screen.queryByText('Execution accepted')).toBeNull();
});
it('preserves an edit draft on stale conflict and requires a new review against the refreshed Issue version', async () => {
  const { ManagementApiError } = await import('../../api/client');
  const { api, router } = setup('/projects/delivery/issues/' + issue.id);
  api.edit.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'conflict',
      message: 'Task changed.',
      status: 409,
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Description' }),
    'private requirements',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save task' }),
  );
  await screen.findByRole('button', { name: 'Refresh current data' });
  api.get.mockResolvedValue({
    ...issue,
    version: 4,
    content: { ...issue.content, description: 'Another edit' },
  });
  api.transitions.mockResolvedValue({
    ...(await api.transitions.mock.results[0]!.value),
    issue_version: 4,
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(screen.getByRole('textbox', { name: 'Description' })).toHaveProperty(
    'value',
    'private requirements',
  );
  expect(screen.queryByRole('button', { name: 'Review task' })).toBeNull();
  const reconcile = screen.getByRole('button', { name: 'Use latest version and keep my edits' });
  expect(document.activeElement).toBe(reconcile);
  await userEvent.click(reconcile);
  expect(api.edit).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save task' }),
  );
  await waitFor(() => expect(api.edit).toHaveBeenCalledTimes(2));
  expect(api.edit.mock.calls[0]![3]['If-Match']).toBe('"1"');
  expect(api.edit.mock.calls[1]![3]['If-Match']).toBe('"4"');
  expect(api.edit.mock.calls[1]![3]['Idempotency-Key']).not.toBe(
    api.edit.mock.calls[0]![3]['Idempotency-Key'],
  );
  expect(
    JSON.stringify(localStorage) + JSON.stringify(sessionStorage) + router.state.location.search,
  ).not.toContain('private requirements');
});
it('offers only the current server choices, explains blocked moves and freezes an explicit organizational move for safe retry', async () => {
  const { ManagementApiError } = await import('../../api/client');
  const edge = {
    id: '50000000-0000-4000-8000-000000000001',
    name: 'Finish triage',
    from: issue.status_id,
    to: workflow.definition.statuses[1].id,
    origins: ['manual'],
    evidence: [],
  };
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.transitions.mockResolvedValue({
      issue_version: 1,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: issue.status_id,
      field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
      items: [
        { transition: edge, available: true, reason: null },
        {
          transition: {
            ...edge,
            id: '50000000-0000-4000-8000-000000000002',
            name: 'Evidence gate',
          },
          available: false,
          reason: 'evidence_required',
        },
      ],
    });
    api.move.mockRejectedValueOnce(
      new ManagementApiError({
        code: 'transport_failure',
        message: 'Disconnected.',
        status: null,
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    );
  });
  await userEvent.click(await screen.findByRole('link', { name: 'Actions' }));
  expect(await screen.findByRole('button', { name: 'Evidence gate' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.getByText('Current execution evidence is required.')).toBeTruthy();
  await userEvent.type(screen.getByRole('textbox', { name: 'Reason (optional)' }), 'reviewed');
  const move = screen.getByRole('button', { name: 'Finish triage' });
  await userEvent.click(move);
  expect(api.move).not.toHaveBeenCalled();
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain('New → Done');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Move task' }));
  await userEvent.click(await within(dialog).findByRole('button', { name: 'Retry same command' }));
  await waitFor(() => expect(api.move).toHaveBeenCalledTimes(2));
  expect(api.move.mock.calls[1]).toEqual(api.move.mock.calls[0]);
  expect(api.move.mock.calls[0]).toEqual([
    'delivery',
    issue.id,
    {
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      expected_status_id: issue.status_id,
      transition_id: edge.id,
      reason: 'reviewed',
    },
    expect.objectContaining({ 'If-Match': '"1"' }),
  ]);
  await userEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
  expect(document.activeElement).toBe(move);
});
it('adopts an exact discovered workflow only after explicit status correspondence and version review', async () => {
  const { api, workflows } = setup('/projects/delivery/issues/' + issue.id + '?section=actions');
  workflows.get.mockImplementation(async (_p: string, _id: string, version: number) =>
    version === 4
      ? {
          ...workflow,
          version: 4,
          definition: {
            ...workflow.definition,
            name: 'Delivery policy',
            statuses: workflow.definition.statuses.map((s: { id: string; name: string }) => ({
              ...s,
              name: 'New ' + s.name,
            })),
          },
        }
      : workflow,
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Change task workflow' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Target workflow' }));
  await userEvent.click(screen.getByRole('option', { name: 'Delivery policy' }));
  await userEvent.click(screen.getByRole('button', { name: 'Load workflow version' }));
  const mapping = await screen.findByRole('combobox', { name: 'Current status New →' });
  expect(api.adopt).not.toHaveBeenCalled();
  expect(screen.getByRole('button', { name: 'Review workflow change' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(mapping);
  await userEvent.click(screen.getByRole('option', { name: 'New Done' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review workflow change' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain('Delivery policy');
  expect(dialog.textContent).toContain('New → New Done');
  expect(dialog.textContent).toContain('Version 4');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Apply workflow change' }));
  await waitFor(() =>
    expect(api.adopt).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      {
        expected_workflow_id: issue.workflow_id,
        expected_workflow_version: 3,
        expected_status_id: issue.status_id,
        target_workflow_id: workflow.id,
        target_workflow_version: 4,
        status_correspondence: { [issue.status_id]: workflow.definition.statuses[1].id },
        expected_field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
      },
      expect.objectContaining({ 'If-Match': '"1"' }),
    ),
  );
});
it('opens an exact requirements revision as read-only history and discards unsaved drafts when leaving the card', async () => {
  const { api, router } = setup('/projects/delivery/issues/' + issue.id);
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Description' }),
    'private discarded draft',
  );
  await userEvent.click(screen.getByRole('link', { name: 'Activity' }));
  await userEvent.click(await screen.findByRole('link', { name: 'Read task version 1' }));
  expect(await screen.findByText('Original requirements')).toBeTruthy();
  expect(api.revision).toHaveBeenCalledWith('delivery', issue.id, 1, expect.any(AbortSignal));
  expect(router.state.location.search).toContain('revision=1');
  expect(screen.getByText('Historical requirements · Version 1')).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Save task' })).toBeNull();
  await router.navigate('/projects/delivery/issues');
  await screen.findByText('No tasks yet. Create your first task here.');
  await router.navigate('/projects/delivery/issues/' + issue.id);
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  expect(screen.getByRole('textbox', { name: 'Description' })).toHaveProperty('value', '');
});
it('uses generated UTF-8 form limits, focuses the draft, and explains source-owned fields without disabling manual planning', async () => {
  setup();
  await userEvent.click(await screen.findByRole('button', { name: 'Create task' }));
  const title = screen.getByRole('textbox', { name: 'Title' });
  expect(document.activeElement).toBe(title);
  await userEvent.type(title, 'я'.repeat(65));
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  expect(await screen.findByRole('alert')).toHaveProperty(
    'textContent',
    expect.stringContaining('128 UTF-8 bytes'),
  );
  expect(screen.queryByRole('dialog')).toBeNull();
});
it('keeps source-owned text read-only with an explanation and allows unowned manual edits', async () => {
  setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.transitions.mockResolvedValue({
      issue_version: 1,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: issue.status_id,
      field_policy: {
        source_owned: ['title', 'description'],
        status_mapping_workflow: null,
        push: null,
      },
      items: [],
    });
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  expect(screen.getByRole('textbox', { name: 'Title' })).toHaveProperty('disabled', true);
  expect(screen.getByRole('textbox', { name: 'Description' })).toHaveProperty('disabled', true);
  expect(screen.getByText('Managed by source: Title, Description')).toBeTruthy();
  expect(screen.getByRole('textbox', { name: 'Acceptance criteria' })).toHaveProperty(
    'disabled',
    false,
  );
});
it.each(['en', 'ru'] as const)(
  'keeps manual requirements visible and explains unavailable authority in %s',
  async (language) => {
    const { DEFAULT_PREFERENCES, PREFERENCES_KEY } =
      await import('../../app/presentation/preferences');
    const { ManagementApiError } = await import('../../api/client');
    localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ ...DEFAULT_PREFERENCES, language }));
    const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
      api.transitions.mockRejectedValue(
        new ManagementApiError({
          code: 'forbidden',
          message: 'Permission required.',
          status: 403,
          requestId: null,
          retryAfterMilliseconds: null,
        }),
      );
    });
    expect(
      await screen.findByRole('heading', { name: 'ISS-42 · Investigate checkout' }),
    ).toBeTruthy();
    expect(await screen.findByRole('alert')).toHaveProperty(
      'textContent',
      expect.stringContaining('forbidden'),
    );
    expect(
      screen.getByRole('button', {
        name: language === 'en' ? 'Edit requirements' : 'Редактировать требования',
      }),
    ).toHaveProperty('disabled', true);
    expect(api.edit).not.toHaveBeenCalled();
  },
);
it('retains visible history on refresh failure and exposes a fresh review entry without treating failure as empty', async () => {
  const { ManagementApiError } = await import('../../api/client');
  const { api, queryClient } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.history.mockResolvedValue({
      items: [
        {
          issue_id: issue.id,
          version: 1,
          workflow_id: issue.workflow_id,
          workflow_version: 3,
          from: null,
          to: issue.status_id,
          transition_id: null,
          previous_workflow: null,
          origin: 'manual',
          reason: 'Retained history',
          actor: { kind: 'worker', identity: 'local-service' },
          occurred_at: 1700000000000,
        },
      ],
      next_cursor: null,
    });
  });
  await screen.findByText('Retained history');
  api.history.mockRejectedValue(
    new ManagementApiError({
      code: 'unavailable',
      message: 'Unavailable.',
      status: 503,
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await queryClient.refetchQueries({ queryKey: ['issues', 'delivery', issue.id, 'history'] });
  expect(screen.getByText('Retained history')).toBeTruthy();
  expect(
    await screen.findByText('Refresh failed. Showing the last loaded Activity preview.'),
  ).toBeTruthy();
  expect(screen.queryByText('No status history reported.')).toBeNull();
  expect(screen.getByRole('button', { name: 'Refresh task' })).toBeTruthy();
});
it('bounds the Activity preview to recent task versions and links older history instead of claiming it is empty', async () => {
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.get.mockResolvedValue({ ...issue, version: 200 });
    api.transitions.mockResolvedValue({
      issue_version: 200,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: issue.status_id,
      field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
      items: [],
    });
  });
  const preview = await screen.findByRole('region', { name: 'Activity preview' });
  expect(
    await within(preview).findByRole('link', { name: 'Open full status history' }),
  ).toBeTruthy();
  expect(api.history).toHaveBeenCalledWith('delivery', issue.id, 150, expect.any(AbortSignal));
  expect(preview.textContent).toContain('Status changes in the last 50 task versions.');
  expect(preview.textContent).not.toContain('No status history reported.');
});
it('shows the exact requirements and Issue revision in confirmation before saving', async () => {
  setup('/projects/delivery/issues/' + issue.id);
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Description' }),
    'Review these requirements',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  const review = screen.getByRole('dialog');
  expect(review.textContent).toContain('Investigate checkout');
  expect(review.textContent).toContain('Review these requirements');
  expect(review.textContent).toContain('Based on task version 1');
});
it('validates a move reason using generated bytes before opening its confirmation', async () => {
  setup('/projects/delivery/issues/' + issue.id + '?section=actions', (api) => {
    api.transitions.mockResolvedValue({
      issue_version: 1,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: issue.status_id,
      field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
      items: [
        {
          transition: {
            id: '50000000-0000-4000-8000-000000000001',
            name: 'Triage',
            from: issue.status_id,
            to: issue.status_id,
            origins: ['manual'],
            evidence: [],
          },
          available: true,
          reason: null,
        },
      ],
    });
  });
  await userEvent.type(
    await screen.findByRole('textbox', { name: 'Reason (optional)' }),
    'я'.repeat(65),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Triage' }));
  expect(await screen.findByRole('alert')).toHaveProperty(
    'textContent',
    expect.stringContaining('128 UTF-8 bytes'),
  );
  expect(screen.queryByRole('dialog')).toBeNull();
});
it('keeps the owning Project name in the card breadcrumb without exposing opaque identities as primary labels', async () => {
  setup('/projects/delivery/issues/' + issue.id);
  const trail = await screen.findByRole('navigation', { name: 'Task breadcrumb' });
  expect((await within(trail).findByRole('link', { name: 'Delivery' })).getAttribute('href')).toBe(
    '/projects/delivery',
  );
  expect(within(trail).getByRole('link', { name: 'Tasks' })).toBeTruthy();
});
it('returns keyboard focus to available actions when a successful move removes its original trigger', async () => {
  const edge = {
    id: '50000000-0000-4000-8000-000000000001',
    name: 'Finish',
    from: issue.status_id,
    to: workflow.definition.statuses[1].id,
    origins: ['manual'],
    evidence: [],
  };
  const { api } = setup('/projects/delivery/issues/' + issue.id + '?section=actions', (api) => {
    api.transitions.mockResolvedValue({
      issue_version: 1,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: issue.status_id,
      field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
      items: [{ transition: edge, available: true, reason: null }],
    });
  });
  api.move.mockImplementation(async () => {
    const moved = { ...issue, version: 2, status_id: edge.to };
    api.get.mockResolvedValue(moved);
    api.transitions.mockResolvedValue({
      issue_version: 2,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: edge.to,
      field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
      items: [],
    });
    return { disposition: 'applied', resource: moved, transition: {} };
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Finish' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Move task' }),
  );
  await userEvent.click(
    await within(screen.getByRole('dialog')).findByRole('button', { name: 'Close' }),
  );
  expect(document.activeElement).toBe(
    screen.getByRole('heading', { name: 'Available status moves' }),
  );
});
it('requires fresh correspondence and restores focus after a stale workflow adoption', async () => {
  const { ManagementApiError } = await import('../../api/client');
  const { api, workflows } = setup('/projects/delivery/issues/' + issue.id + '?section=actions');
  api.adopt.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'conflict',
      message: 'Task changed.',
      status: 409,
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  workflows.get.mockImplementation(async (_p: string, _id: string, version: number) => ({
    ...workflow,
    version,
  }));
  await userEvent.click(await screen.findByRole('button', { name: 'Change task workflow' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Target workflow' }));
  await userEvent.click(screen.getByRole('option', { name: 'Delivery policy' }));
  await userEvent.click(screen.getByRole('button', { name: 'Load workflow version' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Current status New →' }));
  await userEvent.click(screen.getByRole('option', { name: 'Done' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review workflow change' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Apply workflow change' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(api.adopt).toHaveBeenCalledTimes(1);
  expect(screen.getByRole('button', { name: 'Review workflow change' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Abandon' }));
});
it('keeps a successful edit receipt and returns focus to the editor when confirmation closes', async () => {
  const { api } = setup('/projects/delivery/issues/' + issue.id);
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  const review = screen.getByRole('button', { name: 'Review task' });
  await userEvent.click(review);
  api.get.mockResolvedValue({ ...issue, version: 5 });
  api.transitions.mockResolvedValue({
    ...(await api.transitions.mock.results[0]!.value),
    issue_version: 5,
  });
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save task' }),
  );
  expect(await within(screen.getByRole('dialog')).findByText('Saved task version 5.')).toBeTruthy();
  await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }));
  expect(document.activeElement).toBe(review);
});

it('keeps the draft baseline fixed and reconciles untouched remote fields only after explicit review', async () => {
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.get.mockResolvedValue({
      ...issue,
      content: { ...issue.content, description: 'Initial description' },
    });
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Title' }), ' local edit');
  const choices = await api.transitions.mock.results[0]!.value;
  api.get.mockResolvedValue({
    ...issue,
    version: 2,
    content: { ...issue.content, description: 'New description from another operator' },
  });
  api.transitions.mockResolvedValue({ ...choices, issue_version: 2 });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh task' }));
  await screen.findAllByText('New description from another operator');
  expect(screen.getByText('Based on task version 1')).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Review task' })).toBeNull();
  expect(api.edit).not.toHaveBeenCalled();
  await userEvent.click(
    screen.getByRole('button', { name: 'Use latest version and keep my edits' }),
  );
  expect(screen.getByRole('textbox', { name: 'Title' })).toHaveProperty(
    'value',
    'Investigate checkout local edit',
  );
  expect(screen.getByRole('textbox', { name: 'Description' })).toHaveProperty(
    'value',
    'New description from another operator',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save task' }),
  );
  await waitFor(() => expect(api.edit).toHaveBeenCalledTimes(1));
  expect(api.edit.mock.calls[0]![2].description).toBe('New description from another operator');
  expect(api.edit.mock.calls[0]![3]['If-Match']).toBe('"2"');
});

it('refreshes source-owned fields during explicit reconciliation while retaining local editable changes', async () => {
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.transitions.mockResolvedValue({
      issue_version: 1,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: issue.status_id,
      field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
      items: [],
    });
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Title' }), ' no longer editable');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Acceptance criteria' }),
    'Keep my acceptance',
  );
  await userEvent.type(screen.getByRole('spinbutton', { name: 'Priority' }), '8');
  const choices = await api.transitions.mock.results[0]!.value;
  api.get.mockResolvedValue({
    ...issue,
    version: 2,
    content: {
      ...issue.content,
      title: 'Fresh source title',
      description: 'Fresh source description',
    },
  });
  api.transitions.mockResolvedValue({
    ...choices,
    issue_version: 2,
    field_policy: { ...choices.field_policy, source_owned: ['title', 'description'] },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh task' }));
  await userEvent.click(
    await screen.findByRole('button', { name: 'Use latest version and keep my edits' }),
  );
  expect(screen.getByRole('textbox', { name: 'Title' })).toHaveProperty('disabled', true);
  expect(screen.getByRole('textbox', { name: 'Title' })).toHaveProperty(
    'value',
    'Fresh source title',
  );
  expect(screen.getByRole('textbox', { name: 'Description' })).toHaveProperty(
    'value',
    'Fresh source description',
  );
  expect(screen.getByRole('textbox', { name: 'Acceptance criteria' })).toHaveProperty(
    'value',
    'Keep my acceptance',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save task' }),
  );
  await waitFor(() => expect(api.edit).toHaveBeenCalledTimes(1));
  expect(api.edit.mock.calls[0]![2]).toEqual({
    title: 'Fresh source title',
    description: 'Fresh source description',
    acceptance: 'Keep my acceptance',
    priority: 8,
  });
});

it('refreshes the mounted Activity preview together with an externally moved task head', async () => {
  const entry = {
    issue_id: issue.id,
    version: 1,
    workflow_id: issue.workflow_id,
    workflow_version: 3,
    from: null,
    to: issue.status_id,
    transition_id: null,
    previous_workflow: null,
    origin: 'manual',
    reason: 'Old creation history',
    actor: { kind: 'worker', identity: null },
    occurred_at: 1700000000000,
  };
  const { api } = setup('/projects/delivery/issues/' + issue.id, (api) => {
    api.history.mockResolvedValue({ items: [entry], next_cursor: null });
  });
  await screen.findByText('Old creation history');
  const status = workflow.definition.statuses[1].id;
  api.get.mockResolvedValue({ ...issue, version: 2, status_id: status });
  api.transitions.mockResolvedValue({
    ...(await api.transitions.mock.results[0]!.value),
    issue_version: 2,
    status_id: status,
  });
  api.history.mockResolvedValue({
    items: [
      entry,
      { ...entry, version: 2, from: issue.status_id, to: status, reason: 'New external move' },
    ],
    next_cursor: null,
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh task' }));
  await screen.findByText('Business status: Done');
  expect(
    await within(screen.getByRole('region', { name: 'Activity preview' })).findByText(
      'New external move',
    ),
  ).toBeTruthy();
});

it('refreshes revision headers without refetching or replacing selected immutable requirements', async () => {
  const { api, router } = setup(
    '/projects/delivery/issues/' + issue.id + '?section=activity&revision=1',
  );
  await screen.findByText('Original requirements');
  const revisions = await api.revisions.mock.results[0]!.value;
  api.get.mockResolvedValue({
    ...issue,
    version: 2,
    content: { ...issue.content, description: 'New current requirements' },
  });
  api.transitions.mockResolvedValue({
    ...(await api.transitions.mock.results[0]!.value),
    issue_version: 2,
  });
  api.revisions.mockResolvedValue({
    items: [
      ...revisions.items,
      { version: 2, applicability_version: 2, updated_at_unix_ms: 1700000000100 },
    ],
    next_cursor: null,
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh task' }));
  expect(await screen.findByRole('link', { name: 'Read task version 2' })).toBeTruthy();
  expect(screen.getByText('Original requirements')).toBeTruthy();
  expect(router.state.location.search).toBe('?section=activity&revision=1');
  expect(api.revision).toHaveBeenCalledTimes(1);
});

it('retries the frozen edit after a head refresh without rebasing an indeterminate command', async () => {
  const { ManagementApiError } = await import('../../api/client');
  const { api, queryClient } = setup('/projects/delivery/issues/' + issue.id);
  api.edit.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'transport_failure',
      message: 'Disconnected.',
      status: null,
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Description' }), 'Frozen requirements');
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save task' }),
  );
  await screen.findByRole('button', { name: 'Retry same command' });
  api.get.mockResolvedValue({
    ...issue,
    version: 2,
    content: { ...issue.content, description: 'Concurrent requirements' },
  });
  await queryClient.refetchQueries({ queryKey: ['issues', 'delivery', issue.id, 'current'] });
  await userEvent.click(screen.getByRole('button', { name: 'Retry same command' }));
  await waitFor(() => expect(api.edit).toHaveBeenCalledTimes(2));
  expect(api.edit.mock.calls[1]).toEqual(api.edit.mock.calls[0]);
  expect(api.edit.mock.calls[1]![3]['If-Match']).toBe('"1"');
  expect(within(screen.getByRole('dialog')).getByText(/Frozen requirements/u)).toBeTruthy();
});

it('requires reconciliation when field ownership changes without a new requirements revision', async () => {
  const { api } = setup('/projects/delivery/issues/' + issue.id);
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Title' }), ' local title');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Acceptance criteria' }),
    'Local acceptance',
  );
  const choices = await api.transitions.mock.results[0]!.value;
  api.transitions.mockResolvedValue({
    ...choices,
    field_policy: { ...choices.field_policy, source_owned: ['title'] },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh task' }));
  await screen.findByText('Managed by source: Title');
  await userEvent.click(
    screen.getByRole('button', { name: 'Use latest version and keep my edits' }),
  );
  expect(screen.getByRole('textbox', { name: 'Title' })).toHaveProperty(
    'value',
    issue.content.title,
  );
  expect(screen.getByRole('textbox', { name: 'Acceptance criteria' })).toHaveProperty(
    'value',
    'Local acceptance',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Save task' }),
  );
  await waitFor(() => expect(api.edit).toHaveBeenCalledTimes(1));
  expect(api.edit.mock.calls[0]![2].title).toBe(issue.content.title);
});

it('keeps planning ownership out of the requirements editor when the policy exposes new fields', async () => {
  const { api } = setup('/projects/delivery/issues/' + issue.id);
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  const choices = await api.transitions.mock.results[0]!.value;
  api.transitions.mockResolvedValue({
    ...choices,
    field_policy: {
      ...choices.field_policy,
      source_owned: ['title', 'assignee', 'labels', 'properties', 'due_date', 'parent'],
    },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Refresh task' }));
  expect(await screen.findByText('Managed by source: Title')).toBeTruthy();
  expect(screen.getByRole('textbox', { name: 'Title' })).toHaveProperty('disabled', true);
  expect(screen.getByRole('textbox', { name: 'Acceptance criteria' })).toHaveProperty(
    'disabled',
    false,
  );
});
