import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { issueFixture } from '../fixtures/issues';
import { executionFixture } from '../fixtures/issueExecutions';
import { configurationFixture } from '../fixtures/factory';

// These HTTP fixtures exercise the real generated Console client, including lost acknowledgements.
test.use({ trace: 'off', screenshot: 'off', video: 'off' });
for (const language of ['en', 'ru'] as const) {
  test(`${language}: keyboard launch, lost response, pending resume and history survive narrow-screen editing and reload`, async ({
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
    const projectId = FIXTURE_IDS.project;
    let issue = {
      ...issueFixture(projectId),
      content: {
        ...issueFixture(projectId).content,
        description: 'Long requirements '.repeat(60),
        acceptance: 'Check checkout works',
      },
    };
    const base = `/projects/${projectId}/issues/${issue.id}`;
    const apiBase = `/api/v1${base}`;
    const configuration = {
      ...configurationFixture().configuration.reference,
      project_id: projectId,
    };
    const requests: { body: unknown; key: string | undefined; version: string | undefined }[] = [];
    let state: 'pending' | 'admitted' = 'pending';
    let catalogReads = 0;
    const receipt = () => executionFixture(state, 'request-one', projectId);
    await page.route(
      (url) => url.pathname === apiBase,
      async (route) => {
        if (route.request().method() !== 'GET') {
          issue = { ...issue, version: 2, content: route.request().postDataJSON() };
          await route.fulfill({ json: { disposition: 'applied', resource: issue } });
        } else await route.fulfill({ json: issue });
      },
    );
    await page.route(
      (url) => url.pathname === `${apiBase}/transitions`,
      (route) =>
        route.fulfill({
          json: {
            issue_version: issue.version,
            workflow_id: issue.workflow_id,
            workflow_version: issue.workflow_version,
            status_id: issue.status_id,
            items: [],
            field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
          },
        }),
    );
    await page.route(
      (url) => url.pathname === `/api/v1/projects/${projectId}/factory-configurations`,
      (route) => {
        catalogReads++;
        return route.fulfill({
          json: {
            items: [
              { ...configuration, enabled: true, created_at_unix_ms: 1, published_at_unix_ms: 1 },
            ],
            total: 1,
            next_cursor: null,
          },
        });
      },
    );
    await page.route(
      (url) => url.pathname === `/api/v1/projects/${projectId}/repositories`,
      (route) =>
        route.fulfill({
          json: {
            items: [
              {
                id: 'repo',
                project_id: projectId,
                name: 'Application source',
                version: 2,
                published_at_unix_ms: 1,
              },
            ],
            next_cursor: null,
          },
        }),
    );
    await page.route('**/api/v1/repositories/repo/versions/2', (route) =>
      route.fulfill({
        json: {
          id: 'repo',
          project_id: projectId,
          name: 'Application source',
          version: 2,
          published_at_unix_ms: 1,
          definition: {
            vcs_integration_id: 'provider-neutral',
            repository_locator: 'hidden',
            selection: {
              default_reference: 'main',
              allowed_references: ['main'],
              allow_exact_revision: false,
            },
          },
        },
      }),
    );
    await page.route(
      (url) => url.pathname.startsWith(`${apiBase}/executions`),
      async (route) => {
        if (route.request().method() === 'POST') {
          const headers = route.request().headers();
          requests.push({
            body: route.request().postDataJSON(),
            key: headers['idempotency-key'],
            version: headers['if-match'],
          });
          if (requests.length === 1) {
            await route.abort('failed');
            return;
          }
          if (requests.length >= 3) state = 'admitted';
          await route.fulfill({
            status: state === 'pending' ? 202 : 200,
            json: { disposition: 'replayed', resource: receipt() },
          });
        } else
          await route.fulfill({
            json: new URL(route.request().url()).pathname.endsWith('/executions')
              ? {
                  items: [receipt()],
                  next_cursor: null,
                  watermark: 1,
                  observation: 'request-watermark-v1',
                  issue_version: issue.version,
                }
              : receipt(),
          });
      },
    );
    const ru = language === 'ru';
    const names = {
      start: ru ? 'Запустить pipeline' : 'Start pipeline',
      review: ru ? 'Проверить запуск pipeline' : 'Review pipeline launch',
      confirm: ru ? 'Подтвердить запуск pipeline' : 'Confirm pipeline launch',
      retry: ru ? 'Повторить ту же команду' : 'Retry same command',
      resume: ru ? 'Возобновить подготовку' : 'Resume preparation',
      open: ru ? 'Открыть Run' : 'Open Run',
    };
    await page.goto(base);
    await expect(page.getByRole('button', { name: names.start, exact: true })).toBeVisible();
    expect(catalogReads).toBe(0);
    await page.getByRole('button', { name: names.start, exact: true }).focus();
    await page.keyboard.press('Enter');
    const review = page.getByRole('button', { name: names.review, exact: true });
    await expect(review).toBeEnabled();
    await review.focus();
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog');
    const box = (await dialog.boundingBox())!;
    expect(box.height).toBeLessThanOrEqual(844 - 24);
    await expect(dialog.getByRole('button', { name: names.confirm })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(review).toBeFocused();
    await review.click();
    await page.getByRole('dialog').getByRole('button', { name: names.confirm }).click();
    await page.getByRole('button', { name: names.retry, exact: true }).click();
    await expect(page.getByRole('button', { name: names.resume })).toBeVisible();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await page
      .getByRole('button', {
        name: ru ? 'Редактировать требования' : 'Edit requirements',
        exact: true,
      })
      .click();
    await page
      .getByRole('textbox', { name: ru ? 'Описание' : 'Description', exact: true })
      .fill('Later requirements');
    await page
      .getByRole('button', { name: ru ? 'Проверить задачу' : 'Review task', exact: true })
      .click();
    await page
      .getByRole('dialog')
      .getByRole('button', { name: ru ? 'Сохранить задачу' : 'Save task', exact: true })
      .click();
    await page
      .getByRole('dialog')
      .getByRole('button', { name: ru ? 'Закрыть' : 'Close', exact: true })
      .click();
    await page.getByRole('button', { name: names.resume, exact: true }).click();
    await expect(page.getByRole('dialog').getByRole('link', { name: names.open })).toHaveAttribute(
      'href',
      '/factory/retained-run',
    );
    expect(requests).toHaveLength(3);
    expect(requests[1]).toEqual(requests[0]);
    expect(requests[2]).toEqual(requests[0]);
    expect(requests[2]!.version).toBe('"1"');
    await page
      .getByRole('dialog')
      .getByRole('button', { name: ru ? 'Закрыть' : 'Close', exact: true })
      .click();
    await page.getByRole('link', { name: ru ? 'Запуски' : 'Runs', exact: true }).click();
    await expect(page).toHaveURL(/execution=request-one/u);
    await page.reload();
    await expect(page.getByRole('link', { name: names.open, exact: true })).toBeVisible();
    await expect(
      page.getByText(
        ru
          ? 'Выполнение использует предыдущие требования. Текущие изменения войдут в следующий запуск.'
          : 'This execution uses earlier requirements. Current edits apply to a later execution.',
      ),
    ).toBeVisible();
    expect(requests).toHaveLength(3);
    const stored = await page.evaluate(() =>
      JSON.stringify({ local: localStorage, session: sessionStorage }),
    );
    expect(stored).not.toContain(requests[0]!.key);
    expect(stored).not.toContain('Later requirements');
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
  });
}
