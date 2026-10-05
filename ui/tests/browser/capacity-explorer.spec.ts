import { expect, test } from '@playwright/test';

import { CONSOLE_PATHS } from '../../src/app/routes';
import { FIXTURE_IDS, installOperatorFixture } from './operatorFixture';

test('keeps loaded Agent branches stable while expanding and selecting', async ({ page }) => {
  await installOperatorFixture(page);
  let agentListReads = 0;
  let releaseAgentDetail: () => void = () => undefined;
  const agentDetail = new Promise<void>((resolve) => {
    releaseAgentDetail = resolve;
  });

  await page.route(/\/api\/v1\/agents(?:\?.*)?$/u, async (route) => {
    agentListReads += 1;
    await route.fallback();
  });
  await page.route(
    new RegExp(`/api/v1/agents/${FIXTURE_IDS.agent}(?:\\?.*)?$`, 'u'),
    async (route) => {
      await agentDetail;
      await route.fallback();
    },
  );

  await page.goto(CONSOLE_PATHS.agents);
  const explorer = page.getByRole('complementary', { name: 'Agents explorer' });
  await explorer.getByRole('button', { name: 'Expand Accessible Pool' }).click();
  const agentLink = explorer.getByRole('link', { name: 'Accessible Agent' });
  await expect(agentLink).toBeVisible();

  await explorer.getByRole('button', { name: 'Collapse Accessible Pool' }).click();
  await explorer.getByRole('button', { name: 'Expand Accessible Pool' }).click();
  await expect(agentLink).toBeVisible();
  expect(agentListReads).toBe(1);

  const tree = explorer.getByRole('tree', { name: 'Agent capacity hierarchy' });
  await tree.evaluate((element) => {
    element.dataset.stabilityMarker = 'original-tree';
  });
  await agentLink.click();
  await expect(page).toHaveURL(`/agents/${FIXTURE_IDS.agent}`);
  await expect(explorer.locator('[data-stability-marker="original-tree"]')).toHaveCount(1);
  expect(agentListReads).toBe(1);

  releaseAgentDetail();
  await expect(page.getByRole('heading', { name: 'Accessible Agent' })).toBeVisible();
  await expect(explorer.locator('[data-stability-marker="original-tree"]')).toHaveCount(1);
});
