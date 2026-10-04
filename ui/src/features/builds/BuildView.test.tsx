// @vitest-environment jsdom

import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { components } from '../../../.generated/api/schema';
import { createConsoleQueryClient } from '../../app/query';
import { BuildView } from './BuildView';
import type { BuildDiagnosticsApi } from './api';

type AttemptResource = components['schemas']['AttemptResource'];
type BuildResource = components['schemas']['BuildResource'];
type JobResource = components['schemas']['JobResource'];
type JobState = JobResource['state'];
type FailureClassification = NonNullable<JobResource['terminal']>['failure_classification'];

const BUILD_ID = 'build-0001';
const ATTEMPT_ID = 'attempt-0001';

afterEach(cleanup);

describe('Build diagnostics', () => {
  it.each([
    ['running', null, 'Running', 'Not terminal'],
    ['failed', 'execution', 'Failed', 'Execution'],
    ['succeeded', null, 'Succeeded', 'None'],
    ['skipped', 'dependency_policy', 'Skipped', 'Dependency policy'],
  ] satisfies ReadonlyArray<readonly [JobState, FailureClassification, string, string]>)(
    'shows %s Job state and safe failure details',
    async (state, failureClassification, stateLabel, failureLabel) => {
      const job = jobResource('job-only', 'Only job', state, [], failureClassification);
      renderBuild(attemptResource([job], []));

      const details = await screen.findByRole('region', { name: 'Selected Job' });
      expect(within(details).getByText(stateLabel, { selector: 'dd' })).toBeTruthy();
      expect(within(details).getByText(failureLabel, { selector: 'dd' })).toBeTruthy();
    },
  );

  it.each([
    ['active', 'running'],
    ['failed', 'failed'],
    ['succeeded', 'succeeded'],
    ['skipped', 'skipped'],
  ] satisfies ReadonlyArray<readonly [string, JobState]>)(
    'renders the same %s dependency relationship in the SVG and accessible table',
    async (_case, state) => {
      const predecessor = jobResource('job-predecessor', 'Predecessor', state, []);
      const dependent = jobResource('job-dependent', 'Dependent', 'blocked', ['job-predecessor']);
      renderBuild(
        attemptResource([predecessor, dependent], [edge('job-predecessor', 'job-dependent')]),
      );

      const label = 'Predecessor precedes Dependent (All succeeded)';
      expect(
        within(await screen.findByRole('img', { name: 'Attempt dependency graph' })).getByLabelText(
          label,
        ),
      ).toBeTruthy();
      expect(
        within(screen.getByRole('table', { name: 'Attempt dependency table' })).getByLabelText(
          label,
        ),
      ).toBeTruthy();
    },
  );

  it('renders the same mixed dependency relationships in the SVG and accessible table', async () => {
    const prepare = jobResource('job-prepare', 'Prepare', 'succeeded', []);
    const test = jobResource('job-test', 'Test', 'running', ['job-prepare']);
    const packageJob = jobResource('job-package', 'Package', 'blocked', [
      'job-prepare',
      'job-test',
    ]);
    renderBuild(
      attemptResource(
        [prepare, test, packageJob],
        [
          edge('job-prepare', 'job-test'),
          edge('job-prepare', 'job-package'),
          edge('job-test', 'job-package'),
        ],
      ),
    );

    const graph = await screen.findByRole('img', { name: 'Attempt dependency graph' });
    const table = screen.getByRole('table', { name: 'Attempt dependency table' });
    for (const relationship of [
      ['Prepare', 'Test'],
      ['Prepare', 'Package'],
      ['Test', 'Package'],
    ] as const) {
      const label = `${relationship[0]} precedes ${relationship[1]} (All succeeded)`;
      expect(within(graph).getByLabelText(label)).toBeTruthy();
      expect(within(table).getByLabelText(label)).toBeTruthy();
    }
  });

  it('selects a Job from the compact selector or table without changing relationships', async () => {
    const prepare = jobResource('job-prepare', 'Prepare', 'succeeded', []);
    const test = jobResource('job-test', 'Test', 'running', ['job-prepare']);
    renderBuild(attemptResource([prepare, test], [edge('job-prepare', 'job-test')]));

    await userEvent.click(await screen.findByRole('button', { name: 'Select Test, Running' }));
    expect(
      within(screen.getByRole('region', { name: 'Selected Job' })).getByRole('heading', {
        name: 'Test',
      }),
    ).toBeTruthy();

    await userEvent.click(
      within(screen.getByRole('table', { name: 'Attempt dependency table' })).getByRole('button', {
        name: 'Select Prepare',
      }),
    );
    expect(
      within(screen.getByRole('region', { name: 'Selected Job' })).getByRole('heading', {
        name: 'Prepare',
      }),
    ).toBeTruthy();
    expect(screen.getAllByLabelText('Prepare precedes Test (All succeeded)')).toHaveLength(2);
  });

  it('keeps selected Job diagnostics in the URL across a copied deep link', async () => {
    const first = jobResource('job-first', 'First', 'running', []);
    const second = jobResource('job-second', 'Second', 'failed', [], 'execution');
    const { router } = renderBuild(
      attemptResource([first, second], []),
      {},
      `/builds/${BUILD_ID}?job_id=job-second`,
    );

    expect(
      within(await screen.findByRole('region', { name: 'Selected Job' })).getByRole('heading', {
        name: 'Second',
      }),
    ).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Select First, Running' }));
    expect(router.state.location.search).toBe('?job_id=job-first');
  });

  it('updates Job state and failure details from the authoritative follower read', async () => {
    const running = jobResource('job-only', 'Only job', 'running', []);
    const failed = {
      ...jobResource('job-only', 'Only job', 'failed', [], 'execution'),
      event_cursor: 1,
    };
    renderBuild(attemptResource([running], []), {
      getJob: vi.fn().mockResolvedValue(failed),
      getJobEvents: vi.fn().mockResolvedValue({ cursor: 1, items: [jobEvent(1)] }),
    });

    const details = await screen.findByRole('region', { name: 'Selected Job' });
    expect(await within(details).findByText('Failed', { selector: 'dd' })).toBeTruthy();
    expect(within(details).getByText('Execution', { selector: 'dd' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Select Only job, Failed' })).toBeTruthy();
  });

  it('keeps Build-scoped diagnostics available when the current Attempt read fails', async () => {
    renderBuild(attemptResource([], []), {
      getAttempt: vi.fn().mockRejectedValue(new Error('Attempt unavailable')),
    });

    expect(await screen.findByRole('region', { name: 'Published Artifacts' })).toBeTruthy();
    expect((await screen.findByRole('alert')).textContent).toContain(
      'Current Attempt could not be loaded.',
    );
  });

  it('cancels the sole event follower on Job and route changes', async () => {
    const first = jobResource('job-first', 'First', 'running', []);
    const second = jobResource('job-second', 'Second', 'running', []);
    const requests: Array<{ jobId: string; signal: AbortSignal }> = [];
    const { router } = renderBuild(attemptResource([first, second], []), {
      getJobEvents: vi.fn(async (jobId, _request, signal) => {
        requests.push({ jobId, signal });
        return pendingUntilAbort(signal);
      }),
    });

    await waitFor(() => expect(requests.map(({ jobId }) => jobId)).toEqual(['job-first']));
    await userEvent.click(await screen.findByRole('button', { name: 'Select Second, Running' }));
    await waitFor(() => {
      expect(requests[0]?.signal.aborted).toBe(true);
      expect(requests.map(({ jobId }) => jobId)).toEqual(['job-first', 'job-second']);
    });

    await act(async () => router.navigate('/audit'));
    expect(requests[1]?.signal.aborted).toBe(true);
  });
});

function renderBuild(
  attempt: AttemptResource,
  overrides: Partial<BuildDiagnosticsApi> = {},
  initialEntry = `/builds/${BUILD_ID}`,
) {
  const api: BuildDiagnosticsApi = {
    authorizeArtifactDownload: vi.fn(),
    getBuildResultRetention: vi.fn().mockResolvedValue({
      build_id: BUILD_ID,
      deadlines: {
        artifacts_at_unix_ms: 1_700_000_004_000,
        logs_at_unix_ms: 1_700_000_003_000,
        metadata_at_unix_ms: 1_700_000_002_000,
        reports_at_unix_ms: 1_700_000_005_000,
      },
      hold: null,
      visibility: { artifacts: true, logs: true, metadata: true, reports: true },
    }),
    getAttempt: vi.fn().mockResolvedValue(attempt),
    getBuild: vi.fn().mockResolvedValue(buildResource()),
    getJob: vi.fn(async (jobId) => {
      const job = attempt.jobs.find(({ id }) => id === jobId);
      if (job === undefined) throw new Error('Unknown Job fixture');
      return job;
    }),
    getJobEvents: vi.fn((_jobId, _request, signal) => pendingUntilAbort(signal)),
    listBuildArtifacts: vi.fn().mockResolvedValue({ items: [] }),
    listBuildCacheSessions: vi.fn().mockResolvedValue({ items: [] }),
    searchBuildLogs: vi.fn(),
    ...overrides,
  };
  const router = createMemoryRouter(
    [
      { path: '/builds/:buildId', element: <BuildView api={api} /> },
      { path: '/audit', element: <p>Audit</p> },
    ],
    { initialEntries: [initialEntry] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { api, router };
}

function jobEvent(sequence: number) {
  return {
    kind: 'runner.finished',
    occurred_at_unix_ms: 1_700_000_002_000,
    payload: {},
    sequence,
  };
}

function buildResource(): BuildResource {
  return {
    configuration_id: 'configuration-0001',
    configuration_version: 3,
    created_at_unix_ms: 1_700_000_000_000,
    current_attempt: {
      build_id: BUILD_ID,
      created_at_unix_ms: 1_700_000_000_000,
      id: ATTEMPT_ID,
      number: 2,
      retry_of_attempt_id: 'attempt-0000',
      state: 'running',
      updated_at_unix_ms: 1_700_000_001_000,
      version: 4,
    },
    effective_policy: {
      policy: {
        artifacts: {
          artifact_bytes: 0,
          artifact_count: 0,
          report_bytes: 0,
          report_count: 0,
          single_output_bytes: 0,
        },
        cache: { max_bytes: 0, namespaces: [], read: false, write: false },
        concurrency: { active_builds: 1, active_jobs: 1 },
        execution_targets: [],
        identity_profiles: [],
        pools: [],
        repositories: [],
        retention: {
          artifact_seconds: 0,
          build_seconds: 0,
          cache_seconds: 0,
          log_seconds: 0,
        },
        runtimes: [],
        secret_profiles: [],
      },
      sources: [],
    },
    id: BUILD_ID,
    immutable_revision: '0123456789abcdef',
    parameters: {},
    pipeline_id: 'pipeline-0001',
    pipeline_version: 5,
    priority: 0,
    project_id: 'project-0001',
    repository_id: 'repository-0001',
    repository_version: 7,
    source: { kind: 'exact_revision', value: '0123456789abcdef' },
    state: 'running',
    trigger: {
      build_id: BUILD_ID,
      causality: {
        depth: 0,
        parent_occurrence_id: null,
        root_occurrence_id: 'trigger-occurrence-0001',
      },
      cause: { kind: 'manual' },
      created_at: 1_700_000_000_000,
      deduplication_identity: 'manual-0001',
      id: 'trigger-occurrence-0001',
      kind: 'manual',
      source_time: 1_700_000_000_000,
      state: 'accepted',
      target: { configuration_id: 'configuration-0001', configuration_version: 3 },
      trigger: { id: 'trigger-0001', version: 1 },
      updated_at: 1_700_000_000_000,
    },
    updated_at_unix_ms: 1_700_000_001_000,
    version: 4,
  };
}

function attemptResource(jobs: JobResource[], edges: AttemptResource['edges']): AttemptResource {
  return {
    attempt: buildResource().current_attempt,
    edges,
    jobs,
  };
}

function jobResource(
  id: string,
  pipelineNodeId: string,
  state: JobState,
  dependencyJobIds: string[],
  failureClassification: FailureClassification = null,
): JobResource {
  const terminalState =
    state === 'failed' || state === 'succeeded' || state === 'cancelled' || state === 'skipped'
      ? state
      : null;
  return {
    allowed_pool_ids: [],
    assignment: null,
    attempt_id: ATTEMPT_ID,
    created_at_unix_ms: 1_700_000_000_000,
    dependency_job_ids: dependencyJobIds,
    dependency_policy: 'all_succeeded',
    event_cursor: 0,
    id,
    outputs: [],
    pipeline_node_id: pipelineNodeId,
    placement: {
      architecture: 'arm64',
      capabilities: [],
      labels: {},
      minimum_cpu_millis: 0,
      minimum_disk_bytes: 0,
      minimum_memory_bytes: 0,
      operating_system: 'linux',
      runtime_class: 'virtualization',
    },
    queue: null,
    state,
    terminal:
      terminalState === null
        ? null
        : {
            completed_at_unix_ms: 1_700_000_002_000,
            failure_classification:
              failureClassification === 'execution' ||
              failureClassification === 'infrastructure' ||
              failureClassification === 'cancelled' ||
              failureClassification === 'dependency_policy'
                ? failureClassification
                : null,
            state: terminalState,
          },
    updated_at_unix_ms: 1_700_000_001_000,
    version: 2,
  };
}

function edge(predecessorJobId: string, dependentJobId: string): AttemptResource['edges'][number] {
  return {
    dependency_policy: 'all_succeeded',
    dependent_job_id: dependentJobId,
    predecessor_job_id: predecessorJobId,
  };
}

function pendingUntilAbort(signal: AbortSignal): Promise<never> {
  return new Promise((_resolve, reject) => {
    const rejectAbort = () => reject(new DOMException('Aborted', 'AbortError'));
    if (signal.aborted) rejectAbort();
    else signal.addEventListener('abort', rejectAbort, { once: true });
  });
}
