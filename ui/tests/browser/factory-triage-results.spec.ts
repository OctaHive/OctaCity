import { expect, test } from '@playwright/test';
import { triageCompletion } from '../fixtures/factoryTriage';
import { runFixture } from '../fixtures/factory';
import { FIXTURE_IDS, installOperatorFixture } from './operatorFixture';

test.use({ trace: 'off', screenshot: 'off', video: 'off' });
for (const language of ['en', 'ru'] as const) {
  test(`retained triage assessments and separate Reporter receipts are readable with the keyboard at 390px (${language})`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    await page.addInitScript(
      (language) =>
        localStorage.setItem(
          'octacity.console.preferences',
          JSON.stringify({
            version: 2,
            language,
            theme: 'light',
            favorites: [],
            recents: [],
            expanded: [],
            explorerOpen: false,
            explorerWidth: 288,
            notificationLastOpenedAt: null,
          }),
        ),
      language,
    );
    await page.setViewportSize({ width: 390, height: 844 });
    const writes: string[] = [];
    page.on('request', (request) => {
      if (new URL(request.url()).pathname.startsWith('/api/v1/') && request.method() !== 'GET')
        writes.push(request.url());
    });
    await page.route('**/api/v1/factory-runs/**', async (route) => {
      const path = new URL(route.request().url()).pathname;
      const run = runFixture();
      if (path === `/api/v1/factory-runs/${FIXTURE_IDS.factoryRun}`)
        return route.fulfill({
          json: {
            ...run,
            issue_association: {
              project_id: 'original-project',
              issue_id: 'original-issue',
              work_id: run.work_id,
              run_id: run.id,
              request_id: 'original-request',
              requirements_version: 3,
              admitted_at_unix_ms: 1,
            },
          },
        });
      if (path.endsWith('/node-completions'))
        return route.fulfill({
          json: {
            items: ['eligibility', 'classification'].map((stage) => ({
              kind: 'node_completion',
              record: triageCompletion(stage === 'eligibility' ? 'eligibility' : 'classification'),
            })),
            next_cursor: null,
          },
        });
      if (path.endsWith('/effect-delivery'))
        return route.fulfill({
          json: {
            items: [
              {
                kind: 'effect_delivery',
                record: {
                  id: 'c'.repeat(64),
                  record_id: 'd'.repeat(64),
                  kind: 'work.status.projection',
                  input_digest: 'b'.repeat(64),
                  state: 'failed',
                  failure: 'forbidden',
                  attempt: 0,
                  available_at: 2,
                  recorded_at: 3,
                },
              },
            ],
            next_cursor: null,
          },
        });
      return route.fallback();
    });
    const text =
      language === 'en'
        ? {
            project: 'Project assessment',
            analyst: 'Systems analyst assessment',
            reporter: 'Reporter delivery',
            failure: 'Write authority unavailable; review the connection grant',
            provenance: 'Inspect stage provenance',
            journal: 'Diagnostic collection',
          }
        : {
            project: 'Оценка проекта',
            analyst: 'Оценка системного аналитика',
            reporter: 'Доставка Reporter',
            failure: 'Право записи недоступно; проверьте разрешения подключения',
            provenance: 'Открыть происхождение шага',
            journal: 'Коллекция диагностики',
          };
    await page.goto(`/factory/${FIXTURE_IDS.factoryRun}?state=waiting`);
    await expect(page.getByRole('heading', { name: text.project, exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: text.analyst, exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: text.reporter, exact: true })).toBeVisible();
    await expect(page.getByText(text.failure)).toBeVisible();
    await expect(page.locator('body')).not.toContainText('hidden-context');
    await expect(page.locator('body')).not.toContainText('hidden-candidate-payload');
    const provenance = page.getByRole('link', { name: text.provenance }).first();
    await provenance.focus();
    await page.keyboard.press('Enter');
    await expect(page).toHaveURL(/node_attempt_id=original-eligibility-agent-attempt/);
    expect(new URL(page.url()).searchParams.get('diagnostic')).toBe('calls');
    await page.reload();
    await expect(page.getByRole('heading', { name: text.analyst, exact: true })).toBeVisible();
    expect(
      await page.evaluate(() =>
        Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth),
      ),
    ).toBe(0);
    expect(writes).toEqual([]);
  });
}
