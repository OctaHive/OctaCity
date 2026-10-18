import { expect, test, type Page } from '@playwright/test';
import { configurationFixture } from '../fixtures/factory';
import { integrationFixture } from '../fixtures/integrations';
import { installOperatorFixture } from './operatorFixture';

test.use({ trace: 'off', screenshot: 'off', video: 'off' });
async function installSources(page: Page) {
  await installOperatorFixture(page);
  const configuration = configurationFixture().configuration;
  const publications: { body: unknown; headers: Record<string, string> }[] = [];
  await page.route('**/api/v1/projects/*/factory-source**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    const intent =
      route.request().method() === 'POST' ? route.request().postDataJSON().intent : null;
    const publication = intent
      ? {
          project_id: intent.project_id,
          inputs: intent.inputs.map(
            (input: { id: string; expected_version?: number; definition: unknown }) => ({
              expected_version: input.expected_version ?? null,
              input: {
                id: input.id,
                version: 1,
                project_id: intent.project_id,
                root_flow: configuration.flow.closure.root,
                definition: input.definition,
              },
            }),
          ),
          group: null,
        }
      : null;
    let body: unknown = { items: [], next_cursor: null };
    if (path.endsWith('/factory-source-choices')) {
      const choices = integrationFixture('WorkSourceInputChoices');
      body = {
        ...choices,
        items: choices.items.map((item) => ({
          ...item,
          predicate_states: ['native-open', 'native-closed'],
        })),
      };
    } else if (path.endsWith('/preview')) {
      const preview = integrationFixture('WorkSourcePreview');
      preview.candidates[0]!.summary.title = 'synthetic-preview-only';
      body = { ...preview, configuration: intent };
    } else if (path.endsWith('/validate')) body = { publication };
    else if (path.endsWith('/versions')) {
      publications.push({ body: intent, headers: route.request().headers() });
      body = { disposition: 'applied', publication, published_at_unix_ms: 1 };
    }
    await route.fulfill({ json: body });
  });
  await page.route('**/api/v1/repositories/*/versions/1', (route) =>
    route.fulfill({
      json: {
        id: '66666666-6666-4666-8666-666666666666',
        project_id: configuration.reference.project_id,
        name: 'Factory source',
        version: 1,
        definition_digest: 'ab'.repeat(32),
        published_at_unix_ms: 1,
        definition: {
          vcs_integration_id: '55555555-5555-4555-8555-555555555555',
          repository_locator: 'https://another-host.invalid/code',
          selection: {
            allowed_references: ['main'],
            default_reference: 'main',
            allow_exact_revision: false,
          },
        },
      },
    }),
  );
  return { project: configuration.reference.project_id, publications };
}
async function choose(page: Page, name: string, option: string) {
  await page.getByRole('combobox', { name, exact: true }).focus();
  await page.keyboard.press('Enter');
  await page.getByRole('option', { name: option, exact: true }).click();
}
test('configures a pipeline input with keyboard review and keeps drafts and preview out of browser storage', async ({
  page,
}) => {
  const { project, publications } = await installSources(page);
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: 'Inspect Factory Configuration · v1' }).click();
  await page.getByRole('button', { name: 'Configure automatic intake' }).click();
  await page.getByRole('button', { name: 'Add automatic input' }).click();
  await choose(page, 'Source connection', 'Tracker');
  await choose(page, 'Task container', 'team/project');
  await choose(page, 'Selector and input mapping', 'triage-labels');
  await choose(page, 'Code Repository', 'Factory source · v1');
  await page.getByRole('checkbox', { name: 'Customize filters' }).check();
  await expect(page.getByRole('combobox', { name: 'Task state' })).toContainText(
    'Any state (no extra restriction)',
  );
  await expect(
    page.getByText('No label restrictions: tasks without labels are eligible too.'),
  ).toBeVisible();
  await page
    .getByLabel('All required labels (one per line)')
    .fill('synthetic-input-only\nwith, comma');
  await page.getByRole('button', { name: 'Preview source (read-only)' }).click();
  await expect(page.getByText(/synthetic-preview-only/)).toBeVisible();
  expect(publications).toHaveLength(0);
  await page.getByRole('button', { name: 'Validate input', exact: true }).click();
  const review = page.getByRole('button', { name: 'Review input publication' });
  await expect(review).toBeEnabled();
  await review.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog', { name: 'Publish automatic input' });
  await expect(dialog).toContainText('Source connection: Tracker');
  await expect(dialog).toContainText('with, comma');
  await page.keyboard.press('Escape');
  await expect(review).toBeFocused();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Enter');
  await expect(dialog.getByText('Automatic input published.')).toBeVisible();
  expect(publications).toHaveLength(1);
  expect(publications[0]!.headers['if-none-match']).toBe('*');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Close input editor' }).click();
  await page.getByRole('button', { name: 'Add automatic input' }).click();
  await expect(page.getByRole('combobox', { name: 'Source connection' })).toContainText(
    'Choose an option',
  );
  await expect(page.getByText(/synthetic-preview-only/)).toHaveCount(0);
  const storage = await page.evaluate(() =>
    JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
  );
  expect(storage).not.toContain('synthetic-input-only');
  expect(storage).not.toContain('synthetic-preview-only');
});

test('shows Russian input guidance and manual intake at narrow width', async ({ page }) => {
  const { project } = await installSources(page);
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
  await page.setViewportSize({ width: 640, height: 900 });
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: 'Просмотреть конфигурацию · v1' }).click();
  await expect(page.getByRole('button', { name: 'Добавить Work вручную' })).toBeEnabled();
  await page.getByRole('button', { name: 'Настроить автоматический приём' }).click();
  await page.getByRole('button', { name: 'Добавить автоматический вход' }).click();
  await choose(page, 'Подключение источника', 'Tracker');
  await choose(page, 'Контейнер задач', 'team/project');
  await choose(page, 'Отбор и преобразование входных данных', 'triage-labels');
  await page.getByRole('checkbox', { name: 'Настроить фильтры' }).check();
  await expect(
    page.getByText('Ограничений по меткам нет: задачи без меток тоже подходят.'),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    ),
  ).toBe(0);
});
