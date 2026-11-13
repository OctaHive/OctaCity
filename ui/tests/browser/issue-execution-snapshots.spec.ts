import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { journalFixture } from '../fixtures/issueJournal';
import { executionFixture } from '../fixtures/issueExecutions';
import {
  manuallyPreparedIssue,
  materialSnapshotFixture,
  snapshotConfiguration,
  classifiedSnapshotFixture,
} from '../fixtures/issueExecutionSnapshots';

// Selected text and review identities must never enter failure artifacts or browser storage.
test.use({ trace: 'off', screenshot: 'off', video: 'off' });
for (const language of ['en', 'ru'] as const) {
  test(`${language}: reviews exact Issue text, re-reviews selection conflicts, and keeps frozen input on reload at 390px`, async ({
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
    const project = FIXTURE_IDS.project;
    let issue = manuallyPreparedIssue(project);
    const base = `/projects/${project}/issues/${issue.id}`;
    const apiBase = `/api/v1${base}`;
    const configuration = snapshotConfiguration(project);
    const snapshot = materialSnapshotFixture(project);
    const assessed = classifiedSnapshotFixture(project);
    snapshot.input.definition_version = 7;
    snapshot.definitions = assessed.definitions;
    snapshot.attribution.priority = {
      ...assessed.attribution.priority!,
      assessed_version: 1,
      requirements_version: 1,
    };
    configuration.configuration.flow.issue_snapshot = snapshot.input;
    let reviewCount = 0;
    const submissions: { body: unknown; key: string | undefined; version: string | undefined }[] =
      [];
    const receipt = executionFixture('admitted', 'request-one', project);
    receipt.snapshot_digest = 'f'.repeat(64);
    receipt.intent.expected_version = 2;
    let frozen = structuredClone(snapshot);
    await page.route(
      (url) => url.pathname === apiBase,
      (route) => route.fulfill({ json: issue }),
    );
    await page.route(
      (url) => url.pathname === `${apiBase}/revisions/1`,
      (route) =>
        route.fulfill({
          json: {
            ...issue,
            version: 1,
            content: {
              ...issue.content,
              priority: 'high',
              description: 'Original assessed requirements',
            },
          },
        }),
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
      (url) => url.pathname === `/api/v1/projects/${project}/factory-configurations`,
      (route) =>
        route.fulfill({
          json: {
            items: [
              {
                ...configuration.configuration.reference,
                enabled: true,
                created_at_unix_ms: 1,
                published_at_unix_ms: 1,
              },
            ],
            total: 1,
            next_cursor: null,
          },
        }),
    );
    await page.route(
      (url) =>
        url.pathname ===
        `/api/v1/factory-configurations/${configuration.configuration.reference.id}/versions/${configuration.configuration.reference.version}`,
      (route) => route.fulfill({ json: configuration }),
    );
    await page.route(
      (url) => url.pathname === `/api/v1/projects/${project}/repositories`,
      (route) =>
        route.fulfill({
          json: {
            items: [
              {
                id: 'repo',
                project_id: project,
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
          project_id: project,
          name: 'Application source',
          version: 2,
          published_at_unix_ms: 1,
          definition: {
            vcs_integration_id: 'provider-neutral',
            repository_locator: 'hidden',
            selection: {
              default_reference: 'main',
              allowed_references: ['main'],
              allow_exact_revision: true,
            },
          },
        },
      }),
    );
    await page.route(
      (url) => url.pathname.startsWith(`${apiBase}/executions`),
      async (route) => {
        const pathname = new URL(route.request().url()).pathname;
        if (pathname.includes('/journal')) {
          await route.fulfill({ json: journalFixture([], receipt) });
          return;
        }
        if (pathname.endsWith('/review')) {
          reviewCount++;
          const request = route.request().postDataJSON();
          expect(request.classification).toBeUndefined();
          expect(request.target.predecessor_run).toBeUndefined();
          expect(route.request().headers()['idempotency-key']).toBeUndefined();
          const selected = { ...snapshot, target: request.target, version: issue.version };
          await route.fulfill({
            json: {
              version: issue.version,
              target: request.target,
              snapshot: selected,
              snapshot_digest: issue.version === 1 ? 'f'.repeat(64) : '9'.repeat(64),
            },
          });
        } else if (route.request().method() === 'POST') {
          expect(route.request().postDataJSON().expected_snapshot_digest).toBe(
            issue.version === 1 ? 'f'.repeat(64) : '9'.repeat(64),
          );
          const headers = route.request().headers();
          submissions.push({
            body: route.request().postDataJSON(),
            key: headers['idempotency-key'],
            version: headers['if-match'],
          });
          if (submissions.length === 1) {
            issue = { ...issue, version: 2 };
            snapshot.values.context = [
              'Replacement approved text',
              'Reviewed task document v2',
              'Original shared guide',
            ];
            const document = snapshot.materials!.context![1]!;
            const artifact = {
              artifact_id: 'issue-document-v2',
              content_digest: '5'.repeat(64),
              encoded_size: 25,
            };
            document.selection.reference = {
              kind: 'issue_document',
              project_id: project,
              issue_id: issue.id,
              artifact,
            };
            if (document.source.kind === 'issue_document')
              document.source.document.artifact = artifact;
            await route.fulfill({
              status: 409,
              json: {
                code: 'conflict',
                message: 'Issue material selection changed',
                request_id: 'selection-conflict',
              },
            });
          } else {
            receipt.intent.target = route.request().postDataJSON().target;
            receipt.snapshot_digest = route.request().postDataJSON().expected_snapshot_digest;
            frozen = { ...structuredClone(snapshot), target: receipt.intent.target, version: 2 };
            await route.fulfill({ json: { disposition: 'applied', resource: receipt } });
          }
        } else if (pathname.endsWith('/snapshot')) {
          await route.fulfill({
            json: {
              availability: 'available',
              digest: receipt.snapshot_digest,
              snapshot: frozen,
              subject: null,
            },
          });
        } else
          await route.fulfill({
            json: pathname.endsWith('/executions')
              ? {
                  items: submissions.length > 1 ? [receipt] : [],
                  next_cursor: null,
                  watermark: 1,
                  observation: 'request-watermark-v1',
                  issue_version: issue.version,
                }
              : receipt,
          });
      },
    );
    const ru = language === 'ru';
    const name = {
      start: ru ? 'Запустить pipeline' : 'Start pipeline',
      review: ru ? 'Проверить запуск pipeline' : 'Review pipeline launch',
      confirm: ru ? 'Подтвердить запуск pipeline' : 'Confirm pipeline launch',
      refresh: ru ? 'Обновить текущие данные' : 'Refresh current data',
      abandon: ru ? 'Отменить' : 'Abandon',
      frozen: ru ? 'Зафиксированный вход выполнения' : 'Frozen execution input',
    };
    await page.goto(base);
    await page.getByRole('button', { name: name.start, exact: true }).focus();
    await page.keyboard.press('Enter');
    await page.getByRole('combobox', { name: ru ? 'Источник' : 'Source', exact: true }).click();
    await page
      .getByRole('option', { name: ru ? 'Точная ревизия' : 'Exact revision', exact: true })
      .click();
    const revision = page.getByRole('textbox', {
      name: ru ? 'Точная ревизия' : 'Exact revision',
      exact: true,
    });
    await revision.fill('preserved-sha');
    const review = page.getByRole('button', { name: name.review, exact: true });
    await expect(review).toBeEnabled();
    await review.focus();
    await page.keyboard.press('Enter');
    let dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Original task document', { exact: true })).toBeVisible();
    await expect(
      dialog.getByText(ru ? 'Ручное изменение' : 'Manual edit', { exact: true }),
    ).toBeVisible();
    await expect(
      dialog.getByText(ru ? 'Определения полей оценки: v2' : 'Assessment field definitions: v2', {
        exact: true,
      }),
    ).toBeVisible();
    await expect(
      dialog.getByRole('link', {
        name: ru ? 'Версия оценки 1' : 'Assessment revision 1',
        exact: true,
      }),
    ).toHaveAttribute('href', `${base}?section=activity&revision=1`);
    await expect(
      dialog.getByText(
        ru
          ? 'Необязательный вход пропущен: extra · unavailable'
          : 'Optional input omitted: extra · unavailable',
        { exact: true },
      ),
    ).toBeVisible();
    await expect(
      dialog.getByText('Approved comment <script>alert(1)</script>', { exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole('combobox', { name: ru ? 'Предыдущий запуск задачи' : 'Earlier Issue run' }),
    ).toHaveCount(0);
    expect(submissions).toHaveLength(0);
    await dialog.getByRole('button', { name: name.confirm, exact: true }).click();
    await expect(
      dialog.getByText('Issue material selection changed', { exact: true }),
    ).toBeVisible();
    await dialog.getByRole('button', { name: name.refresh, exact: true }).click();
    await expect(revision).toHaveValue('preserved-sha');
    await review.click();
    dialog = page.getByRole('dialog');
    await expect(dialog.getByText('Reviewed task document v2', { exact: true })).toBeVisible();
    await expect(dialog.getByText('Original task document', { exact: true })).toHaveCount(0);
    await expect(dialog.getByRole('button', { name: name.confirm, exact: true })).toBeEnabled();
    await dialog.getByRole('button', { name: name.confirm, exact: true }).click();
    await expect(
      dialog.getByRole('link', { name: ru ? 'Открыть Run' : 'Open Run', exact: true }),
    ).toBeVisible();
    expect(submissions).toHaveLength(2);
    expect(submissions[0]!.key).not.toBe(submissions[1]!.key);
    expect(submissions.map((s) => s.version)).toEqual(['"1"', '"2"']);
    await dialog.getByRole('button', { name: ru ? 'Закрыть' : 'Close', exact: true }).click();
    issue = {
      ...issue,
      version: 3,
      content: { ...issue.content, priority: 'low', description: 'New current document text' },
    };
    await page.getByRole('link', { name: ru ? 'Запуски' : 'Runs', exact: true }).click();
    await page.reload();
    const input = page.getByRole('region', { name: name.frozen, exact: true });
    await expect(input.getByText('Reviewed task document v2', { exact: true })).toBeVisible();
    await expect(input.getByText('High · high', { exact: true })).toBeVisible();
    await expect(
      input.getByText(ru ? 'Определения полей оценки: v2' : 'Assessment field definitions: v2', {
        exact: true,
      }),
    ).toBeVisible();
    await input
      .getByRole('link', {
        name: ru ? 'Версия оценки 1' : 'Assessment revision 1',
        exact: true,
      })
      .click();
    await expect(page.getByText('Original assessed requirements', { exact: true })).toBeVisible();
    expect(new URL(page.url()).searchParams.get('execution')).toBe('request-one');
    await page.getByRole('link', { name: ru ? 'Запуски' : 'Runs', exact: true }).click();
    await expect(input.getByText('Reviewed task document v2', { exact: true })).toBeVisible();
    expect(new URL(page.url()).searchParams.get('execution')).toBe('request-one');
    await expect(input.getByText('New current document text', { exact: true })).toHaveCount(0);
    expect(submissions).toHaveLength(2);
    expect(reviewCount).toBeGreaterThanOrEqual(2);
    const stored = await page.evaluate(() =>
      JSON.stringify({ local: localStorage, session: sessionStorage }),
    );
    expect(stored).not.toContain('Reviewed task document');
    expect(stored).not.toContain(submissions[1]!.key);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
  });
}
