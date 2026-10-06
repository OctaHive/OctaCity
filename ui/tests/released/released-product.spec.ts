import { expect, test, type Page, type Request } from '@playwright/test';

const origin = required('OCTACITY_RELEASE_CONSOLE_ORIGIN');
const projectId = required('OCTACITY_RELEASE_PROJECT_ID');
const buildId = required('OCTACITY_RELEASE_BUILD_ID');
const jobId = required('OCTACITY_RELEASE_JOB_ID');

test('serves the released console through the credential-free same-origin contract', async ({
  context,
  page,
}) => {
  await context.addCookies([
    {
      name: 'browser-session-probe',
      url: origin,
      value: 'must-not-reach-management',
    },
  ]);
  const managementRequests: Request[] = [];
  page.on('request', (request) => {
    if (new URL(request.url()).pathname.startsWith('/api/v1/')) managementRequests.push(request);
  });

  const projectResponsePromise = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return url.pathname === `/api/v1/projects/${projectId}`;
  });
  await page.goto(`/projects/${projectId}`);
  await expect(page).toHaveURL(`/projects/${projectId}`);
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  const projectResponse = await projectResponsePromise;
  expect(projectResponse.status()).toBe(200);
  await expectNoCors(projectResponse.allHeaders());

  const eventRequestPromise = page.waitForRequest((request) => {
    const url = new URL(request.url());
    return url.pathname === `/api/v1/jobs/${jobId}/events`;
  });
  await page.goto(`/builds/${buildId}`);
  await expect(page).toHaveURL(`/builds/${buildId}`);
  const eventRequest = await eventRequestPromise;
  const eventUrl = new URL(eventRequest.url());
  expect(eventUrl.searchParams.get('after')).toBe('0');
  expect(eventUrl.searchParams.get('limit')).toBe('256');
  expect(eventUrl.searchParams.get('wait_ms')).toBe('30000');

  const replayed = await replayCancelledBuild(page, buildId);
  expect(replayed.first.key).toMatch(/^[0-9a-f-]{36}$/iu);
  expect(replayed.second).toEqual(replayed.first);

  expect(managementRequests.length).toBeGreaterThan(0);
  for (const request of managementRequests) {
    const headers = request.headers();
    expect(
      headers.cookie,
      `${request.method()} ${request.url()} sent a browser cookie`,
    ).toBeUndefined();
    expect(
      headers.authorization,
      `${request.method()} ${request.url()} sent browser authorization`,
    ).toBeUndefined();
  }
});

async function replayCancelledBuild(page: Page, selectedBuildId: string) {
  const requests: Array<{ body: string | null; key: string | undefined }> = [];
  let call = 0;
  await page.route(`**/api/v1/builds/${selectedBuildId}/cancel`, async (route) => {
    call += 1;
    requests.push({
      body: route.request().postData(),
      key: route.request().headers()['idempotency-key'],
    });
    if (call === 1) {
      const committed = await route.fetch();
      expect(committed.ok()).toBe(true);
      await route.abort('connectionfailed');
      return;
    }
    await route.continue();
  });

  await page.getByRole('button', { name: 'Cancel Build' }).click();
  const dialog = page.getByRole('dialog', { name: `Cancel Build ${selectedBuildId}?` });
  await dialog.getByRole('button', { name: 'Cancel Build' }).click();
  await expect(dialog.getByRole('alert')).toContainText('management API could not be reached');
  await dialog.getByRole('button', { name: 'Retry same command' }).click();
  await expect(dialog.getByRole('status')).toContainText('Replayed');
  expect(requests).toHaveLength(2);
  return { first: requests[0]!, second: requests[1]! };
}

async function expectNoCors(headersPromise: Promise<Record<string, string>>) {
  const headers = await headersPromise;
  expect(headers['access-control-allow-origin']).toBeUndefined();
  expect(headers['access-control-allow-credentials']).toBeUndefined();
}

function required(name: string): string {
  const value = process.env[name];
  if (value === undefined || value.length === 0) throw new Error(`${name} must be set`);
  return value;
}
