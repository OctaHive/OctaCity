import { setChecked } from './manualAuthoringControls';
import { expect, type Page } from '@playwright/test';
import { choose, selectedNode, t } from './manualAuthoringControls';

export async function handoff(page: Page, producer: string, outcome?: string, fields = ['']) {
  const input = selectedNode(page).getByRole('group', {
    name: t('factory.acceptedResults'),
    exact: true,
  });
  await choose(page, input, t('factory.resultFromStep'), producer);
  if (outcome) await choose(page, input, t('factory.transitionOutcome'), outcome);
  for (const field of fields) {
    const name = field || t('factory.entireTypedValue');
    await choose(page, input, t('factory.resultField'), name);
    await choose(page, input, t('factory.stepInputField'), name);
    const apply = input.getByRole('button', { name: t('factory.useAcceptedField'), exact: true });
    await expect(apply).toBeEnabled();
    await apply.press('Enter');
  }
}

export async function gate(
  page: Page,
  key: string,
  inputSchema: string,
  outputSchema: string,
  accepted: string,
  path: string,
  value: boolean | string,
  projection?: string,
) {
  await page.getByRole('button', { name: t('factory.addNode'), exact: true }).press('Enter');
  const node = selectedNode(page);
  await node.getByRole('textbox', { name: t('factory.nodeName'), exact: true }).fill(key);
  await choose(page, node, t('factory.nodeKind'), t('factory.kind.deterministic_gate'));
  await choose(page, node, t('factory.inputSchema'), `${inputSchema} · v1`);
  for (const outcome of [accepted, 'review']) {
    await node.getByRole('button', { name: t('factory.addOutcome'), exact: true }).press('Enter');
    await node
      .getByRole('textbox', { name: t('factory.outcomeName'), exact: true })
      .last()
      .fill(outcome);
    const selector = node
      .getByRole('combobox', { name: t('factory.outcomeSchema'), exact: true })
      .last();
    await selector.press('Enter');
    await page.getByRole('option', { name: `${outputSchema} · v1`, exact: true }).press('Enter');
  }
  const condition = node.getByRole('group', { name: t('factory.conditionCheck'), exact: true });
  await choose(page, condition, t('factory.conditionFallback'), 'review');
  await condition
    .getByRole('button', { name: t('factory.addCondition'), exact: true })
    .press('Enter');
  await choose(page, condition, t('factory.predicateOperation'), t('factory.condition.compare'));
  await condition
    .getByRole('textbox', { name: t('factory.observationPath'), exact: true })
    .fill(path);
  await choose(
    page,
    condition,
    t('factory.constantType'),
    t(typeof value === 'boolean' ? 'factory.boolean' : 'factory.string'),
  );
  if (typeof value === 'boolean')
    await setChecked(
      condition.getByRole('checkbox', { name: t('factory.constantValue'), exact: true }),
      value,
    );
  else
    await condition
      .getByRole('textbox', { name: t('factory.constantValue'), exact: true })
      .fill(value);
  await choose(page, condition, t('factory.conditionOutcome'), accepted);
  if (projection !== undefined) {
    await setChecked(
      condition.getByRole('checkbox', { name: t('factory.chooseConditionData'), exact: true }),
      true,
    );
    await condition
      .getByRole('textbox', { name: t('factory.selectionPath'), exact: true })
      .fill(projection);
  }
  const apply = condition.getByRole('button', { name: t('factory.useCondition'), exact: true });
  await expect(apply).toBeEnabled();
  await apply.press('Enter');
  await expect(condition).toContainText(t('factory.conditionConfigured'));
}

export async function route(
  page: Page,
  source: string,
  outcome: string,
  target: string,
  terminal = false,
) {
  await page
    .getByRole('button', { name: t('factory.connectGraphStep', { name: source }), exact: true })
    .press('Enter');
  const connection = page.getByRole('group', { name: t('factory.newGraphRoute'), exact: true });
  await choose(page, connection, t('factory.transitionOutcome'), outcome);
  await choose(
    page,
    connection,
    t('factory.target'),
    t(terminal ? 'factory.terminalTarget' : 'factory.nodeTarget', { name: target }),
  );
  await connection
    .getByRole('button', { name: t('factory.addGraphRoute'), exact: true })
    .press('Enter');
  await expect(connection).toHaveCount(0);
}
