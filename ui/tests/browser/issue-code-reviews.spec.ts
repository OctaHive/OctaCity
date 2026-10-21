import { reviewAccessFixture } from '../fixtures/issueCodeReviews';
import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import {
  reviewFixture,
  reviewPage,
  reviewConnectionFixture,
  reviewResourcesFixture,
} from '../fixtures/issueCodeReviews';
import { issueFixture } from '../fixtures/issues';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';

for (const language of ['en', 'ru'] as const) {
  test(`reviews and attaches a PR/MR with keyboard at 390px in ${language}`, async ({ page }) => {
    await installOperatorFixture(page);
    await page.addInitScript(
      ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
      { key: PREFERENCES_KEY, preferences: { ...DEFAULT_PREFERENCES, language } },
    );
    await page.setViewportSize({ width: 390, height: 844 });
    const issue = issueFixture(FIXTURE_IDS.project);
    const review = reviewFixture(issue.project_id);
    const base = `/api/v1/projects/${issue.project_id}/issues/${issue.id}`;
    const commands: { body: unknown; headers: Record<string, string> }[] = [];
    const unexpected: string[] = [];
    page.on('request', (request) => {
      if (request.method() !== 'GET' && !request.url().endsWith('/code-reviews'))
        unexpected.push(request.url());
    });
    await page.route(`**${base}/code-reviews*`, async (route) => {
      if (route.request().method() === 'POST') {
        commands.push({ body: route.request().postDataJSON(), headers: route.request().headers() });
        await route.fulfill({
          status: 201,
          json: { disposition: 'applied', association: review.association },
        });
      } else await route.fulfill({ json: reviewPage([review]) });
    });
    const connection = reviewConnectionFixture();
    await page.route('**/api/v1/integrations?*', (route) =>
      route.fulfill({ json: { items: [connection], next_cursor: null } }),
    );
    await page.route(
      `**/api/v1/integrations/${connection.id}/projects/${issue.project_id}/access`,
      (route) => route.fulfill({ json: reviewAccessFixture(issue.project_id) }),
    );
    await page.route(
      `**/api/v1/integrations/${connection.id}/projects/${issue.project_id}/resources`,
      (route) => route.fulfill({ json: reviewResourcesFixture() }),
    );
    await page.goto(`/projects/${issue.project_id}/issues/${issue.id}`);
    await page
      .getByRole('button', { name: language === 'en' ? 'Properties' : 'Свойства', exact: true })
      .click();
    const summary = page.getByRole('region', {
      name: language === 'en' ? 'PR/MR summary' : 'Сводка PR/MR',
    });
    const link = summary.getByRole('link', { name: 'MR-42 · Preserve manual tasks <script>' });
    await link.focus();
    await page.keyboard.press('Enter');
    const results = page.getByRole('region', {
      name: language === 'en' ? 'Pull and merge requests' : 'Запросы на слияние',
    });
    await expect(
      results.getByRole('heading', { name: 'MR-42 · Preserve manual tasks <script>' }),
    ).toBeVisible();
    await expect(results.locator('article')).toBeFocused();
    expect(commands).toEqual([]);
    const attachLabel = language === 'en' ? 'Attach PR/MR' : 'Привязать PR/MR';
    await results.getByRole('button', { name: attachLabel }).click();
    await results
      .getByRole('combobox', { name: language === 'en' ? 'Code connection' : 'Подключение кода' })
      .click();
    await page.getByRole('option', { name: 'Code forge · fake-mr' }).click();
    await results
      .getByRole('combobox', { name: language === 'en' ? 'Code repository' : 'Репозиторий кода' })
      .click();
    await page.getByRole('option', { name: 'team/code · review-profile / v1' }).click();
    await results
      .getByRole('textbox', { name: language === 'en' ? 'Native PR/MR ID' : 'Native ID PR/MR' })
      .fill('native-mr-42');
    const submit = results.getByRole('button', {
      name: language === 'en' ? 'Review attachment' : 'Проверить привязку',
    });
    await submit.click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByRole('button', { name: attachLabel })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(submit).toBeFocused();
    expect(commands).toEqual([]);
    await submit.click();
    await dialog.getByRole('button', { name: attachLabel }).click();
    await expect(
      dialog.getByText(language === 'en' ? 'PR/MR attached.' : 'PR/MR привязан.'),
    ).toBeVisible();
    expect(commands).toHaveLength(1);
    expect(commands[0]!.body).toEqual({ selection: review.association.selection });
    expect(commands[0]!.headers['if-match']).toBe('"1"');
    expect(commands[0]!.headers['idempotency-key']).toBeTruthy();
    expect(unexpected).toEqual([]);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
    const browserState = await page.evaluate(() =>
      JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
    );
    expect(browserState).not.toContain('native-mr-42');
    expect(browserState).not.toContain(commands[0]!.headers['idempotency-key']);
  });
}

