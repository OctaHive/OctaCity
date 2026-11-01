import { expect, type Locator, type Page } from '@playwright/test';
import { translate, type MessageKey } from '../../src/app/presentation/messages';

export const language = process.env.OCTACITY_RELEASE_AUTHORING_LOCALE === 'ru' ? 'ru' : 'en';
export const t = (key: MessageKey, values: Record<string, string | number> = {}) =>
  translate(language, key, values);

/** Checkbox interaction stays on the keyboard in both narrow-layout journeys. */
export async function setChecked(input: Locator, checked = true) {
  await expect(input).toBeEnabled();
  if ((await input.isChecked()) !== checked) await input.press('Space');
  await expect(input).toBeChecked({ checked });
}

export async function choose(page: Page, scope: Page | Locator, label: string, name: string) {
  await scope.getByRole('combobox', { name: label, exact: true }).press('Enter');
  await page.getByRole('option', { name, exact: true }).press('Enter');
}

export async function confirm(page: Page, review: MessageKey) {
  await page.getByRole('button', { name: t(review), exact: true }).press('Enter');
  const dialog = page.getByRole('dialog');
  await dialog
    .getByRole('button', { name: t('factory.confirmPublish'), exact: true })
    .press('Enter');
  await expect(dialog).toContainText(t('factory.published'));
  await dialog.getByRole('button', { name: t('common.close'), exact: true }).press('Enter');
}

export async function schema(
  page: Page,
  name: string,
  shape: 'boolean' | 'string' | Record<string, 'boolean' | 'string'>,
) {
  const editor = page.getByRole('group', { name: t('factory.schemas'), exact: true });
  await editor.getByRole('textbox', { name: t('factory.schemaName'), exact: true }).fill(name);
  await editor.getByRole('textbox', { name: t('factory.schemaVersion'), exact: true }).fill('v1');
  const kind = typeof shape === 'string' ? shape : 'object';
  await editor
    .getByRole('combobox', { name: t('factory.schemaType'), exact: true })
    .first()
    .press('Enter');
  await page.getByRole('option', { name: t(`factory.${kind}`), exact: true }).press('Enter');
  if (kind === 'string') await stringBounds(editor);
  if (typeof shape !== 'string')
    for (const [field, type] of Object.entries(shape)) {
      await editor.getByRole('button', { name: t('factory.addField'), exact: true }).press('Enter');
      const input = editor
        .getByRole('textbox', { name: t('factory.fieldName'), exact: true })
        .last();
      await input.fill(field);
      const group = input.locator('xpath=ancestor::fieldset[1]');
      await choose(page, group, t('factory.schemaType'), t(`factory.${type}`));
      if (type === 'string') await stringBounds(group);
    }
  await editor.getByRole('button', { name: t('factory.addSchema'), exact: true }).press('Enter');
  await expect(
    editor.getByRole('textbox', { name: t('factory.schemaName'), exact: true }),
  ).toHaveValue('');
}

async function stringBounds(group: Locator) {
  await group.getByRole('spinbutton', { name: t('factory.minimum'), exact: true }).fill('1');
  await group.getByRole('spinbutton', { name: t('factory.maximum'), exact: true }).fill('256');
}

export const selectedNode = (page: Page) =>
  page.getByRole('group', { name: t('factory.node'), exact: true });

export async function reasoning(
  page: Page,
  key: string,
  outcome: string,
  result: string,
  agent: string,
  profile: string,
  task: string,
  runtime = 'manual-runtime',
) {
  await page.getByRole('button', { name: t('factory.addNode'), exact: true }).press('Enter');
  const node = selectedNode(page);
  await node.getByRole('textbox', { name: t('factory.nodeName'), exact: true }).fill(key);
  await choose(page, node, t('factory.nodeKind'), t('factory.kind.reasoning'));
  await node.getByRole('button', { name: t('factory.addOutcome'), exact: true }).press('Enter');
  await node.getByRole('textbox', { name: t('factory.outcomeName'), exact: true }).fill(outcome);
  await choose(page, node, t('factory.outcomeSchema'), `${result} · v1`);
  await choose(page, node, t('factory.executorRole'), `${agent} · v1`);
  await choose(page, node, t('factory.executorProfile'), `${profile} · v1`);
  await choose(page, node, t('factory.executorRuntime'), `codex-cli · ${runtime}`);
  const apply = node.getByRole('button', { name: t('factory.useExecutorProfile'), exact: true });
  await expect(apply).toBeEnabled();
  await apply.press('Enter');
  await expect(node).toContainText(
    t('factory.selectedExecutorProfile', { name: profile, version: 1 }),
  );
  await node
    .getByRole('button', {
      name: t('factory.addInstruction', {
        label: t('factory.taskInstructions').toLocaleLowerCase(),
      }),
      exact: true,
    })
    .press('Enter');
  await node
    .getByRole('textbox', { name: t('factory.taskInstructions') + ' 1', exact: true })
    .fill(task);
  await setChecked(node.getByRole('checkbox', { name: t('factory.workTask'), exact: true }), true);
}

export async function documentInputs(page: Page, names: string[], instructions?: string) {
  const docs = selectedNode(page).getByRole('group', { name: t('factory.documents'), exact: true });
  for (const name of names)
    await setChecked(docs.getByRole('checkbox', { name, exact: true }), true);
  const ticket = docs.getByRole('listitem', { name: 'Ticket', exact: true });
  await ticket
    .getByRole('button', { name: t('factory.previewDocument'), exact: true })
    .press('Enter');
  await expect(ticket).toContainText('ticket-request-canary');
  if (instructions)
    await choose(
      page,
      docs,
      t('factory.documentRole', { name: instructions }),
      t('factory.documentInstructions'),
    );
  await setChecked(docs.getByRole('checkbox', { name: 'Scratch guidance', exact: true }), true);
  await docs
    .getByRole('button', {
      name: t('factory.removeDocument', { name: 'Scratch guidance' }),
      exact: true,
    })
    .press('Enter');
  await expect(
    docs.getByRole('checkbox', { name: 'Scratch guidance', exact: true }),
  ).not.toBeChecked();
}
