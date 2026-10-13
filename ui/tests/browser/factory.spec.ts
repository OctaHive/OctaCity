import { expect, test } from '@playwright/test';
import { FIXTURE_IDS, installOperatorFixture } from './operatorFixture';

test('copied Factory Run filters survive search, collapse and resize, and the visible Project opens the guided workbench', async ({
  page,
}) => {
  await installOperatorFixture(page);
  await page.goto(`/factory/${FIXTURE_IDS.factoryRun}?state=waiting&source=manual`);
  await expect(page.getByRole('heading', { name: 'Factory Run', level: 1 })).toBeVisible();
  const explorer = page.getByRole('complementary', { name: 'Factory explorer' });
  await expect(explorer.getByRole('link', { name: 'Requested change' })).toHaveAttribute(
    'href',
    `/factory/${FIXTURE_IDS.factoryRun}?state=waiting&source=manual&configuration_id=22222222-2222-4222-8222-222222222222`,
  );
  await page.getByRole('button', { name: 'Search Factory' }).click();
  await expect(explorer.locator('[data-factory-filters]')).toBeFocused();
  await page.getByRole('button', { name: 'Collapse Factory explorer' }).click();
  await page.getByRole('button', { name: 'Open Factory explorer' }).click();
  await expect(explorer.getByRole('button', { name: 'Collapse Factory Project' })).toBeVisible();
  const resize = page.getByRole('separator', { name: 'Resize Factory explorer' });
  await resize.focus();
  await page.keyboard.press('ArrowRight');
  await expect(explorer.getByRole('link', { name: 'Requested change' })).toBeVisible();
  await page.getByRole('button', { name: 'Add to Favorites' }).click();
  await expect(page.getByRole('button', { name: 'Remove from Favorites' })).toBeVisible();
  await page.getByRole('link', { name: 'Open Project Factory workbench' }).click();
  await expect(
    page.getByRole('main').getByRole('heading', { level: 1, name: 'Factory' }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Inspect Flow Definition · v1' }).click();
  await page.getByRole('button', { name: 'Edit Flow draft' }).click();
  await expect(page.getByRole('textbox', { name: 'Node name' })).toHaveValue('implement');
  await page.getByRole('heading', { name: 'Flow graph editor' }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: '/tmp/octacity-factory-workbench-15.png' });
  await page.getByRole('textbox', { name: 'Node name' }).fill('visible_operator_draft');
  expect(page.url()).not.toContain('visible_operator_draft');
  expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain(
    'visible_operator_draft',
  );
  await page.getByRole('button', { name: 'Discard draft' }).click();
});
