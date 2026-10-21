import { fakeIssueReviewConnections } from '../../../tests/fixtures/issueTestSupport';
// @vitest-environment jsdom
import { cleanup, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { setup, issue, fakeIssueCodeReviewsApi } from '../../../tests/fixtures/issueTestSupport';
import { reviewFixture, reviewPage } from '../../../tests/fixtures/issueCodeReviews';

afterEach(cleanup);
it('opens retained PR/MR facts from the compact summary in Results without selecting or starting a Run', async () => {
  const reviews = {
    ...fakeIssueCodeReviewsApi(),
    list: vi.fn().mockResolvedValue(reviewPage()),
    attach: vi.fn(),
    refresh: vi.fn(),
    observations: vi.fn(),
  };
  const { router } = setup(
    `/projects/delivery/issues/${issue.id}?execution=old-request`,
    undefined,
    { issueCodeReviewsApi: reviews },
  );
  const summary = await screen.findByRole('region', { name: 'PR/MR summary' });
  const link = await within(summary).findByRole('link', {
    name: 'MR-42 · Preserve manual tasks <script>',
  });
  await userEvent.click(link);
  const results = await screen.findByRole('region', { name: 'Pull and merge requests' });
  expect(
    await within(results).findByRole('heading', { name: 'MR-42 · Preserve manual tasks <script>' }),
  ).toBeTruthy();
  for (const fact of [
    'independent-forge · native-code',
    'Visible author',
    'Open',
    'head-two',
    'base-one',
    '3',
    '12',
    '4',
    'Review pending',
    '2 external checks passed',
  ]) {
    expect(within(results).getByText(fact, { exact: true })).toBeTruthy();
  }
  expect(within(results).getByRole('link', { name: 'Open review' }).getAttribute('href')).toBe(
    'https://forge.example/team/code/reviews/42',
  );
  expect(
    within(results).getByText('Retained observation; freshness is not confirmed.'),
  ).toBeTruthy();
  expect(new URLSearchParams(router.state.location.search).get('execution')).toBe('old-request');
  expect(results.querySelector('script')).toBeNull();
  expect(reviews.attach).not.toHaveBeenCalled();
  expect(reviews.refresh).not.toHaveBeenCalled();
});

it('labels revoked read authority and unsupported fields while leaving local editing and another ticket source usable', async () => {
  const review = reviewFixture();
  review.availability = 'blocked';
  Object.assign(review.observation!, {
    author: null,
    state: null,
    draft: null,
    changed_files: null,
    additions: null,
    deletions: null,
    review: null,
    checks: null,
  });
  const reviews = {
    ...fakeIssueCodeReviewsApi(),
    list: vi.fn().mockResolvedValue(reviewPage([review])),
  };
  const sources = fakeIssueSourcesApi();
  sources.review.mockResolvedValue(issueSourceReview(true));
  const { api } = setup(`/projects/delivery/issues/${issue.id}?section=results`, undefined, {
    issueCodeReviewsApi: reviews,
    issueSourcesApi: sources,
  });
  const results = await screen.findByRole('region', { name: 'Pull and merge requests' });
  expect(
    await within(results).findByText('Read capability blocked; retained facts may be stale.'),
  ).toBeTruthy();
  expect(within(results).getAllByText('Not reported').length).toBe(8);
  expect(within(results).getByRole('button', { name: 'Refresh PR/MR' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(
    (
      await within(
        screen.getByRole('region', { name: 'Source and synchronization' }),
      ).findAllByText(/opaque-7/u)
    ).length,
  ).toBeGreaterThan(0);
  await userEvent.click(screen.getByRole('link', { name: 'Overview' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  expect(await screen.findByRole('textbox', { name: 'Title' })).toBeTruthy();
  expect(api.edit).not.toHaveBeenCalled();
});
import { fakeIssueSourcesApi } from '../../../tests/fixtures/issueTestSupport';
import { issueSourceReview } from '../../../tests/fixtures/issueSources';

it('attaches only a selected authorized code repository after a frozen current-version review, with exact retry', async () => {
  const review = reviewFixture();
  const reviews = {
    ...fakeIssueCodeReviewsApi(),
    list: vi.fn().mockResolvedValue(reviewPage([])),
    attach: vi
      .fn()
      .mockRejectedValueOnce(
        new ManagementApiError({
          code: 'transport_failure',
          status: null,
          message: 'Acknowledgement lost',
          requestId: null,
          retryAfterMilliseconds: null,
        }),
      )
      .mockResolvedValue({ disposition: 'replayed', association: review.association }),
  };
  const connections = {
    ...fakeIssueReviewConnections(),
    listConnections: vi
      .fn()
      .mockResolvedValue({ items: [reviewConnectionFixture()], next_cursor: null }),
    listProjectResources: vi.fn().mockResolvedValue(reviewResourcesFixture()),
  };
  setup(`/projects/delivery/issues/${issue.id}?section=results`, undefined, {
    issueCodeReviewsApi: reviews,
    issueReviewConnections: connections,
  });
  const results = await screen.findByRole('region', { name: 'Pull and merge requests' });
  await userEvent.click(within(results).getByRole('button', { name: 'Attach PR/MR' }));
  await userEvent.click(await within(results).findByRole('combobox', { name: 'Code connection' }));
  await userEvent.click(screen.getByRole('option', { name: 'Code forge · fake-mr' }));
  await userEvent.click(await within(results).findByRole('combobox', { name: 'Code repository' }));
  await userEvent.click(
    await screen.findByRole('option', { name: 'team/code · review-profile / v1' }),
  );
  await userEvent.type(
    within(results).getByRole('textbox', { name: 'Native PR/MR ID' }),
    'native-mr-42',
  );
  await userEvent.click(within(results).getByRole('button', { name: 'Review attachment' }));
  const dialog = screen.getByRole('dialog');
  expect(dialog.textContent).toContain('native-mr-42');
  expect(reviews.attach).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Attach PR/MR' }));
  await userEvent.click(await within(dialog).findByRole('button', { name: 'Retry same command' }));
  expect(await within(dialog).findByText('PR/MR attached.')).toBeTruthy();
  expect(reviews.attach).toHaveBeenCalledTimes(2);
  const [, , body, headers] = reviews.attach.mock.calls[0]!;
  expect(body).toEqual({ selection: review.association.selection });
  expect(headers).toEqual({ 'If-Match': '"1"', 'Idempotency-Key': expect.any(String) });
  expect(reviews.attach.mock.calls[1]).toEqual(reviews.attach.mock.calls[0]);
  expect(connections.listProjectResources).toHaveBeenCalledWith(
    reviewConnectionFixture().id,
    'delivery',
    expect.any(AbortSignal),
  );
});
import {
  reviewConnectionFixture,
  reviewResourcesFixture,
} from '../../../tests/fixtures/issueCodeReviews';
import { ManagementApiError } from '../../api/client';

it('keeps the original execution head historical after the review advances and pages its retained observations on demand', async () => {
  const review = reviewFixture();
  review.association.binding = {
    association: {
      issue_id: issue.id,
      project_id: 'delivery',
      request_id: 'original-request',
      requirements_version: 1,
      work_id: 'original-work',
      run_id: 'original-run',
    },
    record: 'bc'.repeat(32),
    candidate: {
      exact: { project_id: 'delivery', repository_id: 'code', base_revision: 'base-one' },
      candidate_revision: 'head-one',
      changeset_digest: 'cd'.repeat(32),
    },
  };
  const old = {
    ...review.observation!,
    head: 'head-one',
    checks: 'Old-head checks passed',
    observed_at: 1699999999000,
  };
  const reviews = {
    ...fakeIssueCodeReviewsApi(),
    list: vi.fn().mockResolvedValue(reviewPage([review])),
    observations: vi
      .fn()
      .mockResolvedValueOnce({
        association: review.association,
        items: [
          {
            id: 'de'.repeat(32),
            sequence: 1,
            association_id: review.association.id,
            observation: old,
          },
        ],
        availability: 'available',
        next_cursor: 'history-page-two',
      })
      .mockResolvedValue({
        association: review.association,
        items: [],
        availability: 'blocked',
        next_cursor: null,
      }),
  };
  setup(
    `/projects/delivery/issues/${issue.id}?section=results&execution=original-request`,
    undefined,
    { issueCodeReviewsApi: reviews },
  );
  const section = await screen.findByRole('region', { name: 'Pull and merge requests' });
  expect(
    await within(section).findByText(
      'Historical execution head: head-one. Current review head differs.',
    ),
  ).toBeTruthy();
  expect(reviews.observations).not.toHaveBeenCalled();
  await userEvent.click(within(section).getByRole('button', { name: 'Observation history' }));
  const history = await within(section).findByRole('region', { name: 'Retained review history' });
  expect(await within(history).findByText('Old-head checks passed')).toBeTruthy();
  expect(within(history).getByText('head-one', { exact: true })).toBeTruthy();
  await userEvent.click(within(history).getByRole('button', { name: 'Next page' }));
  await within(history).findByText('Read capability blocked; retained facts may be stale.');
  expect(reviews.observations).toHaveBeenLastCalledWith(
    'delivery',
    issue.id,
    review.association.id,
    'history-page-two',
    expect.any(AbortSignal),
  );
});

it('refreshes only after confirmation and keeps last known facts visibly stale during provider outage', async () => {
  const reviews = {
    ...fakeIssueCodeReviewsApi(),
    list: vi.fn().mockResolvedValue(reviewPage()),
    refresh: vi.fn().mockRejectedValue(
      new ManagementApiError({
        code: 'unavailable',
        status: 503,
        message: 'Review provider unavailable',
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    ),
  };
  setup(`/projects/delivery/issues/${issue.id}?section=results`, undefined, {
    issueCodeReviewsApi: reviews,
  });
  const results = await screen.findByRole('region', { name: 'Pull and merge requests' });
  await userEvent.click(await within(results).findByRole('button', { name: 'Refresh PR/MR' }));
  expect(reviews.refresh).not.toHaveBeenCalled();
  const dialog = screen.getByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Refresh PR/MR' }));
  await within(dialog).findByText('Review provider unavailable');
  expect(reviews.refresh).toHaveBeenCalledWith(
    'delivery',
    issue.id,
    reviewFixture().association.id,
    { 'Idempotency-Key': expect.any(String) },
  );
  await userEvent.click(within(dialog).getByRole('button', { name: 'Abandon' }));
  expect(
    within(results).getByText('Refresh failed; showing the last known observation.'),
  ).toBeTruthy();
  expect(within(results).getByText('head-two', { exact: true })).toBeTruthy();
  await userEvent.click(screen.getByRole('link', { name: 'Overview' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit requirements' }));
  expect(await screen.findByRole('textbox', { name: 'Title' })).toBeTruthy();
});

it('pages one bounded review collection without losing selected execution and recovers from a failed retained read', async () => {
  const second = reviewFixture();
  second.association.id = 'ef'.repeat(32);
  second.observation!.number = 'PR-99';
  const reviews = {
    ...fakeIssueCodeReviewsApi(),
    list: vi
      .fn()
      .mockResolvedValueOnce({ ...reviewPage(), next_cursor: 'review-page-two' })
      .mockRejectedValueOnce(
        new ManagementApiError({
          code: 'invalid_request',
          status: 400,
          message: 'Review cursor expired',
          requestId: null,
          retryAfterMilliseconds: null,
        }),
      )
      .mockResolvedValue(reviewPage([second])),
  };
  const { router, queryClient } = setup(
    `/projects/delivery/issues/${issue.id}?section=results&execution=old-request`,
    undefined,
    { issueCodeReviewsApi: reviews },
  );
  const section = await screen.findByRole('region', { name: 'Pull and merge requests' });
  await userEvent.click(await within(section).findByRole('button', { name: 'Next page' }));
  await within(section).findByText('Review cursor expired');
  await userEvent.click(within(section).getByRole('button', { name: 'First page' }));
  await within(section).findByRole('heading', { name: 'PR-99 · Preserve manual tasks <script>' });
  expect(new URLSearchParams(router.state.location.search).get('execution')).toBe('old-request');
  expect(reviews.list).toHaveBeenCalledWith(
    'delivery',
    issue.id,
    'review-page-two',
    expect.any(AbortSignal),
  );
  reviews.list.mockRejectedValue(
    new ManagementApiError({
      code: 'unavailable',
      status: 503,
      message: 'Retained read failed',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await queryClient.invalidateQueries({ queryKey: queryKeys.issueRoot('delivery', issue.id) });
  await within(section).findByText('Retained read failed');
  expect(
    within(section).getByRole('heading', { name: 'PR-99 · Preserve manual tasks <script>' }),
  ).toBeTruthy();
});
import { queryKeys } from '../../app/query';

it('preserves manual creation when review reads are unavailable and needs no connection for the task', async () => {
  const reviews = {
    ...fakeIssueCodeReviewsApi(),
    list: vi.fn().mockRejectedValue(
      new ManagementApiError({
        code: 'capability_unavailable',
        status: 422,
        message: 'Review capability unavailable',
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    ),
  };
  const { api } = setup('/projects/delivery/issues', undefined, { issueCodeReviewsApi: reviews });
  await userEvent.click(await screen.findByRole('button', { name: 'Create task' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Title' }), 'Independent manual task');
  await userEvent.click(screen.getByRole('button', { name: 'Review task' }));
  const dialog = screen.getByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Create task' }));
  await userEvent.click(await within(dialog).findByRole('link', { name: 'Open ISS-42' }));
  await screen.findByText('Review capability unavailable');
  expect(screen.getByRole('heading', { name: 'ISS-42 · Investigate checkout' })).toBeTruthy();
  expect(api.create).toHaveBeenCalledWith(
    'delivery',
    { title: 'Independent manual task', description: '', acceptance: null, priority: null },
    { 'Idempotency-Key': expect.any(String) },
  );
});

it('does not offer ticket-read or Delivery grants as review-read choices', async () => {
  const resources = reviewResourcesFixture();
  resources.resources[0]!.capabilities[0]!.purpose = 'task_read';
  const connections = {
    ...fakeIssueReviewConnections(),
    listConnections: vi
      .fn()
      .mockResolvedValue({ items: [reviewConnectionFixture()], next_cursor: null }),
    listProjectResources: vi.fn().mockResolvedValue(resources),
  };
  const reviews = fakeIssueCodeReviewsApi();
  setup(`/projects/delivery/issues/${issue.id}?section=results`, undefined, {
    issueCodeReviewsApi: reviews,
    issueReviewConnections: connections,
  });
  const results = await screen.findByRole('region', { name: 'Pull and merge requests' });
  await userEvent.click(within(results).getByRole('button', { name: 'Attach PR/MR' }));
  await userEvent.click(await within(results).findByRole('combobox', { name: 'Code connection' }));
  await userEvent.click(screen.getByRole('option', { name: 'Code forge · fake-mr' }));
  await within(results).findByText(
    'No authorized repository-read choices. An installed connection alone does not establish review capability.',
  );
  await userEvent.type(
    within(results).getByRole('textbox', { name: 'Native PR/MR ID' }),
    'native-mr-42',
  );
  expect(within(results).getByRole('button', { name: 'Review attachment' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(reviews.attach).not.toHaveBeenCalled();
});

it('uses the Project grant’s retained connection version instead of retargeting an attachment to latest metadata', async () => {
  const original = reviewConnectionFixture();
  const latest = structuredClone(original);
  latest.version = 3;
  latest.definition.installation = {
    identity: 'different-installation',
    version: 'v2',
    digest: '03'.repeat(32),
  };
  const connections = {
    ...fakeIssueReviewConnections(),
    listConnections: vi.fn().mockResolvedValue({ items: [latest], next_cursor: null }),
    listProjectResources: vi.fn().mockResolvedValue(reviewResourcesFixture()),
    getProjectAccess: vi.fn().mockResolvedValue({
      access: {
        connection_id: original.id,
        connection_version: 2,
        version: 7,
        project_id: 'delivery',
        resources: [],
        published_at_unix_ms: 1,
      },
    }),
    getConnectionVersion: vi.fn().mockResolvedValue(original),
  };
  const reviews = fakeIssueCodeReviewsApi();
  reviews.attach.mockResolvedValue({
    disposition: 'applied',
    association: reviewFixture().association,
  });
  setup(`/projects/delivery/issues/${issue.id}?section=results`, undefined, {
    issueCodeReviewsApi: reviews,
    issueReviewConnections: connections,
  });
  const results = await screen.findByRole('region', { name: 'Pull and merge requests' });
  await userEvent.click(within(results).getByRole('button', { name: 'Attach PR/MR' }));
  await userEvent.click(await within(results).findByRole('combobox', { name: 'Code connection' }));
  await userEvent.click(screen.getByRole('option', { name: 'Code forge · fake-mr' }));
  await userEvent.click(await within(results).findByRole('combobox', { name: 'Code repository' }));
  await userEvent.click(
    await screen.findByRole('option', { name: 'team/code · review-profile / v1' }),
  );
  await userEvent.type(
    within(results).getByRole('textbox', { name: 'Native PR/MR ID' }),
    'native-mr-42',
  );
  await userEvent.click(within(results).getByRole('button', { name: 'Review attachment' }));
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Attach PR/MR' }),
  );
  await waitFor(() => expect(reviews.attach).toHaveBeenCalled());
  expect(reviews.attach.mock.calls[0]![2]).toEqual({
    selection: reviewFixture().association.selection,
  });
});

it('does not review native IDs containing Unicode control characters', async () => {
  const connections = {
    ...fakeIssueReviewConnections(),
    listConnections: vi
      .fn()
      .mockResolvedValue({ items: [reviewConnectionFixture()], next_cursor: null }),
    listProjectResources: vi.fn().mockResolvedValue(reviewResourcesFixture()),
  };
  setup(`/projects/delivery/issues/${issue.id}?section=results`, undefined, {
    issueReviewConnections: connections,
  });
  const results = await screen.findByRole('region', { name: 'Pull and merge requests' });
  await userEvent.click(within(results).getByRole('button', { name: 'Attach PR/MR' }));
  await userEvent.click(await within(results).findByRole('combobox', { name: 'Code connection' }));
  await userEvent.click(screen.getByRole('option', { name: 'Code forge · fake-mr' }));
  await userEvent.click(await within(results).findByRole('combobox', { name: 'Code repository' }));
  await userEvent.click(
    await screen.findByRole('option', { name: 'team/code · review-profile / v1' }),
  );
  await userEvent.type(
    within(results).getByRole('textbox', { name: 'Native PR/MR ID' }),
    'bad\u0085id',
  );
  expect(within(results).getByRole('button', { name: 'Review attachment' })).toHaveProperty(
    'disabled',
    true,
  );
});
