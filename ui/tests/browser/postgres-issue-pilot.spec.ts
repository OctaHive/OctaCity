import { expect, test } from '@playwright/test';

const origin = process.env.OCTACITY_PILOT_API_ORIGIN;

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: 'wait' });
});

test('PostgreSQL manual task survives real HTTP edits, discussion and narrow keyboard navigation', async ({
  page,
  request,
}) => {
  test.skip(
    !origin,
    'Run through cargo test -p octacity-server --test issue_console_pilot with disposable PostgreSQL.',
  );
  const created = await request.post(`${origin}/api/v1/projects`, {
    headers: { 'Idempotency-Key': crypto.randomUUID() },
    data: { name: 'PostgreSQL Console pilot' },
  });
  expect(created.status()).toBe(201);
  const project = (await created.json()).resource.id;
  // Proxy bytes to the real runtime, with no fixture responses or browser-side task authority.
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const response = await route.fetch({ url: `${origin}${url.pathname}${url.search}` });
    await route.fulfill({ response });
  });
  await page.goto(`/projects/${project}/issues`);
  await page.getByRole('button', { name: 'Create task', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Title', exact: true })).toBeFocused();
  await page.getByRole('textbox', { name: 'Title', exact: true }).fill('Independent manual task');
  await page
    .getByRole('textbox', { name: 'Description', exact: true })
    .fill('Original reviewed requirements');
  await page
    .getByRole('textbox', { name: 'Acceptance criteria', exact: true })
    .fill('Reviewed acceptance');
  await page.getByRole('button', { name: 'Review task' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Create task' }).click();
  await page.getByRole('link', { name: 'Open ISS-1' }).click();
  await expect(
    page.getByRole('heading', { name: 'ISS-1 · Independent manual task', exact: true }),
  ).toBeVisible();
  await expect(page.getByText('Original reviewed requirements', { exact: true })).toBeVisible();
  const taskUrl = page.url();
  await page.getByRole('button', { name: 'Edit requirements', exact: true }).click();
  await page
    .getByRole('textbox', { name: 'Description', exact: true })
    .fill('Requirements edited without execution');
  await page.getByRole('button', { name: 'Review task' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Save task', exact: true }).click();
  await page.reload();
  await expect(
    page.getByText('Requirements edited without execution', { exact: true }),
  ).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  const properties = page.getByRole('button', { name: 'Properties', exact: true });
  await properties.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('complementary', { name: 'Properties' })).toBeInViewport();
  await expect(page.getByRole('complementary', { name: 'Properties' })).toContainText('Unassigned');
  await page.getByRole('button', { name: 'Close properties', exact: true }).click();
  await expect(properties).toBeFocused();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
    ),
  ).toBe(true);
  const issue = new URL(taskUrl).pathname.split('/').at(-1);
  const api = `${origin}/api/v1/projects/${project}`;
  const currentResponse = await request.get(`${api}/issues/${issue}`);
  const current = await currentResponse.json();
  const defaults = await (await request.get(`${api}/issue-workflow-default`)).json();
  const definition = structuredClone(defaults.workflow.definition);
  definition.name = 'Pilot editorial policy';
  definition.statuses[1].name = 'Ready for review';
  const edge = crypto.randomUUID();
  const target = crypto.randomUUID();
  definition.transitions = [
    {
      id: edge,
      name: 'Review readiness',
      from: definition.initial_status_id,
      to: definition.statuses[1].id,
      origins: ['manual'],
      evidence: [],
    },
  ];
  expect(
    (
      await request.post(`${api}/issue-workflows`, {
        headers: { 'Idempotency-Key': crypto.randomUUID() },
        data: { id: target, definition },
      })
    ).status(),
  ).toBe(201);
  const policy = await (await request.get(`${api}/issues/${issue}/transitions`)).json();
  expect(
    (
      await request.post(`${api}/issues/${issue}/workflow-adoption`, {
        headers: { 'Idempotency-Key': crypto.randomUUID(), 'If-Match': `"${current.version}"` },
        data: {
          expected_workflow_id: current.workflow_id,
          expected_workflow_version: current.workflow_version,
          expected_status_id: current.status_id,
          target_workflow_id: target,
          target_workflow_version: 1,
          status_correspondence: { [current.status_id]: definition.initial_status_id },
          expected_field_policy: policy.field_policy,
        },
      })
    ).status(),
  ).toBe(200);
  await page.reload();
  await page.getByRole('link', { name: 'Actions', exact: true }).click();
  await page.getByRole('button', { name: 'Review readiness', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Move task', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('Status recorded');
  await page.getByRole('dialog').getByRole('button', { name: 'Close', exact: true }).click();
  await expect(page.getByText('Business status: Ready for review', { exact: true })).toBeVisible();
  const parent = await (
    await request.post(`${api}/issues`, {
      headers: { 'Idempotency-Key': crypto.randomUUID() },
      data: { title: 'Visible parent', description: '' },
    })
  ).json();
  const definitions = await request.post(`${api}/issue-property-definitions`, {
    headers: { 'Idempotency-Key': crypto.randomUUID() },
    data: {
      expected_version: 0,
      definition: {
        labels: [{ id: 'reviewed', name: 'Reviewed', retired: false }],
        properties: [
          {
            id: 'estimate',
            name: 'Estimate',
            retired: false,
            kind: { type: 'number', minimum: 0, maximum: 10 },
          },
        ],
      },
    },
  });
  expect(definitions.status()).toBe(200);
  const child = await (await request.get(`${api}/issues/${issue}`)).json();
  expect(
    (
      await request.patch(`${api}/issues/${issue}/planning`, {
        headers: { 'Idempotency-Key': crypto.randomUUID(), 'If-Match': `"${child.version}"` },
        data: {
          definition_version: 1,
          assignee: null,
          due_date: '2028-02-29',
          labels: ['reviewed'],
          values: { estimate: { type: 'number', value: 3 } },
          parent_id: parent.resource.id,
        },
      })
    ).status(),
  ).toBe(200);
  // The reverse edge would create a cycle; the public command must reject it.
  expect(
    (
      await request.patch(`${api}/issues/${parent.resource.id}/planning`, {
        headers: { 'Idempotency-Key': crypto.randomUUID(), 'If-Match': '"1"' },
        data: {
          definition_version: 1,
          assignee: null,
          due_date: null,
          labels: [],
          values: {},
          parent_id: issue,
        },
      })
    ).status(),
  ).toBe(409);
  await page.reload();
  await page.getByRole('button', { name: 'Properties', exact: true }).click();
  await expect(
    page.getByRole('link', { name: 'ISS-2 · Visible parent', exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('complementary', { name: 'Properties' })).toContainText('Reviewed');
  await page.getByRole('button', { name: 'Close properties', exact: true }).click();
  await page.getByRole('link', { name: 'Activity', exact: true }).click();
  await page
    .getByRole('textbox', { name: 'Comment', exact: true })
    .fill('@agent retry: discussion only');
  await page.getByRole('button', { name: 'Review comment', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Append comment', exact: true })
    .click();
  await expect(page.getByRole('dialog')).toContainText('Operator comment recorded');
  await page.getByRole('dialog').getByRole('button', { name: 'Close', exact: true }).click();
  await page.reload();
  await page.getByRole('button', { name: 'Read comment', exact: true }).click();
  await expect(page.getByText('@agent retry: discussion only', { exact: true })).toBeVisible();
  const history = await request.get(
    `${origin}/api/v1/projects/${project}/issues/${issue}/executions`,
  );
  expect(history.status()).toBe(200);
  expect((await history.json()).items).toEqual([]);
  const runs = await request.get(`${origin}/api/v1/projects/${project}/factory-runs`);
  expect(runs.status()).toBe(200);
  expect((await runs.json()).items).toEqual([]);
});

test('PostgreSQL original executions retain frozen revisions through reload and a later reviewed launch', async ({
  page,
  request,
}) => {
  const path = process.env.OCTACITY_PILOT_ISSUE_PATH;
  test.skip(
    !origin || !path,
    'Run with OCTACITY_PILOT_BROWSER=1 through the project_text_intake PostgreSQL/MinIO pilot.',
  );
  const api = `${origin}/api/v1${path}`;
  const before = await (await request.get(`${api}/executions`)).json();
  expect(before.items).toHaveLength(2);
  const original = before.items.find(
    (item: { intent: { expected_version: number } }) => item.intent.expected_version === 1,
  );
  expect(original).toBeDefined();
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    await route.fulfill({
      response: await route.fetch({ url: `${origin}${url.pathname}${url.search}` }),
    });
  });
  await page.goto(`${path}?section=runs&execution=${original.id}`);
  await expect(page.getByRole('link', { name: 'Open Run', exact: true })).toHaveAttribute(
    'href',
    `/factory/${original.association.run_id}`,
  );
  await expect(
    page.getByRole('region', { name: 'Run overview' }).getByText('original-base', { exact: true }),
  ).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload();
  await expect(page.getByRole('region', { name: 'Selected execution' })).toContainText(
    'Task version 1',
  );
  await page.goto(`${path}?section=results&result_pane=artifacts&execution=${original.id}`);
  await expect(
    page.getByRole('heading', { name: 'Execution artifacts', exact: true }),
  ).toBeVisible();
  await page
    .getByRole('heading', { name: 'task', exact: true })
    .locator('..')
    .getByRole('button', { name: 'Preview', exact: true })
    .click();
  await expect(page.locator('pre')).toContainText('точные');
  await page.getByRole('button', { name: 'Close preview', exact: true }).click();
  await page.goto(`${path}?section=runs&execution=${original.id}`);
  await page.getByRole('button', { name: 'Start pipeline', exact: true }).click();
  await page.getByRole('textbox', { name: 'Exact revision', exact: true }).fill('original-base');
  await page.getByRole('button', { name: 'Review pipeline launch', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Confirm pipeline launch', exact: true })
    .click();
  await expect
    .poll(async () => (await (await request.get(`${api}/executions`)).json()).items.length)
    .toBe(3);
  const retained = await (await request.get(`${api}/executions/${original.id}`)).json();
  expect(retained.task_digest).toBe(original.task_digest);
  expect(retained.association).toEqual(original.association);
  expect(retained.intent.expected_version).toBe(1);
});

test('PostgreSQL imported executions share one card, original Runs and historical external checks after restart', async ({
  page,
  request,
}) => {
  const path = process.env.OCTACITY_PILOT_ISSUE_PATH;
  test.skip(!origin || !path, 'Run through the PostgreSQL imported-card pilot.');
  const api = `${origin}/api/v1${path}`;
  const legacy = await (await request.get(`${api}/legacy-work`)).json();
  expect(legacy.items).toHaveLength(2);
  const writes: string[] = [];
  page.on('request', (request) => {
    if (request.method() !== 'GET') writes.push(request.url());
  });
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    await route.fulfill({
      response: await route.fetch({ url: `${origin}${url.pathname}${url.search}` }),
    });
  });
  for (const item of legacy.items) {
    await page.goto(`${path}?section=runs&work=${item.work_id}`);
    await expect(
      page.getByRole('link', { name: 'Open original Run', exact: true }),
    ).toHaveAttribute('href', `/factory/${item.run_id}`);
    await expect(page.getByRole('region', { name: 'Run overview' })).toBeVisible();
    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload();
    await expect(
      page.getByRole('link', { name: 'Open original Run', exact: true }),
    ).toHaveAttribute('href', `/factory/${item.run_id}`);
  }
  await page.goto(`${path}?section=results`);
  await expect(
    page.getByRole('heading', { name: 'review-42 · Retained review', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Observation history', exact: true }).click();
  await expect(page.getByText('Observed head: old-head', { exact: true })).toBeVisible();
  await page.getByRole('link', { name: 'Checks', exact: true }).click();
  const ci = page.getByRole('region', { name: 'External CI', exact: true });
  for (const state of ['Passed', 'Failed', 'Missing', 'Indeterminate']) {
    await expect(ci.getByText(state, { exact: true })).toBeVisible();
  }
  expect(writes).toEqual([]);
});

test('PostgreSQL expired execution inputs retain provenance and cannot reopen preview', async ({
  page,
  request,
}) => {
  const path = process.env.OCTACITY_PILOT_ISSUE_PATH;
  test.skip(!origin || !path, 'Run through the PostgreSQL/MinIO retention pilot.');
  const history = await (await request.get(`${origin}/api/v1${path}/executions`)).json();
  const original = history.items.find(
    (item: { intent: { expected_version: number } }) => item.intent.expected_version === 1,
  );
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    await route.fulfill({
      response: await route.fetch({ url: `${origin}${url.pathname}${url.search}` }),
    });
  });
  await page.goto(`${path}?section=results&result_pane=artifacts&execution=${original.id}`);
  await expect(page.getByRole('heading', { name: 'Original inputs', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Preview', exact: true })).toHaveCount(0);
  await expect(
    page.getByRole('heading', { name: 'Execution artifacts', exact: true }),
  ).toBeVisible();
});

test('PostgreSQL qualified Job journal keeps original events and logs through reconnect', async ({
  page,
}) => {
  const path = process.env.OCTACITY_PILOT_ISSUE_PATH;
  test.skip(!origin || !path, 'Run through the real PostgreSQL Job-journal pilot.');
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    await route.fulfill({
      response: await route.fetch({ url: `${origin}${url.pathname}${url.search}` }),
    });
  });
  await page.goto(path!);
  const events = page.getByRole('list', { name: 'Ordered diagnostics', exact: true });
  await expect(events.getByRole('listitem')).toHaveCount(2);
  await expect(events).not.toContainText('private transcript');
  await page.getByRole('button', { name: 'Inspect event #1', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Payload', exact: true })).toContainText(
    'private transcript',
  );
  await expect(
    page
      .getByRole('link', { name: 'Open original Job logs', exact: true })
      .and(
        page.locator(
          `a[href*="job_id=${new URL(path!, 'http://localhost').searchParams.get('journal_job')}"]`,
        ),
      ),
  ).toHaveAttribute('href', /\/builds\/.*job_id=/u);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload();
  await expect(page.getByRole('region', { name: 'Payload', exact: true })).toContainText(
    'private transcript',
  );
  await page.getByRole('button', { name: 'Resume live events', exact: true }).click();
  await expect(events.getByRole('listitem')).toHaveCount(2);
});
