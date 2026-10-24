// @vitest-environment jsdom
import { act, cleanup, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import {
  issue,
  workflow,
  setup,
  fakeIssueSourcesApi,
} from '../../../tests/fixtures/issueTestSupport';
import { issueSourceChoice, issueSourceReview } from '../../../tests/fixtures/issueSources';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});
it('shows a local manual task without a Source prerequisite and keeps task editing available', async () => {
  const sources = fakeIssueSourcesApi();
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  const section = await screen.findByRole('region', { name: 'Source and synchronization' });
  expect(await within(section).findByText('Local task · no source link')).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Edit requirements' })).toHaveProperty(
    'disabled',
    false,
  );
  expect(sources.review).toHaveBeenCalledWith('delivery', issue.id, expect.any(AbortSignal));
  expect(within(section).queryByLabelText(/token|password|endpoint/i)).toBeNull();
});

it('reviews field ownership through its separate mapping without editing the Project workflow', async () => {
  const sources = fakeIssueSourcesApi();
  const review = issueSourceReview(true);
  const choice = issueSourceChoice();
  sources.review.mockResolvedValue(review);
  sources.choices.mockResolvedValue({ issue_version: issue.version, items: [choice] });
  sources.publishMapping.mockResolvedValue({
    disposition: 'applied',
    resource: { version: 1 },
    mapping_reference: null,
  });
  const { workflows } = setup('/projects/delivery/issues/' + issue.id, undefined, {
    issueSourcesApi: sources,
  });
  await screen.findByText('opaque-7');
  await userEvent.click(screen.getByRole('button', { name: 'Configure source' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Description authority' }));
  await userEvent.click(screen.getByRole('option', { name: 'Source owns · pull' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review field ownership and mapping' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain(workflow.definition.name);
  expect(sources.publishMapping).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish mapping' }));
  await waitFor(() =>
    expect(sources.publishMapping).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      {
        expected_mapping: null,
        definition: {
          source: choice.source,
          reporter: null,
          push_enabled: false,
          fields: { description: 'source_owned_pull' },
          status: null,
        },
      },
      { 'If-Match': '"1"', 'Idempotency-Key': expect.any(String) },
    ),
  );
  expect(workflows.publish).not.toHaveBeenCalled();
  expect(workflows.create).not.toHaveBeenCalled();
});

it('links a manual task only after explicit native discovery and a frozen ownership review', async () => {
  const sources = fakeIssueSourcesApi();
  const choice = issueSourceChoice();
  const review = issueSourceReview();
  sources.review.mockResolvedValue(review);
  sources.choices.mockResolvedValue({ issue_version: issue.version, items: [choice] });
  sources.candidates.mockResolvedValue({
    scan_id: '05'.repeat(32),
    items: [{ resource: choice.native_resource, item: 'opaque-7' }],
    next_cursor: null,
  });
  sources.attach.mockResolvedValue({
    kind: 'linked',
    disposition: 'applied',
    link: issueSourceReview(true).link,
  });
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  await screen.findByText('Local task · no source link');
  expect(sources.candidates).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Configure source' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Source profile' }));
  await userEvent.click(screen.getByRole('option', { name: new RegExp(choice.native_resource) }));
  await userEvent.click(screen.getByRole('button', { name: 'Find native tasks' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Native task' }));
  await userEvent.click(screen.getByRole('option', { name: 'opaque-7' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review source attachment' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain('opaque-7');
  expect(sources.attach).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Attach source' }));
  await waitFor(() =>
    expect(sources.attach).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      {
        source: choice.source,
        item: { resource: choice.native_resource, item: 'opaque-7' },
        expected_field_policy: review.field_policy,
        scan_id: '05'.repeat(32),
        continuation: null,
      },
      { 'If-Match': '"1"', 'Idempotency-Key': expect.any(String) },
    ),
  );
  expect(
    [...Object.values(localStorage), ...Object.values(sessionStorage)].join(' '),
  ).not.toContain('opaque-7');
});

it('maps an opaque native status and labels to an explicit workflow status before publication', async () => {
  const sources = fakeIssueSourcesApi();
  const choice = issueSourceChoice();
  sources.review.mockResolvedValue(issueSourceReview(true));
  sources.choices.mockResolvedValue({ issue_version: issue.version, items: [choice] });
  sources.publishMapping.mockResolvedValue({
    disposition: 'applied',
    resource: { version: 1 },
    mapping_reference: null,
  });
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  await screen.findByText('opaque-7');
  await userEvent.click(screen.getByRole('button', { name: 'Configure source' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Status authority' }));
  await userEvent.click(screen.getByRole('option', { name: 'Source owns · pull' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add pull rule' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Native state' }));
  await userEvent.click(screen.getByRole('option', { name: 'Waiting for native review' }));
  const target = workflow.definition.statuses[0]!;
  await userEvent.click(screen.getByRole('combobox', { name: 'Workflow status' }));
  await userEvent.click(screen.getByRole('option', { name: target.name }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'All labels: needs-human-review' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review field ownership and mapping' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain('Waiting for native review');
  expect(dialog.textContent).toContain(target.name);
  expect(dialog.textContent).toContain('needs-human-review');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish mapping' }));
  await waitFor(() =>
    expect(sources.publishMapping).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      expect.objectContaining({
        definition: expect.objectContaining({
          fields: { status: 'source_owned_pull' },
          push_enabled: false,
          status: {
            workflow: { id: workflow.id, version: workflow.version },
            push: null,
            pull: [
              {
                status_id: target.id,
                native: {
                  state: 'Waiting for native review',
                  all_of: ['needs-human-review'],
                  any_of: [],
                  none_of: [],
                },
              },
            ],
          },
        }),
      }),
      expect.any(Object),
    ),
  );
});

it('reviews only authorized push effects with an explicit independent enable switch', async () => {
  const sources = fakeIssueSourcesApi();
  const base = issueSourceChoice();
  const writer = {
    binding: { ...base.source, capability: { ...base.source.capability, identity: 'task-update' } },
    effects: { states: ['Waiting for native review'], labels: ['needs-human-review'] },
  };
  const choice = { ...base, push_fields: ['status' as const], writer };
  sources.review.mockResolvedValue(issueSourceReview(true));
  sources.choices.mockResolvedValue({ issue_version: issue.version, items: [choice] });
  sources.publishMapping.mockResolvedValue({
    disposition: 'applied',
    resource: { version: 1 },
    mapping_reference: null,
  });
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  await screen.findByText('opaque-7');
  await userEvent.click(screen.getByRole('button', { name: 'Configure source' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Status authority' }));
  await userEvent.click(screen.getByRole('option', { name: 'OctaCity owns · push' }));
  const enabled = screen.getByRole('checkbox', { name: 'Enable outbound status updates' });
  expect(enabled).toHaveProperty('checked', false);
  await userEvent.click(screen.getByRole('button', { name: 'Add status mapping' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Logical status' }));
  const target = workflow.definition.statuses[0]!;
  await userEvent.click(screen.getByRole('option', { name: target.name }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Add label: needs-human-review' }));
  await userEvent.click(enabled);
  await userEvent.click(screen.getByRole('button', { name: 'Review field ownership and mapping' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain('needs-human-review');
  expect(dialog.textContent).toContain(target.name);
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish mapping' }));
  await waitFor(() =>
    expect(sources.publishMapping).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      expect.objectContaining({
        definition: expect.objectContaining({
          reporter: writer.binding,
          push_enabled: true,
          fields: { status: 'octacity_owned_push' },
          status: {
            workflow: { id: workflow.id, version: workflow.version },
            pull: [],
            push: {
              statuses: [
                { status: target.id, add: ['needs-human-review'], remove: [], state: null },
              ],
            },
          },
        }),
      }),
      expect.any(Object),
    ),
  );
});

it('shows retained freshness and reviews pause independently from explicit provider refresh', async () => {
  const sources = fakeIssueSourcesApi();
  sources.review.mockResolvedValue(issueSourceReview(true));
  sources.setPull.mockResolvedValue({ enabled: false, generation: 2 });
  sources.refresh.mockResolvedValue({ disposition: 'unchanged', issue_id: issue.id });
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  await screen.findByText('opaque-7');
  expect(screen.getByText(/Last accepted observation/)).toBeTruthy();
  expect(sources.refresh).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Pause incoming synchronization' }));
  expect(sources.setPull).not.toHaveBeenCalled();
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', {
      name: 'Confirm synchronization action',
    }),
  );
  await waitFor(() =>
    expect(sources.setPull).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      { expected_generation: 1, enabled: false },
      { 'Idempotency-Key': expect.any(String) },
    ),
  );
  await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }));
  await userEvent.click(screen.getByRole('button', { name: 'Refresh source observation' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', {
      name: 'Confirm synchronization action',
    }),
  );
  await waitFor(() =>
    expect(sources.refresh).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      { source: issueSourceChoice().source, continuation: null },
      { 'Idempotency-Key': expect.any(String) },
    ),
  );
  expect(await screen.findByText('Observation outcome: unchanged')).toBeTruthy();
});

it('preserves a mapping draft when the server workflow changes and requires explicit reload', async () => {
  const sources = fakeIssueSourcesApi();
  const initial = issueSourceReview(true);
  const choice = issueSourceChoice();
  sources.review.mockResolvedValue(initial);
  sources.choices.mockResolvedValue({ issue_version: issue.version, items: [choice] });
  const { queryClient } = setup('/projects/delivery/issues/' + issue.id, undefined, {
    issueSourcesApi: sources,
  });
  await screen.findByText('opaque-7');
  await userEvent.click(screen.getByRole('button', { name: 'Configure source' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Description authority' }));
  await userEvent.click(screen.getByRole('option', { name: 'Source owns · pull' }));
  sources.review.mockResolvedValue({
    ...initial,
    workflow: { ...initial.workflow, version: initial.workflow.version + 1 },
  });
  await queryClient.invalidateQueries({
    queryKey: ['issues', 'delivery', issue.id, 'source'],
    exact: true,
  });
  await waitFor(() =>
    expect(
      screen.getByRole('button', { name: 'Review field ownership and mapping' }),
    ).toHaveProperty('disabled', true),
  );
  expect(screen.getByRole('combobox', { name: 'Description authority' }).textContent).toContain(
    'Source owns',
  );
  expect(sources.publishMapping).not.toHaveBeenCalled();
});

it('shows only the server-authorized canonical task on an attachment conflict', async () => {
  const sources = fakeIssueSourcesApi();
  const choice = issueSourceChoice();
  const { IssueSourceAssociationConflict } = await import('./sourceApi');
  const canonical = {
    ...issueSourceReview(true).link!,
    issue_id: '40000000-0000-4000-8000-000000000099',
  };
  sources.choices.mockResolvedValue({ issue_version: issue.version, items: [choice] });
  sources.candidates.mockResolvedValue({
    scan_id: '05'.repeat(32),
    items: [canonical.selection.item],
    next_cursor: null,
  });
  sources.attach.mockRejectedValue(new IssueSourceAssociationConflict(canonical));
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  await screen.findByText('Local task · no source link');
  await userEvent.click(screen.getByRole('button', { name: 'Configure source' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Source profile' }));
  await userEvent.click(screen.getByRole('option', { name: new RegExp(choice.native_resource) }));
  await userEvent.click(screen.getByRole('button', { name: 'Find native tasks' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Native task' }));
  await userEvent.click(screen.getByRole('option', { name: 'opaque-7' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review source attachment' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Attach source' }),
  );
  const link = await within(screen.getByRole('dialog')).findByRole('link', {
    name: 'Open canonical task',
  });
  expect(link.getAttribute('href')).toBe('/projects/delivery/issues/' + canonical.issue_id);
});

it('keeps a draft but blocks a new review after the installed readable choices change', async () => {
  const sources = fakeIssueSourcesApi();
  const choice = issueSourceChoice();
  sources.review.mockResolvedValue(issueSourceReview(true));
  sources.choices.mockResolvedValue({ issue_version: issue.version, items: [choice] });
  const { queryClient } = setup('/projects/delivery/issues/' + issue.id, undefined, {
    issueSourcesApi: sources,
  });
  await screen.findByText('opaque-7');
  await userEvent.click(screen.getByRole('button', { name: 'Configure source' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Description authority' }));
  await userEvent.click(screen.getByRole('option', { name: 'Source owns · pull' }));
  sources.choices.mockResolvedValue({
    issue_version: issue.version,
    items: [{ ...choice, readable: { states: [], labels: [] } }],
  });
  await queryClient.invalidateQueries({
    queryKey: ['issues', 'delivery', issue.id, 'source', 'choices'],
    exact: true,
  });
  await waitFor(() =>
    expect(
      screen.getByRole('button', { name: 'Review field ownership and mapping' }),
    ).toHaveProperty('disabled', true),
  );
  expect(screen.getByRole('combobox', { name: 'Description authority' }).textContent).toContain(
    'Source owns',
  );
});

it('returns focus to the stable source control after attachment changes the card', async () => {
  const sources = fakeIssueSourcesApi();
  const choice = issueSourceChoice();
  sources.choices.mockResolvedValue({ issue_version: issue.version, items: [choice] });
  sources.candidates.mockResolvedValue({
    scan_id: '05'.repeat(32),
    items: [{ resource: choice.native_resource, item: 'opaque-7' }],
    next_cursor: null,
  });
  sources.attach.mockImplementation(async () => {
    sources.review.mockResolvedValue(issueSourceReview(true));
    return { kind: 'linked', disposition: 'applied', link: issueSourceReview(true).link! };
  });
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  await screen.findByText('Local task · no source link');
  await userEvent.click(screen.getByRole('button', { name: 'Configure source' }));
  const toggle = screen.getByRole('button', { name: 'Abandon' });
  await userEvent.click(await screen.findByRole('combobox', { name: 'Source profile' }));
  await userEvent.click(screen.getByRole('option', { name: new RegExp(choice.native_resource) }));
  await userEvent.click(screen.getByRole('button', { name: 'Find native tasks' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Native task' }));
  await userEvent.click(screen.getByRole('option', { name: 'opaque-7' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review source attachment' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Attach source' }),
  );
  await within(screen.getByRole('dialog')).findByText('Source attached: opaque-7');
  await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }));
  expect(document.activeElement).toBe(toggle);
});

it('labels expired retained observations without silently refreshing the provider', async () => {
  const sources = fakeIssueSourcesApi();
  sources.review.mockResolvedValue(issueSourceReview(true));
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  expect(await screen.findByText('Observation expired · refresh explicitly')).toBeTruthy();
  expect(sources.refresh).not.toHaveBeenCalled();
});

it('marks a displayed observation expired at its original deadline without provider I/O', async () => {
  const sources = fakeIssueSourcesApi();
  const review = issueSourceReview(true);
  let resolveReview!: (value: typeof review) => void;
  sources.review.mockReturnValue(
    new Promise((resolve) => {
      resolveReview = resolve;
    }),
  );
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  await waitFor(() => expect(sources.review).toHaveBeenCalled());

  vi.useFakeTimers();
  review.observation!.accepted.expires_at = Date.now() + 300;
  await act(async () => {
    resolveReview(review);
    await vi.advanceTimersByTimeAsync(0);
  });
  expect(screen.getByText('opaque-7')).toBeTruthy();
  await act(() => vi.advanceTimersByTimeAsync(299));
  expect(screen.queryByText('Observation expired · refresh explicitly')).toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(screen.getByText('Observation expired · refresh explicitly')).toBeTruthy();
  expect(sources.refresh).not.toHaveBeenCalled();
});

it('discards old native rules and write enablement when explicitly changing the installed profile', async () => {
  const sources = fakeIssueSourcesApi();
  const first = issueSourceChoice();
  const next = {
    ...first,
    source: {
      ...first.source,
      configuration: { ...first.source.configuration, identity: 'other-read-profile' },
    },
    readable: { states: ['Other native state'], labels: [] },
  };
  sources.review.mockResolvedValue(issueSourceReview(true));
  sources.choices.mockResolvedValue({ issue_version: issue.version, items: [first, next] });
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  await screen.findByText('opaque-7');
  await userEvent.click(screen.getByRole('button', { name: 'Configure source' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Status authority' }));
  await userEvent.click(screen.getByRole('option', { name: 'Source owns · pull' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add pull rule' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Source profile' }));
  await userEvent.click(screen.getByRole('option', { name: /other-read-profile/ }));
  expect(screen.getByRole('combobox', { name: 'Status authority' }).textContent).toContain(
    'Not synchronized',
  );
  expect(screen.queryByRole('combobox', { name: 'Native state' })).toBeNull();
});

it('refreshes retained task lists after an observation can update source-owned requirements', async () => {
  const sources = fakeIssueSourcesApi();
  sources.review.mockResolvedValue(issueSourceReview(true));
  sources.refresh.mockResolvedValue({ disposition: 'fresh', issue_id: issue.id });
  const { api, router, queryClient } = setup('/projects/delivery/issues/' + issue.id, undefined, {
    issueSourcesApi: sources,
  });
  const key = ['issues', 'delivery', 'list', null];
  queryClient.setQueryDefaults(key, { staleTime: Infinity });
  queryClient.setQueryData(key, {
    items: [{ ...issue, title: 'Previous title' }],
    next_cursor: null,
  });
  api.list.mockResolvedValue({
    items: [{ ...issue, title: 'Updated native title' }],
    next_cursor: null,
  });
  await screen.findByText('opaque-7');
  await userEvent.click(screen.getByRole('button', { name: 'Refresh source observation' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', {
      name: 'Confirm synchronization action',
    }),
  );
  await within(screen.getByRole('dialog')).findByText('Observation outcome: fresh');
  await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Close' }));
  await router.navigate('/projects/delivery/issues');
  expect(await screen.findByRole('link', { name: /Updated native title/ })).toBeTruthy();
});

it.each(['pull', 'push'] as const)(
  'enforces the published per-row label ceiling for %s while allowing deselection',
  async (direction) => {
    const sources = fakeIssueSourcesApi();
    const base = issueSourceChoice();
    const labels = Array.from({ length: 65 }, (_, i) => `tag-${i}`);
    const choice = {
      ...base,
      readable: { ...base.readable, labels },
      push_fields: ['status' as const],
      writer: {
        binding: { ...base.source, capability: { ...base.source.capability, identity: 'writer' } },
        effects: { states: [], labels },
      },
    };
    sources.review.mockResolvedValue(issueSourceReview(true));
    sources.choices.mockResolvedValue({ issue_version: issue.version, items: [choice] });
    setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
    await screen.findByText('opaque-7');
    await userEvent.click(screen.getByRole('button', { name: 'Configure source' }));
    await userEvent.click(await screen.findByRole('combobox', { name: 'Status authority' }));
    await userEvent.click(
      screen.getByRole('option', {
        name: direction === 'pull' ? 'Source owns · pull' : 'OctaCity owns · push',
      }),
    );
    await userEvent.click(
      screen.getByRole('button', {
        name: direction === 'pull' ? 'Add pull rule' : 'Add status mapping',
      }),
    );
    const boxes = screen.getAllByRole('checkbox', {
      name: direction === 'pull' ? /^All labels: tag-/ : /^Add label: tag-/,
    });
    for (const box of boxes.slice(0, 64)) await userEvent.click(box);
    expect(boxes[64]).toHaveProperty('disabled', true);
    expect(boxes[0]).toHaveProperty('disabled', false);
    await userEvent.click(boxes[0]!);
    expect(boxes[64]).toHaveProperty('disabled', false);
  },
  20000,
);

it('attaches a second-page native task with the original preview scan and opaque cursor', async () => {
  const sources = fakeIssueSourcesApi();
  const choice = issueSourceChoice();
  sources.review.mockResolvedValue(issueSourceReview());
  sources.choices.mockResolvedValue({ issue_version: issue.version, items: [choice] });
  const scan = '07'.repeat(32);
  sources.candidates
    .mockResolvedValueOnce({ scan_id: scan, items: [], next_cursor: 'opaque-page-2' })
    .mockResolvedValueOnce({
      scan_id: scan,
      items: [{ resource: choice.native_resource, item: 'opaque-7' }],
      next_cursor: null,
    });
  sources.attach.mockResolvedValue({
    kind: 'linked',
    disposition: 'applied',
    link: issueSourceReview(true).link,
  });
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  await screen.findByText('Local task · no source link');
  await userEvent.click(screen.getByRole('button', { name: 'Configure source' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Source profile' }));
  await userEvent.click(screen.getByRole('option', { name: new RegExp(choice.native_resource) }));
  await userEvent.click(screen.getByRole('button', { name: 'Find native tasks' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Next native tasks' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Native task' }));
  await userEvent.click(screen.getByRole('option', { name: 'opaque-7' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review source attachment' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Attach source' }),
  );
  await waitFor(() => expect(sources.attach).toHaveBeenCalled());
  expect(sources.candidates.mock.calls[1]?.[2]).toEqual({
    source: choice.source,
    scan_id: scan,
    continuation: 'opaque-page-2',
  });
  expect(sources.attach.mock.calls[0]?.[2]).toMatchObject({
    scan_id: scan,
    continuation: 'opaque-page-2',
    item: { item: 'opaque-7' },
  });
});

it('continues a bounded source refresh with the frozen request and original operation key', async () => {
  const sources = fakeIssueSourcesApi();
  sources.review.mockResolvedValue(issueSourceReview(true));
  sources.refresh
    .mockResolvedValueOnce({ disposition: 'scanning', issue_id: issue.id })
    .mockResolvedValueOnce({ disposition: 'unchanged', issue_id: issue.id });
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  await screen.findByText('opaque-7');
  await userEvent.click(screen.getByRole('button', { name: 'Refresh source observation' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', {
      name: 'Confirm synchronization action',
    }),
  );
  expect(
    await screen.findByText('Source scan is in progress. Continue the original refresh.'),
  ).toBeTruthy();
  expect(sources.refresh).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole('button', { name: 'Continue refresh' }));
  await waitFor(() => expect(sources.refresh).toHaveBeenCalledTimes(2));
  expect(sources.refresh.mock.calls[1]).toEqual(sources.refresh.mock.calls[0]);
  expect(await screen.findByText('Observation outcome: unchanged')).toBeTruthy();
});
