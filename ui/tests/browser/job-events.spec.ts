import { expect, test, type Page, type Route } from '@playwright/test';

const BUILD_ID = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const ATTEMPT_ID = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const JOB_ID = 'cccccccc-cccc-4ccc-8ccc-cccccccccccc';
const ARTIFACT_ID = 'dddddddd-dddd-4ddd-8ddd-dddddddddddd';

test('confirms Build cancellation and retry with distinct command identities', async ({ page }) => {
  let state: 'cancelled' | 'failed' | 'running' = 'running';
  const commands: Array<{ key: string | undefined; path: string }> = [];
  await mockBuildDiagnostics(page, {
    onBuildRequest: async (route) => fulfillJson(route, buildResource(state)),
    onCancel: async (route) => {
      commands.push({
        key: route.request().headers()['idempotency-key'],
        path: new URL(route.request().url()).pathname,
      });
      state = 'cancelled';
      await fulfillJson(route, {
        attempt_id: ATTEMPT_ID,
        build_id: BUILD_ID,
        cancelled_job_ids: [JOB_ID],
        cancelling_job_ids: [],
        disposition: 'applied',
      });
    },
    onEventRequest: async (route) => fulfillJson(route, { cursor: 0, items: [] }),
    onJobRequest: async (route) =>
      fulfillJson(route, { event_cursor: 0, id: JOB_ID, state: 'succeeded' }),
    onRetry: async (route) => {
      commands.push({
        key: route.request().headers()['idempotency-key'],
        path: new URL(route.request().url()).pathname,
      });
      state = 'running';
      await fulfillJson(route, {
        attempt_id: 'attempt-next',
        attempt_number: 2,
        build_id: BUILD_ID,
        disposition: 'applied',
        ready_job_ids: [JOB_ID],
        source_attempt_id: ATTEMPT_ID,
      });
    },
  });

  await page.goto(`/builds/${BUILD_ID}`);
  await page.getByRole('button', { name: 'Cancel Build' }).click();
  let dialog = page.getByRole('dialog', { name: `Cancel Build ${BUILD_ID}?` });
  await dialog.getByRole('button', { name: 'Cancel Build' }).click();
  await expect(dialog.getByRole('status')).toContainText('Cancellation accepted (Applied).');
  await expect(page.getByText('Cancelled')).toBeVisible();
  await dialog.getByRole('button', { name: 'Close' }).click();

  state = 'failed';
  await page.reload();
  await page.getByRole('button', { name: 'Retry Build' }).click();
  dialog = page.getByRole('dialog', { name: `Retry Build ${BUILD_ID}?` });
  await dialog.getByRole('button', { name: 'Retry Build' }).click();
  await expect(dialog.getByRole('status')).toContainText('Retry Attempt 2 created (Applied).');
  await expect(page.getByText('Running', { exact: true }).first()).toBeVisible();

  expect(commands.map(({ path }) => path)).toEqual([
    `/api/v1/builds/${BUILD_ID}/cancel`,
    `/api/v1/builds/${BUILD_ID}/retry`,
  ]);
  expect(commands[0]?.key).toMatch(/^[0-9a-f-]{36}$/i);
  expect(commands[1]?.key).toMatch(/^[0-9a-f-]{36}$/i);
  expect(commands[0]?.key).not.toBe(commands[1]?.key);
});

test('keeps a single-Job Attempt graph compact', async ({ page }) => {
  await mockBuildDiagnostics(page, {
    onEventRequest: async (route) => {
      await fulfillJson(route, { cursor: 0, items: [] });
    },
    onJobRequest: async (route) => {
      await fulfillJson(route, { event_cursor: 0, id: JOB_ID, state: 'succeeded' });
    },
  });

  await page.goto(`/builds/${BUILD_ID}`);

  const graph = page.getByRole('img', { name: 'Attempt dependency graph' });
  const nodeBounds = await graph.locator('g[aria-label^="compile, "] > rect').boundingBox();
  if (nodeBounds === null) throw new Error('single Job graph node was not rendered');
  expect(nodeBounds.width).toBeLessThanOrEqual(180);
  expect(nodeBounds.height).toBeLessThanOrEqual(80);
});

