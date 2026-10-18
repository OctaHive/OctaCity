import { expect, test } from '@playwright/test';
import { installOperatorFixture } from './operatorFixture';
import { integrationFixture } from '../fixtures/integrations';

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
