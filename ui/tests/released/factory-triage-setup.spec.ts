import { expect, type Locator, type Page } from '@playwright/test';
import { writeFile } from 'node:fs/promises';
import { test } from './sourceTest';
import { importNativeTask } from './sourceJourney';
import { translate } from '../../src/app/presentation/messages';
import type { components } from '../../.generated/api/schema';

type Schema = components['schemas'];
const language = process.env.OCTACITY_RELEASE_TRIAGE_LOCALE === 'ru' ? 'ru' : 'en';
const t = (key: Parameters<typeof translate>[1], values: Parameters<typeof translate>[2] = {}) =>
  translate(language, key, values);

async function choose(page: Page, scope: Page | Locator, label: string, option: string) {
  await scope.getByRole('combobox', { name: label, exact: true }).press('Enter');
  await page.getByRole('option', { name: option, exact: true }).press('Enter');
}

// Clean-state construction is qualified by factory-manual-authoring.spec.ts.
// This case upgrades a saved triage definition using only the common controls.
test('opens saved triage in common controls and explicitly enables its ordinary input', async ({
  page,
}) => {
  test.setTimeout(120_000);
  page.setDefaultTimeout(10_000);
  const project = process.env.OCTACITY_RELEASE_PROJECT_ID!;
  const base = `/api/v1/projects/${project}`;
  const failures: string[] = [];
  const presetCalls: string[] = [];
  page.on('request', (request) => {
    if (new URL(request.url()).pathname.includes('/factory-presets'))
      presetCalls.push(request.url());
  });
  page.on('response', (response) => {
    if (new URL(response.url()).pathname.startsWith('/api/v1/') && response.status() >= 400)
      failures.push(`${response.status()} ${new URL(response.url()).pathname}`);
  });
  const configurations: Schema['FactoryConfigurationSummaryPage'] = await (
    await page.request.get(`${base}/factory-configurations?limit=100`)
  ).json();
  expect(configurations.items).toHaveLength(1);
  const target = configurations.items[0]!;
  const path = `/api/v1/factory-configurations/${target.id}/versions/${target.version}`;
  const retained: Schema['FactoryConfigurationDetails'] = await (
    await page.request.get(path)
  ).json();
  const flow = retained.configuration.flow;
  const definition = flow.closure.definitions.find(
    (item) => item.reference.id === flow.closure.root.id,
  )!;
  expect(definition.nodes.map((node) => node.key)).toEqual(
    expect.arrayContaining(['eligibility', 'classification']),
  );
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`/factory?project_id=${project}`);
  if (language === 'ru') {
    await page
      .getByRole('button', { name: translate('en', 'operator.menu'), exact: true })
      .press('Enter');
    await choose(
      page,
      page,
      translate('en', 'language.label'),
      translate('en', 'language.russian'),
    );
    await page.keyboard.press('Escape');
  }
  await expect(
    page.getByRole('button', {
      name: language === 'en' ? 'Set up triage' : 'Настроить триаж',
      exact: true,
    }),
  ).toHaveCount(0);
  await page.goto(
    `/factory?project_id=${project}&flow_definition_id=${flow.closure.root.id}&flow_version=${flow.closure.root.version}`,
  );
  await page.getByRole('button', { name: t('factory.editFlow'), exact: true }).press('Enter');
  await expect(
    page.getByRole('region', { name: t('factory.stepGraph'), exact: true }),
  ).toBeVisible();
  await expect(page.getByRole('textbox', { name: t('factory.nodeName'), exact: true })).toHaveValue(
    'eligibility',
  );
  const validation = page.waitForResponse(
    (response) => new URL(response.url()).pathname === '/api/v1/flow-definitions/validate',
  );
  await page.getByRole('button', { name: t('factory.validateDraft'), exact: true }).press('Enter');
  const validated = await validation;
  expect(validated.status(), await validated.text()).toBe(200);
  const normalized: Schema['FactoryFlowDefinitionValidation'] = await validated.json();
  expect(normalized.closure).toEqual(flow.closure);
  await page.getByRole('button', { name: t('factory.discardDraft'), exact: true }).press('Enter');
  expect(await (await page.request.get(path)).json()).toEqual(retained);
  expect((await (await page.request.get(`${base}/factory-runs`)).json()).items).toEqual([]);
  await page.goto(
    `/factory?project_id=${project}&configuration_id=${target.id}&configuration_version=${target.version}`,
  );
  await page
    .getByRole('button', { name: t('factory.configureAutomatic'), exact: true })
    .press('Enter');
  await page.getByRole('button', { name: t('factory.addInput'), exact: true }).press('Enter');
  const automatic = page.getByRole('region', { name: t('factory.automaticInputs'), exact: true });
  await choose(page, automatic, t('factory.sourceConnection'), `Packaged tasks ${project}`);
  await choose(page, automatic, t('factory.taskContainer'), 'owner/tickets');
  await choose(page, automatic, t('factory.inputProfile'), 'github-issues · v2');
  const repositories = (await (await page.request.get(`${base}/repositories`)).json()).items;
  const repository = repositories[0];
  await choose(
    page,
    automatic,
    t('factory.codeRepository'),
    `${repository.name} · v${repository.version}`,
  );
  await automatic
    .getByRole('checkbox', { name: t('factory.customizeFilters'), exact: true })
    .check();
  await choose(page, automatic, t('factory.taskState'), 'open');
  await choose(page, automatic, t('factory.labelPresence'), t('factory.labelsNone'));
  await choose(page, automatic, t('factory.reporterMapping'), 'github-status · v1');
  const mapping = automatic.getByRole('region', {
    name: t('factory.reporterMapping'),
    exact: true,
  });
  await mapping
    .getByRole('checkbox', { name: t('factory.customizeReporting'), exact: true })
    .check();
  while (
    await mapping.getByRole('button', { name: t('factory.removeMapping'), exact: true }).count()
  )
    await mapping
      .getByRole('button', { name: t('factory.removeMapping'), exact: true })
      .first()
      .click();
  await mapping.getByRole('button', { name: t('factory.addMapping'), exact: true }).press('Enter');
  await mapping
    .getByRole('textbox', { name: t('factory.logicalStatus'), exact: true })
    .fill('requirements');
  await choose(page, mapping, t('factory.resultingState'), 'open');
  await expect(
    automatic.getByRole('checkbox', { name: t('factory.inputEnabled'), exact: true }),
  ).not.toBeChecked();
  await automatic
    .getByRole('button', { name: t('factory.previewSource'), exact: true })
    .press('Enter');
  await expect(
    automatic.getByRole('region', { name: t('factory.sourcePreview'), exact: true }),
  ).toContainText('Feature: expose bounded triage evidence');
  await automatic
    .getByRole('button', { name: t('factory.validateInput'), exact: true })
    .press('Enter');
  await expect(
    automatic.getByRole('button', { name: t('factory.reviewInput'), exact: true }),
  ).toBeEnabled();
  await automatic
    .getByRole('button', { name: t('factory.reviewInput'), exact: true })
    .press('Enter');
  await page
    .getByRole('dialog')
    .getByRole('button', { name: t('factory.confirmInput'), exact: true })
    .press('Enter');
  await expect(page.getByRole('dialog')).toContainText(t('factory.sourcePublished'));
  expect(presetCalls).toEqual([]);
  const inputs = (await (await page.request.get(`${base}/factory-sources`)).json()).items;
  expect(inputs).toHaveLength(1);
  expect(inputs[0].definition.definition.enabled).toBe(false);
  expect(inputs[0].definition.definition.predicate.label_presence).toBe('none');
  await page
    .getByRole('dialog')
    .getByRole('button', { name: t('common.close'), exact: true })
    .press('Enter');
  const issuePage = await page.context().newPage();
  const issue = await importNativeTask(issuePage, project, {
    language,
    existingIssues: 1,
    title: 'Feature: expose bounded triage evidence',
  });
  await issuePage.close();
  expect((await (await page.request.get(`${base}/factory-runs`)).json()).items).toEqual([]);
  await automatic.getByRole('checkbox', { name: t('factory.inputEnabled'), exact: true }).check();
  await automatic
    .getByRole('button', { name: t('factory.validateInput'), exact: true })
    .press('Enter');
  await expect(
    automatic.getByRole('button', { name: t('factory.reviewInput'), exact: true }),
  ).toBeEnabled();
  await automatic
    .getByRole('button', { name: t('factory.reviewInput'), exact: true })
    .press('Enter');
  const enabled = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname ===
        `${base}/factory-source-inputs/${inputs[0].definition.id}/versions` &&
      response.request().method() === 'POST',
  );
  await page
    .getByRole('dialog')
    .getByRole('button', { name: t('factory.confirmInput'), exact: true })
    .press('Enter');
  const publicationReceipt = await (await enabled).json();
  const active = publicationReceipt.publication.inputs[0].input;
  expect(active.definition.enabled).toBe(true);
  expect(active.version).toBe(2);
  expect(failures).toEqual([]);
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth),
  ).toBeLessThanOrEqual(0);
  const evidence = test.info().outputPath('triage-console-setup.json');
  await writeFile(
    evidence,
    JSON.stringify({
      configuration: active.definition.target,
      input: active,
      publication: publicationReceipt,
      issue,
      language,
    }),
  );
  await page.context().storageState({ path: test.info().outputPath('triage-console-state.json') });
  await test
    .info()
    .attach('triage-console-setup', { path: evidence, contentType: 'application/json' });
});
