import { setChecked } from './manualAuthoringControls';
import { expect, test } from '@playwright/test';
import {
  language,
  t,
  choose,
  confirm,
  schema,
  reasoning,
  documentInputs,
  selectedNode,
} from './manualAuthoringControls';

import { budget, runBudget, limits, processInput } from './manualPolicyControls';
import { gate, handoff, route } from './manualGraphControls';

test('authors the first document-guided pipeline through the packaged Console', async ({
  page,
}) => {
  test.setTimeout(180_000);
  page.setDefaultTimeout(10_000);
  const project = process.env.OCTACITY_RELEASE_PROJECT_ID!;
  const base = `/api/v1/projects/${project}`;
  await page.addInitScript((language) => {
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
    );
  }, language);
  await page.setViewportSize({ width: 390, height: 844 });
  for (const collection of [
    'flow-definitions',
    'factory-configurations',
    'builds',
    'factory-runs',
    'factory-roles',
    'input-documents',
  ]) {
    const response = await page.request.get(`${base}/${collection}?limit=100`);
    expect(response.ok(), collection).toBe(true);
    expect((await response.json()).items).toEqual([]);
  }
  await page.goto(`/projects/${project}`);
  await page.getByRole('button', { name: t('projectDocument.manage'), exact: true }).press('Enter');
  const documents = page.getByRole('region', { name: t('projectDocument.manage'), exact: true });
  for (const [name, content] of [
    ['Ticket', 'ticket-request-canary: add copying of an Issue identifier from its card'],
    [
      'Project goals',
      'project-goals-canary: improve the operator console for software work coordination',
    ],
    ['Components', 'component-guide-canary: console means operator-facing interface'],
    ['Priorities', 'priority-guide-canary: normal means a useful non-urgent improvement'],
    ['Scratch guidance', 'Unused guidance must not reach either agent.'],
  ]) {
    await documents
      .getByRole('button', { name: t('projectDocument.new'), exact: true })
      .press('Enter');
    await documents
      .getByRole('textbox', { name: t('projectDocument.name'), exact: true })
      .fill(name!);
    await documents
      .getByRole('textbox', { name: t('projectDocument.content'), exact: true })
      .fill(content!);
    await documents
      .getByRole('button', { name: t('projectDocument.publish'), exact: true })
      .press('Enter');
    const dialog = page.getByRole('dialog');
    await expect(dialog).toContainText(content!);
    await dialog
      .getByRole('button', { name: t('projectDocument.publish'), exact: true })
      .press('Enter');
    await expect(dialog).toContainText(t('projectDocument.published'));
    await dialog.getByRole('button', { name: t('common.close'), exact: true }).press('Enter');
    await documents
      .getByRole('button', { name: t('projectDocument.back'), exact: true })
      .press('Enter');
  }
  const catalog = await (await page.request.get(`${base}/input-documents?limit=100`)).json();
  expect(catalog.items).toHaveLength(5);
  for (const document of catalog.items) {
    const read = await page.request.get(`${base}/input-documents/${document.artifact.artifact_id}`);
    expect(read.headers()['cache-control']).toContain('no-store');
    expect((await read.json()).content).toBeTruthy();
  }
  await page.goto(`/ai-settings?project_id=${project}&view=agents`);
  for (const [agent, profile, runtime] of [
    ['manual-first-agent', 'Assessment worker', 'manual-runtime'],
    ['manual-second-agent', 'Classification worker', 'manual-code-runtime'],
  ]) {
    await page.getByRole('button', { name: t('factory.createAgent'), exact: true }).press('Enter');
    const editor = page.getByRole('region', { name: t('factory.agentEditor'), exact: true });
    await editor.getByRole('textbox', { name: t('factory.agentName'), exact: true }).fill(agent!);
    await editor
      .getByRole('textbox', { name: t('factory.sharedInstruction') + ' 1', exact: true })
      .fill('Work only on the assigned task.');
    await confirm(page, 'factory.reviewAgent');
    await page.getByRole('button', { name: agent!, exact: true }).press('Enter');
    await page
      .getByRole('button', { name: t('factory.createProfile'), exact: true })
      .press('Enter');
    const settings = page.getByRole('region', { name: t('factory.profileEditor'), exact: true });
    await choose(page, settings, t('factory.profileRuntime'), `${runtime} · codex · finite-model`);
    await settings
      .getByRole('textbox', { name: t('factory.profileName'), exact: true })
      .fill(profile!);
    await confirm(page, 'factory.reviewProfile');
  }
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: t('factory.addPipeline'), exact: true }).press('Enter');
  await expect(
    page.getByRole('heading', { name: t('factory.graphEditor'), exact: true }),
  ).toBeVisible();
  const inventory = page.getByRole('group', { name: t('factory.availableCode'), exact: true });
  await choose(page, inventory, t('factory.projectRepository'), 'Manual code guide · v1');
  await choose(page, inventory, t('factory.codeSelection'), t('manualBuild.exactRevision'));
  await inventory
    .getByRole('textbox', { name: t('manualBuild.exactRevision'), exact: true })
    .fill(process.env.OCTACITY_RELEASE_CODE_REVISION!);
  await inventory
    .getByRole('button', { name: t('factory.addCodeSource'), exact: true })
    .press('Enter');
  await schema(page, 'manual.input', 'boolean');
  await schema(page, 'manual.assessment', {
    eligible: 'boolean',
    summary: 'string',
    duplicate_search: 'string',
  });
  await schema(page, 'manual.summary', 'string');
  await schema(page, 'manual.classification', { component: 'string', priority: 'string' });
  await reasoning(
    page,
    'inspect',
    'assessed',
    'manual.assessment',
    'manual-first-agent',
    'Assessment worker',
    'Assess Project fit. Retain duplicate uncertainty without inventing search evidence.',
  );
  await documentInputs(page, ['Ticket', 'Project goals'], 'Project goals');
  await reasoning(
    page,
    'route',
    'classified',
    'manual.classification',
    'manual-second-agent',
    'Classification worker',
    'Classify only the accepted eligible ticket using the selected component and priority guidance. Read the selected code guide.',
    'manual-code-runtime',
  );
  await documentInputs(page, ['Ticket', 'Components', 'Priorities']);
  const stepCode = selectedNode(page).getByRole('group', {
    name: t('factory.stepCode'),
    exact: true,
  });
  await choose(page, stepCode, t('factory.stepCodeSelection'), 'Manual code guide · v1');
  await choose(page, stepCode, t('factory.codeScopeMode'), t('factory.codeSelectedPaths'));
  await stepCode
    .getByRole('textbox', { name: t('factory.codePath'), exact: true })
    .fill('README.md');

  const docs = selectedNode(page).getByRole('group', { name: t('factory.documents'), exact: true });
  await expect(
    docs.getByRole('checkbox', { name: 'Project goals', exact: true }),
  ).not.toBeChecked();
  await choose(page, page, t('factory.graphNode'), 'inspect');
  await expect(docs.getByRole('checkbox', { name: 'Project goals', exact: true })).toBeChecked();
  await expect(docs.getByRole('checkbox', { name: 'Components', exact: true })).not.toBeChecked();
  await gate(
    page,
    'eligibility-check',
    'manual.assessment',
    'manual.summary',
    'eligible',
    '/eligible',
    true,
    '/summary',
  );
  await handoff(page, 'inspect', undefined, ['eligible', 'summary', 'duplicate_search']);
  await choose(page, page, t('factory.graphNode'), 'route');
  await selectedNode(page).getByText(t('factory.advancedMappings'), { exact: true }).press('Enter');
  await choose(page, selectedNode(page), t('factory.inputSchema'), 'manual.summary · v1');
  await handoff(page, 'eligibility-check', 'eligible');
  await gate(
    page,
    'route-check',
    'manual.classification',
    'manual.classification',
    'ready',
    '/priority',
    'normal',
    '',
  );
  await handoff(page, 'route', undefined, ['component', 'priority']);
  const capability = await (
    await page.request.get(`${base}/factory-profile-capabilities/manual-runtime`)
  ).json();
  await limits(page, capability.definition.permissions);
  await choose(page, page, t('factory.graphNode'), 'inspect');
  await selectedNode(page).getByText(t('factory.advancedMappings'), { exact: true }).press('Enter');
  await choose(page, selectedNode(page), t('factory.inputSchema'), 'manual.input · v1');
  await selectedNode(page).getByText(t('factory.advancedMappings'), { exact: true }).press('Enter');
  await choose(page, selectedNode(page), t('factory.mappingKind'), t('factory.constant'));
  await selectedNode(page)
    .getByRole('checkbox', { name: t('factory.constantValue'), exact: true })
    .press('Space');
  await processInput(
    page,
    'goals',
    'pipeline-goal-canary: retain a bounded document-guided assessment',
  );
  await processInput(page, 'criteria', 'process-criteria-canary: preliminary draft');
  await processInput(
    page,
    'criteria',
    'process-criteria-canary: retain the assessment and classification, or an explicit review outcome',
  );
  for (const name of ['inspect', 'route']) {
    await setChecked(
      page.getByRole('checkbox', { name: t('factory.criteriaRecipient', { name }), exact: true }),
      true,
    );
  }
  await setChecked(
    page.getByRole('checkbox', {
      name: t('factory.goalsRecipient', { name: 'inspect' }),
      exact: true,
    }),
    true,
  );
  for (const [from, outcome, target, terminal] of [
    ['inspect', 'assessed', 'eligibility-check', false],
    ['eligibility-check', 'eligible', 'route', false],
    ['eligibility-check', 'review', 'assessment-review', true],
    ['route', 'classified', 'route-check', false],
    ['route-check', 'ready', 'finished', true],
    ['route-check', 'review', 'routing-review', true],
  ] as const)
    await route(page, from, outcome, target, terminal);
  expect((await (await page.request.get(`${base}/builds?limit=100`)).json()).items).toEqual([]);
  const validate = page.getByRole('button', { name: t('factory.validateDraft'), exact: true });
  await choose(page, page, t('factory.graphNode'), 'inspect');
  const workTask = selectedNode(page).getByRole('checkbox', {
    name: t('factory.workTask'),
    exact: true,
  });
  await setChecked(workTask, false);
  const ticketInput = selectedNode(page).getByRole('listitem', { name: 'Ticket', exact: true });
  await ticketInput.getByText(t('factory.documentInputDetails'), { exact: true }).press('Enter');
  await choose(page, ticketInput, t('factory.documentProfileInput', { name: 'Ticket' }), 'task');
  await expect(validate).toBeEnabled();
  await validate.press('Enter');
  await expect(
    page.getByRole('alert').getByRole('button', { name: t('factory.workTask'), exact: true }),
  ).toBeVisible();
  expect(
    (await (await page.request.get(`${base}/flow-definitions?limit=100`)).json()).items,
  ).toEqual([]);
  await ticketInput.getByText(t('factory.documentInputDetails'), { exact: true }).press('Enter');
  await choose(
    page,
    ticketInput,
    t('factory.documentProfileInput', { name: 'Ticket' }),
    t('factory.additionalDocument'),
  );
  await setChecked(workTask, true);
  await validate.press('Enter');
  const review = page.getByRole('button', { name: t('factory.reviewPublication'), exact: true });
  await expect(review).toBeEnabled();
  await confirm(page, 'factory.reviewPublication');
  const flows = await (await page.request.get(`${base}/flow-definitions?limit=100`)).json();
  expect(flows.items).toHaveLength(1);
  const definition = flows.items[0].flow.closure.definitions[0];
  expect(definition.nodes.map((node: { key: string }) => node.key).sort()).toEqual([
    'eligibility-check',
    'inspect',
    'route',
    'route-check',
  ]);
  expect(definition.transitions).toHaveLength(6);
  expect(definition.code_sources.code_1.repository.id).toBe(
    process.env.OCTACITY_RELEASE_CODE_REPOSITORY,
  );
  expect(definition.code_sources.code_1.selection).toEqual({
    kind: 'exact_revision',
    value: process.env.OCTACITY_RELEASE_CODE_REVISION,
  });
  const inspect = definition.nodes.find((node: { key: string }) => node.key === 'inspect');
  const classifier = definition.nodes.find((node: { key: string }) => node.key === 'route');
  expect(inspect.code).toEqual({ kind: 'none' });
  expect(classifier.code).toMatchObject({
    kind: 'source',
    source: 'code_1',
    access: 'read_only',
    scope: { kind: 'selected', entries: [{ kind: 'path', path: 'README.md' }] },
  });
  expect(inspect.context_binding.work_task).toBe('task');
  expect(classifier.context_binding.work_task).toBe('task');

  expect(definition.process.criteria.recipients).toEqual(['inspect', 'route']);
  expect(definition.process.goals.recipients).toEqual(['inspect']);
  await page.getByRole('button', { name: t('factory.discardDraft'), exact: true }).press('Enter');
  await page
    .getByRole('button', { name: t('factory.createConfiguration'), exact: true })
    .press('Enter');
  const configuration = page.getByRole('group', {
    name: t('factory.configurationDraft'),
    exact: true,
  });
  await setChecked(
    configuration.getByRole('checkbox', { name: t('factory.enabled'), exact: true }),
    true,
  );
  await choose(page, configuration, t('factory.admissionPolicy'), 'manual-admission · v1');
  await choose(page, configuration, t('factory.permissionCeiling'), 'manual-ceiling · v1');
  await budget(configuration, t('factory.hardBudget'), runBudget);
  await expect(validate).toBeEnabled();
  await validate.press('Enter');
  await expect(review).toBeEnabled();
  await confirm(page, 'factory.reviewPublication');
  await page.getByRole('button', { name: t('factory.discardDraft'), exact: true }).press('Enter');
  await page.getByRole('button', { name: t('factory.admitWork'), exact: true }).press('Enter');
  await page
    .getByRole('textbox', { name: t('factory.workLabel'), exact: true })
    .fill('manual/ticket-1');
  await choose(page, page, t('factory.workSubject'), t('factory.projectInputSubject'));
  await choose(page, page, t('factory.inputOrigin'), t('factory.projectDocumentsOrigin'));
  await choose(page, page, t('factory.taskDocument'), 'Ticket');
  await page
    .getByRole('combobox', { name: t('factory.criteriaDocument'), exact: true })
    .press('Enter');
  const publications = page
    .getByRole('option')
    .filter({ hasText: t('factory.pipelineCriteriaName') });
  const publicationNames = await publications.allTextContents();
  expect(publicationNames).toHaveLength(2);
  expect(
    new Set(publicationNames).size,
    'exact criteria publications must be distinguishable',
  ).toBe(2);
  await page.keyboard.press('Escape');
  const criteria = page.getByRole('group', { name: t('factory.criteriaDocument'), exact: true });
  const revised =
    'process-criteria-canary: retain the assessment and classification, or an explicit review outcome';
  await choose(page, criteria, t('factory.criteriaDocument'), publicationNames[0]!);
  await criteria
    .getByRole('button', { name: t('factory.previewDocument'), exact: true })
    .press('Enter');
  await expect(criteria.locator('pre')).toBeVisible();
  if ((await criteria.locator('pre').textContent()) !== revised) {
    await choose(page, criteria, t('factory.criteriaDocument'), publicationNames[1]!);
    await criteria
      .getByRole('button', { name: t('factory.previewDocument'), exact: true })
      .press('Enter');
  }
  await expect(criteria.locator('pre')).toHaveText(revised);
  await page.getByRole('spinbutton', { name: t('factory.workPriority'), exact: true }).fill('40');
  const admission = page.getByRole('button', { name: t('factory.reviewWork'), exact: true });
  await expect(admission).toBeEnabled();
  await admission.press('Enter');
  const admitted = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      new URL(response.url()).pathname === `${base}/factory-work`,
  );
  await page
    .getByRole('dialog')
    .getByRole('button', { name: t('factory.confirmWork'), exact: true })
    .press('Enter');
  const response = await admitted;
  expect(response.ok()).toBe(true);
  const accepted = await response.json();
  expect(accepted.factory_run_id).toBeTruthy();
  const runs = await (await page.request.get(`${base}/factory-runs?limit=100`)).json();
  expect(runs.items).toHaveLength(1);
  expect(runs.items[0].id).toBe(accepted.factory_run_id);
  await expect(page).toHaveURL(new RegExp(`/factory/${accepted.factory_run_id}$`));
  await expect(
    page.getByRole('heading', { name: t('factory.runCommands'), exact: true }),
  ).toBeVisible();
  const wide = await page.evaluate(() =>
    [...document.querySelectorAll('main *')]
      .filter((element) => element.getBoundingClientRect().right > innerWidth + 1)
      .slice(-12)
      .map((element) => ({
        tag: element.tagName,
        class: element.className,
        width: Math.round(element.getBoundingClientRect().width),
      })),
  );
  await expect
    .poll(() => page.evaluate(() => document.documentElement.scrollWidth), {
      message: JSON.stringify(wide),
    })
    .toBeLessThanOrEqual(390);
  const persisted = await page.evaluate(() =>
    JSON.stringify({ ...localStorage, ...sessionStorage }),
  );
  expect(persisted).not.toContain('ticket-request-canary');
});
