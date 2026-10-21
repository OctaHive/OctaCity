// @vitest-environment jsdom
import { cleanup, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ManagementApiError } from '../../api/client';
import { focusManager } from '@tanstack/react-query';
import { afterEach, expect, it, vi } from 'vitest';
import { setup, issue } from '../../../tests/fixtures/issueTestSupport';
import { fakeActivity } from '../../../tests/fixtures/issueActivity';
import { journalEventsFixture } from '../../../tests/fixtures/issueJournal';
import { executionFixture } from '../../../tests/fixtures/issueExecutions';
import { factoryApi } from '../factory/api';
import { runFixture } from '../../../tests/fixtures/factory';
afterEach(cleanup);
function mount(activity: ReturnType<typeof fakeActivity>, selection = '') {
  return setup(
    `/projects/delivery/issues/${issue.id}?section=runs&execution=old-request&journal_build=${'a'.repeat(64)}&journal_job=original-job${selection}`,
    undefined,
    {
      issueActivityApi: activity,
      issueExecutionsApi: {
        get: vi.fn().mockResolvedValue(executionFixture('admitted', 'old-request')),
        list: vi.fn().mockResolvedValue({
          items: [],
          next_cursor: null,
          watermark: 0,
          issue_version: 1,
          observation: 'request-watermark-v1',
        }),
        start: vi.fn(),
      },
      factoryApi: {
        ...factoryApi,
        getRun: vi.fn().mockResolvedValue({
          ...runFixture(),
          id: 'retained-run',
          work_id: 'retained-work',
          project_id: 'delivery',
        }),
      },
    },
  );
}
it('reconnects using the opaque tail, deduplicates rows and stops once the terminal Job cursor is consumed', async () => {
  const activity = fakeActivity(vi);
  const first = journalEventsFixture();
  activity.journalEvents
    .mockResolvedValueOnce({
      ...first,
      events: first.events.slice(0, 1),
      job: { ...first.job, job: { ...first.job.job, state: 'running', terminal: null } },
    })
    .mockResolvedValue({ ...first, cursor: 'terminal.tail' });
  mount(activity);
  const region = await screen.findByRole('region', { name: 'Job events' });
  await within(region).findByText('src/main.rs');
  expect(activity.journalEvents).toHaveBeenNthCalledWith(
    2,
    'delivery',
    issue.id,
    'old-request',
    { build_execution: 'a'.repeat(64), job_id: 'original-job' },
    'tail.one',
    expect.any(AbortSignal),
  );
  expect(within(region).getAllByRole('button', { name: 'Inspect event #1' })).toHaveLength(1);
  expect(await within(region).findByText('Event history complete')).toBeTruthy();
});
it('bounds retained rows across real page sizes, preserves scroll and displays gaps', async () => {
  const activity = fakeActivity(vi);
  const fixture = journalEventsFixture();
  activity.journalEvents.mockImplementation(async () => {
    const index = activity.journalEvents.mock.calls.length - 1;
    const last = index === 10;
    return {
      ...fixture,
      cursor: `opaque-tail-${index}`,
      events: Array.from({ length: 50 }, (_, i) => ({
        ...fixture.events[2]!,
        sequence: index * 50 + i + 1 + (last ? 1 : 0),
      })),
      gaps: last ? [{ from_sequence: 501, through_sequence: 501 }] : [],
      job: {
        ...fixture.job,
        log: { ...fixture.job.log, event_cursor: 551 },
        job: last ? fixture.job.job : { ...fixture.job.job, state: 'running', terminal: null },
      },
    };
  });
  mount(activity);
  const list = await screen.findByRole('list', { name: 'Ordered diagnostics' });
  list.scrollTop = 120;
  await waitFor(() => expect(activity.journalEvents).toHaveBeenCalledTimes(11), {
    timeout: 10000,
    interval: 100,
  });
  await waitFor(() => expect(within(list).getAllByRole('listitem')).toHaveLength(512));
  expect(list.scrollTop).toBe(120);
  expect(await screen.findByText('Unavailable events: 501–501')).toBeTruthy();
  expect(
    screen.getByText(
      'Showing the latest 512 events; 38 older events were removed from this live view.',
    ),
  ).toBeTruthy();
});
it('aborts the selected stream when leaving Runs and never reads private event content automatically', async () => {
  const activity = fakeActivity(vi);
  let signal: AbortSignal | undefined;
  activity.journalEvents.mockImplementation(async (_p, _i, _r, _target, _after, s) => {
    signal = s;
    return new Promise(() => {});
  });
  const { router } = mount(activity);
  await waitFor(() => expect(activity.journalEvents).toHaveBeenCalled());
  await router.navigate(`/projects/delivery/issues/${issue.id}?section=overview`);
  await waitFor(() => expect(signal?.aborted).toBe(true));
  expect(activity.journalEvent).not.toHaveBeenCalled();
});
it.each(['blocked', 'ready', 'running', 'failed', 'succeeded', 'cancelled'] as const)(
  'keeps %s independent from reported usage and pauses/resumes the same observation',
  async (state) => {
    const activity = fakeActivity(vi);
    const page = journalEventsFixture();
    page.job.job.state = state;
    page.job.job.terminal =
      state === 'failed' || state === 'succeeded' || state === 'cancelled'
        ? { ...page.job.job.terminal!, state }
        : null;
    page.events = [
      {
        sequence: 1,
        occurred_at_unix_ms: 1300,
        detail_url: '/unused',
        facts: {
          kind: 'resource_usage',
          usage: {
            cpu_time_ms: 0,
            elapsed_ms: 800,
            observed_at_unix_ms: 1300,
            disk_current_bytes: 0,
            disk_peak_bytes: 0,
            memory_current_bytes: 0,
            memory_peak_bytes: 0,
            io_read_bytes: 0,
            io_written_bytes: 0,
            network_received_bytes: null,
            network_transmitted_bytes: null,
          },
        },
      },
    ];
    page.job.log.event_cursor = 1;
    activity.journalEvents.mockResolvedValue(page);
    mount(activity);
    const region = await screen.findByRole('region', { name: 'Job events' });
    expect(await within(region).findByText('800')).toBeTruthy();
    expect(within(region).getAllByText('0').length).toBeGreaterThan(0);
    expect(within(region).getAllByText('Not reported')).toHaveLength(2);
    await userEvent.click(within(region).getByRole('button', { name: 'Pause live events' }));
    expect(within(region).getByRole('button', { name: 'Resume live events' })).toBeTruthy();
    expect(activity.journalEvent).not.toHaveBeenCalled();
  },
);

