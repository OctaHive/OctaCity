import { expect, test, type Page } from '@playwright/test';
import { configurationFixture, runFixture } from '../fixtures/factory';
import { issueFixture } from '../fixtures/issues';
import { integrationFixture } from '../fixtures/integrations';
import { FIXTURE_IDS, installOperatorFixture } from './operatorFixture';

test.use({ trace: 'off', screenshot: 'off', video: 'off' });
async function installSources(
  page: Page,
  options: { reporting?: boolean; progress?: boolean; labelPresence?: boolean } = {},
) {
  await installOperatorFixture(page);
  const configuration = configurationFixture().configuration;
  const diagnostics = integrationFixture('WorkSourceDiagnostics');
  if (diagnostics.configuration.kind === 'input')
    diagnostics.configuration.definition.definition.target = configuration.reference;
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
    let body: unknown = {
      items: options.progress ? [diagnostics.configuration] : [],
      next_cursor: null,
    };
    if (path.endsWith('/factory-source-choices')) {
      const choices = integrationFixture('WorkSourceInputChoices', options.reporting ? 1 : 0);
      body = {
        ...choices,
        items: choices.items.flatMap((item) => {
          const reader = {
            ...item,
            predicate_states: ['native-open', 'native-closed'],
            ...(options.labelPresence ? { predicate_label_presence: ['any', 'none'] } : {}),
          };
          return options.labelPresence && item.purpose === 'task_read'
            ? [
                reader,
                {
                  ...reader,
                  binding: {
                    ...reader.binding,
                    configuration: {
                      ...reader.binding.configuration,
                      version: 'v2',
                      digest: 'cd'.repeat(32),
                    },
                  },
                  predicate_states: ['native-closed'],
                },
              ]
            : [reader];
        }),
      };
    } else if (path.endsWith('/diagnostics')) body = diagnostics;
    else if (path.endsWith('/preview')) {
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
  return { project: configuration.reference.project_id, publications, diagnostics };
}
async function choose(page: Page, name: string, option: string) {
  await page.getByRole('combobox', { name, exact: true }).focus();
  await page.keyboard.press('Enter');
  await page.getByRole('option', { name: option, exact: true }).click();
}

test('enables read-only execution input independently of task import with separate code and literal predicates at 390px', async ({
  page,
}) => {
  const { project, publications } = await installSources(page);
  const commands: string[] = [];
  page.on('request', (request) => {
    const path = new URL(request.url()).pathname;
    if (path.startsWith('/api/v1/') && request.method() !== 'GET') commands.push(path);
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: 'Inspect Factory Configuration · v1' }).click();
  await expect(page.getByRole('button', { name: 'Admit manual Work' })).toBeEnabled();
  await page.getByRole('button', { name: 'Configure automatic intake' }).click();
  await page.getByRole('button', { name: 'Add automatic input' }).click();
  await choose(page, 'Source connection', 'Tracker');
  await choose(page, 'Task container', 'team/project');
  await choose(page, 'Selector and input mapping', 'triage-labels · v1');
  await expect(
    page.getByText(
      'This profile prepares bounded task and acceptance Artifacts. Missing acceptance blocks execution, not task import.',
    ),
  ).toBeVisible();
  await choose(page, 'Code Repository', 'Factory source · v1');
  await expect(page.getByRole('combobox', { name: 'Reporter mapping' })).toContainText(
    'No external status updates',
  );
  await expect(page.getByRole('checkbox', { name: 'Automatic intake enabled' })).not.toBeChecked();
  await page.getByRole('checkbox', { name: 'Customize filters' }).check();
  await choose(page, 'Task state', 'native-open');
  await page.getByLabel('All required labels (one per line)').fill('required, literal');
  await page.getByLabel('Any matching labels (one per line)').fill('assessment\nimplementation');
  await page.getByLabel('Excluded labels (one per line)').fill('paused');
  await page.getByRole('button', { name: 'Preview source (read-only)' }).click();
  await expect(page.getByText(/synthetic-preview-only/)).toBeVisible();
  expect(publications).toHaveLength(0);
  await page.getByRole('checkbox', { name: 'Automatic intake enabled' }).focus();
  await page.keyboard.press('Space');
  await page.getByRole('button', { name: 'Validate input', exact: true }).click();
  const review = page.getByRole('button', { name: 'Review input publication' });
  await expect(review).toBeEnabled();
  await review.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog', { name: 'Publish automatic input' });
  await expect(dialog).toContainText('Automatic intake enabled');
  await expect(dialog).toContainText('paused');
  await page.keyboard.press('Escape');
  await expect(review).toBeFocused();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Enter');
  await expect(dialog.getByText('Automatic input published.')).toBeVisible();
  expect(publications).toHaveLength(1);
  const input = integrationFixture('WorkSourceConfigurationSubmission').intent.inputs[0]!;
  expect(publications[0]!.body).toMatchObject({
    project_id: project,
    group: null,
    inputs: [
      {
        expected_version: null,
        definition: {
          enabled: true,
          source: input.definition.source,
          target: configurationFixture().configuration.reference,
          repository: {
            id: '66666666-6666-4666-8666-666666666666',
            version: 1,
            definition_digest: 'ab'.repeat(32),
          },
          reporter: null,
          connection: { reporter_configuration: null },
          predicate: {
            state: 'native-open',
            all_of: ['required, literal'],
            any_of: ['assessment', 'implementation'],
            none_of: ['paused'],
          },
        },
      },
    ],
  });
  expect(commands).toEqual([
    `/api/v1/projects/${project}/factory-sources/preview`,
    `/api/v1/projects/${project}/factory-sources/validate`,
    expect.stringMatching(/\/factory-source-inputs\/[^/]+\/versions$/u),
  ]);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
    ),
  ).toBe(true);
});