test('follows one bounded ordered Job stream without duplicate rows or parallel pollers', async ({
  page,
}) => {
  const eventRequests: Array<{ after: string | null; limit: string | null; wait: string | null }> =
    [];
  let activeEventRequests = 0;
  let maximumActiveEventRequests = 0;
  let jobReads = 0;
  await mockBuildDiagnostics(page, {
    onEventRequest: async (route, url) => {
      activeEventRequests += 1;
      maximumActiveEventRequests = Math.max(maximumActiveEventRequests, activeEventRequests);
      eventRequests.push({
        after: url.searchParams.get('after'),
        limit: url.searchParams.get('limit'),
        wait: url.searchParams.get('wait_ms'),
      });
      const requestNumber = eventRequests.length;
      const items =
        requestNumber === 1
          ? [jobEvent(1, 'runner.started')]
          : [jobEvent(1, 'runner.started'), jobEvent(2, 'runner.finished')];
      await fulfillJson(route, { cursor: requestNumber, items });
      activeEventRequests -= 1;
    },
    onJobRequest: async (route) => {
      jobReads += 1;
      await fulfillJson(route, {
        event_cursor: 2,
        id: JOB_ID,
        state: jobReads === 1 ? 'running' : 'succeeded',
      });
    },
  });

  await page.goto(`/builds/${BUILD_ID}`);

  const events = page.getByRole('table', { name: 'Ordered Job events' });
  await expect(events.getByRole('row')).toHaveCount(3);
  await expect(events.getByRole('row').nth(1)).toContainText('runner.started');
  await expect(events.getByRole('row').nth(2)).toContainText('runner.finished');
  await expect(
    page.getByRole('status').filter({ hasText: 'Event history complete' }),
  ).toBeVisible();
  await expect
    .poll(() => eventRequests)
    .toEqual([
      { after: '0', limit: '256', wait: '30000' },
      { after: '1', limit: '256', wait: '30000' },
    ]);

  await page.waitForTimeout(100);
  expect(eventRequests).toHaveLength(2);
  expect(maximumActiveEventRequests).toBe(1);
});

test('round-trips scoped log filters and paginates only redacted search responses', async ({
  page,
}) => {
  const searchRequests: URL[] = [];
  const browserMessages: string[] = [];
  page.on('console', (message) => browserMessages.push(message.text()));
  await mockBuildDiagnostics(page, {
    onEventRequest: async (route) => {
      await fulfillJson(route, { cursor: 0, items: [] });
    },
    onJobRequest: async (route) => {
      await fulfillJson(route, { event_cursor: 0, id: JOB_ID, state: 'succeeded' });
    },
    onLogSearch: async (route, url) => {
      searchRequests.push(url);
      const cursor = url.searchParams.get('after');
      const fullText = url.searchParams.get('mode') === 'full_text';
      await fulfillJson(route, {
        debug_private_location: 'DO_NOT_LOG_RAW_BODY',
        freshness: {
          caught_up: cursor !== null || fullText,
          committed_through: 9,
          indexed_through: cursor === null && !fullText ? 7 : 9,
        },
        items: fullText
          ? []
          : [
              {
                attempt_id: ATTEMPT_ID,
                build_id: BUILD_ID,
                chunk_id: cursor === null ? 'chunk-one' : 'chunk-two',
                first_sequence: cursor === null ? 4 : 8,
                job_id: JOB_ID,
                last_sequence: cursor === null ? 6 : 9,
                occurred_at_unix_ms: 1_700_000_000_000,
                snippet:
                  cursor === null
                    ? 'safe <script>window.stolen = true</script> text'
                    : 'second redacted page',
                stream: 'stderr',
              },
            ],
        next_cursor: cursor === null && !fullText ? 'opaque-next' : null,
      });
    },
  });

  await page.goto(
    `/builds/${BUILD_ID}?log_query=compile&log_mode=literal&log_scope=job%3A${JOB_ID}&log_stream=stderr`,
  );

  const search = page.getByRole('region', { name: 'Redacted log search' });
  await expect(search.getByText(/safe <script>window\.stolen/)).toBeVisible();
  await expect(search.getByRole('status')).toContainText('Results may be incomplete');
  expect(await page.locator('script').filter({ hasText: 'window.stolen' }).count()).toBe(0);
  expect(searchRequests[0]?.searchParams.get('build_id')).toBe(BUILD_ID);
  expect(searchRequests[0]?.searchParams.get('attempt_id')).toBe(ATTEMPT_ID);
  expect(searchRequests[0]?.searchParams.get('job_id')).toBe(JOB_ID);

  await search.getByRole('button', { name: 'Load more log matches' }).click();
  await expect(search.getByText('second redacted page')).toBeVisible();
  expect(searchRequests[1]?.searchParams.get('after')).toBe('opaque-next');

  await search.getByRole('searchbox', { name: 'Log query' }).fill('link failed');
  await search.getByRole('combobox', { name: 'Search mode' }).selectOption('full_text');
  await search.getByRole('combobox', { name: 'Search scope' }).selectOption('attempt');
  await search.getByRole('combobox', { name: 'Log stream' }).selectOption('stdout');
  await search.getByRole('button', { name: 'Search logs' }).click();
  await expect(page).toHaveURL(
    `/builds/${BUILD_ID}?log_query=link+failed&log_mode=full_text&log_scope=attempt&log_stream=stdout`,
  );
  await expect(search.getByText('No redacted log matches the selected filters.')).toBeVisible();
  expect(searchRequests[2]?.searchParams.get('mode')).toBe('full_text');
  expect(searchRequests[2]?.searchParams.get('job_id')).toBeNull();
  expect(browserMessages.some((message) => message.includes('DO_NOT_LOG_RAW_BODY'))).toBe(false);
});

