import { expect, test, type Page } from '@playwright/test';

import { CONSOLE_PATHS } from '../../src/app/routes';

async function openReadyConsole(page: Page, path: string) {
  await page.route('**/health/ready', async (route) => {
    await route.fulfill({
      body: JSON.stringify({ status: 'ready' }),
      contentType: 'application/json',
      status: 200,
    });
  });
  await page.route('**/api/v1/projects*', async (route) => {
    const url = new URL(route.request().url());
    const parentId = url.searchParams.get('parent_id');
    const items =
      parentId === null
        ? [browserProject('platform', 'Platform', null)]
        : parentId === 'platform'
          ? [browserProject('delivery', 'Delivery', 'platform')]
          : [];
    await route.fulfill({
      body: JSON.stringify({ items, next_cursor: null }),
      contentType: 'application/json',
      status: 200,
    });
  });
  await page.goto(path);
  await expect(page.getByRole('status')).toHaveText('Server ready');
}

test('renders the semantic contextual workbench', async ({ page }) => {
  await page.setViewportSize({ height: 900, width: 1_440 });
  await openReadyConsole(page, CONSOLE_PATHS.projects);

  await expect(page.getByRole('banner')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Search resources' })).toBeVisible();
  await expect(page.getByRole('combobox', { name: 'Theme' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Notifications' })).toBeVisible();
  await expect(page.getByLabel('Operator menu')).toBeVisible();

  const sectionNavigation = page.getByRole('navigation', { name: 'Primary sections' });
  await expect(sectionNavigation.getByRole('link')).toHaveCount(4);
  await expect(sectionNavigation.getByRole('link', { name: 'Projects' })).toHaveAttribute(
    'aria-current',
    'page',
  );

  const explorer = page.getByRole('complementary', { name: 'Projects explorer' });
  await expect(explorer).toBeVisible();
  await expect(explorer.getByRole('heading', { name: 'Favorites' })).toBeVisible();
  await expect(explorer.getByRole('heading', { name: 'Browse' })).toBeVisible();
  await expect(explorer.getByRole('link', { name: 'Platform' })).toBeVisible();
  await explorer.getByRole('button', { name: 'Expand Platform' }).click();
  await expect(explorer.getByRole('link', { name: 'Delivery' })).toBeVisible();
  await expect(page.getByRole('main').getByRole('link', { name: 'Platform' })).toHaveCount(0);
  await expect(page.locator('#console-content')).toBeVisible();
  await expect(page.getByLabel('Security notice')).toContainText('Trusted network only');
});

function browserProject(id: string, name: string, parentId: string | null) {
  return {
    created_at_unix_ms: 1_700_000_000_000,
    id,
    name,
    parent_id: parentId,
    updated_at_unix_ms: 1_700_000_000_000,
    version: 1,
  };
}

test('resizes and restores the desktop explorer within its bounds', async ({ page }) => {
  await page.setViewportSize({ height: 900, width: 1_440 });
  await openReadyConsole(page, CONSOLE_PATHS.audit);

  const separator = page.getByRole('separator', { name: 'Resize Audit explorer' });
  await expect(separator).toHaveAttribute('aria-valuenow', '288');
  await separator.focus();
  await separator.press('End');
  await expect(separator).toHaveAttribute('aria-valuenow', '480');

  await page.reload();
  await expect(page.getByRole('status')).toHaveText('Server ready');
  const restored = page.getByRole('separator', { name: 'Resize Audit explorer' });
  await expect(restored).toHaveAttribute('aria-valuenow', '480');

  const handle = await restored.boundingBox();
  expect(handle).not.toBeNull();
  if (handle !== null) {
    await page.mouse.move(handle.x + handle.width / 2, handle.y + 120);
    await page.mouse.down();
    await page.mouse.move(0, handle.y + 120, { steps: 4 });
    await page.mouse.up();
  }
  await expect(restored).toHaveAttribute('aria-valuenow', '240');

  await page.reload();
  await expect(page.getByRole('separator', { name: 'Resize Audit explorer' })).toHaveAttribute(
    'aria-valuenow',
    '240',
  );

  await page.setViewportSize({ height: 900, width: 640 });
  await expect(page.getByRole('button', { name: 'Open Audit explorer' })).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Audit explorer' })).toBeHidden();
});

test('uses a focus-managed explorer overlay at the narrow supported width', async ({ page }) => {
  await page.setViewportSize({ height: 900, width: 640 });
  await openReadyConsole(page, CONSOLE_PATHS.audit);

  const content = page.locator('#console-content');
  await expect(content).toBeVisible();
  await expect(page.getByRole('heading', { level: 1, name: 'Audit' })).toBeVisible();
  await expect(page.getByRole('combobox', { name: 'Theme' })).toBeVisible();
  await expect(page.getByLabel('Operator menu')).toBeVisible();

  const reopen = page.getByRole('button', { name: 'Open Audit explorer' });
  await expect(reopen).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Audit explorer' })).toBeHidden();

  await reopen.press('Enter');
  const explorer = page.getByRole('dialog', { name: 'Audit explorer' });
  await expect(explorer).toBeVisible();
  await expect(page.getByRole('heading', { level: 2, name: 'Audit' })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('button', { name: 'Search Audit' })).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(page.getByRole('link', { name: 'Audit filters' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Open Audit explorer' })).toBeFocused();

  const viewport = await page.evaluate(() => ({
    client: document.documentElement.clientWidth,
    scroll: document.documentElement.scrollWidth,
  }));
  expect(viewport.scroll).toBe(viewport.client);
});

test('opens global and explorer search with focus restoration', async ({ page }) => {
  await page.setViewportSize({ height: 900, width: 1_280 });
  await openReadyConsole(page, CONSOLE_PATHS.projects);

  const globalSearch = page.getByRole('button', { name: 'Search resources' });
  await globalSearch.click();
  await expect(page.getByRole('dialog', { name: 'Search resources' })).toBeVisible();
  await expect(page.getByRole('searchbox', { name: 'Search query' })).toBeFocused();
  await expect(page.getByText('Scope: All resources')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(globalSearch).toBeFocused();

  await page.getByRole('link', { name: 'Builds' }).focus();
  await page.keyboard.press('Control+k');
  await expect(page.getByRole('searchbox', { name: 'Search query' })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('button', { exact: true, name: 'Close' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('link', { name: 'Builds' })).toBeFocused();

  await page.getByRole('button', { name: 'Search Projects' }).click();
  await expect(page.getByText('Scope: Projects')).toBeVisible();
});
