// @vitest-environment jsdom
import { cleanup, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { setup, issue } from '../../../tests/fixtures/issueTestSupport';

import {
  record,
  execution,
  subject,
  checksPage,
  artifactsPage,
  fakeResultPanels,
} from '../../../tests/fixtures/issueResultPanels';
const path = `/projects/delivery/issues/${issue.id}?section=results&execution=old-request&result_pane=checks&check_record=${record}`;
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
it('opens exact historical Factory checks without turning a partial pass into acceptance', async () => {
  const panels = {
    ...fakeResultPanels(vi),
    checks: vi.fn().mockResolvedValue(checksPage()),
    artifacts: vi.fn(),
    externalChecks: vi.fn(),
    content: vi.fn(),
  };
  setup(path, (api) => api.get.mockResolvedValue({ ...issue, version: 2 }), {
    issueResultPanelsApi: panels,
    issueExecutionsApi: {
      get: vi.fn().mockResolvedValue(execution),
      list: vi.fn(),
      start: vi.fn(),
    },
  });
  const section = await screen.findByRole('region', { name: 'Factory checks' });
  expect(await within(section).findByText('Required unit tests <script>')).toBeTruthy();
  expect(within(section).getByText('Missing')).toBeTruthy();
  expect(within(section).getByText('Passed')).toBeTruthy();
  expect(within(section).getAllByText('historical-head').length).toBeGreaterThan(0);
  expect(
    screen.getByText(
      'This execution uses earlier requirements. Current edits apply to a later execution.',
    ),
  ).toBeTruthy();
  expect(
    within(section).getByText(
      'Partial observation. Empty or final pages do not establish verification or acceptance.',
    ),
  ).toBeTruthy();
  expect(section.querySelector('script')).toBeNull();
  expect(within(section).getByText('Provenance').parentElement?.hasAttribute('open')).toBe(false);
  await userEvent.click(within(section).getByText('Provenance'));
  expect(within(section).getAllByText(record).length).toBeGreaterThan(0);
  expect(panels.checks).toHaveBeenCalledWith(
    'delivery',
    issue.id,
    'retained-work',
    record,
    null,
    expect.any(AbortSignal),
  );
  expect(panels.content).not.toHaveBeenCalled();
});

import { fakeActivity, resultsFixture } from '../../../tests/fixtures/issueActivity';
import { waitFor } from '@testing-library/react';
it.each(['candidate', 'changeset'] as const)(
  'selects a retained %s publication and pages only its checks, preserving selection through navigation',
  async (kind) => {
    const activity = fakeActivity(vi);
    const publicationPage = resultsFixture([
      {
        kind: 'domain_fact',
        record: {
          id: record,
          flow_run_id: 'flow',
          cycle_id: 'cycle',
          node_attempt_id: 'node',
          input_digest: 'ef'.repeat(32),
          observed_at: 1,
          facts:
            kind === 'candidate'
              ? { kind, subject }
              : {
                  kind,
                  id: '77777777-7777-4777-8777-777777777777',
                  node_attempt_id: 'node',
                  subject,
                },
        },
      },
    ]);
    activity.results.mockImplementation(async (_project, _issue, _work, collection) =>
      collection === 'candidates' ? publicationPage : resultsFixture(),
    );
    const panels = {
      ...fakeResultPanels(vi),
      checks: vi.fn().mockResolvedValue({ ...checksPage(), next_cursor: 'check-tail' }),
    };
    const { router } = setup(path.replace(`&check_record=${record}`, ''), undefined, {
      issueResultPanelsApi: panels,
      issueActivityApi: activity,
      issueExecutionsApi: {
        get: vi.fn().mockResolvedValue(execution),
        list: vi.fn(),
        start: vi.fn(),
      },
    });
    const candidates = await screen.findByRole('region', { name: 'Retained candidates' });
    expect(panels.checks).not.toHaveBeenCalled();
    await userEvent.click(await within(candidates).findByRole('link', { name: 'historical-head' }));
    const section = await screen.findByRole('region', { name: 'Factory checks' });
    await userEvent.click(await within(section).findByRole('button', { name: 'Next page' }));
    await waitFor(() =>
      expect(panels.checks).toHaveBeenCalledWith(
        'delivery',
        issue.id,
        'retained-work',
        record,
        'check-tail',
        expect.any(AbortSignal),
      ),
    );
    await userEvent.click(screen.getByRole('link', { name: 'Activity' }));
    await userEvent.click(screen.getByRole('link', { name: 'Results' }));
    expect(new URLSearchParams(router.state.location.search).get('check_record')).toBe(record);
    expect(new URLSearchParams(router.state.location.search).get('check_after')).toBe('check-tail');
    expect(activity.results).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      'retained-work',
      'candidates',
      null,
      expect.any(AbortSignal),
    );
  },
);

