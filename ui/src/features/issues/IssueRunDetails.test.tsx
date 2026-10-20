// @vitest-environment jsdom
import { cleanup, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { setup, issue } from '../../../tests/fixtures/issueTestSupport';
import { fakeActivity, resultsFixture } from '../../../tests/fixtures/issueActivity';
import { executionFixture } from '../../../tests/fixtures/issueExecutions';
import { diagnosticFixture, runFixture } from '../../../tests/fixtures/factory';
import { factoryApi } from '../factory/api';
import type { IssueExecutionsApi } from './executionApi';
import { ManagementApiError } from '../../api/client';
afterEach(cleanup);
it.each([
  ['calls', 'Execution profiles'],
  ['builds', 'Original execution Jobs'],
] as const)(
  'recovers %s from an unreadable continuation without leaving Runs',
  async (collection, label) => {
    const activity = fakeActivity(vi);
    activity.results.mockImplementation(async (_project, _issue, _work, selected, after) => {
      if (selected !== collection) return resultsFixture();
      if (after !== null) {
        throw new ManagementApiError({
          code: 'invalid_request',
          status: 400,
          message: 'Run observation expired',
          requestId: null,
          retryAfterMilliseconds: null,
        });
      }
      return { ...resultsFixture(), next_cursor: 'expired-run-page' };
    });
    const { router } = setup(
      `/projects/delivery/issues/${issue.id}?section=runs&execution=old-request`,
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
        factoryApi: { ...factoryApi, getRun: vi.fn().mockResolvedValue(runFixture()) },
      },
    );
    const section = await screen.findByRole('region', { name: label });
    await userEvent.click(await within(section).findByRole('button', { name: 'Next page' }));
    await within(section).findByText('Run observation expired');
    await userEvent.click(within(section).getByRole('button', { name: 'First page' }));
    await waitFor(() =>
      expect(activity.results).toHaveBeenLastCalledWith(
        'delivery',
        issue.id,
        'retained-work',
        collection,
        null,
        expect.any(AbortSignal),
      ),
    );
    expect(await within(section).findByRole('button', { name: 'Next page' })).toBeTruthy();
    expect(new URLSearchParams(router.state.location.search).get('execution')).toBe('old-request');
    expect(activity.append).not.toHaveBeenCalled();
  },
);
it('shows only reported execution profiles and links original Build Attempt Job logs and separate declared responses', async () => {
  const activity = fakeActivity(vi);
  const node = diagnosticFixture('node-attempts')[0];
  if (node?.kind !== 'node_attempt') throw new Error('node fixture');
  const reference = { identity: 'reported-coder', version: 'v3', digest: 'c'.repeat(64) };
  activity.results.mockImplementation(async (_p, _i, _w, collection) =>
    resultsFixture(
      collection === 'calls'
        ? [
            {
              kind: 'call',
              record: {
                node: node.record,
                profile: {
                  build_configuration: {
                    id: 'configuration',
                    version: 2,
                    project_id: 'delivery',
                    definition_digest: 'a'.repeat(64),
                  },
                  context_digest: 'd'.repeat(64),
                  context_id: 'context-one',
                  model_or_tool: reference,
                  operation_id: 'e'.repeat(64),
                  permission_digest: 'f'.repeat(64),
                  plugin: reference,
                  task_digest: 'a'.repeat(64),
                  tool: reference,
                },
              },
            },
            {
              kind: 'call',
              record: { node: { ...node.record, id: 'unprofiled-node' }, profile: null },
            },
          ]
        : [
            {
              kind: 'build',
              record: {
                id: 'execution-one',
                operation_id: 'operation-one',
                node_attempt_id: 'node-a',
                build_id: 'original-build',
                attempt_id: 'original-attempt',
                jobs: ['original-job'],
              },
            },
          ],
    ),
  );
  const get = vi
    .fn<IssueExecutionsApi['get']>()
    .mockResolvedValue(executionFixture('admitted', 'old-request'));
  setup(`/projects/delivery/issues/${issue.id}?section=runs&execution=old-request`, undefined, {
    issueActivityApi: activity,
    issueExecutionsApi: {
      get,
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
        project_id: 'delivery',
        id: 'retained-run',
        work_id: 'retained-work',
      }),
    },
  });
  expect(await screen.findByText('reported-coder · v3')).toBeTruthy();
  expect(screen.getByText('Execution profile not reported.')).toBeTruthy();
  const log = await screen.findByRole('link', { name: 'Open original Job logs' });
  expect(log.getAttribute('href')).toBe(
    '/builds/original-build?attempt_id=original-attempt&job_id=original-job&log_scope=job%3Aoriginal-job',
  );
  expect(
    screen.getByRole('link', { name: 'Open declared Run responses' }).getAttribute('href'),
  ).toBe('/factory/retained-run?diagnostic=node-attempts');
  expect(activity.append).not.toHaveBeenCalled();
});