test('shows secret-free Build Result diagnostics and launches an ephemeral download', async ({
  page,
}) => {
  const privateUrl = 'https://objects.example.invalid/output?capability=DO_NOT_PERSIST';
  const cacheSecret = 'DO_NOT_RENDER_CACHE_BEARER';
  const downloads: Array<{ features: string | null; target: string | null; url: string }> = [];
  const browserMessages: string[] = [];
  page.on('console', (message) => browserMessages.push(message.text()));
  await page.exposeFunction(
    'recordArtifactDownload',
    (url: string, target: string | null, features: string | null) => {
      downloads.push({ features, target, url });
    },
  );
  await page.addInitScript(() => {
    const browser = globalThis as unknown as {
      open: (url?: string | URL, target?: string, features?: string) => Window | null;
      recordArtifactDownload: (
        url: string,
        target: string | null,
        features: string | null,
      ) => Promise<void>;
    };
    browser.open = (_url, target, features) => {
      const popupDocument = document.implementation.createHTMLDocument();
      return {
        close: () => undefined,
        document: popupDocument,
        location: {
          replace: (url: string) => {
            void browser.recordArtifactDownload(String(url), target ?? null, features ?? null);
          },
        },
        opener: browser,
      } as unknown as Window;
    };
  });
  let downloadRequests = 0;
  await mockBuildDiagnostics(page, {
    onArtifactDownload: async (route) => {
      downloadRequests += 1;
      await fulfillJson(route, {
        artifact: artifactResource(),
        expires_at_unix_ms: 1_700_000_100_000,
        get_url: privateUrl,
      });
    },
    onArtifacts: async (route, url) => {
      expect(url.searchParams.get('limit')).toBe('100');
      await fulfillJson(route, { items: [artifactResource()] });
    },
    onCacheSessions: async (route, url) => {
      expect(url.searchParams.get('limit')).toBe('100');
      await fulfillJson(route, {
        items: [
          {
            agent_id: 'agent-1',
            build_id: BUILD_ID,
            created_at_unix_ms: 1_700_000_000_000,
            credential: cacheSecret,
            expires_at_unix_ms: 1_700_000_200_000,
            id: 'cache-session-1',
            job_id: JOB_ID,
            lease_id: 'lease-1',
            namespace: 'compiler-cache',
            physical_location: `s3://private/${cacheSecret}`,
            project_id: 'project-1',
            quota_bytes: 1_048_576,
            read: true,
            registration_epoch: 1,
            retention_until_unix_ms: 1_700_000_300_000,
            revoked_at_unix_ms: null,
            state: 'active',
            write: true,
          },
        ],
      });
    },
    onEventRequest: async (route) => {
      await fulfillJson(route, { cursor: 0, items: [] });
    },
    onJobRequest: async (route) => {
      await fulfillJson(route, { event_cursor: 0, id: JOB_ID, state: 'succeeded' });
    },
    onRetention: async (route) => {
      await fulfillJson(route, retentionResource('active'));
    },
  });

  await page.goto(`/builds/${BUILD_ID}`);

  await expect(page.getByRole('region', { name: 'Published Artifacts' })).toContainText(
    'coverage.xml',
  );
  await expect(page.getByRole('region', { name: 'Cache sessions' })).toContainText(
    'compiler-cache',
  );
  await expect(page.getByRole('region', { name: 'Build Result retention' })).toContainText(
    'Active hold',
  );
  expect(downloadRequests).toBe(0);
  expect(await page.locator('html').textContent()).not.toContain(cacheSecret);
  expect(await page.locator('html').textContent()).not.toContain(privateUrl);

  await page.getByRole('button', { name: 'Download coverage.xml' }).click();
  await expect
    .poll(() => downloads)
    .toEqual([{ features: 'popup', target: '_blank', url: privateUrl }]);
  expect(downloadRequests).toBe(1);
  expect(await page.locator('html').textContent()).not.toContain(privateUrl);
  expect(await page.evaluate(() => [localStorage.length, sessionStorage.length])).toEqual([0, 0]);
  expect(
    browserMessages.some(
      (message) => message.includes(privateUrl) || message.includes(cacheSecret),
    ),
  ).toBe(false);
});

