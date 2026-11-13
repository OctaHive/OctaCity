import { expect, test } from '@playwright/test';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';
import { issueClassificationFixture, classificationRun } from '../fixtures/issueClassification';
import { FIXTURE_IDS, installOperatorFixture } from './operatorFixture';

for (const language of ['en', 'ru'] as const) {
  test(`reviews Project classification and preserves a conflicting draft with keyboard (${language})`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    await page.addInitScript(
      ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
      { key: PREFERENCES_KEY, preferences: { ...DEFAULT_PREFERENCES, language } },
    );
    await page.setViewportSize({ width: 390, height: 844 });
    const ru = language === 'ru';
    const f = issueClassificationFixture(FIXTURE_IDS.project);
    let issue = f.issue;
    const base = `/api/v1/projects/${FIXTURE_IDS.project}/issues/${issue.id}`;
    const commands: { body: unknown; match: string | undefined }[] = [];
    await page.route(`**/api/v1/projects/${FIXTURE_IDS.project}/issue-property-choices`, (route) =>
      route.fulfill({ json: f.catalog }),
    );
    await page.route(`**${base}`, (route) => route.fulfill({ json: issue }));
    await page.route(`**${base}/revisions/*/classification`, (route) =>
      route.fulfill({ json: { ...f.review, version: issue.version } }),
    );
    await page.route(`**${base}/transitions`, (route) =>
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
    await page.route(`**${base}/outcomes`, (route) =>
      route.fulfill({
        json: {
          items: [
            {
              slot: 'triage',
              run_id: classificationRun,
              conclusion: 'Proposed large delivery',
              links: [],
              presentation: {
                kind: 'triage',
                decision: 'eligible',
                recommended_size: 'large',
                classification: { status: 'conflict' },
              },
            },
          ],
        },
      }),
    );
    await page.route(`**${base}/planning`, async (route) => {
      const request = route.request();
      const body = request.postDataJSON();
      commands.push({ body, match: request.headers()['if-match'] });
      if (commands.length === 1) {
        issue = { ...issue, version: 4 };
        return route.fulfill({
          status: 409,
          json: {
            code: 'conflict',
            message: 'Classification changed.',
            request_id: 'classification-conflict',
          },
        });
      }
      issue = { ...issue, version: 5, planning: body };
      return route.fulfill({ json: { disposition: 'applied', resource: issue } });
    });
    await page.goto(`/projects/${FIXTURE_IDS.project}/issues/${issue.id}`);
    await page
      .getByRole('button', { name: ru ? 'Свойства' : 'Properties', exact: true })
      .press('Enter');
    const panel = page.getByRole('complementary', { name: ru ? 'Свойства' : 'Properties' });
    const classification = panel.getByRole('region', {
      name: ru ? 'Классификация' : 'Classification',
    });
    await expect(
      classification.getByText(ru ? 'Требования изменились' : 'Requirements changed'),
    ).toBeVisible();
    await expect(panel.getByText('Critical incident', { exact: true })).toBeVisible();
    await expect(
      classification.getByRole('link', { name: ru ? 'Запуск оценки' : 'Producing Run' }),
    ).toHaveAttribute('href', `/factory/${classificationRun}`);
    await expect(
      page.getByText(
        ru
          ? 'Не удалось применить классификацию из-за конфликта'
          : 'Classification could not be applied',
        { exact: true },
      ),
    ).toBeVisible();
    const editPriority = panel.getByRole('button', {
      name: ru ? 'Изменить приоритет' : 'Change priority',
    });
    await editPriority.press('Enter');
    await expect(
      page.getByRole('combobox', { name: ru ? 'Приоритет' : 'Priority', exact: true }),
    ).toBeFocused();
    // Changing this field is a separate reviewed content command; abandon it here.
    await page.getByRole('button', { name: ru ? 'Отказаться' : 'Abandon', exact: true }).click();
    await panel
      .getByRole('button', { name: ru ? 'Изменить свойства' : 'Edit properties', exact: true })
      .press('Enter');
    const size = panel.getByRole('combobox', {
      name: ru ? 'Размер задачи' : 'Issue size',
      exact: true,
    });
    await size.focus();
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await expect(size).toContainText(ru ? 'Большой' : 'Large');
    const review = panel.getByRole('button', {
      name: ru ? 'Проверить свойства' : 'Review properties',
    });
    await review.press('Enter');
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
    await expect(dialog.getByText('Classification changed.')).toBeVisible();
    await dialog
      .getByRole('button', { name: ru ? 'Обновить текущие данные' : 'Refresh current data' })
      .click();
    await expect(size).toContainText(ru ? 'Большой' : 'Large');
    await expect(review).toBeDisabled();
    await panel
      .getByRole('button', {
        name: ru ? 'Обновить свойства и сбросить черновик' : 'Reload properties and discard draft',
      })
      .click();
    await size.press('ArrowDown');
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await review.press('Enter');
    await dialog
      .getByRole('button', { name: ru ? 'Сохранить свойства' : 'Save properties' })
      .click();
    await expect(dialog.getByRole('status')).toContainText(
      ru ? 'Сохранена версия задачи 5' : 'Saved task version 5',
    );
    await dialog.getByRole('button', { name: ru ? 'Закрыть' : 'Close', exact: true }).click();
    await expect(
      panel.getByRole('button', {
        name: ru ? 'Изменить свойства' : 'Edit properties',
        exact: true,
      }),
    ).toBeFocused();
    expect(commands).toEqual([
      { body: { ...f.issue.planning, size: 'large' }, match: '"3"' },
      { body: { ...f.issue.planning, size: 'large' }, match: '"4"' },
    ]);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
    expect(
      await page.evaluate(() =>
        [...Object.values(localStorage), ...Object.values(sessionStorage)].some(
          (v) => v.includes('payments') || v.includes('critical'),
        ),
      ),
    ).toBe(false);
    await page.screenshot({
      path: test.info().outputPath(`classification-${language}.png`),
      fullPage: true,
    });
  });
}