test('opens the authoritative task card, original Work and Run from retained input progress', async ({
  page,
}) => {
  const { project, diagnostics } = await installSources(page, { progress: true });
  const run = runFixture();
  const issue = issueFixture(FIXTURE_IDS.project);
  const admission = diagnostics.admissions[0]!;
  admission.issue = { project_id: issue.project_id, issue_id: issue.id };
  admission.association.run_id = run.id;
  admission.association.work_id = run.work_id;
  await page.route(`**/api/v1/factory-runs/${run.id}/work*`, (route) =>
    route.fulfill({
      json: {
        items: [
          {
            kind: 'work',
            record: {
              id: run.work_id,
              configuration: configurationFixture().configuration.reference,
              external_identity: run.external_identity,
              priority: 0,
              risk: 'low',
              subject: {
                project_id: project,
                repository_id: '66666666-6666-4666-8666-666666666666',
                base_revision: 'revision-a',
              },
            },
          },
        ],
        next_cursor: null,
      },
    }),
  );
  const path = `/factory?project_id=${project}`;
  async function openProgress() {
    await page.goto(path);
    await page.getByRole('button', { name: 'Inspect Factory Configuration · v1' }).click();
    await page.getByRole('button', { name: 'Configure automatic intake' }).click();
    await page.getByRole('button', { name: 'Source and reporting progress' }).click();
  }
  await openProgress();
  await page.getByRole('link', { name: 'Open task card' }).click();
  await expect(page).toHaveURL(`/projects/${issue.project_id}/issues/${issue.id}`);
  await expect(page.getByRole('heading', { name: 'ISS-42 · Investigate checkout' })).toBeVisible();
  await openProgress();
  await page.getByRole('link', { name: 'Work summary' }).click();
  await expect(page).toHaveURL(`/factory/${run.id}?diagnostic=work&diagnostic_id=${run.work_id}`);
  await expect(page.getByRole('heading', { name: 'Factory Run', level: 1 })).toBeVisible();
  await expect(
    page.getByRole('button', { name: `Inspect record · ${run.work_id}` }),
  ).toHaveAttribute('aria-pressed', 'true');
  await openProgress();
  await page.getByRole('link', { name: 'Open Run' }).click();
  await expect(page).toHaveURL(`/factory/${run.id}`);
  await expect(page.getByRole('heading', { name: 'Factory Run', level: 1 })).toBeVisible();
});

