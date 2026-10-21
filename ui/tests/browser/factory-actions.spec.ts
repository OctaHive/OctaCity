import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import type { FactoryActionItem } from '../../src/api/factoryActions';

// Exercises generated HTTP controls and memory-only transport replay; no provider is configured.
test.use({ trace: 'off', screenshot: 'off', video: 'off' });
for (const language of ['en', 'ru'] as const) {
  test(`${language}: exact parallel gate, lost acknowledgement and recorded receipt survive reload`, async ({
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
    const run = FIXTURE_IDS.factoryRun,
      attempt = '55555555-5555-4555-8555-555555555555';
    const reference = { identity: 'operator.response', version: 'v1', digest: 'a'.repeat(64) };
    const row: FactoryActionItem = {
      project_id: FIXTURE_IDS.project,
      association: null,
      action: {
        work_id: 'work',
        run_id: run,
        run_version: 9,
        flow_run_id: 'flow',
        cycle_id: 'cycle',
        node_attempt_id: attempt,
        contract: { id: 'definition', version: 1, digest: 'a'.repeat(64) },
        node_key: 'legal_review',
        input_digest: 'b'.repeat(64),
        candidate: null,
        actionable_at: 11,
        deadline: null,
        kind: {
          kind: 'respond',
          outcomes: [{ key: 'approved', kind: 'success', schema: reference }],
          binding: null,
          schemas: [{ reference, shape: { type: 'boolean' } }],
        },
      },
    };
    let recorded = false;
    const requests: { body: unknown; version: string | undefined; key: string | undefined }[] = [];
    await page.route(
      (url) => url.pathname === '/api/v1/factory-actions',
      (route) =>
        route.fulfill({
          json: {
            total: recorded ? 0 : 2,
            coverage: 'complete',
            items: recorded
              ? []
              : [
                  {
                    ...row,
                    action: {
                      ...row.action,
                      run_id: 'other-run',
                      node_attempt_id: 'other-attempt',
                      run_version: 3,
                    },
                  },
                  row,
                ],
            next_cursor: null,
            observed_at: 12,
          },
        }),
    );
    const target = `/api/v1/factory-runs/${run}/nodes/${attempt}`;
    await page.route(
      (url) => url.pathname === `${target}/action`,
      (route) =>
        route.fulfill({
          json: {
            run_id: run,
            node_attempt_id: attempt,
            run_version: recorded ? 10 : 9,
            state: recorded ? 'recorded' : 'waiting',
            description: recorded ? null : row.action,
            accepted_candidate: false,
            recorded_version: recorded ? 9 : null,
            recorded_at: recorded ? 13 : null,
            observed_at: 14,
          },
        }),
    );
    await page.route(
      (url) => url.pathname === `${target}/responses`,
      async (route) => {
        const headers = route.request().headers();
        requests.push({
          body: route.request().postDataJSON(),
          version: headers['if-match'],
          key: headers['idempotency-key'],
        });
        recorded = true;
        if (requests.length === 1) {
          await route.abort('failed');
          return;
        }
        await route.fulfill({
          json: {
            run_id: run,
            version: 10,
            state: 'waiting',
            disposition: 'replayed',
            cancellation_requested: false,
          },
        });
      },
    );
    const ru = language === 'ru';
    await page.goto('/factory-actions');
    const article = page.getByRole('article', { name: `legal_review · ${run}` });
    const review = article.getByRole('button', {
      name: ru ? 'Проверить действие' : 'Review action',
    });
    await review.focus();
    await page.keyboard.press('Enter');
    await page
      .getByRole('checkbox', { name: ru ? 'Ответ человека' : 'Human response', exact: true })
      .check();
    const response = page.getByRole('button', {
      name: ru ? 'Просмотреть ответ HumanGate' : 'Review HumanGate response',
    });
    await response.click();
    const confirm = page
      .getByRole('dialog')
      .getByRole('button', { name: ru ? 'Подтвердить команду' : 'Confirm command' });
    await expect(confirm).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(response).toBeFocused();
    await response.click();
    await confirm.click();
    await page
      .getByRole('button', {
        name: ru ? 'Повторить ту же команду' : 'Retry same command',
        exact: true,
      })
      .click();
    await expect(
      page
        .getByRole('dialog')
        .getByText(
          ru
            ? 'Команда записана; ожидается обработка владельцем выполнения.'
            : 'Command recorded; awaiting execution owner.',
        ),
    ).toBeVisible();
    expect(requests).toHaveLength(2);
    expect(requests[1]).toEqual(requests[0]);
    expect(requests[0]!.version).toBe('"9"');
    expect(requests[0]!.body).toEqual({ outcome: 'approved', value: true });
    await page
      .getByRole('dialog')
      .getByRole('button', { name: ru ? 'Закрыть' : 'Close', exact: true })
      .click();
    await page.reload();
    await expect(
      page.getByText(
        ru ? 'Ожидается обработка владельцем выполнения.' : 'Awaiting execution owner.',
        { exact: true },
      ),
    ).toBeVisible();
    expect(requests).toHaveLength(2);
    const stored = await page.evaluate(() =>
      JSON.stringify({ local: localStorage, session: sessionStorage }),
    );
    expect(stored).not.toContain(requests[0]!.key);
    expect(stored).not.toContain('approved');
    expect(page.url()).not.toContain('approved');
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
  });
}