it('groups authorized artifact metadata and leaves expired or unavailable content as references', async () => {
  const panels = {
    ...fakeResultPanels(vi),
    checks: vi.fn(),
    artifacts: vi.fn().mockResolvedValue(artifactsPage()),
    content: vi.fn(),
  };
  setup(path.replace('result_pane=checks', 'result_pane=artifacts'), undefined, {
    issueResultPanelsApi: panels,
    issueExecutionsApi: {
      get: vi.fn().mockResolvedValue(execution),
      list: vi.fn(),
      start: vi.fn(),
    },
  });
  const section = await screen.findByRole('region', { name: 'Execution artifacts' });
  expect(await within(section).findByText('Original task')).toBeTruthy();
  expect(within(section).getByRole('heading', { name: 'Original inputs' })).toBeTruthy();
  expect(within(section).getByRole('heading', { name: 'Reports' })).toBeTruthy();
  expect(within(section).getByText('Project input')).toBeTruthy();
  const expired = within(section).getByText('Expired report').closest('li')!;
  expect(within(expired).getByText('Expired')).toBeTruthy();
  expect(within(expired).queryByRole('button', { name: 'Preview' })).toBeNull();
  expect(within(expired).queryByRole('button', { name: 'Download' })).toBeNull();
  expect(within(section).getByText('Unavailable artifact')).toBeTruthy();
  expect(section.textContent).not.toContain('unused');
  expect(panels.content).not.toHaveBeenCalled();
  await userEvent.click(within(section).getByRole('button', { name: 'Next page' }));
  await waitFor(() =>
    expect(panels.artifacts).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      'retained-work',
      'artifact-tail',
      expect.any(AbortSignal),
    ),
  );
});

import { Blob as BinaryBlob } from 'node:buffer';
it('reads a plaintext preview only on demand and requires an explicit download request', async () => {
  vi.stubGlobal(
    'URL',
    class extends URL {
      static override createObjectURL = vi
        .fn()
        .mockReturnValue('blob:http://localhost/retained-output');
      static override revokeObjectURL = vi.fn();
    },
  );
  const page = artifactsPage();
  page.items = [page.items[0]!];
  const panels = {
    ...fakeResultPanels(vi),
    checks: vi.fn(),
    artifacts: vi.fn().mockResolvedValue(page),
    content: vi.fn().mockImplementation(
      async (_p, _i, _w, _a, mode) =>
        new BinaryBlob(['<script>private report text</script>'], {
          type: mode === 'preview' ? 'text/plain; charset=utf-8' : 'application/octet-stream',
        }),
    ),
  };
  const view = setup(path.replace('result_pane=checks', 'result_pane=artifacts'), undefined, {
    issueResultPanelsApi: panels,
    issueExecutionsApi: {
      get: vi.fn().mockResolvedValue(execution),
      list: vi.fn(),
      start: vi.fn(),
    },
  });
  const section = await screen.findByRole('region', { name: 'Execution artifacts' });
  await within(section).findByText('Original task');
  expect(panels.content).not.toHaveBeenCalled();
  await userEvent.click(within(section).getByRole('button', { name: 'Preview' }));
  expect(await within(section).findByText('<script>private report text</script>')).toBeTruthy();
  expect(section.querySelector('script')).toBeNull();
  expect(panels.content).toHaveBeenLastCalledWith(
    'delivery',
    issue.id,
    'retained-work',
    'input-one',
    'preview',
    expect.any(AbortSignal),
  );
  await userEvent.click(within(section).getByRole('button', { name: 'Download' }));
  const save = await within(section).findByRole('link', { name: 'Save file' });
  expect(save.getAttribute('href')).toBe('blob:http://localhost/retained-output');
  expect(save.getAttribute('download')).toBe('Original task');
  expect(panels.content).toHaveBeenLastCalledWith(
    'delivery',
    issue.id,
    'retained-work',
    'input-one',
    'download',
    expect.any(AbortSignal),
  );
  await userEvent.click(within(section).getByRole('button', { name: 'Close preview' }));
  expect(within(section).queryByText('<script>private report text</script>')).toBeNull();
  view.router.dispose();
  cleanup();
  expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:http://localhost/retained-output');
  vi.unstubAllGlobals();
});

