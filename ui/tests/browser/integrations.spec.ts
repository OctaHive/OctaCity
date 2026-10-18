import { expect, test } from '@playwright/test';
import { installOperatorFixture } from './operatorFixture';
import { integrationFixture } from '../fixtures/integrations';

test('finishes a partially configured connection by opening the missing credential and activating', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const connection = integrationFixture('IntegrationConnection');
  const state = integrationFixture('IntegrationAdministrationState');
  state.enabled = false;
  state.active_version = null;
  const reader = connection.definition.resources[0]!.capabilities[0]!;
  connection.definition.resources[0]!.capabilities.push({
    ...reader,
    purpose: 'task_update',
    credential: { ...reader.credential, reference: 'writer' },
  });
  await page.route(`**/api/v1/integrations/${connection.id}`, async (route) =>
    route.fulfill({ json: connection }),
  );
  await page.route(`**/api/v1/integrations/${connection.id}/state`, async (route) =>
    route.fulfill({ json: state }),
  );
  await page.route(`**/api/v1/integrations/${connection.id}/credentials`, async (route) => {
    const body = route.request().postDataJSON();
    expect(body.intent.action).toEqual({
      operation: 'credential',
      reference: { provider: 'installed', reference: 'writer' },
      kind: 'provision',
      activate_connection: true,
    });
    expect(body.value).toBe('synthetic-update-token');
    state.credentials.push({
      configured: true,
      reference: { provider: 'installed', reference: 'writer' },
    });
    state.enabled = true;
    state.active_version = 1;
    state.revision = 3;
    await route.fulfill({ json: integrationFixture('IntegrationAdministrationReceipt') });
  });
  await page.goto(`/integrations/${connection.id}`);
  await expect(page.getByRole('list', { name: 'Credential status' })).toContainText(
    'Task updates · Not configured',
  );
  await expect(page.getByRole('button', { name: 'Activate connection' })).toBeDisabled();
  await page.getByRole('button', { name: 'Manage credentials' }).click();
  await expect(page.getByRole('combobox', { name: 'Credential scope' })).toHaveText(
    'installed · writer',
  );
  await expect(page.getByRole('combobox', { name: 'Credential operation' })).toHaveText(
    'Provision',
  );
  await page.getByLabel('Credential value', { exact: true }).fill('synthetic-update-token');
  await page
    .getByRole('checkbox', { name: 'Activate this connection after credential setup' })
    .check();
  await page.getByRole('button', { name: 'Review credential change' }).click();
  const dialog = page.getByRole('dialog', { name: 'Confirm credential change' });
  await dialog.getByRole('button', { name: 'Confirm change' }).click();
  await expect(dialog.getByText('Change completed.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Disable connection' })).toBeEnabled();
  await expect(page.getByRole('list', { name: 'Credential status' })).toContainText(
    'Task updates · Configured',
  );
});

test('saves the first token without activating a connection that still needs another access', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const connection = integrationFixture('IntegrationConnection');
  const state = integrationFixture('IntegrationAdministrationState');
  state.enabled = false;
  state.credentials = [];
  const reader = connection.definition.resources[0]!.capabilities[0]!;
  connection.definition.resources[0]!.capabilities.push({
    ...reader,
    purpose: 'task_update',
    credential: { ...reader.credential, reference: 'writer' },
  });
  await page.route(`**/api/v1/integrations/${connection.id}`, async (route) =>
    route.fulfill({ json: connection }),
  );
  await page.route(`**/api/v1/integrations/${connection.id}/state`, async (route) =>
    route.fulfill({ json: state }),
  );
  let submissions = 0;
  await page.route(`**/api/v1/integrations/${connection.id}/credentials`, async (route) => {
    const body = route.request().postDataJSON();
    expect(body.intent.action.kind).toBe('provision');
    expect(body.intent.action.activate_connection).toBe(false);
    expect(body.value).toBe('synthetic-first-token');
    submissions += 1;
    await route.fulfill({ json: integrationFixture('IntegrationAdministrationReceipt') });
  });
  await page.goto(`/integrations/${connection.id}`);
  await expect(page.getByRole('button', { name: 'Activate connection' })).toBeDisabled();
  await page.getByRole('button', { name: 'Manage credentials' }).click();
  await expect(
    page.getByRole('checkbox', { name: 'Activate this connection after credential setup' }),
  ).toBeDisabled();
  await expect(
    page.getByText(
      'Configure every selected access before activation. You can save this token without activating the connection.',
    ),
  ).toBeVisible();
  await page.getByLabel('Credential value', { exact: true }).fill('synthetic-first-token');
  await page.getByRole('button', { name: 'Review credential change' }).click();
  const dialog = page.getByRole('dialog', { name: 'Confirm credential change' });
  await dialog.getByRole('button', { name: 'Confirm change' }).click();
  await expect(dialog.getByText('Change completed.')).toBeVisible();
  expect(submissions).toBe(1);
  await expect(page.getByLabel('Credential value', { exact: true })).toHaveValue('');
});

