import { expect, test, type Page, type Route } from '@playwright/test';

import { CONSOLE_PATHS } from '../../src/app/routes';

const PROJECT_ID = '33333333-3333-4333-8333-333333333333';
const CONFIGURATION_ID = '11111111-1111-4111-8111-111111111111';
const BUILD_A = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1';
const BUILD_B = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb2';
const BUILD_C = 'cccccccc-cccc-4ccc-8ccc-ccccccccccc3';
const PAGE_CURSOR = 'equal-time-page-2';

test('replays one confirmed manual Build with an unchanged identity after a lost response', async ({
  page,
}) => {
  const requests: Array<{ body: Record<string, unknown>; key: string | undefined }> = [];
  let accepted = false;
  await mockProjectApi(page, {
    onBuilds: async (route) => {
      await fulfillJson(
        route,
        pageResponse(accepted ? [build('build-new', 1_700_000_001_000)] : []),
      );
    },
    onManualTrigger: async (route) => {
      requests.push({
        body: route.request().postDataJSON() as Record<string, unknown>,
        key: route.request().headers()['idempotency-key'],
      });
      if (requests.length === 1) {
        await route.abort('connectionfailed');
        return;
      }
      accepted = true;
      await fulfillJson(route, {
        attempt_id: 'attempt-new',
        build_id: 'build-new',
        disposition: 'replayed',
        outcome: 'accepted',
        ready_job_ids: [],
        trigger_occurrence_id: 'occurrence-new',
      });
    },
  });

  await page.goto(`/projects/${PROJECT_ID}`);
  await page.getByRole('button', { name: 'View Release details' }).click();
  await page.getByRole('textbox', { name: 'Environment' }).fill('staging');
  await page.getByRole('button', { name: 'Review Build' }).click();
  const dialog = page.getByRole('dialog', { name: 'Start Build from Release?' });
  await dialog.getByRole('button', { name: 'Start Build' }).click();
  await expect(dialog.getByRole('alert')).toContainText('management API could not be reached');
  await dialog.getByRole('button', { name: 'Retry same command' }).click();
  await expect(dialog.getByRole('status')).toContainText('Build build-new accepted (Replayed).');
  await expect(
    page.getByRole('region', { name: 'Recent Builds' }).getByRole('link', { name: 'build-new' }),
  ).toBeVisible();

  expect(requests).toHaveLength(2);
  expect(requests[0]).toEqual(requests[1]);
  expect(requests[0]?.key).toMatch(/^[0-9a-f-]{36}$/i);
  expect(requests[0]?.body.deduplication_identity).toBe(requests[0]?.key);
  expect(requests[0]?.body.configuration_id).toBe(CONFIGURATION_ID);
});

test('round-trips copied Build filters, preserves server order, and follows the cursor', async ({
  page,
}) => {
  const buildRequests: URL[] = [];
  await mockProjectApi(page, { buildRequests });
  const sharedPath = `/projects/${PROJECT_ID}?configuration_id=${CONFIGURATION_ID}&state=failed`;

  await page.goto(sharedPath);
  await expect(page.getByRole('status').filter({ hasText: 'Server ready' })).toHaveText(
    'Server ready',
  );
  await expect(page.getByRole('combobox', { name: 'Build Configuration' })).toHaveValue(
    CONFIGURATION_ID,
  );
  await expect(page.getByRole('combobox', { name: 'Build state' })).toHaveValue('failed');
  await expect
    .poll(() => buildRequestFilters(buildRequests.at(-1)))
    .toEqual({ configurationId: CONFIGURATION_ID, cursor: null, state: 'failed' });

  const builds = page.getByRole('region', { name: 'Recent Builds' });
  await expect(builds.getByRole('link')).toHaveText([BUILD_B, BUILD_A]);

  const copiedUrl = page.url();
  await page.goto(CONSOLE_PATHS.projects);
  await page.goto(copiedUrl);
  await expect(page.getByRole('combobox', { name: 'Build Configuration' })).toHaveValue(
    CONFIGURATION_ID,
  );
  await expect(page.getByRole('combobox', { name: 'Build state' })).toHaveValue('failed');

  await builds.getByRole('button', { name: 'Load more Builds' }).click();
  await expect(builds.getByRole('link', { name: BUILD_C })).toBeVisible();
  await expect(builds.getByRole('link')).toHaveText([BUILD_B, BUILD_A, BUILD_C]);
  await expect
    .poll(() => buildRequestFilters(buildRequests.at(-1)))
    .toEqual({ configurationId: CONFIGURATION_ID, cursor: PAGE_CURSOR, state: 'failed' });

  await page.getByRole('combobox', { name: 'Build Configuration' }).selectOption('');
  await page.getByRole('combobox', { name: 'Build state' }).selectOption('');
  await expect(page).toHaveURL(`/projects/${PROJECT_ID}`);
  await expect
    .poll(() => buildRequestFilters(buildRequests.at(-1)))
    .toEqual({ configurationId: null, cursor: null, state: null });

  await builds.getByRole('link', { name: BUILD_B }).click();
  await expect(page).toHaveURL(`/builds/${BUILD_B}`);
});

