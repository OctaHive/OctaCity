// @vitest-environment jsdom
import { cleanup, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { setup, issue } from '../../../tests/fixtures/issueTestSupport';
import {
  fakeActivity,
  resultsFixture,
  timelineFixture,
} from '../../../tests/fixtures/issueActivity';
import { executionFixture } from '../../../tests/fixtures/issueExecutions';
import type { IssueExecutionsApi } from './executionApi';
afterEach(cleanup);
const path = `/projects/delivery/issues/${issue.id}?section=results&execution=old-request&result_collection=decisions&result_id=old-decision`;
it.each(['Runs', 'Activity'] as const)(
  'starts fresh Results when another execution is selected through %s',
  async (navigation) => {
    const activity = fakeActivity(vi);
    activity.results.mockResolvedValue({
      ...resultsFixture(),
      next_cursor: 'original-result-page',
    });
    activity.timeline.mockResolvedValue(
      timelineFixture([
        { kind: 'execution', at: 1, request_id: 'later-request', requirements_version: 1 },
      ]),
    );
    const original = executionFixture('admitted', 'old-request');
    const later = executionFixture('admitted', 'later-request');
    if (!later.association) throw new Error('admitted execution fixture');
    later.association.work_id = 'later-work';
    later.association.run_id = 'later-run';
    const executions = {
      get: vi
        .fn<IssueExecutionsApi['get']>()
        .mockImplementation(async (_p, _i, id) => (id === 'later-request' ? later : original)),
      list: vi.fn<IssueExecutionsApi['list']>().mockResolvedValue({
        items: [original, later],
        next_cursor: null,
        watermark: 2,
        issue_version: 1,
        observation: 'request-watermark-v1',
      }),
      start: vi.fn<IssueExecutionsApi['start']>(),
    };
    const { router } = setup(path, undefined, {
      issueActivityApi: activity,
      issueExecutionsApi: executions,
    });
    const results = await screen.findByRole('region', { name: 'Execution results' });
    await userEvent.click(await within(results).findByRole('button', { name: 'Next page' }));
    await waitFor(() =>
      expect(activity.results).toHaveBeenCalledWith(
        'delivery',
        issue.id,
        'retained-work',
        'decisions',
        'original-result-page',
        expect.any(AbortSignal),
      ),
    );
    await userEvent.click(screen.getByRole('link', { name: navigation }));
    const section = await screen.findByRole('region', {
      name: navigation === 'Runs' ? 'Execution history' : 'Task activity',
    });
    const links = await within(section).findAllByRole('link');
    const selectLater = links.find(
      (link) =>
        new URL(link.getAttribute('href')!, 'http://console.test').searchParams.get('execution') ===
        'later-request',
    );
    expect(selectLater).toBeDefined();
    await userEvent.click(selectLater!);
    const selected = new URLSearchParams(router.state.location.search);
    expect(selected.get('execution')).toBe('later-request');
    expect(selected.get('result_after')).toBeNull();
    expect(selected.get('result_id')).toBeNull();
    expect(selected.get('result_collection')).toBe('decisions');
    await userEvent.click(screen.getByRole('link', { name: 'Results' }));
    await waitFor(() =>
      expect(activity.results).toHaveBeenCalledWith(
        'delivery',
        issue.id,
        'later-work',
        'decisions',
        null,
        expect.any(AbortSignal),
      ),
    );
    expect(activity.results).not.toHaveBeenCalledWith(
      'delivery',
      issue.id,
      'later-work',
      'decisions',
      'original-result-page',
      expect.any(AbortSignal),
    );
    expect(executions.start).not.toHaveBeenCalled();
  },
);
it('retains historical result details and continuation when reselecting the same execution', async () => {
  const activity = fakeActivity(vi);
  const page = resultsFixture([
    {
      kind: 'domain_fact',
      record: {
        id: 'old-decision',
        flow_run_id: 'original-flow',
        cycle_id: 'original-cycle',
        node_attempt_id: 'original-node',
        input_digest: 'a'.repeat(64),
        observed_at: 1,
        facts: {
          kind: 'decision',
          id: 'decision-one',
          subject: {
            exact: {
              project_id: 'delivery',
              repository_id: 'repo',
              base_revision: 'original-base',
            },
            candidate_revision: 'historical-head',
            changeset_digest: 'b'.repeat(64),
          },
          outcome: 'reject',
        },
      },
    },
  ]);
  page.current_version = 2;
  page.association.request_id = 'old-request';
  Object.assign(page.items[0]!.record, { private_transcript: 'must not render' });
  activity.results.mockResolvedValue(page);
  const executions = {
    get: vi
      .fn<IssueExecutionsApi['get']>()
      .mockResolvedValue(executionFixture('admitted', 'old-request')),
    list: vi.fn<IssueExecutionsApi['list']>().mockResolvedValue({
      items: [executionFixture('admitted', 'old-request')],
      next_cursor: null,
      watermark: 0,
      issue_version: 1,
      observation: 'request-watermark-v1',
    }),
    start: vi.fn<IssueExecutionsApi['start']>(),
  };
  const { router } = setup(
    path + '&result_after=retained-result-page',
    (api) => api.get.mockResolvedValue({ ...issue, version: 2 }),
    {
      issueActivityApi: activity,
      issueExecutionsApi: executions,
    },
  );
  const section = await screen.findByRole('region', { name: 'Execution results' });
  expect(await within(section).findByText('historical-head')).toBeTruthy();
  expect(within(section).getByText('reject')).toBeTruthy();
  expect(section.textContent).not.toContain('must not render');
  expect(
    within(section).getByText(
      'Partial observation. Empty or final pages do not establish verification or acceptance.',
    ),
  ).toBeTruthy();
  expect(activity.results).toHaveBeenCalledWith(
    'delivery',
    issue.id,
    'retained-work',
    'decisions',
    'retained-result-page',
    expect.any(AbortSignal),
  );
  await userEvent.click(screen.getByRole('link', { name: 'Runs' }));
  const history = await screen.findByRole('region', { name: 'Execution history' });
  await userEvent.click(await within(history).findByRole('link', { name: /old-request/u }));
  expect(new URLSearchParams(router.state.location.search).get('result_after')).toBe(
    'retained-result-page',
  );
  expect(new URLSearchParams(router.state.location.search).get('result_id')).toBe('old-decision');
  await userEvent.click(screen.getByRole('link', { name: 'Activity' }));
  await userEvent.click(screen.getByRole('link', { name: 'Results' }));
  expect(new URLSearchParams(router.state.location.search).get('execution')).toBe('old-request');
  expect(new URLSearchParams(router.state.location.search).get('result_id')).toBe('old-decision');
  expect(new URLSearchParams(router.state.location.search).get('result_after')).toBe(
    'retained-result-page',
  );
  expect(executions.start).not.toHaveBeenCalled();
});

it('offers a fresh result observation after a continuation becomes unreadable', async () => {
  const activity = fakeActivity(vi);
  activity.results
    .mockResolvedValueOnce({ ...resultsFixture(), next_cursor: 'old-boundary' })
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'invalid_request',
        status: 400,
        message: 'Result observation expired',
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    )
    .mockResolvedValue(resultsFixture());
  setup(path, undefined, {
    issueActivityApi: activity,
    issueExecutionsApi: {
      get: vi.fn().mockResolvedValue(executionFixture('admitted', 'old-request')),
      list: vi.fn(),
      start: vi.fn(),
    },
  });
  const section = await screen.findByRole('region', { name: 'Execution results' });
  await userEvent.click(await within(section).findByRole('button', { name: 'Next page' }));
  await within(section).findByText('Result observation expired');
  await userEvent.click(within(section).getByRole('button', { name: 'First page' }));
  expect(
    await within(section).findByText(
      'Partial observation. Empty or final pages do not establish verification or acceptance.',
    ),
  ).toBeTruthy();
});
import { ManagementApiError } from '../../api/client';
