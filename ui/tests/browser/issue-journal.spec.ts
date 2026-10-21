import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { issueFixture } from '../fixtures/issues';
import { executionFixture } from '../fixtures/issueExecutions';
import { journalFixture, journalEventsFixture } from '../fixtures/issueJournal';
import { diagnosticFixture } from '../fixtures/factory';
for (const language of ['en', 'ru'])
  test(`${language}: historical journal supports keyboard inspection at 390px without automatic content reads`, async ({
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
    const issue = issueFixture(FIXTURE_IDS.project);
    const base = `/api/v1/projects/${issue.project_id}/issues/${issue.id}/executions`;
    let details = 0;
    let writes = 0;
    await page.route(
      (url) => url.pathname.startsWith(base),
      async (route) => {
        const url = new URL(route.request().url());
        if (route.request().method() !== 'GET') writes++;
        if (url.pathname.endsWith('/events/1')) {
          details++;
          await route.fulfill({
            json: {
              cursor: 1,
              items: [
                {
                  sequence: 1,
                  occurred_at_unix_ms: 1100,
                  kind: 'public_operation',
                  payload: { note: '<img src=x onerror=alert(1)>' },
                },
              ],
            },
          });
        } else if (url.pathname.endsWith('/events'))
          await route.fulfill({
            json: journalEventsFixture(
              executionFixture('admitted', 'old-request', issue.project_id).association!,
            ),
          });
        else if (url.pathname.endsWith('/journal'))
          await route.fulfill({
            json: journalFixture(
              url.searchParams.get('collection') === 'builds'
                ? [{ kind: 'build', record: journalEventsFixture().job.execution }]
                : diagnosticFixture(url.searchParams.get('collection') ?? 'node-attempts'),
              executionFixture('admitted', 'old-request', issue.project_id),
            ),
          });
        else
          await route.fulfill({
            json:
              url.pathname === base
                ? {
                    items: [],
                    next_cursor: null,
                    watermark: 0,
                    issue_version: 1,
                    observation: 'request-watermark-v1',
                  }
                : executionFixture('admitted', 'old-request', issue.project_id),
          });
      },
    );
    await page.goto(
      `/projects/${issue.project_id}/issues/${issue.id}?section=runs&execution=old-request`,
    );
    const ru = language === 'ru';
    const job = page.getByRole('button', {
      name: ru ? 'Открыть Job original-job' : 'Inspect Job original-job',
    });
    await job.focus();
    await page.keyboard.press('Enter');
    await expect(page.getByText('src/main.rs')).toBeVisible();
    expect(details).toBe(0);
    const event = page.getByRole('button', {
      name: ru ? 'Открыть событие №1' : 'Inspect event #1',
      exact: true,
    });
    await event.focus();
    await page.keyboard.press('Enter');
    await expect(
      page.locator('pre').filter({ hasText: '<img src=x onerror=alert(1)>' }),
    ).toBeVisible();
    await expect(page.locator('pre img')).toHaveCount(0);
    expect(details).toBe(1);
    expect(writes).toBe(0);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
    const stored = await page.evaluate(() =>
      JSON.stringify({ local: localStorage, session: sessionStorage }),
    );
    expect(stored).not.toContain('onerror');
    await expect(page).toHaveURL(/execution=old-request/u);
  });
