import { setChecked } from './manualAuthoringControls';
import { expect, type Locator, type Page } from '@playwright/test';
import type { components } from '../../.generated/api/schema';
import { choose, selectedNode, t } from './manualAuthoringControls';

export const runBudget = {
  max_attempts: 8,
  max_elapsed_millis: 600000,
  max_tokens: 4000,
  max_cost_micro_units: 4000,
  max_output_bytes: 67108864,
};
export async function budget(
  scope: Locator,
  label: string,
  value: components['schemas']['FactoryBudget'],
) {
  for (const [field, message] of [
    ['max_attempts', 'factory.maxAttempts'],
    ['max_elapsed_millis', 'factory.maxElapsed'],
    ['max_tokens', 'factory.maxTokens'],
    ['max_cost_micro_units', 'factory.maxCost'],
    ['max_output_bytes', 'factory.maxOutput'],
  ] as const)
    await scope
      .getByRole('spinbutton', { name: `${label} · ${t(message)}`, exact: true })
      .fill(String(value[field]));
}
async function grants(scope: Locator, value: components['schemas']['FactoryPermissions']) {
  for (const input of await scope.getByRole('checkbox').all()) {
    await expect(input).toBeEnabled();
    if (!(await input.isChecked())) await input.press('Space');
  }
  await scope
    .getByRole('spinbutton', { name: t('factory.descendants'), exact: true })
    .fill(String(value.max_descendants));
  for (const [key, number] of Object.entries(value.resources))
    await scope
      .getByRole('spinbutton', { name: `${t('factory.resource')} · ${key}`, exact: true })
      .fill(String(number));
  for (const [key, number] of Object.entries(value.outputs))
    if (typeof number === 'number')
      await scope
        .getByRole('spinbutton', { name: `${t('factory.outputLimit')} · ${key}`, exact: true })
        .fill(String(number));
}
export async function limits(page: Page, permissions: components['schemas']['FactoryPermissions']) {
  await page.getByRole('button', { name: t('factory.flowSettings'), exact: true }).press('Enter');
  const settings = page.getByRole('group', { name: t('factory.flowSettings'), exact: true });
  await choose(page, page, t('factory.permissionPreset'), 'manual-ceiling · v1');
  await choose(page, settings, t('factory.entry'), 'inspect');
  await choose(page, settings, t('factory.flowInputSchema'), 'manual.input · v1');
  await settings.getByText(t('factory.admissionLimitsDetails'), { exact: true }).press('Enter');
  await settings
    .getByRole('spinbutton', {
      name: `${t('factory.admission')} · ${t('factory.expandedNodes')}`,
      exact: true,
    })
    .fill('8');
  await budget(settings, t('factory.admissionBudget'), runBudget);
  await budget(settings, t('factory.executionBudget'), runBudget);
  await settings.getByText(t('factory.admissionAccessDetails'), { exact: true }).press('Enter');
  await grants(
    settings.getByRole('group', { name: t('factory.admissionPermissions'), exact: true }),
    permissions,
  );
  await grants(
    settings.getByRole('group', { name: t('factory.executionPermissions'), exact: true }),
    permissions,
  );
  for (const [name, schema] of [
    ['finished', 'manual.classification'],
    ['assessment-review', 'manual.summary'],
    ['routing-review', 'manual.classification'],
  ]) {
    await settings
      .getByRole('button', { name: t('factory.addTerminal'), exact: true })
      .press('Enter');
    await settings
      .getByRole('textbox', { name: t('factory.terminalName'), exact: true })
      .last()
      .fill(name!);
    await settings
      .getByRole('combobox', { name: t('factory.terminalSchema'), exact: true })
      .last()
      .press('Enter');
    await page.getByRole('option', { name: `${schema} · v1`, exact: true }).press('Enter');
  }
  await page.getByRole('button', { name: t('factory.flowSettings'), exact: true }).press('Enter');
  for (const key of ['inspect', 'route', 'eligibility-check', 'route-check']) {
    await choose(page, page, t('factory.graphNode'), key);
    const node = selectedNode(page);
    await budget(node, t('factory.nodeBudget'), {
      max_attempts: 1,
      max_elapsed_millis: 120000,
      max_tokens: 1000,
      max_cost_micro_units: 1000,
      max_output_bytes: 16777216,
    });
    if (['inspect', 'route'].includes(key))
      await grants(
        node.getByRole('group', { name: t('factory.nodePermissions'), exact: true }),
        permissions,
      );
    if (key === 'inspect')
      await setChecked(
        node
          .getByRole('group', { name: t('factory.nodePermissions'), exact: true })
          .getByRole('checkbox', {
            name: `${t('factory.mount')} · /workspace/source · read_only`,
            exact: true,
          }),
        false,
      );
  }
}
export async function processInput(page: Page, kind: 'goals' | 'criteria', content: string) {
  await page
    .getByRole('button', {
      name: t(kind === 'goals' ? 'factory.editPipelineGoals' : 'factory.editCompletionCriteria'),
      exact: true,
    })
    .press('Enter');
  await page
    .getByRole('textbox', {
      name: t(kind === 'goals' ? 'factory.pipelineGoals' : 'factory.completionCriteria'),
      exact: true,
    })
    .fill(content);
  await page
    .getByRole('button', {
      name: t(
        kind === 'goals' ? 'factory.reviewGoalsPublication' : 'factory.reviewCriteriaPublication',
      ),
      exact: true,
    })
    .press('Enter');
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText(content);
  await dialog
    .getByRole('button', {
      name: t(kind === 'goals' ? 'factory.saveGoalsInput' : 'factory.saveCriteriaInput'),
      exact: true,
    })
    .press('Enter');
  await expect(dialog).toContainText(t('factory.processInputSaved'));
  await dialog.getByRole('button', { name: t('common.close'), exact: true }).press('Enter');
}
