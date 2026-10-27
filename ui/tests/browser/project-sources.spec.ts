import { expect, test, type Page } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { integrationFixture } from '../fixtures/integrations';
import { issueFixture } from '../fixtures/issues';
import { issueSourceReview } from '../fixtures/issueSources';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';
import { translate, type MessageKey } from '../../src/app/presentation/messages';
import type {
  IssueSourceImport,
  IssueSourceImportIntent,
} from '../../src/features/issues/sourceApi';

async function installSources(page: Page) {
  await installOperatorFixture(page);
  const connection = integrationFixture('IntegrationConnection');
  const choices = integrationFixture('WorkSourceInputChoices');
  const read = choices.items.find((item) => item.purpose === 'task_read')!;
  const issue = issueFixture(FIXTURE_IDS.project);
  issue.content.title = 'Imported native task';
  let imported = false;
  let configuration: IssueSourceImport | null = null;
  const publications: { intent: IssueSourceImportIntent; headers: Record<string, string> }[] = [];
  const unexpected: string[] = [];
  page.on('request', (request) => {
    const path = new URL(request.url()).pathname;
    if (path.startsWith('/api/v1/') && path.includes('/factory')) unexpected.push(request.url());
  });
  await page.route('**/api/v1/integrations?*', (route) =>
    route.fulfill({ json: { items: [connection], next_cursor: null } }),
  );
  await page.route(
    (url) => url.pathname.startsWith(`/api/v1/projects/${issue.project_id}/issue-source`),
    async (route) => {
      const path = new URL(route.request().url()).pathname;
      if (path.endsWith('/issue-source-choices')) return route.fulfill({ json: choices });
      if (path.endsWith('/preview'))
        return route.fulfill({
          json: {
            source: read.binding,
            failure: null,
            has_more: false,
            observed_at_unix_ms: 1,
            retry_after_millis: null,
            items: [
              {
                display_id: 'NATIVE-7',
                title: 'preview-native-private',
                item: { resource: read.native_id, item: 'opaque-7' },
                state: 'Waiting for native review',
                labels: [],
              },
            ],
          },
        });
      if (path.endsWith('/versions')) {
        const intent: IssueSourceImportIntent = route.request().postDataJSON().intent;
        if (
          intent.definition.enabled &&
          intent.definition.connection.access_version !== choices.access_version
        )
          return route.fulfill({
            status: 412,
            json: {
              code: 'precondition_failed',
              message: 'Project access changed',
              request_id: '0199a6f4-d56c-7440-9aa2-6a320f862795',
            },
          });
        publications.push({ intent, headers: route.request().headers() });
        configuration = {
          id: intent.id,
          project_id: intent.project_id,
          definition: intent.definition,
          version: (intent.expected_current_version ?? 0) + 1,
          published_at: 1,
        };
        imported ||= intent.definition.enabled;
        return route.fulfill({ json: { disposition: 'applied', configuration } });
      }
      return route.fulfill({
        json: {
          items: configuration && !configuration.definition.retired ? [configuration] : [],
          next_cursor: null,
        },
      });
    },
  );
  const base = `/api/v1/projects/${issue.project_id}/issues`;
  await page.route(
    (url) => url.pathname === base,
    (route) => {
      expect(route.request().method()).toBe('GET');
      return route.fulfill({
        json: {
          items: imported
            ? [{ id: issue.id, display_key: issue.display_key, title: issue.content.title }]
            : [],
          next_cursor: null,
        },
      });
    },
  );
  await page.route(`**${base}/${issue.id}`, (route) => route.fulfill({ json: issue }));
  const synchronization = issueSourceReview(true);
  synchronization.link!.selection.project_id = issue.project_id;
  await page.route(`**${base}/${issue.id}/synchronization`, (route) =>
    route.fulfill({ json: synchronization }),
  );
  await page.route(`**${base}/${issue.id}/synchronization/projections*`, (route) =>
    route.fulfill({
      json: { items: [], next_cursor: null, observation: 'empty-projections', observed_at: 1 },
    }),
  );
  return { issue, connection, read, choices, publications, unexpected };
}

