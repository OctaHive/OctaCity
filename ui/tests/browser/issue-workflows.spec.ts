import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { workflowFixture } from '../fixtures/issueWorkflows';

test('publishes a reviewed workflow over HTTP without provider or Factory requests and restores keyboard focus', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  const commands: {
    path: string;
    body: unknown;
    precondition: string | undefined;
    key: string | undefined;
  }[] = [];
  const forbiddenRequests: string[] = [];
  page.on('request', (request) => {
    if (/\/api\/v1\/(?:integrations|factory)/u.test(request.url()))
      forbiddenRequests.push(request.url());
  });
  await page.route('**/api/v1/projects/*/issue-workflows/**', async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (request.method() !== 'POST') {
      await route.fulfill({ json: workflow });
      return;
    }
    const body = request.postDataJSON();
    commands.push({
      path,
      body,
      precondition: request.headers()['if-match'],
      key: request.headers()['idempotency-key'],
    });
    await route.fulfill({
      json: path.endsWith('/validate')
        ? body.definition
        : {
            disposition: 'applied',
            resource: { ...workflow, version: 4, definition: body.definition },
          },
    });
  });
  await page.goto(`/projects/${FIXTURE_IDS.project}`);
  await page.getByRole('link', { name: 'Task workflow' }).click();
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  await page.getByRole('textbox', { name: 'Status name 1' }).fill('Inbox');
  const initial = page.getByRole('combobox', { name: 'Initial status' });
  await initial.focus();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await expect(initial).toContainText('Done');
  await expect(initial).toBeFocused();
  await page.getByRole('button', { name: 'Validate draft' }).click();
  const review = page.getByRole('button', { name: 'Review publication' });
  await expect(review).toBeEnabled();
  await review.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog');
  const publish = dialog.getByRole('button', { name: 'Publish version' });
  await expect(publish).toBeFocused();
  await expect(dialog).toContainText('Initial status: Done');
  await page.keyboard.press('Escape');
  await expect(review).toBeFocused();
  await review.click();
  await publish.click();
  await expect(dialog).toContainText('Published version 4.');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(review).toBeFocused();
  expect(commands).toHaveLength(2);
  expect(commands[0]?.precondition).toBeUndefined();
  expect(commands[1]?.precondition).toBe('"3"');
  expect(commands[1]?.key).toMatch(/^[0-9a-f-]{36}$/u);
  expect(commands[1]?.body).toMatchObject({
    definition: { initial_status_id: workflow.definition.statuses[1]!.id },
  });
  expect(forbiddenRequests).toEqual([]);
});

test('shows Russian settings at narrow width with bounded status rows and no graph canvas', async ({
  page,
}) => {
  await installOperatorFixture(page);
  await page.addInitScript(() =>
    localStorage.setItem(
      'octacity.console.preferences',
      JSON.stringify({
        version: 2,
        language: 'ru',
        theme: 'light',
        favorites: [],
        recents: [],
        expanded: [],
        explorerOpen: true,
        explorerWidth: 288,
        notificationLastOpenedAt: null,
      }),
    ),
  );
  await page.setViewportSize({ width: 640, height: 900 });
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Редактировать workflow по умолчанию' }).click();
  await expect(page.getByRole('heading', { name: 'Workflow задач', level: 1 })).toBeVisible();
  await expect(page.getByRole('combobox', { name: 'Начальный статус' })).toBeVisible();
  await expect(
    page.getByRole('group', { name: 'Завершающие статусы' }).getByRole('checkbox'),
  ).toHaveCount(3);
  await expect(page.getByRole('button', { name: 'Добавить переход' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Проверить публикацию' })).toBeDisabled();
  await expect(page.locator('canvas')).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth),
    ),
  ).toBe(0);
});

test('retains the draft and presents a denied publication without retrying or falling back to another API', async ({
  page,
}) => {
  await installOperatorFixture(page);
  let publications = 0;
  await page.route('**/api/v1/projects/*/issue-workflows/**', async (route) => {
    if (route.request().url().endsWith('/validate')) {
      await route.fulfill({ json: route.request().postDataJSON().definition });
      return;
    }
    publications++;
    await route.fulfill({
      status: 403,
      json: {
        code: 'forbidden',
        message: 'Workflow administration is not allowed.',
        request_id: 'workflow-denied',
      },
    });
  });
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  await page.getByRole('textbox', { name: 'Workflow name' }).fill('My policy');
  await page.getByRole('button', { name: 'Validate draft' }).click();
  await page.getByRole('button', { name: 'Review publication' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Publish version' }).click();
  await expect(page.getByRole('dialog')).toContainText('forbidden');
  await expect(page.getByRole('button', { name: 'Retry same command' })).toHaveCount(0);
  expect(publications).toBe(1);
  await page.getByRole('dialog').getByRole('button', { name: 'Abandon' }).click();
  await expect(page.getByRole('textbox', { name: 'Workflow name' })).toHaveValue('My policy');
});

test('keeps visible policies editable when the selected default is hidden', async ({ page }) => {
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  workflow.id = '20000000-0000-4000-8000-000000000002';
  workflow.definition.name = 'Visible policy';
  await page.route('**/api/v1/projects/*/issue-workflow-default', (route) =>
    route.fulfill({
      status: 404,
      json: {
        code: 'not_found',
        message: 'Default is not visible.',
        request_id: 'workflow-default-hidden',
      },
    }),
  );
  await page.route('**/api/v1/projects/*/issue-workflows**', (route) =>
    route.fulfill({
      json: new URL(route.request().url()).pathname.endsWith('/issue-workflows')
        ? { items: [{ ...workflow, name: workflow.definition.name }], next_cursor: null }
        : workflow,
    }),
  );
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await expect(page.getByRole('alert')).toContainText('not_found');
  await page.getByRole('button', { name: 'Open Visible policy' }).click();
  await expect(page.getByRole('heading', { name: 'Visible policy' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Use for new tasks' })).toBeDisabled();
  await page.getByRole('button', { name: 'Edit this version' }).click();
  await expect(page.getByRole('textbox', { name: 'Workflow name' })).toHaveValue('Visible policy');
});

test('returns keyboard focus to validation after refreshing a publication conflict', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  await page.route('**/api/v1/projects/*/issue-workflows/**', async (route) => {
    const request = route.request();
    if (request.method() === 'GET') {
      await route.fulfill({ json: { ...workflow, version: 5 } });
    } else if (request.url().endsWith('/validate')) {
      await route.fulfill({ json: request.postDataJSON().definition });
    } else {
      await route.fulfill({
        status: 409,
        json: { code: 'conflict', message: 'Workflow changed.', request_id: 'workflow-stale' },
      });
    }
  });
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  await page.getByRole('textbox', { name: 'Workflow name' }).fill('Retained draft');
  await page.getByRole('button', { name: 'Validate draft' }).click();
  await page.getByRole('button', { name: 'Review publication' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Publish version' }).click();
  const refresh = page.getByRole('button', { name: 'Refresh current data' });
  await expect(refresh).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByRole('textbox', { name: 'Workflow name' })).toHaveValue('Retained draft');
  await expect(page.getByRole('button', { name: 'Review publication' })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Validate draft' })).toBeFocused();
});