test('replays one permanent hold intent and releases the current version with audit correlation', async ({
  page,
}) => {
  let retentionState: 'active' | 'released' | null = null;
  const placements: Array<{ body: unknown; key: string | undefined }> = [];
  const releases: Array<{
    ifMatch: string | undefined;
    key: string | undefined;
  }> = [];
  await mockBuildDiagnostics(page, {
    onEventRequest: async (route) => fulfillJson(route, { cursor: 0, items: [] }),
    onJobRequest: async (route) =>
      fulfillJson(route, { event_cursor: 0, id: JOB_ID, state: 'succeeded' }),
    onPlaceHold: async (route) => {
      placements.push({
        body: route.request().postDataJSON() as unknown,
        key: route.request().headers()['idempotency-key'],
      });
      if (placements.length === 1) {
        await route.abort('connectionreset');
        return;
      }
      retentionState = 'active';
      await route.fulfill({
        body: JSON.stringify({
          disposition: 'replayed',
          retention: retentionResource(retentionState),
        }),
        contentType: 'application/json',
        status: 201,
      });
    },
    onReleaseHold: async (route) => {
      releases.push({
        ifMatch: route.request().headers()['if-match'],
        key: route.request().headers()['idempotency-key'],
      });
      retentionState = 'released';
      await fulfillJson(route, {
        disposition: 'applied',
        retention: retentionResource(retentionState),
      });
    },
    onRetention: async (route) => fulfillJson(route, retentionResource(retentionState)),
  });

  await page.goto(`/builds/${BUILD_ID}`);
  const retention = page.getByRole('region', { name: 'Build Result retention' });
  await retention.getByRole('textbox', { name: 'Hold reason' }).fill('release investigation');
  await retention.getByRole('button', { name: 'Review hold' }).click();

  let dialog = page.getByRole('dialog', { name: `Place hold on Build ${BUILD_ID}?` });
  await dialog.getByRole('button', { name: 'Place hold' }).click();
  await expect(dialog.getByRole('alert')).toContainText('could not be reached');
  await dialog.getByRole('button', { name: 'Retry same command' }).click();
  await expect(dialog.getByRole('status')).toContainText('Retention hold placed (Replayed).');
  await expect(dialog.getByRole('link', { name: 'View placement audit evidence' })).toHaveAttribute(
    'href',
    '/audit?request_identity=request-placement',
  );
  expect(placements).toHaveLength(2);
  expect(placements[0]?.body).toEqual({ reason: 'release investigation' });
  expect(placements[1]?.body).toEqual(placements[0]?.body);
  expect(placements[0]?.key).toMatch(/^[0-9a-f-]{36}$/i);
  expect(placements[1]?.key).toBe(placements[0]?.key);

  await dialog.getByRole('button', { name: 'Close' }).click();
  await retention.getByRole('button', { name: 'Release hold' }).click();
  dialog = page.getByRole('dialog', { name: `Release hold on Build ${BUILD_ID}?` });
  await dialog.getByRole('button', { name: 'Release hold' }).click();
  await expect(dialog.getByRole('status')).toContainText('Retention hold released (Applied).');
  await expect(dialog.getByRole('link', { name: 'View release audit evidence' })).toHaveAttribute(
    'href',
    '/audit?request_identity=request-release',
  );
  await dialog.getByRole('button', { name: 'Close' }).click();
  await expect(retention.getByRole('heading', { name: 'Build Result retention' })).toBeFocused();
  expect(releases).toHaveLength(1);
  expect(releases[0]?.ifMatch).toBe('"1"');
  expect(releases[0]?.key).toMatch(/^[0-9a-f-]{36}$/i);
  expect(releases[0]?.key).not.toBe(placements[0]?.key);
});

