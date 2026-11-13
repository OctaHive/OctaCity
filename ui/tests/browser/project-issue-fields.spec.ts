import { expect, test } from '@playwright/test';
import type { components } from '../../.generated/api/schema';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { issueFixture } from '../fixtures/issues';

type S = components['schemas'];
test.use({ trace: 'off', screenshot: 'off', video: 'off' });
for (const language of ['en', 'ru'] as const) {
  test(`${language}: publishes Project field contracts with conflict review, exact retry and Issue selectors at 390px`, async ({
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
    let catalog: S['PublishedIssueProperties'] = {
      project_id: project,
      version: 0,
      published_at_unix_ms: 1,
      definition: { labels: [], properties: [] },
    };
    let issue = issueFixture(project);
    const publications: { body: S['PublishIssuePropertiesRequest']; key: string | undefined }[] =
      [];
    const planning: S['IssuePlanning'][] = [];
    await page.route(`**/api/v1/projects/${project}/issue-property-choices`, (route) =>
      route.fulfill({ json: { definitions: catalog, assignees: { available: false, items: [] } } }),
    );
    await page.route(`**/api/v1/projects/${project}/issue-property-definitions`, async (route) => {
      const body: S['PublishIssuePropertiesRequest'] = route.request().postDataJSON();
      publications.push({ body, key: route.request().headers()['idempotency-key'] });
      expect(route.request().headers()['if-match']).toBeUndefined();
      if (publications.length === 1) {
        catalog = {
          ...catalog,
          version: 1,
          definition: {
            ...catalog.definition,
            properties: [
              {
                id: 'imported_flag',
                name: 'Imported flag',
                retired: false,
                kind: { type: 'boolean' },
              },
            ],
          },
        };
        await route.fulfill({
          status: 409,
          json: {
            code: 'conflict',
            message: 'Project catalog changed',
            request_id: 'field-conflict',
          },
        });
      } else if (publications.length === 2) {
        catalog = { ...catalog, version: 2, definition: body.definition };
        await route.abort('connectionfailed');
      } else await route.fulfill({ json: { disposition: 'replayed', resource: catalog } });
    });
    const issueBase = `/api/v1/projects/${project}/issues/${issue.id}`;
    await page.route(
      (url) => url.pathname === issueBase,
      (route) => route.fulfill({ json: issue }),
    );
    await page.route(
      (url) => url.pathname === `${issueBase}/transitions`,
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
      (url) => url.pathname === `${issueBase}/planning`,
      async (route) => {
        const body: S['IssuePlanning'] = route.request().postDataJSON();
        planning.push(body);
        issue = { ...issue, version: 2, planning: body };
        await route.fulfill({ json: { disposition: 'applied', resource: issue } });
      },
    );
    const ru = language === 'ru';
    const names = {
      settings: ru ? 'Настройки полей Issue' : 'Issue field settings',
      components: ru ? 'Компоненты' : 'Components',
      priorities: ru ? 'Приоритеты' : 'Priorities',
      types: ru ? 'Типы Issue' : 'Issue types',
      newOption: ru ? 'Название нового варианта' : 'New option name',
      addOption: ru ? 'Добавить вариант' : 'Add option',
      optionName: ru ? 'Название варианта' : 'Option display name',
      fields: ru ? 'Пользовательские поля Issue' : 'Custom Issue fields',
      newField: ru ? 'Название нового поля' : 'New field name',
      fieldType: ru ? 'Тип поля' : 'Field type',
      addField: ru ? 'Добавить поле' : 'Add field',
      review: ru ? 'Проверить определения полей' : 'Review field definitions',
      publish: ru ? 'Опубликовать определения полей' : 'Publish field definitions',
      reconcile: ru ? 'Согласовать с текущим каталогом' : 'Reconcile with current catalog',
      close: ru ? 'Закрыть' : 'Close',
    };
    await page.goto(`/projects/${project}`);
    const toggle = page.getByRole('button', { name: names.settings, exact: true });
    await toggle.focus();
    await page.keyboard.press('Enter');
    const components = page.getByRole('group', { name: names.components, exact: true });
    await components.getByRole('textbox', { name: names.newOption, exact: true }).fill('Payments');
    await components.getByRole('button', { name: names.addOption, exact: true }).click();
    const priorities = page.getByRole('group', { name: names.priorities, exact: true });
    await priorities
      .getByRole('textbox', { name: names.optionName, exact: true })
      .nth(1)
      .fill('Urgent response');
    await priorities
      .getByRole('checkbox', { name: ru ? 'Выведен из использования' : 'Retired', exact: true })
      .nth(3)
      .check();
    const types = page.getByRole('group', { name: names.types, exact: true });
    await types.getByRole('textbox', { name: names.newOption, exact: true }).fill('Incident');
    await types.getByRole('button', { name: names.addOption, exact: true }).click();
    const fields = page.getByRole('group', { name: names.fields, exact: true });
    async function addField(name: string, type: string) {
      await fields.getByRole('textbox', { name: names.newField, exact: true }).fill(name);
      await fields.getByRole('combobox', { name: names.fieldType, exact: true }).click();
      await page.getByRole('option', { name: type, exact: true }).click();
      await fields.getByRole('button', { name: names.addField, exact: true }).click();
      return fields.getByRole('group', { name, exact: true });
    }
    const environment = await addField('Environment', ru ? 'Текст' : 'Text');
    await environment
      .getByRole('spinbutton', {
        name: ru ? 'Максимум байт UTF-8' : 'Maximum UTF-8 bytes',
        exact: true,
      })
      .fill('128');
    const exposure = await addField('Exposure', ru ? 'Выбор' : 'Choice');
    await exposure.getByRole('textbox', { name: names.newOption, exact: true }).fill('Routine');
    await exposure.getByRole('button', { name: names.addOption, exact: true }).click();
    await addField('Customer visible', ru ? 'Логическое' : 'Boolean');
    const effort = await addField('Effort', ru ? 'Целое число' : 'Integer');
    await effort
      .getByRole('spinbutton', { name: ru ? 'Максимальное целое' : 'Maximum integer', exact: true })
      .fill('20');
    await addField('Review day', ru ? 'Дата' : 'Date');
    await page
      .getByRole('combobox', { name: ru ? 'Поле риска' : 'Risk field', exact: true })
      .click();
    await page.getByRole('option', { name: 'Exposure', exact: true }).click();
    await toggle.click();
    await toggle.click();
    await expect(environment.getByRole('spinbutton')).toHaveValue('128');
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
    const review = page.getByRole('button', { name: names.review, exact: true });
    await review.focus();
    await page.keyboard.press('Enter');
    let dialog = page.getByRole('dialog');
    await expect(dialog.getByRole('button', { name: names.publish, exact: true })).toBeFocused();
    await expect(dialog).toContainText('Payments');
    await expect(dialog).toContainText('128');
    expect(publications).toHaveLength(0);
    await dialog.getByRole('button', { name: names.publish, exact: true }).click();
    await expect(dialog).toContainText('Project catalog changed');
    await dialog
      .getByRole('button', {
        name: ru ? 'Обновить текущие данные' : 'Refresh current data',
        exact: true,
      })
      .click();
    await expect(
      components.getByRole('textbox', { name: names.optionName, exact: true }),
    ).toHaveValue('Payments');
    await page.getByRole('button', { name: names.reconcile, exact: true }).click();
    await review.click();
    dialog = page.getByRole('dialog');
    await expect(dialog).toContainText('Imported flag');
    await dialog.getByRole('button', { name: names.publish, exact: true }).click();
    await dialog
      .getByRole('button', {
        name: ru ? 'Повторить ту же команду' : 'Retry same command',
        exact: true,
      })
      .click();
    await expect(dialog).toContainText(
      ru ? 'Определения полей опубликованы.' : 'Field definitions published.',
    );
    expect(publications).toHaveLength(3);
    expect(publications[1]).toEqual(publications[2]);
    expect(publications[0]!.key).not.toBe(publications[1]!.key);
    expect(publications.map((r) => r.body.expected_version)).toEqual([0, 1, 1]);
    expect(catalog.definition.priorities?.map((c) => c.id)).toEqual([
      'blocker',
      'high',
      'medium',
      'low',
    ]);
    expect(catalog.definition.types?.slice(0, 2).map((c) => c.id)).toEqual([
      'defect',
      'feature_request',
    ]);
    expect(planning).toHaveLength(0);
    await dialog.getByRole('button', { name: names.close, exact: true }).click();
    await page.goto(`/projects/${project}/issues/${issue.id}`);
    await page.getByRole('button', { name: ru ? 'Свойства' : 'Properties', exact: true }).click();
    const properties = page.getByRole('complementary', {
      name: ru ? 'Свойства' : 'Properties',
      exact: true,
    });
    await properties
      .getByRole('button', { name: ru ? 'Изменить свойства' : 'Edit properties', exact: true })
      .click();
    await properties
      .getByRole('combobox', { name: ru ? 'Компонент задачи' : 'Issue component', exact: true })
      .click();
    await properties.getByRole('option', { name: 'Payments', exact: true }).click();
    await properties
      .getByRole('combobox', { name: ru ? 'Тип задачи' : 'Issue type', exact: true })
      .click();
    await properties.getByRole('option', { name: 'Incident', exact: true }).click();
    await properties
      .getByRole('combobox', { name: ru ? 'Размер задачи' : 'Issue size', exact: true })
      .click();
    await expect(
      properties.getByRole('option', { name: ru ? 'Малый' : 'Small', exact: true }),
    ).toBeVisible();
    await properties.getByRole('option', { name: ru ? 'Малый' : 'Small', exact: true }).click();
    await properties.getByRole('textbox', { name: 'Environment', exact: true }).fill('staging');
    await properties
      .getByRole('button', { name: ru ? 'Проверить свойства' : 'Review properties', exact: true })
      .click();
    await page
      .getByRole('dialog')
      .getByRole('button', { name: ru ? 'Сохранить свойства' : 'Save properties', exact: true })
      .click();
    expect(planning).toHaveLength(1);
    expect(planning[0]).toMatchObject({
      definition_version: 2,
      component: catalog.definition.components![0]!.id,
      type: catalog.definition.types!.at(-1)!.id,
      size: 'small',
    });
    const environmentId = catalog.definition.properties.find((p) => p.name === 'Environment')!.id;
    expect(planning[0]!.values[environmentId]).toEqual({ type: 'text', value: 'staging' });
    await page.getByRole('dialog').getByRole('button', { name: names.close, exact: true }).click();
    const stored = await page.evaluate(() =>
      JSON.stringify({ local: localStorage, session: sessionStorage }),
    );
    expect(stored).not.toContain('staging');
    expect(stored).not.toContain('Urgent response');
    expect(stored).not.toContain(publications[1]!.key);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
  });
}
