import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { issueFixture } from '../fixtures/issues';
const base = `/projects/${FIXTURE_IDS.project}/issues`;

test('creates a manual task over HTTP without provider/Factory calls and restores confirmation focus', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const commands: { body: unknown; key: string | undefined }[] = [];
  const unexpected: string[] = [];
  page.on('request', (request) => {
    if (/\/api\/v1\/(integrations|factory)/u.test(request.url())) unexpected.push(request.url());
  });
  const issue = issueFixture(FIXTURE_IDS.project);
  await page.route(
    (url) => url.pathname === `/api/v1${base}`,
    async (route) => {
      if (route.request().method() === 'POST') {
        commands.push({
          body: route.request().postDataJSON(),
          key: route.request().headers()['idempotency-key'],
        });
        await route.fulfill({ status: 201, json: { disposition: 'applied', resource: issue } });
      } else await route.fulfill({ json: { items: [], next_cursor: null } });
    },
  );
  await page.goto(`/projects/${FIXTURE_IDS.project}`);
  await page.getByRole('link', { name: 'Tasks', exact: true }).click();
  await expect(page.getByText('No tasks yet. Create your first task here.')).toBeVisible();
  await page.getByRole('button', { name: 'Create task', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Title', exact: true })).toBeFocused();
  await page.getByRole('textbox', { name: 'Title', exact: true }).fill(issue.content.title);
  const review = page.getByRole('button', { name: 'Review task' });
  await review.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('button', { name: 'Create task' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(review).toBeFocused();
  await review.click();
  await dialog.getByRole('button', { name: 'Create task' }).click();
  await dialog.getByRole('link', { name: 'Open ISS-42' }).click();
  await expect(page.getByRole('heading', { name: 'ISS-42 · Investigate checkout' })).toBeVisible();
  expect(commands).toEqual([
    {
      body: { title: 'Investigate checkout', description: '', acceptance: null, priority: null },
      key: expect.stringMatching(/^[0-9a-f-]{36}$/u),
    },
  ]);
  expect(unexpected).toEqual([]);
});

test('keeps Russian manual card and historical selection usable with keyboard Properties at 390px', async ({
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
        explorerOpen: false,
        explorerWidth: 288,
        notificationLastOpenedAt: null,
      }),
    ),
  );
  await page.setViewportSize({ width: 390, height: 844 });
  const issue = issueFixture(FIXTURE_IDS.project);
  await page.route(`**/api/v1${base}/${issue.id}/revisions/1`, async (route) =>
    route.fulfill({ json: issue }),
  );
  await page.goto(`${base}/${issue.id}?section=activity&revision=1`);
  await expect(
    page.getByRole('heading', { name: 'Исторические требования · Версия 1' }),
  ).toBeVisible();
  const trigger = page.getByRole('button', { name: 'Свойства', exact: true });
  await trigger.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('complementary', { name: 'Свойства' })).toBeVisible();
  await expect(page.getByRole('complementary', { name: 'Свойства' })).toBeFocused();
  await expect(page.getByRole('complementary', { name: 'Свойства' })).toBeInViewport();
  await page.getByRole('button', { name: 'Закрыть свойства', exact: true }).click();
  await expect(trigger).toBeFocused();
  await expect(page.getByRole('button', { name: 'Запустить pipeline' })).toBeEnabled();
  await page.getByRole('link', { name: 'Обзор', exact: true }).click();
  await expect(page).toHaveURL(/revision=1/u);
  await expect(page.getByRole('heading', { name: 'Описание', exact: true })).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
    ),
  ).toBe(true);
});
test('keeps a compact task header above desktop content and right-side properties', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const issue = issueFixture(FIXTURE_IDS.project);
  await page.goto(`${base}/${issue.id}`);
  const heading = page.getByRole('heading', { name: 'ISS-42 · Investigate checkout', exact: true });
  const start = page.getByRole('button', { name: 'Start pipeline' });
  const properties = page.getByRole('complementary', { name: 'Properties' });
  const description = page.getByRole('heading', { name: 'Description', exact: true });
  await expect(heading).toBeVisible();
  await expect(properties).toBeVisible();
  await expect(page.getByText('Business status: New', { exact: true })).toBeVisible();
  const titleBox = (await heading.boundingBox())!;
  const startBox = (await start.boundingBox())!;
  const propertyBox = (await properties.boundingBox())!;
  const contentBox = (await description.boundingBox())!;
  expect(startBox.y).toBeLessThan(titleBox.y + titleBox.height + 30);
  expect(propertyBox.x).toBeGreaterThan(contentBox.x + contentBox.width);
  await page.screenshot({
    path: test.info().outputPath('manual-card-desktop.png'),
    fullPage: true,
  });
});