import { reviewFixture, reviewPage } from '../../../tests/fixtures/issueCodeReviews';
import { fakeIssueCodeReviewsApi } from '../../../tests/fixtures/issueTestSupport';
it('shows historical external CI on a manual task independently of Factory and pages the original review', async () => {
  const review = reviewFixture();
  const external: components['schemas']['IssueExternalChecksPage'] = {
    association: review.association,
    availability: 'available',
    coverage: 'partial',
    source: 'external_ci',
    next_cursor: 'older-head',
    items: [
      {
        id: 'aa'.repeat(32),
        head: 'head-one',
        observed_at: 1700000000000,
        checks: {
          source: 'external_ci',
          head: 'head-one',
          observed_at: 1700000000000,
          has_more: true,
          items: [
            {
              id: 'ci-lint',
              name: 'External lint',
              producer: 'independent-ci',
              state: 'skipped',
              required: null,
            },
          ],
        },
      },
    ],
  };
  const panels = {
    ...fakeResultPanels(vi),
    externalChecks: vi.fn().mockResolvedValue(external),
    checks: vi.fn(),
    artifacts: vi.fn(),
    content: vi.fn(),
  };
  const reviews = {
    ...fakeIssueCodeReviewsApi(),
    list: vi.fn().mockResolvedValue(reviewPage([review])),
  };
  setup(`/projects/delivery/issues/${issue.id}?section=results&result_pane=checks`, undefined, {
    issueResultPanelsApi: panels,
    issueCodeReviewsApi: reviews,
  });
  const section = await screen.findByRole('region', { name: 'External CI · MR-42' });
  expect(await within(section).findByText('External lint')).toBeTruthy();
  expect(within(section).getByText('Skipped')).toBeTruthy();
  expect(within(section).getByText('Required policy not reported')).toBeTruthy();
  expect(within(section).getByText('Historical observation for a different head.')).toBeTruthy();
  expect(within(section).getByText('More external checks were not retained.')).toBeTruthy();
  expect(
    within(section).getByText(
      'External reviews and checks do not establish Factory acceptance or change task status.',
    ),
  ).toBeTruthy();
  await userEvent.click(within(section).getByRole('button', { name: 'Next page' }));
  await waitFor(() =>
    expect(panels.externalChecks).toHaveBeenCalledWith(
      'delivery',
      issue.id,
      review.association.id,
      'older-head',
      expect.any(AbortSignal),
    ),
  );
  expect(panels.checks).not.toHaveBeenCalled();
  expect(reviews.refresh).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('link', { name: 'Overview' }));
  expect(await screen.findByRole('button', { name: 'Edit requirements' })).toBeTruthy();
});

it('clears candidate and artifact continuations when another execution is deliberately selected', async () => {
  const activity = fakeActivity(vi);
  const later = {
    ...execution,
    id: 'later-request',
    association: {
      ...execution.association!,
      request_id: 'later-request',
      work_id: 'later-work',
      run_id: 'later-run',
    },
  };
  const panels = fakeResultPanels(vi);
  const { router } = setup(
    path + '&check_after=old-checks&candidate_after=old-candidates&artifact_after=old-artifacts',
    undefined,
    {
      issueResultPanelsApi: panels,
      issueActivityApi: activity,
      issueExecutionsApi: {
        get: vi
          .fn()
          .mockImplementation(async (_p, _i, id) => (id === 'later-request' ? later : execution)),
        list: vi.fn().mockResolvedValue({
          items: [execution, later],
          next_cursor: null,
          watermark: 2,
          issue_version: 1,
          observation: 'request-watermark-v1',
        }),
        start: vi.fn(),
      },
    },
  );
  await screen.findByRole('region', { name: 'Factory checks' });
  await userEvent.click(screen.getByRole('link', { name: 'Runs' }));
  const history = await screen.findByRole('region', { name: 'Execution history' });
  const links = await within(history).findAllByRole('link');
  const link = links.find(
    (link) =>
      new URL(link.getAttribute('href')!, 'http://test').searchParams.get('execution') ===
      'later-request',
  )!;
  await userEvent.click(link);
  const next = new URLSearchParams(router.state.location.search);
  for (const key of ['check_record', 'check_after', 'candidate_after', 'artifact_after'])
    expect(next.get(key)).toBeNull();
  expect(next.get('result_pane')).toBe('checks');
});