it('backs off an empty live tail, stops polling while hidden and resumes the same cursor on focus', async () => {
  const activity = fakeActivity(vi);
  const page = journalEventsFixture();
  page.events = [];
  page.job.job.terminal = null;
  page.job.job.state = 'running';
  activity.journalEvents.mockResolvedValue(page);
  focusManager.setFocused(false);
  try {
    mount(activity);
    await waitFor(() => expect(activity.journalEvents).toHaveBeenCalledTimes(1));
    await new Promise((resolve) => setTimeout(resolve, 1100));
    expect(activity.journalEvents).toHaveBeenCalledTimes(1);
    focusManager.setFocused(true);
    await waitFor(() => expect(activity.journalEvents).toHaveBeenCalledTimes(2));
    expect(activity.journalEvents.mock.calls[1]?.[4]).toBe('tail.one');
    await new Promise((resolve) => setTimeout(resolve, 1100));
    expect(activity.journalEvents).toHaveBeenCalledTimes(2);
    expect(activity.journalEvent).not.toHaveBeenCalled();
  } finally {
    focusManager.setFocused(undefined);
  }
});

it('increases transport backoff across separate failed reads without a hot retry loop', async () => {
  const activity = fakeActivity(vi);
  activity.journalEvents.mockRejectedValue(
    new ManagementApiError({
      code: 'unavailable',
      status: 503,
      message: 'Temporarily unavailable',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  mount(activity);
  await waitFor(() => expect(activity.journalEvents).toHaveBeenCalledTimes(2), { timeout: 2500 });
  await new Promise((resolve) => setTimeout(resolve, 1200));
  expect(activity.journalEvents).toHaveBeenCalledTimes(2);
});

it('opens a deep-linked exact event independently of the retained live window', async () => {
  const activity = fakeActivity(vi);
  activity.journalEvent.mockResolvedValue({
    cursor: 7,
    items: [
      {
        sequence: 7,
        occurred_at_unix_ms: 2000,
        kind: 'original',
        payload: 'expanded-sequence-seven',
      },
    ],
  });
  mount(activity, '&journal_event=7');
  expect(await screen.findByText(/expanded-sequence-seven/u)).toBeTruthy();
  expect(activity.journalEvent).toHaveBeenCalledWith(
    'delivery',
    issue.id,
    'old-request',
    { build_execution: 'a'.repeat(64), job_id: 'original-job' },
    7,
    expect.any(AbortSignal),
  );
});
it('keeps an explicitly expanded event stable by pausing live reads while its content is open', async () => {
  const activity = fakeActivity(vi);
  const page = journalEventsFixture();
  page.job.job.state = 'running';
  page.job.job.terminal = null;
  activity.journalEvents.mockResolvedValue(page);
  mount(activity);
  await userEvent.click(await screen.findByRole('button', { name: 'Inspect event #1' }));
  await waitFor(() => expect(activity.journalEvent).toHaveBeenCalledTimes(1));
  await new Promise((resolve) => setTimeout(resolve, 1200));
  expect(activity.journalEvents).toHaveBeenCalledTimes(1);
});