test('reads retained head history and reports explicit provider-refresh failure through the generated client', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const issue = issueFixture(FIXTURE_IDS.project);
  const review = reviewFixture(issue.project_id);
  const base = `/api/v1/projects/${issue.project_id}/issues/${issue.id}/code-reviews`;
  const requests: { method: string; body: unknown; headers: Record<string, string> }[] = [];
  await page.route(`**${base}?*`, (route) => route.fulfill({ json: reviewPage([review]) }));
  await page.route(`**${base}/${review.association.id}/observations*`, async (route) => {
    const request = route.request();
    requests.push({
      method: request.method(),
      body: request.method() === 'POST' ? request.postDataJSON() : null,
      headers: request.headers(),
    });
    if (request.method() === 'POST') {
      await route.fulfill({
        status: 503,
        json: {
          code: 'unavailable',
          message: 'Review provider unavailable',
          request_id: 'review-refresh-outage',
        },
      });
    } else
      await route.fulfill({
        json: {
          association: review.association,
          items: [
            {
              id: 'de'.repeat(32),
              sequence: 1,
              association_id: review.association.id,
              observation: {
                ...review.observation!,
                head: 'head-one',
                checks: 'Old-head checks passed',
              },
            },
          ],
          availability: 'available',
          next_cursor: null,
        },
      });
  });
  await page.goto(`/projects/${issue.project_id}/issues/${issue.id}?section=results`);
  const results = page.getByRole('region', { name: 'Pull and merge requests' });
  await expect(results.getByText('head-two', { exact: true })).toBeVisible();
  expect(requests).toEqual([]);
  await results.getByRole('button', { name: 'Observation history' }).click();
  const history = results.getByRole('region', { name: 'Retained review history' });
  await expect(history.getByText('Old-head checks passed')).toBeVisible();
  await expect(history.getByText('Historical observation for a different head.')).toBeVisible();
  await results.getByRole('button', { name: 'Refresh PR/MR' }).click();
  const dialog = page.getByRole('dialog');
  expect(requests.map((request) => request.method)).toEqual(['GET']);
  await dialog.getByRole('button', { name: 'Refresh PR/MR' }).click();
  await expect(dialog.getByText('Review provider unavailable')).toBeVisible();
  await dialog.getByRole('button', { name: 'Abandon' }).click();
  await expect(
    results.getByText('Refresh failed; showing the last known observation.'),
  ).toBeVisible();
  await expect(results.getByText('head-two', { exact: true })).toBeVisible();
  expect(requests.map((request) => request.method)).toEqual(['GET', 'POST']);
  expect(requests[1]!.body).toEqual({});
  expect(requests[1]!.headers['if-match']).toBeUndefined();
  expect(requests[1]!.headers['idempotency-key']).toBeTruthy();
  await page.getByRole('link', { name: 'Overview', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Edit requirements' })).toBeEnabled();
});