it('exposes exact Build, Attempt, Job and candidate provenance without rendering private extra members', async () => {
  const page = artifactsPage();
  page.items = [page.items[0]!];
  const item = page.items[0]!;
  item.producer = {
    build_id: 'producer-build',
    attempt_id: 'producer-attempt',
    job_id: 'producer-job',
    plugin: { identity: 'safe-plugin', version: 'v1', digest: 'ab'.repeat(32) },
    tool: { identity: 'safe-tool', version: 'v1', digest: 'cd'.repeat(32) },
  };
  item.subject = subject;
  item.record = 'ef'.repeat(32);
  Object.assign(item.producer, { private_url: 'https://private.example/secret' });
  const panels = { ...fakeResultPanels(vi), artifacts: vi.fn().mockResolvedValue(page) };
  setup(path.replace('result_pane=checks', 'result_pane=artifacts'), undefined, {
    issueResultPanelsApi: panels,
    issueExecutionsApi: {
      get: vi.fn().mockResolvedValue(execution),
      list: vi.fn(),
      start: vi.fn(),
    },
  });
  const section = await screen.findByRole('region', { name: 'Execution artifacts' });
  await userEvent.click(await within(section).findByText('Provenance'));
  for (const value of ['producer-build', 'producer-attempt', 'producer-job', 'historical-head'])
    expect(within(section).getByText(value)).toBeTruthy();
  expect(section.textContent).not.toContain('https://private.example/secret');
  expect(
    within(section).getByRole('link', { name: 'Open producing Build' }).getAttribute('href'),
  ).toBe('/builds/producer-build?attempt_id=producer-attempt&job_id=producer-job');
});