test('recovers a classified input failure with safe correlation and a transient selector draft', async ({
  page,
}) => {
  const { project, publications } = await installSources(page);
  const requestId = '0199a6f4-d56c-7440-9aa2-6a320f862795';
  let rejected = false;
  await page.route('**/factory-sources/validate', async (route) => {
    if (rejected) return route.fallback();
    rejected = true;
    await route.fulfill({
      status: 403,
      headers: { 'X-Request-Id': requestId },
      json: {
        code: 'forbidden',
        message: 'private-provider-token /private/runtime.json',
        request_id: requestId,
      },
    });
  });
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: 'Inspect Factory Configuration · v1' }).click();
  await page.getByRole('button', { name: 'Configure automatic intake' }).click();
  await page.getByRole('button', { name: 'Add automatic input' }).click();
  await choose(page, 'Source connection', 'Tracker');
  await choose(page, 'Task container', 'team/project');
  await choose(page, 'Selector and input mapping', 'triage-labels · v1');
  await choose(page, 'Code Repository', 'Factory source · v1');
  await page.getByRole('checkbox', { name: 'Customize filters' }).check();
  const selector = page.getByLabel('All required labels (one per line)');
  await selector.fill('transient-selector-only');
  await page.getByRole('button', { name: 'Validate input', exact: true }).click();
  const alert = page.getByRole('alert');
  await expect(alert).toContainText(
    'Check your permissions for this operation and the selected resource',
  );
  await expect(alert).toContainText(requestId);
  await expect(page.locator('body')).not.toContainText('private-provider-token');
  await expect(selector).toHaveValue('transient-selector-only');
  await alert.getByRole('button', { name: 'Retry' }).click();
  await expect(alert).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Review input publication' })).toBeEnabled();
  await expect(selector).toHaveValue('transient-selector-only');
  const storage = await page.evaluate(() =>
    JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
  );
  expect(storage).not.toContain('transient-selector-only');
  expect(storage).not.toContain('private-provider-token');
  await page.getByRole('button', { name: 'Close input editor' }).click();
  await page.getByRole('button', { name: 'Add automatic input' }).click();
  await expect(page.getByRole('combobox', { name: 'Source connection' })).toContainText(
    'Choose an option',
  );
  expect(publications).toHaveLength(0);
});

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
  await choose(page, 'Selector and input mapping', 'triage-labels · v1');
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

test('authors and reviews status effects with keyboard access and clears mappings on leaving without browser persistence', async ({
  page,
}) => {
  const { project, publications } = await installSources(page, { reporting: true });
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: 'Inspect Factory Configuration · v1' }).click();
  await page.getByRole('button', { name: 'Configure automatic intake' }).click();
  await page.getByRole('button', { name: 'Add automatic input' }).click();
  await choose(page, 'Source connection', 'Tracker');
  await choose(page, 'Task container', 'team/project');
  await choose(page, 'Selector and input mapping', 'triage-labels · v1');
  await choose(page, 'Code Repository', 'Factory source · v1');
  await choose(page, 'Reporter mapping', 'editorial-effects · v1');
  await page.getByRole('checkbox', { name: 'Customize status mapping' }).focus();
  await page.keyboard.press('Space');
  await page.getByRole('textbox', { name: 'Logical status' }).fill('synthetic-status-only');
  await choose(page, 'Resulting task state', 'native-closed');
  await page.getByRole('button', { name: 'Validate input', exact: true }).click();
  const review = page.getByRole('button', { name: 'Review input publication' });
  await expect(review).toBeEnabled();
  await review.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog', { name: 'Publish automatic input' });
  await expect(dialog).toContainText('synthetic-status-only');
  await expect(dialog).toContainText('ready / review');
  await expect(dialog).toContainText('needs context');
  await expect(dialog).toContainText('native-closed');
  await page.keyboard.press('Escape');
  await expect(review).toBeFocused();
  await page.getByRole('button', { name: 'Close input editor' }).click();
  await page.getByRole('button', { name: 'Add automatic input' }).click();
  await expect(page.getByRole('textbox', { name: 'Logical status' })).toHaveCount(0);
  const storage = await page.evaluate(() =>
    JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
  );
  expect(storage).not.toContain('synthetic-status-only');
  expect(publications).toHaveLength(0);
});

test('shows Russian input guidance and manual intake at narrow width', async ({ page }) => {
  const { project } = await installSources(page, { reporting: true });
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
  await choose(page, 'Отбор и преобразование входных данных', 'triage-labels · v1');
  await choose(page, 'Маппинг обновлений внешней системы', 'editorial-effects · v1');
  await expect(page.getByText('Остальные метки сохраняются.')).toBeVisible();
  await page.getByRole('checkbox', { name: 'Настроить маппинг статусов' }).check();
  await expect(page.getByRole('textbox', { name: 'Логический статус' })).toHaveValue(
    'needs-human-eyes',
  );
  await page.getByRole('checkbox', { name: 'Настроить фильтры' }).check();
  await expect(
    page.getByText('Ограничений по меткам нет: задачи без меток тоже подходят.'),
  ).toBeVisible();
  expect(
    await page.evaluate(() =>
      Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth),
    ),
  ).toBe(0);
});