test('opens dedicated integration administration and clears credential review with the keyboard', async ({
  page,
}) => {
  await installOperatorFixture(page);
  await page.goto('/projects');
  await page
    .getByRole('navigation', { name: 'Primary sections' })
    .getByRole('link', { name: 'Integrations' })
    .click();
  await expect(page.getByRole('heading', { level: 1, name: 'Integrations' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Manual Factory intake' })).toHaveAttribute(
    'href',
    '/factory',
  );
  await expect(page.getByRole('complementary', { name: 'Integrations explorer' })).toHaveCount(0);
  await page.getByRole('link', { name: 'Tracker', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Tracker' })).toBeVisible();
  await page.getByRole('button', { name: 'Manage credentials' }).click();
  await page.getByLabel('Credential value', { exact: true }).fill('synthetic-browser-secret');
  await page.getByRole('button', { name: 'Review credential change' }).click();
  const dialog = page.getByRole('dialog', { name: 'Confirm credential change' });
  await expect(dialog).toBeVisible();
  await expect(dialog).not.toContainText('synthetic-browser-secret');
  await expect(dialog.getByRole('button', { name: 'Confirm change' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(page.getByLabel('Credential value', { exact: true })).toHaveValue('');
  await expect(page.getByRole('button', { name: 'Review credential change' })).toBeFocused();
});

test('renders Russian administration and safe rejected credential outcomes without persisted values', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const connection = integrationFixture('IntegrationConnection');
  const secret = 'synthetic-rejected-credential';
  await page.route('**/api/v1/integrations/*/credentials', async (route) => {
    const payload = route.request().postDataJSON();
    expect(payload.value).toBe(secret);
    await route.fulfill({
      status: 403,
      contentType: 'application/json',
      body: JSON.stringify({ code: 'forbidden', message: secret, request_id: secret }),
    });
  });
  await page.addInitScript(() => {
    localStorage.setItem(
      'octacity.console.preferences',
      JSON.stringify({
        version: 2,
        language: 'ru',
        theme: 'light',
        favorites: [],
        recents: [],
        expanded: [],
        explorerOpen: true,
        explorerWidth: 288,
        notificationLastOpenedAt: null,
      }),
    );
  });
  await page.setViewportSize({ width: 640, height: 900 });
  await page.goto(`/integrations/${connection.id}`);
  await page.getByRole('button', { name: 'Управлять учётными данными' }).click();
  await page.getByLabel('Секретное значение', { exact: true }).fill(secret);
  await page.getByRole('button', { name: 'Проверить изменение учётных данных' }).click();
  await page.getByRole('button', { name: 'Подтвердить изменение', exact: true }).click();
  await expect(
    page.getByText('Не удалось выполнить операцию с интеграцией.', { exact: true }),
  ).toBeVisible();
  await expect(page.getByLabel('Секретное значение', { exact: true })).toHaveValue('');
  await expect(page.locator('body')).not.toContainText(secret);
  const persisted = await page.evaluate(() =>
    JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
  );
  expect(persisted).not.toContain(secret);
  await page.screenshot({
    path: test.info().outputPath('integration-russian-narrow.png'),
    fullPage: true,
  });
});

test('opens Project access separately from checks and reviews explicit revocation with keyboard focus', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const connection = integrationFixture('IntegrationConnection');
  await page.route('**/api/v1/integrations/*/projects/**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    await route.fulfill({
      json: path.endsWith('/dependencies')
        ? { items: [], next_cursor: null }
        : path.endsWith('/resources')
          ? { resources: [] }
          : { access: null },
    });
  });
  await page.goto(`/integrations/${connection.id}`);
  await page.getByRole('button', { name: 'Manage Project access' }).click();
  await page.getByRole('combobox', { name: 'OctaCity Project' }).click();
  await page.getByRole('option', { name: 'Accessible Project' }).click();
  await expect(page.getByText('No resources are currently granted for use.')).toBeVisible();
  await page.getByRole('button', { name: 'Edit Project access' }).click();
  await page.getByRole('button', { name: 'Review Project access' }).click();
  const dialog = page.getByRole('dialog', { name: 'Confirm Project access' });
  await expect(dialog).toContainText('Revoke all resource purposes');
  await expect(dialog).toContainText(
    'Accepted Work, Run results and already sent remote requests remain retained',
  );
  await expect(dialog.getByRole('button', { name: 'Confirm change' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Review Project access' })).toBeFocused();
  await expect(page.getByRole('button', { name: 'Check access (read-only)' })).toBeVisible();
});