import { ManagementApiError } from '../../api/client';
it('retains stale artifact metadata but closes byte expansions when a refresh is forbidden', async () => {
  const page = artifactsPage();
  page.items = [page.items[0]!];
  const panels = {
    ...fakeResultPanels(vi),
    artifacts: vi.fn().mockResolvedValue(page),
    content: vi
      .fn()
      .mockResolvedValue(new BinaryBlob(['retained private text'], { type: 'text/plain' })),
  };
  setup(path.replace('result_pane=checks', 'result_pane=artifacts'), undefined, {
    issueResultPanelsApi: panels,
    issueExecutionsApi: {
      get: vi.fn().mockResolvedValue(execution),
      list: vi.fn(),
      start: vi.fn(),
    },
  });
  const section = await screen.findByRole('region', { name: 'Execution artifacts' });
  await userEvent.click(await within(section).findByRole('button', { name: 'Preview' }));
  await within(section).findByText('retained private text');
  panels.artifacts.mockRejectedValue(
    new ManagementApiError({
      code: 'forbidden',
      status: 403,
      message: 'Artifact access revoked',
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(within(section).getByRole('button', { name: 'Refresh results' }));
  await within(section).findByText('Artifact access revoked');
  expect(within(section).getByText('Original task')).toBeTruthy();
  expect(within(section).queryByText('retained private text')).toBeNull();
  expect(within(section).queryByRole('button', { name: 'Preview' })).toBeNull();
  expect(within(section).getByRole('button', { name: 'Next page' }).hasAttribute('disabled')).toBe(
    true,
  );
  await userEvent.click(
    screen.getByRole('navigation', { name: 'Result panels' }).querySelector('a')!,
  );
  expect(await screen.findByRole('region', { name: 'Execution results' })).toBeTruthy();
});
it.each(['text/html', 'image/svg+xml'])(
  'refuses %s bytes as a preview without creating a content URL',
  async (type) => {
    vi.stubGlobal(
      'URL',
      class extends URL {
        static override createObjectURL = vi.fn();
        static override revokeObjectURL = vi.fn();
      },
    );
    const page = artifactsPage();
    page.items = [page.items[0]!];
    const panels = {
      ...fakeResultPanels(vi),
      artifacts: vi.fn().mockResolvedValue(page),
      content: vi
        .fn()
        .mockResolvedValue(new BinaryBlob(['<script>must not appear</script>'], { type })),
    };
    setup(path.replace('result_pane=checks', 'result_pane=artifacts'), undefined, {
      issueResultPanelsApi: panels,
      issueExecutionsApi: {
        get: vi.fn().mockResolvedValue(execution),
        list: vi.fn(),
        start: vi.fn(),
      },
    });
    const section = await screen.findByRole('region', { name: 'Execution artifacts' });
    await userEvent.click(await within(section).findByRole('button', { name: 'Preview' }));
    await within(section).findByRole('alert');
    expect(section.textContent).not.toContain('must not appear');
    expect(section.querySelector('iframe,object,script')).toBeNull();
    expect(URL.createObjectURL).not.toHaveBeenCalled();
  },
);
it.each(['stale', 'unavailable'] as const)(
  'shows %s coverage independently without an empty-success message or content controls',
  async (coverage) => {
    const artifacts = { ...artifactsPage(), coverage, items: [], next_cursor: null };
    const panels = { ...fakeResultPanels(vi), artifacts: vi.fn().mockResolvedValue(artifacts) };
    setup(path.replace('result_pane=checks', 'result_pane=artifacts'), undefined, {
      issueResultPanelsApi: panels,
      issueExecutionsApi: {
        get: vi.fn().mockResolvedValue(execution),
        list: vi.fn(),
        start: vi.fn(),
      },
    });
    const section = await screen.findByRole('region', { name: 'Execution artifacts' });
    await waitFor(() =>
      expect(within(section).queryByText('Loading Execution artifacts…')).toBeNull(),
    );
    expect(within(section).queryByText('No artifact references in this partial page.')).toBeNull();
    expect(within(section).queryByRole('button', { name: 'Preview' })).toBeNull();
    expect(panels.content).not.toHaveBeenCalled();
  },
);

it('announces a pending download independently from preview and aborts it on leave', async () => {
  const page = artifactsPage();
  page.items = [page.items[0]!];
  let signal: AbortSignal | undefined;
  const panels = {
    ...fakeResultPanels(vi),
    artifacts: vi.fn().mockResolvedValue(page),
    content: vi.fn().mockImplementation((_p, _i, _w, _a, _mode, readSignal) => {
      signal = readSignal;
      return new Promise(() => {});
    }),
  };
  setup(path.replace('result_pane=checks', 'result_pane=artifacts'), undefined, {
    issueResultPanelsApi: panels,
    issueExecutionsApi: {
      get: vi.fn().mockResolvedValue(execution),
      list: vi.fn(),
      start: vi.fn(),
    },
  });
  const section = await screen.findByRole('region', { name: 'Execution artifacts' });
  await userEvent.click(await within(section).findByRole('button', { name: 'Download' }));
  expect(await within(section).findByText('Loading Download…')).toBeTruthy();
  await userEvent.click(screen.getByRole('link', { name: 'Overview' }));
  expect(signal?.aborted).toBe(true);
});

import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../app/presentation/preferences';
it.each([
  ['en', 'Execution artifacts', 'No expiry'],
  ['ru', 'Артефакты выполнения', 'Без срока удаления'],
] as const)(
  'shows known indefinite artifact retention in %s without treating it as unknown metadata',
  async (language, title, label) => {
    localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ ...DEFAULT_PREFERENCES, language }));
    try {
      const page = artifactsPage();
      page.items = [page.items[0]!];
      page.items[0]!.metadata!.expires_at = null;
      setup(path.replace('result_pane=checks', 'result_pane=artifacts'), undefined, {
        issueResultPanelsApi: {
          ...fakeResultPanels(vi),
          artifacts: vi.fn().mockResolvedValue(page),
        },
        issueExecutionsApi: {
          get: vi.fn().mockResolvedValue(execution),
          list: vi.fn(),
          start: vi.fn(),
        },
      });
      const section = await screen.findByRole('region', { name: title });
      expect(await within(section).findByText(label)).toBeTruthy();
      expect(
        within(section).queryByText(language === 'en' ? 'Not reported' : 'Не сообщено'),
      ).toBeNull();
    } finally {
      localStorage.removeItem(PREFERENCES_KEY);
    }
  },
);