test('opens retained source reporting with the keyboard and keeps Work/Run navigation without a task card', async ({
  page,
}) => {
  const { project } = await installSources(page, { progress: true });
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: 'Inspect Factory Configuration · v1' }).click();
  await page.getByRole('button', { name: 'Configure automatic intake' }).click();
  const progress = page.getByRole('button', { name: 'Source and reporting progress' });
  await progress.focus();
  await page.keyboard.press('Enter');
  const panel = page.getByRole('region', { name: 'Retained source progress' });
  await expect(panel.getByText('Awaiting external confirmation')).toBeVisible();
  await expect(panel.getByRole('link', { name: 'Work summary' })).toHaveAttribute(
    'href',
    /diagnostic=work/,
  );
  await expect(panel.getByRole('link', { name: 'Open Run' })).toHaveAttribute(
    'href',
    /\/factory\//,
  );
  await expect(panel.getByRole('link', { name: 'Open task card' })).toHaveCount(0);
  await expect(
    panel.getByText('Reporting health does not determine task business status or Run execution.'),
  ).toBeVisible();
});

test('edits a stage source action with the keyboard and discards its unpublished effects on leaving', async ({
  page,
}) => {
  const { project } = await installSources(page, { reporting: true });
  const flow = configurationFixture().configuration.flow;
  const root = flow.closure.definitions.find((item) => item.reference.id === flow.closure.root.id)!;
  const node = root.nodes.find((item) => item.action)!;
  node.action = {
    ...node.action!,
    action: 'set_status',
    parameters: { status: 'needs-human-eyes', outbox: { max_attempts: 2, retry_delay_millis: 5 } },
  };
  const choices = integrationFixture('WorkSourceInputChoices', 1);
  const writer = choices.items.find((item) => item.purpose === 'task_update')!;
  const source = choices.items.find((item) => item.purpose === 'task_read')!;
  writer.binding.capability = node.action.plugin;
  await page.route('**/api/v1/flow-definitions/*/versions/1', (route) =>
    route.fulfill({ json: { flow, project_id: project, published_at_unix_ms: 1 } }),
  );
  await page.route('**/api/v1/projects/*/factory-source-choices**', (route) =>
    route.fulfill({ json: choices }),
  );
  await page.route('**/api/v1/flow-definitions/validate', (route) => {
    const definition = route.request().postDataJSON().definition;
    return route.fulfill({
      json: {
        ...flow,
        closure: {
          ...flow.closure,
          definitions: [{ ...definition.root, reference: flow.closure.root }],
        },
      },
    });
  });
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: 'Inspect Flow Definition · v1' }).click();
  await page.getByRole('button', { name: 'Edit Flow draft' }).click();
  await page.getByRole('button', { name: node.key, exact: true }).click();
  await choose(page, 'Source connection', 'Tracker');
  await choose(
    page,
    'Original task source',
    `${source.locator} · ${source.binding.configuration.identity}`,
  );
  await choose(page, 'Reporter mapping', 'editorial-effects');
  await page.getByRole('checkbox', { name: 'Customize status mapping' }).focus();
  await page.keyboard.press('Space');
  await page.getByRole('textbox', { name: 'Logical status' }).fill('browser-stage-only');
  const apply = page.getByRole('button', { name: 'Use reviewed source effects' });
  await apply.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByText('browser-stage-only', { exact: true })).toBeVisible();
  await page.setViewportSize({ width: 640, height: 900 });
  await page.getByRole('button', { name: 'Validate draft' }).click();
  await page.getByRole('button', { name: 'Review publication' }).click();
  const dialog = page.getByRole('dialog', { name: 'Publish Flow Definition?' });
  await expect(dialog).toContainText('browser-stage-only');
  expect(
    await page.evaluate(() =>
      Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth),
    ),
  ).toBe(0);
  await page.keyboard.press('Escape');
  expect(
    await page.evaluate(() =>
      JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
    ),
  ).not.toContain('browser-stage-only');
  await page.getByRole('button', { name: 'Discard draft' }).click();
  await page.getByRole('button', { name: 'Edit Flow draft' }).click();
  await expect(page.getByText('browser-stage-only', { exact: true })).toHaveCount(0);
});

