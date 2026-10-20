// @vitest-environment jsdom
import { cleanup, screen, within, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { issue, setup, fakeIssueSourcesApi } from '../../../tests/fixtures/issueTestSupport';
import userEvent from '@testing-library/user-event';
import { issueSourceChoice, issueSourceReview } from '../../../tests/fixtures/issueSources';

afterEach(cleanup);
it('shows exact retained native values and field direction without a provider read', async () => {
  const sources = fakeIssueSourcesApi();
  const review = issueSourceReview(true);
  review.observation!.values = {
    mapping: review.link!.selection.source.configuration,
    observation: '55'.repeat(32),
    state: 'Waiting for native review',
    labels: ['needs-human-review'],
  };
  review.mapping = {
    project_id: issue.project_id,
    issue_id: issue.id,
    version: 1,
    published_at: 1700000000000,
    definition: {
      source: review.link!.selection.source,
      reporter: null,
      push_enabled: false,
      fields: { status: 'source_owned_pull' },
      status: null,
    },
  };
  sources.review.mockResolvedValue(review);
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  expect(
    await screen.findByText('Last reconciled native status: Waiting for native review'),
  ).toBeTruthy();
  expect(screen.getByText('needs-human-review')).toBeTruthy();
  expect(screen.getByText('Status: Source owns · pull')).toBeTruthy();
  expect(
    screen.getByText(/Retained Source values are not a current Reporter inspection/),
  ).toBeTruthy();
  expect(sources.refresh).not.toHaveBeenCalled();
});
it.each([
  { state: 'pending', failure: null, label: 'Pending delivery' },
  { state: 'claimed', failure: null, label: 'Delivery in progress' },
  { state: 'delivered', failure: null, label: 'Delivery observed' },
  { state: 'failed', failure: 'conflict', label: 'Managed-field conflict · review required' },
  { state: 'failed', failure: 'exhausted', label: 'Delivery bounds exhausted · review required' },
  { state: 'failed', failure: 'forbidden', label: 'Delivery blocked by current access' },
  { state: 'failed', failure: 'unsupported', label: 'Requested write is unsupported' },
])('shows $label independently of business status', async ({ state, failure, label }) => {
  const review = issueSourceReview(true);
  const sources = Object.assign(fakeIssueSourcesApi(), {
    projections: vi.fn().mockResolvedValue({
      items: [
        {
          operation_id: '11'.repeat(32),
          record_id: '22'.repeat(32),
          issue_version: 1,
          mapping: review.link!.selection.source.configuration,
          native_correlation: review.link!.native_correlation,
          workflow: review.workflow,
          status_id: issue.status_id,
          effects: { status: issue.status_id, add: ['custom-ready'], remove: [], state: null },
          pipeline: null,
          resolves: null,
          accepted_at: 1700000000000,
          state,
          failure,
          attempt: 0,
          recorded_at: 1700000000000,
          available_at: 1700000001000,
        },
      ],
      next_cursor: null,
      observation: 'live-reporter-v1',
      observed_at: 1700000002000,
    }),
  });
  sources.review.mockResolvedValue(review);
  setup('/projects/delivery/issues/' + issue.id, undefined, { issueSourcesApi: sources });
  const section = await screen.findByRole('region', { name: 'Outgoing synchronization' });
  expect(await within(section).findByText(label)).toBeTruthy();
  expect(within(section).getByText(/custom-ready/)).toBeTruthy();
  expect(
    within(section).getByText(/Synchronization does not determine execution acceptance/),
  ).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Edit requirements' })).toHaveProperty(
    'disabled',
    false,
  );
  expect(sources.refresh).not.toHaveBeenCalled();
  expect(sources.publishMapping).not.toHaveBeenCalled();
  expect(within(section).queryByRole('button', { name: /force|retry/i })).toBeNull();
  expect(
    !!within(section).queryByRole('button', { name: 'Review synchronization resolution' }),
  ).toBe(state === 'failed');
});

it('reviews a new policy for the exact failed receipt through the existing mapping editor', async () => {
  const review = issueSourceReview(true);
  const choice = issueSourceChoice();
  choice.push_fields = ['status'];
  choice.writer = { binding: choice.source, effects: { labels: ['custom-ready'], states: [] } };
  review.mapping_reference = choice.source.configuration;
  review.mapping = {
    project_id: issue.project_id,
    issue_id: issue.id,
    version: 1,
    published_at: 1700000000000,
    definition: {
      source: choice.source,
      reporter: choice.source,
      push_enabled: true,
      fields: { status: 'octacity_owned_push' },
      status: {
        workflow: review.workflow,
        pull: [],
        push: {
          statuses: [{ status: issue.status_id, add: ['custom-ready'], remove: [], state: null }],
        },
      },
    },
  };
  const failed = {
    operation_id: '11'.repeat(32),
    record_id: '22'.repeat(32),
    issue_version: 1,
    mapping: review.mapping_reference,
    native_correlation: review.link!.native_correlation,
    workflow: review.workflow,
    status_id: issue.status_id,
    effects: review.mapping.definition.status!.push!.statuses[0],
    pipeline: null,
    resolves: null,
    accepted_at: 1700000000000,
    state: 'failed',
    failure: 'conflict',
    attempt: 0,
    recorded_at: 1700000001000,
    available_at: 1700000001000,
  };
  const sources = fakeIssueSourcesApi();
  sources.review.mockResolvedValue(review);
  sources.choices.mockResolvedValue({ issue_version: 1, items: [choice] });
  sources.projections.mockResolvedValue({
    items: [failed],
    next_cursor: null,
    observation: 'live-reporter-v1',
    observed_at: 1700000002000,
  });
  sources.resolve.mockResolvedValue({
    disposition: 'applied',
    resource: { ...review.mapping, version: 2 },
    mapping_reference: review.mapping_reference,
    projection: '33'.repeat(32),
  });
  const { queryClient } = setup('/projects/delivery/issues/' + issue.id, undefined, {
    issueSourcesApi: sources,
  });
  await userEvent.click(
    await screen.findByRole('button', { name: 'Review synchronization resolution' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Review resolution policy' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain(failed.operation_id);
  expect(dialog.textContent).toContain('custom-ready');
  expect(sources.resolve).not.toHaveBeenCalled();
  sources.projections.mockRejectedValueOnce(new Error('unavailable'));
  await queryClient.invalidateQueries({
    queryKey: ['issues', 'delivery', issue.id, 'projections'],
  });
  await waitFor(() =>
    expect(
      within(dialog).getByRole('button', { name: 'Publish resolution intent' }),
    ).toHaveProperty('disabled', true),
  );
  expect(screen.getByText('Managed-field conflict · review required')).toBeTruthy();
  expect(sources.resolve).not.toHaveBeenCalled();
  await queryClient.invalidateQueries({
    queryKey: ['issues', 'delivery', issue.id, 'projections'],
  });
  await waitFor(() =>
    expect(
      within(dialog).getByRole('button', { name: 'Publish resolution intent' }),
    ).toHaveProperty('disabled', false),
  );

  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish resolution intent' }));
  await waitFor(() =>
    expect(sources.resolve).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      {
        mapping: {
          expected_mapping: review.mapping_reference,
          definition: review.mapping!.definition,
        },
        review: { operation_id: failed.operation_id, expected_record: failed.record_id },
      },
      { 'If-Match': '"1"', 'Idempotency-Key': expect.any(String) },
    ),
  );
  expect(sources.publishMapping).not.toHaveBeenCalled();
});