async function mockBuildDiagnostics(
  page: Page,
  handlers: {
    onArtifactDownload?: (route: Route) => Promise<void>;
    onArtifacts?: (route: Route, url: URL) => Promise<void>;
    onBuildRequest?: (route: Route) => Promise<void>;
    onCacheSessions?: (route: Route, url: URL) => Promise<void>;
    onCancel?: (route: Route) => Promise<void>;
    onEventRequest: (route: Route, url: URL) => Promise<void>;
    onJobRequest: (route: Route) => Promise<void>;
    onLogSearch?: (route: Route, url: URL) => Promise<void>;
    onPlaceHold?: (route: Route) => Promise<void>;
    onReleaseHold?: (route: Route) => Promise<void>;
    onRetention?: (route: Route) => Promise<void>;
    onRetry?: (route: Route) => Promise<void>;
  },
) {
  await page.route('**/health/ready', async (route) => {
    await fulfillJson(route, { status: 'ready' });
  });
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    if (url.pathname === `/api/v1/builds/${BUILD_ID}/cancel` && handlers.onCancel !== undefined) {
      await handlers.onCancel(route);
      return;
    }
    if (url.pathname === `/api/v1/builds/${BUILD_ID}/retry` && handlers.onRetry !== undefined) {
      await handlers.onRetry(route);
      return;
    }
    if (url.pathname === `/api/v1/builds/${BUILD_ID}`) {
      await (handlers.onBuildRequest?.(route) ?? fulfillJson(route, buildResource()));
      return;
    }
    if (url.pathname === `/api/v1/attempts/${ATTEMPT_ID}`) {
      await fulfillJson(route, attemptResource());
      return;
    }
    if (url.pathname === `/api/v1/builds/${BUILD_ID}/artifacts`) {
      await (handlers.onArtifacts?.(route, url) ?? fulfillJson(route, { items: [] }));
      return;
    }
    if (url.pathname === `/api/v1/builds/${BUILD_ID}/cache-sessions`) {
      await (handlers.onCacheSessions?.(route, url) ?? fulfillJson(route, { items: [] }));
      return;
    }
    if (
      url.pathname === `/api/v1/builds/${BUILD_ID}/retention/hold/release` &&
      handlers.onReleaseHold !== undefined
    ) {
      await handlers.onReleaseHold(route);
      return;
    }
    if (
      url.pathname === `/api/v1/builds/${BUILD_ID}/retention/hold` &&
      handlers.onPlaceHold !== undefined
    ) {
      await handlers.onPlaceHold(route);
      return;
    }
    if (url.pathname === `/api/v1/builds/${BUILD_ID}/retention`) {
      await (handlers.onRetention?.(route) ?? fulfillJson(route, retentionResource(null)));
      return;
    }
    if (url.pathname === `/api/v1/artifacts/${ARTIFACT_ID}/download`) {
      if (handlers.onArtifactDownload === undefined) {
        await route.fulfill({ body: '{}', contentType: 'application/json', status: 404 });
      } else {
        await handlers.onArtifactDownload(route);
      }
      return;
    }
    if (url.pathname === `/api/v1/jobs/${JOB_ID}/events`) {
      await handlers.onEventRequest(route, url);
      return;
    }
    if (url.pathname === `/api/v1/jobs/${JOB_ID}`) {
      await handlers.onJobRequest(route);
      return;
    }
    if (
      url.pathname === `/api/v1/projects/project-1/build-logs/search` &&
      handlers.onLogSearch !== undefined
    ) {
      await handlers.onLogSearch(route, url);
      return;
    }
    await route.fulfill({ body: '{}', contentType: 'application/json', status: 404 });
  });
}