async function mockProjectApi(
  page: Page,
  handlers: {
    buildRequests?: URL[];
    onBuilds?: (route: Route, url: URL) => Promise<void>;
    onManualTrigger?: (route: Route) => Promise<void>;
  } = {},
) {
  await page.route('**/health/ready', async (route) => {
    await fulfillJson(route, { status: 'ready' });
  });
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const projectPath = `/api/v1/projects/${PROJECT_ID}`;
    if (url.pathname === '/api/v1/triggers/manual' && handlers.onManualTrigger !== undefined) {
      await handlers.onManualTrigger(route);
      return;
    }
    if (url.pathname === projectPath) {
      await fulfillJson(route, {
        ancestors: [],
        project: {
          created_at_unix_ms: 1_700_000_000_000,
          id: PROJECT_ID,
          name: 'Delivery',
          parent_id: null,
          updated_at_unix_ms: 1_700_000_000_000,
          version: 1,
        },
      });
      return;
    }
    if (url.pathname === '/api/v1/projects') {
      await fulfillJson(route, pageResponse([]));
      return;
    }
    if (url.pathname === `${projectPath}/build-configurations`) {
      await fulfillJson(
        route,
        pageResponse([
          {
            enabled: true,
            id: CONFIGURATION_ID,
            name: 'Release',
            project_id: PROJECT_ID,
            published_at_unix_ms: 1_700_000_000_000,
            version: 4,
          },
        ]),
      );
      return;
    }
    if (url.pathname === `/api/v1/build-configurations/${CONFIGURATION_ID}/versions/4`) {
      await fulfillJson(route, buildConfiguration());
      return;
    }
    if (url.pathname === `${projectPath}/builds`) {
      handlers.buildRequests?.push(url);
      if (handlers.onBuilds !== undefined) {
        await handlers.onBuilds(route, url);
        return;
      }
      const items =
        url.searchParams.get('after') === PAGE_CURSOR
          ? [build(BUILD_C, 1_699_999_999_000)]
          : [build(BUILD_B, 1_700_000_000_000), build(BUILD_A, 1_700_000_000_000)];
      await fulfillJson(route, pageResponse(items, items.length === 1 ? null : PAGE_CURSOR));
      return;
    }
    if (url.pathname === `${projectPath}/trigger-definitions`) {
      await fulfillJson(
        route,
        pageResponse([
          {
            configuration_id: CONFIGURATION_ID,
            configuration_version: 4,
            enabled: true,
            id: 'manual-trigger',
            kind: 'manual',
            project_id: PROJECT_ID,
            published_at_unix_ms: 1_700_000_000_000,
            version: 2,
          },
        ]),
      );
      return;
    }
    if (
      url.pathname === `${projectPath}/pipelines` ||
      url.pathname === `${projectPath}/repositories`
    ) {
      await fulfillJson(route, pageResponse([]));
      return;
    }
    await route.fulfill({ body: '{}', contentType: 'application/json', status: 404 });
  });
}

function buildConfiguration() {
  return {
    definition: {
      agent_requirements: {
        capabilities: [],
        labels: {},
        minimum_cpu_millis: 0,
        minimum_disk_bytes: 0,
        minimum_memory_bytes: 0,
      },
      allowed_pools: [],
      artifacts: {
        artifact_bytes: 0,
        artifact_count: 0,
        report_bytes: 0,
        report_count: 0,
        single_output_bytes: 0,
      },
      cache: { namespace: null, read: false, write: false },
      enabled: true,
      job_concurrency_limit: 1,
      parameters: {
        deny_unknown: true,
        parameters: {
          environment: { default: null, required: true, value_type: 'string' },
        },
      },
      pipeline_id: 'pipeline-1',
      pipeline_version: 1,
      repository_id: 'repository-1',
      repository_version: 1,
      retry: { max_attempts: 1, retry_on: [] },
      runtime: {
        architecture: 'arm64',
        class: 'virtualization',
        cpu_millis: 1_000,
        immutable_image: null,
        memory_bytes: 1_073_741_824,
        network: { mode: 'disabled' },
        operating_system: 'linux',
        timeout_seconds: 600,
        workload_identity_profile: null,
        writable_disk_bytes: 1_073_741_824,
      },
      triggers: ['manual'],
    },
    id: CONFIGURATION_ID,
    name: 'Release',
    project_id: PROJECT_ID,
    published_at_unix_ms: 1_700_000_000_000,
    version: 4,
  };
}

function build(id: string, createdAt: number) {
  return {
    cause: { kind: 'manual' },
    configuration_id: CONFIGURATION_ID,
    configuration_version: 4,
    created_at_unix_ms: createdAt,
    current_attempt_id: 'dddddddd-dddd-4ddd-8ddd-dddddddddddd',
    current_attempt_number: 1,
    current_attempt_state: 'failed',
    id,
    project_id: PROJECT_ID,
    state: 'failed',
    terminal_at_unix_ms: createdAt + 1_000,
  };
}

function pageResponse(items: unknown[], nextCursor: string | null = null) {
  return { items, next_cursor: nextCursor };
}

function buildRequestFilters(url: URL | undefined) {
  return {
    configurationId: url?.searchParams.get('configuration_id') ?? null,
    cursor: url?.searchParams.get('after') ?? null,
    state: url?.searchParams.get('state') ?? null,
  };
}

async function fulfillJson(route: Route, body: unknown) {
  await route.fulfill({ body: JSON.stringify(body), contentType: 'application/json', status: 200 });
}