for (const language of ['en', 'ru'] as const) {
  test(`selects unlabeled inputs with the keyboard and reviews exclusions at 390px in ${language}`, async ({
    page,
  }) => {
    const { project, publications } = await installSources(page, { labelPresence: true });
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
    const text =
      language === 'en'
        ? {
            inspect: 'Inspect Factory Configuration · v1',
            configure: 'Configure automatic intake',
            add: 'Add automatic input',
            connection: 'Source connection',
            container: 'Task container',
            profile: 'Selector and input mapping',
            repository: 'Code Repository',
            customize: 'Customize filters',
            presence: 'Label presence',
            taskState: 'Task state',
            none: 'Only tasks without labels',
            excluded: 'Excluded labels (one per line)',
            validate: 'Validate input',
            review: 'Review input publication',
            dialog: 'Publish automatic input',
            confirm: 'Confirm publication',
          }
        : {
            inspect: 'Просмотреть конфигурацию · v1',
            configure: 'Настроить автоматический приём',
            add: 'Добавить автоматический вход',
            connection: 'Подключение источника',
            container: 'Контейнер задач',
            profile: 'Отбор и преобразование входных данных',
            repository: 'Репозиторий кода',
            customize: 'Настроить фильтры',
            presence: 'Наличие меток',
            taskState: 'Состояние задачи',
            none: 'Только задачи без меток',
            excluded: 'Исключённые метки (по одной на строку)',
            validate: 'Проверить вход',
            review: 'Просмотреть публикацию входа',
            dialog: 'Опубликовать автоматический вход',
            confirm: 'Подтвердить публикацию',
          };
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`/factory?project_id=${project}`);
    await page.getByRole('button', { name: text.inspect }).click();
    await page.getByRole('button', { name: text.configure }).click();
    await page.getByRole('button', { name: text.add, exact: true }).click();
    await choose(page, text.connection, 'Tracker');
    await choose(page, text.container, 'team/project');
    await choose(page, text.profile, 'triage-labels · v1');
    await choose(page, text.repository, 'Factory source · v1');
    await page.getByRole('checkbox', { name: text.customize }).check();
    const presence = page.getByRole('combobox', { name: text.presence });
    await presence.focus();
    await page.keyboard.press('Enter');
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await expect(presence).toBeFocused();
    await expect(presence).toContainText(text.none);
    await choose(page, text.taskState, 'native-open');
    await choose(page, text.profile, 'triage-labels · v2');
    const taskState = page.getByRole('combobox', { name: text.taskState });
    await expect(taskState).toHaveText('native-open');
    await expect(page.getByRole('alert')).toBeVisible();
    await expect(page.getByRole('button', { name: text.validate, exact: true })).toBeDisabled();
    await taskState.focus();
    await page.keyboard.press('Enter');
    await page.keyboard.press('Home');
    await page.keyboard.press('Enter');
    await expect(taskState).toBeFocused();
    await expect(page.getByRole('alert')).toHaveCount(0);

    await page.getByLabel(text.excluded).fill('on hold');
    expect(
      await page.evaluate(() =>
        Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth),
      ),
    ).toBe(0);
    await page.getByRole('button', { name: text.validate, exact: true }).click();
    await expect(page.getByRole('button', { name: text.review })).toBeEnabled();
    await page.getByRole('button', { name: text.review }).focus();
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog', { name: text.dialog });
    await expect(dialog).toContainText(text.none);
    await expect(dialog).toContainText('on hold');
    expect(
      await page.evaluate(() =>
        Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth),
      ),
    ).toBe(0);
    await dialog.getByRole('button', { name: text.confirm }).click();
    await expect.poll(() => publications.length).toBe(1);
    expect(publications[0]!.body).toMatchObject({
      inputs: [
        {
          definition: {
            source: { configuration: { version: 'v2' } },
            predicate: {
              state: null,
              label_presence: 'none',
              all_of: [],
              any_of: [],
              none_of: ['on hold'],
            },
          },
        },
      ],
    });
  });
}
