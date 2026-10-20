import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { comment, timelineFixture, resultsFixture } from '../fixtures/issueActivity';
import { issueFixture } from '../fixtures/issues';
import { executionFixture } from '../fixtures/issueExecutions';
import { enMessages } from '../../src/app/presentation/messages.en';
import { ruMessages } from '../../src/app/presentation/messages.ru';
// Synthetic discussion content must not be captured in browser trace/screenshot artifacts.
test.use({ trace: 'off', screenshot: 'off', video: 'off' });
for (const language of ['en', 'ru'] as const) {
  const labels = language === 'en' ? enMessages : ruMessages;
  test(`${language}: operator discussion remains plain text, retries one frozen reply and discards drafts on leave/reload`, async ({
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
    await page.setViewportSize({ width: language === 'ru' ? 390 : 1280, height: 844 });
    const issue = issueFixture(FIXTURE_IDS.project);
    const base = `/projects/${FIXTURE_IDS.project}/issues/${issue.id}`;
    const apiBase = `/api/v1${base}`;
    const metadata = { ...comment, project_id: issue.project_id, issue_id: issue.id };
    const reply = {
      ...metadata,
      id: '22222222-2222-4222-8222-222222222222',
      sequence: 2,
      parent_id: metadata.id,
    };
    let contentReads = 0;
    let appended = false;
    const body = '<img src=x onerror=alert(1)> @agent approve — только обсуждение';
    const commands: { body: unknown; key: string | undefined; version: string | undefined }[] = [];
    const unrelated: string[] = [];
    page.on('request', (r) => {
      if (r.method() !== 'GET' && !new URL(r.url()).pathname.endsWith('/comments'))
        unrelated.push(r.url());
    });
    await page.route(
      (url) => url.pathname === `${apiBase}/timeline`,
      (route) =>
        route.fulfill({
          json: timelineFixture([
            { kind: 'discussion', comment: metadata },
            ...(appended ? [{ kind: 'discussion' as const, comment: reply }] : []),
          ]),
        }),
    );
    await page.route(
      (url) => url.pathname === `${apiBase}/comments/${metadata.id}`,
      (route) => {
        contentReads++;
        return route.fulfill({
          headers: { 'cache-control': 'no-store' },
          json: { comment: metadata, body },
        });
      },
    );
    await page.route(
      (url) => url.pathname === `${apiBase}/comments`,
      async (route) => {
        commands.push({
          body: route.request().postDataJSON(),
          key: route.request().headers()['idempotency-key'],
          version: route.request().headers()['if-match'],
        });
        appended = true;
        if (commands.length === 1) await route.abort('failed');
        else await route.fulfill({ json: { disposition: 'replayed', comment: reply } });
      },
    );
    await page.goto(`${base}?section=activity`);
    const activity = page.getByRole('region', { name: labels['issues.taskActivity'] });
    await expect(
      activity.getByRole('button', { name: labels['issues.readComment'] }),
    ).toBeVisible();
    expect(contentReads).toBe(0);
    await activity.getByRole('button', { name: labels['issues.readComment'] }).click();
    await expect(activity.getByText(body, { exact: true })).toBeVisible();
    await expect(activity.locator('img')).toHaveCount(0);
    await activity.getByRole('button', { name: labels['issues.replyComment'] }).click();
    const input = page.getByRole('textbox', { name: labels['issues.comment'], exact: true });
    await expect(input).toBeFocused();
    await input.fill(body);
    await page.getByRole('button', { name: labels['issues.reviewComment'] }).click();
    const dialog = page.getByRole('dialog');
    await expect(
      dialog.getByRole('button', { name: labels['issues.appendComment'] }),
    ).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(page.getByRole('button', { name: labels['issues.reviewComment'] })).toBeFocused();
    await page.getByRole('button', { name: labels['issues.reviewComment'] }).click();
    await dialog.getByRole('button', { name: labels['issues.appendComment'] }).click();
    await dialog.getByRole('button', { name: labels['command.retrySame'] }).click();
    await expect(dialog.getByText(labels['issues.commentRecorded'])).toBeVisible();
    expect(commands).toHaveLength(2);
    expect(commands[1]).toEqual(commands[0]);
    expect(commands[0]?.body).toEqual({ body, parent_id: metadata.id });
    expect(commands[0]?.key).toMatch(/^[0-9a-f-]{36}$/u);
    expect(commands[0]?.version).toBeUndefined();
    await dialog.getByRole('button', { name: labels['common.close'], exact: true }).click();
    await expect(input).toHaveValue('');
    await expect(activity.getByRole('button', { name: labels['issues.replyComment'] })).toHaveCount(
      1,
    );
    await input.fill('unfinished private draft');
    await page.getByRole('link', { name: labels['issues.overview'], exact: true }).click();
    await page.getByRole('link', { name: labels['issues.activity'], exact: true }).click();
    await expect(
      page.getByRole('textbox', { name: labels['issues.comment'], exact: true }),
    ).toHaveValue('');
    await page.reload();
    await expect(
      page.getByRole('textbox', { name: labels['issues.comment'], exact: true }),
    ).toHaveValue('');
    expect(contentReads).toBe(1);
    const persisted = await page.evaluate(() =>
      JSON.stringify({ local: localStorage, session: sessionStorage }),
    );
    expect(persisted).not.toContain(body);
    expect(persisted).not.toContain('unfinished private draft');
    expect(persisted).not.toContain(commands[0]!.key);
    expect(page.url()).not.toContain('private');
    expect(page.url()).not.toContain('Idempotency');
    expect(unrelated).toEqual([]);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
  });
  test(`${language}: historical Results preserve their subject across section navigation and show independent stale coverage`, async ({
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
    await page.setViewportSize({ width: language === 'ru' ? 390 : 1280, height: 844 });
    const issue = issueFixture(FIXTURE_IDS.project);
    const base = `/projects/${issue.project_id}/issues/${issue.id}`;
    const apiBase = `/api/v1${base}`;
    let stale = false;
    const reads: string[] = [];
    const writes: string[] = [];
    page.on('request', (request) => {
      if (request.method() !== 'GET') writes.push(request.url());
    });
    await page.route(
      (url) => url.pathname === `${apiBase}/timeline`,
      (route) => route.fulfill({ json: timelineFixture([]) }),
    );
    await page.route(
      (url) => url.pathname === `${apiBase}/executions/old-request`,
      (route) =>
        route.fulfill({ json: executionFixture('admitted', 'old-request', issue.project_id) }),
    );
    const result = resultsFixture([
      {
        kind: 'domain_fact',
        record: {
          id: 'old-decision',
          flow_run_id: 'original-flow',
          cycle_id: 'original-cycle',
          node_attempt_id: 'original-node',
          input_digest: 'a'.repeat(64),
          observed_at: 1,
          facts: {
            kind: 'decision',
            id: 'decision-one',
            subject: {
              exact: {
                project_id: issue.project_id,
                repository_id: 'code',
                base_revision: 'original-base',
              },
              candidate_revision: 'historical-head',
              changeset_digest: 'b'.repeat(64),
            },
            outcome: 'reject',
          },
        },
      },
    ]);
    result.association = {
      ...result.association,
      project_id: issue.project_id,
      issue_id: issue.id,
      request_id: 'old-request',
    };
    await page.route(
      (url) => url.pathname === `${apiBase}/results`,
      (route) => {
        reads.push(route.request().url());
        return route.fulfill({
          json: stale ? { ...result, coverage: 'stale', items: [] } : result,
        });
      },
    );
    await page.goto(
      `${base}?section=results&execution=old-request&result_collection=decisions&result_id=old-decision`,
    );
    await expect(page.getByText('historical-head', { exact: true })).toBeVisible();
    await expect(page.getByText('reject', { exact: true })).toBeVisible();
    await expect(page.getByText(labels['issues.resultCoverage.partial'])).toBeVisible();
    await page.getByRole('link', { name: labels['issues.activity'], exact: true }).click();
    await page.getByRole('link', { name: labels['issues.results'], exact: true }).click();
    await expect(page.getByText('historical-head', { exact: true })).toBeVisible();
    await page.reload();
    await expect(page.getByText('historical-head', { exact: true })).toBeVisible();
    expect(new URL(page.url()).searchParams.get('execution')).toBe('old-request');
    expect(new URL(page.url()).searchParams.get('result_id')).toBe('old-decision');
    stale = true;
    await page.getByRole('button', { name: labels['issues.refreshResults'] }).click();
    await expect(page.getByText(labels['issues.resultCoverage.stale'])).toBeVisible();
    expect(
      reads.every((value) => {
        const url = new URL(value);
        return (
          url.searchParams.get('work_id') === 'retained-work' &&
          url.searchParams.get('collection') === 'decisions' &&
          url.searchParams.get('limit') === '50'
        );
      }),
    ).toBe(true);
    expect(writes).toEqual([]);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
  });
}