for (const language of ['en', 'ru'] as const) {
  test(`edits and removes task import with the keyboard at narrow width in ${language} while retaining imported tasks`, async ({
    page,
  }) => {
    const fixture = await installSources(page);
    await page.addInitScript(
      ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
      {
        key: PREFERENCES_KEY,
        preferences: { ...DEFAULT_PREFERENCES, language },
      },
    );
    const text = (key: MessageKey) => translate(language, key);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`/projects/${fixture.issue.project_id}/issues`);
    await page.getByRole('button', { name: text('issueImport.open') }).click();
    await page.getByRole('button', { name: text('issueImport.add') }).click();
    for (const [label, option] of [
      [text('factory.sourceConnection'), fixture.connection.definition.name],
      [
        text('issueSource.profile'),
        `${fixture.read.locator} · ${fixture.read.binding.configuration.identity} / ${fixture.read.binding.configuration.version}`,
      ],
    ] as const) {
      await page.getByRole('combobox', { name: label }).click();
      await page.getByRole('option', { name: option, exact: true }).click();
    }
    await page.getByRole('checkbox', { name: text('issueImport.enabled') }).check();
    await page.getByRole('button', { name: text('issueImport.review') }).click();
    await page
      .getByRole('dialog')
      .getByRole('button', { name: text('issueImport.confirm') })
      .click();
    await expect(page.getByRole('dialog')).toContainText(text('issueImport.published'));
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: text('issueImport.edit') }).focus();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('checkbox', { name: text('issueImport.enabled') })).toBeChecked();
    await page.getByRole('checkbox', { name: text('issueImport.enabled') }).focus();
    await page.keyboard.press('Space');
    await page.getByRole('button', { name: text('issueImport.review') }).click();
    await page
      .getByRole('dialog')
      .getByRole('button', { name: text('issueImport.confirm') })
      .click();
    await expect(page.getByRole('dialog')).toContainText(text('issueImport.published'));
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: text('issueImport.remove') }).focus();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('dialog')).toContainText(text('issueImport.removeHelp'));
    await page
      .getByRole('dialog')
      .getByRole('button', { name: text('issueImport.confirmRemove') })
      .press('Enter');
    await expect(page.getByRole('dialog')).toContainText(text('issueImport.removed'));
    await page.keyboard.press('Escape');
    await expect(page.getByRole('button', { name: text('issueImport.remove') })).toHaveCount(0);
    await page.getByRole('button', { name: text('issueImport.refreshTasks') }).click();
    await expect(
      page.getByRole('link', {
        name: `${fixture.issue.display_key} · ${fixture.issue.content.title}`,
      }),
    ).toBeVisible();
    expect(fixture.publications).toHaveLength(3);
    expect(fixture.publications.map((p) => p.intent.id)).toEqual(
      Array(3).fill(fixture.publications[0]!.intent.id),
    );
    expect(fixture.publications.map((p) => p.intent.expected_current_version)).toEqual([
      null,
      1,
      2,
    ]);
    expect(fixture.publications[1]!.headers['if-match']).toBe('"1"');
    expect(fixture.publications[2]!.headers['if-match']).toBe('"2"');
    expect(fixture.publications[2]!.intent.definition).toMatchObject({
      enabled: false,
      retired: true,
    });
    expect(fixture.unexpected).toEqual([]);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
  });

  test(`reviews Project task import with the keyboard at narrow width and retains canonical source feedback in ${language}`, async ({
    page,
  }) => {
    const fixture = await installSources(page);
    await page.addInitScript(
      ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
      { key: PREFERENCES_KEY, preferences: { ...DEFAULT_PREFERENCES, language } },
    );
    const text = (key: MessageKey) => translate(language, key);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`/projects/${fixture.issue.project_id}/issues`);
    const toggle = page.getByRole('button', { name: text('issueImport.open') });
    await toggle.focus();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('button', { name: text('issueImport.add') })).toBeEnabled();
    await page.getByRole('button', { name: text('issueImport.add') }).focus();
    await page.keyboard.press('Enter');
    for (const [label, option] of [
      [text('factory.sourceConnection'), fixture.connection.definition.name],
      [
        text('issueSource.profile'),
        `${fixture.read.locator} · ${fixture.read.binding.configuration.identity} / ${fixture.read.binding.configuration.version}`,
      ],
    ] as const) {
      await page.getByRole('combobox', { name: label }).focus();
      await page.keyboard.press('Enter');
      await page.getByRole('option', { name: option, exact: true }).click();
    }
    await page.getByRole('button', { name: text('issueImport.preview'), exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect(
      page.getByRole('region', { name: text('issueImport.previewTitle') }),
    ).toContainText('preview-native-private');
    expect(fixture.publications).toHaveLength(0);
    const enabled = page.getByRole('checkbox', { name: text('issueImport.enabled') });
    await enabled.focus();
    await page.keyboard.press('Space');
    const review = page.getByRole('button', { name: text('issueImport.review') });
    await review.focus();
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog', { name: text('issueImport.publish') });
    await expect(dialog).toContainText(fixture.connection.definition.name);
    await page.keyboard.press('Escape');
    await expect(review).toBeFocused();
    await page.keyboard.press('Enter');
    await dialog.getByRole('button', { name: text('issueImport.confirm') }).focus();
    await page.keyboard.press('Enter');
    await expect(dialog).toContainText(text('issueImport.published'));
    await page.keyboard.press('Escape');
    await expect(page.getByRole('button', { name: text('issueImport.add') })).toBeFocused();
    await page.getByRole('button', { name: text('issueImport.disable') }).focus();
    await page.keyboard.press('Enter');
    await dialog.getByRole('button', { name: text('issueImport.confirm') }).focus();
    await page.keyboard.press('Enter');
    await expect(dialog).toContainText(text('issueImport.published'));
    await page.keyboard.press('Escape');
    expect(fixture.publications).toHaveLength(2);
    const [first, second] = fixture.publications;
    expect(first!.intent.definition.enabled).toBe(true);
    expect(first!.headers['if-none-match']).toBe('*');
    expect(second!.intent).toEqual({
      ...first!.intent,
      expected_current_version: 1,
      definition: { ...first!.intent.definition, enabled: false },
    });
    expect(second!.headers['if-match']).toBe('"1"');
    fixture.choices.access_version += 1;
    await page.getByRole('button', { name: text('issueImport.enable') }).focus();
    await page.keyboard.press('Enter');
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: text('issueImport.confirm') }).focus();
    await page.keyboard.press('Enter');
    await expect(dialog).toContainText(text('issueImport.published'));
    await page.keyboard.press('Escape');
    expect(fixture.publications).toHaveLength(3);
    const renewed = fixture.publications[2]!;
    expect(renewed.intent).toEqual({
      ...first!.intent,
      expected_current_version: 2,
      definition: {
        ...first!.intent.definition,
        connection: {
          ...first!.intent.definition.connection,
          access_version: fixture.choices.access_version,
        },
      },
    });
    expect(renewed.headers['if-match']).toBe('"2"');
    // A stable scrollbar gutter can make scrollWidth smaller than clientWidth.
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      ),
    ).toBeLessThanOrEqual(0);
    expect(
      await page.evaluate(() =>
        JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
      ),
    ).not.toContain('preview-native-private');
    await page.getByRole('button', { name: text('issueImport.refreshTasks') }).click();
    await page.getByRole('link', { name: 'ISS-42 · Imported native task' }).click();
    await expect(
      page.getByRole('heading', { name: 'ISS-42 · Imported native task' }),
    ).toBeVisible();
    await page.getByRole('button', { name: text('issues.properties'), exact: true }).click();
    await expect(page.getByRole('region', { name: text('issueSource.title') })).toContainText(
      'opaque-7',
    );
    await expect(page.getByRole('region', { name: text('issueSync.outbound') })).toContainText(
      text('issueSync.empty'),
    );
    expect(fixture.unexpected).toEqual([]);
  });
}
