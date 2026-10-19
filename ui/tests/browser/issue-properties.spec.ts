import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { issueFixture, issuePropertyChoicesFixture } from '../fixtures/issues';

for (const language of ['en', 'ru'] as const) {
  test(`edits provider-independent Properties with keyboard and related tasks (${language})`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    if (language === 'ru') {
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
    }
    const ru = language === 'ru';
    let issue = issueFixture(FIXTURE_IDS.project);
    const base = `/api/v1/projects/${FIXTURE_IDS.project}/issues`;
    const historical = structuredClone(issue);
    await page.route(`**${base}/${issue.id}/revisions/1`, (route) =>
      route.fulfill({ json: historical }),
    );
    const choices = issuePropertyChoicesFixture(FIXTURE_IDS.project);
    choices.assignees = {
      available: true,
      items: [
        { id: 'person-1', name: 'Alice' },
        { id: 'person-2', name: 'A'.repeat(128) },
      ],
    };
    choices.definitions.definition.labels = [
      { id: 'important', name: 'Important', retired: false },
    ];
    const commands: { body: unknown; match: string | undefined; key: string | undefined }[] = [];
    const unexpected: string[] = [];
    page.on('request', (request) => {
      if (/\/api\/v1\/(integrations|factory)/u.test(request.url())) unexpected.push(request.url());
    });
    await page.route(`**/api/v1/projects/${FIXTURE_IDS.project}/issue-property-choices`, (route) =>
      route.fulfill({ json: choices }),
    );
    await page.route(`**${base}/${issue.id}`, (route) => route.fulfill({ json: issue }));
    await page.route(`**${base}/${issue.id}/transitions`, (route) =>
      route.fulfill({
        json: {
          issue_version: issue.version,
          workflow_id: issue.workflow_id,
          workflow_version: issue.workflow_version,
          status_id: issue.status_id,
          field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
          items: [],
        },
      }),
    );
    await page.route(`**${base}/${issue.id}/planning`, async (route) => {
      const request = route.request();
      const body = request.postDataJSON();
      commands.push({
        body,
        match: request.headers()['if-match'],
        key: request.headers()['idempotency-key'],
      });
      issue = { ...issue, version: 2, planning: body };
      await route.fulfill({ json: { disposition: 'applied', resource: issue } });
    });
    await page.route(`**${base}/${issue.id}/children?*`, (route) =>
      route.fulfill({
        json: {
          items: [
            {
              ...issue,
              id: '40000000-0000-4000-8000-000000000002',
              display_key: 'ISS-43',
              title: 'A visible sub-issue',
            },
          ],
          next_cursor: null,
        },
      }),
    );
    await page.goto(
      `/projects/${FIXTURE_IDS.project}/issues/${issue.id}?section=activity&revision=1`,
    );
    if (ru) {
      await page.getByRole('button', { name: 'Свойства', exact: true }).focus();
      await page.keyboard.press('Enter');
    }
    const panel = page.getByRole('complementary', { name: ru ? 'Свойства' : 'Properties' });
    await panel
      .getByRole('button', { name: ru ? 'Изменить свойства' : 'Edit properties', exact: true })
      .click();
    const assignee = panel.getByRole('combobox', {
      name: ru ? 'Ответственный' : 'Responsible party',
      exact: true,
    });
    await expect(assignee).toBeFocused();
    await assignee.click();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
    await panel.getByRole('option', { name: 'Alice', exact: true }).click();
    await panel.getByLabel('Important', { exact: true }).check();
    const checkboxSize = await panel.getByLabel('Important', { exact: true }).evaluate((el) => ({
      width: el.getBoundingClientRect().width,
      label: el.closest('label')!.getBoundingClientRect().width,
    }));
    expect(checkboxSize.width).toBeLessThan(checkboxSize.label / 2);
    await page.screenshot({
      path: test.info().outputPath(`properties-edit-${language}.png`),
      fullPage: true,
    });
    await panel.getByLabel(ru ? 'Срок' : 'Due date', { exact: true }).fill('2026-11-20');
    await panel.getByRole('combobox', { name: 'Task type', exact: true }).click();
    await panel.getByRole('option', { name: 'Defect', exact: true }).click();
    await panel.getByLabel('Component', { exact: true }).fill('ui');
    const review = panel.getByRole('button', {
      name: ru ? 'Проверить свойства' : 'Review properties',
    });
    await review.focus();
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog');
    await expect(
      dialog.getByRole('button', { name: ru ? 'Сохранить свойства' : 'Save properties' }),
    ).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(review).toBeFocused();
    await review.press('Enter');
    await dialog
      .getByRole('button', { name: ru ? 'Сохранить свойства' : 'Save properties' })
      .click();
    await expect(dialog.getByRole('status')).toContainText(
      ru ? 'Сохранена версия задачи 2' : 'Saved task version 2',
    );
    await dialog.getByRole('button', { name: ru ? 'Закрыть' : 'Close', exact: true }).click();
    await expect(
      panel.getByRole('button', {
        name: ru ? 'Изменить свойства' : 'Edit properties',
        exact: true,
      }),
    ).toBeFocused();
    await expect(panel.getByRole('link', { name: 'ISS-43 · A visible sub-issue' })).toBeVisible();
    await expect(page).toHaveURL(/section=activity&revision=1/u);
    await expect(page.getByText(issue.id, { exact: true })).toBeHidden();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
    expect(commands).toEqual([
      {
        body: {
          definition_version: 7,
          assignee: 'person-1',
          labels: ['important'],
          due_date: '2026-11-20',
          parent_id: null,
          values: {
            type: { type: 'choice', value: 'defect' },
            component: { type: 'text', value: 'ui' },
          },
        },
        match: '"1"',
        key: expect.stringMatching(/^[0-9a-f-]{36}$/u),
      },
    ]);
    expect(unexpected).toEqual([]);
    expect(
      await page.evaluate(() =>
        [...Object.values(localStorage), ...Object.values(sessionStorage)].some(
          (v) => v.includes('person-1') || v.includes('2026-11-20'),
        ),
      ),
    ).toBe(false);
    await page.screenshot({
      path: test.info().outputPath(`properties-${language}.png`),
      fullPage: true,
    });
  });
}