function buildResource(state: 'cancelled' | 'failed' | 'running' = 'running') {
  return {
    configuration_id: 'configuration-1',
    configuration_version: 1,
    created_at_unix_ms: 1_700_000_000_000,
    current_attempt: attemptSummary(),
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
    pipeline_id: 'pipeline-1',
    pipeline_version: 1,
    priority: 0,
    project_id: 'project-1',
    repository_id: 'repository-1',
    repository_version: 1,
    source: { kind: 'exact_revision', value: '0123456789abcdef' },
    state,
    trigger: {
      build_id: BUILD_ID,
      causality: {
        depth: 0,
        parent_occurrence_id: null,
        root_occurrence_id: 'trigger-occurrence-1',
      },
      cause: { kind: 'manual' },
      created_at: 1_700_000_000_000,
      deduplication_identity: 'manual-1',
      id: 'trigger-occurrence-1',
      kind: 'manual',
      source_time: 1_700_000_000_000,
      state: 'accepted',
      target: { configuration_id: 'configuration-1', configuration_version: 1 },
      trigger: { id: 'trigger-1', version: 1 },
      updated_at: 1_700_000_000_000,
    },
    updated_at_unix_ms: 1_700_000_001_000,
    version: 1,
  };
}

function attemptResource() {
  return { attempt: attemptSummary(), edges: [], jobs: [jobResource()] };
}

function attemptSummary() {
  return {
    build_id: BUILD_ID,
    created_at_unix_ms: 1_700_000_000_000,
    id: ATTEMPT_ID,
    number: 1,
    retry_of_attempt_id: null,
    state: 'running',
    updated_at_unix_ms: 1_700_000_001_000,
    version: 1,
  };
}

function jobResource() {
  return {
    allowed_pool_ids: [],
    assignment: null,
    attempt_id: ATTEMPT_ID,
    created_at_unix_ms: 1_700_000_000_000,
    dependency_job_ids: [],
    dependency_policy: 'all_succeeded',
    event_cursor: 0,
    id: JOB_ID,
    outputs: [],
    pipeline_node_id: 'compile',
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
    state: 'running',
    terminal: null,
    updated_at_unix_ms: 1_700_000_001_000,
    version: 1,
  };
}

function jobEvent(sequence: number, kind: string) {
  return {
    kind,
    occurred_at_unix_ms: 1_700_000_000_000 + sequence,
    payload: { sequence },
    sequence,
  };
}

function artifactResource() {
  return {
    attempt_id: ATTEMPT_ID,
    build_id: BUILD_ID,
    id: ARTIFACT_ID,
    job_id: JOB_ID,
    media_type: 'application/xml',
    name: 'coverage.xml',
    output_type: { format: 'cobertura', kind: 'report' },
    published_at_unix_ms: 1_700_000_000_000,
    sha256: 'a'.repeat(64),
    size_bytes: 2_048,
  };
}

function retentionResource(state: 'active' | 'released' | null) {
  return {
    build_id: BUILD_ID,
    deadlines: {
      artifacts_at_unix_ms: 1_700_000_400_000,
      logs_at_unix_ms: 1_700_000_300_000,
      metadata_at_unix_ms: 1_700_000_200_000,
      reports_at_unix_ms: 1_700_000_500_000,
    },
    hold:
      state === null
        ? null
        : {
            created_at_unix_ms: 1_700_000_000_000,
            creation_audit: {
              actor_identity: null,
              actor_kind: 'unauthenticated_management',
              request_identity: 'request-placement',
            },
            expires_at_unix_ms: null,
            reason: 'incident evidence',
            release_audit:
              state === 'released'
                ? {
                    actor_identity: null,
                    actor_kind: 'unauthenticated_management',
                    request_identity: 'request-release',
                  }
                : null,
            released_at_unix_ms: state === 'released' ? 1_700_000_010_000 : null,
            state,
            version: 1,
          },
    visibility: { artifacts: true, logs: true, metadata: true, reports: true },
  };
}

async function fulfillJson(route: Route, body: unknown) {
  await route.fulfill({ body: JSON.stringify(body), contentType: 'application/json', status: 200 });
}
