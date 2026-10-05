import { expect, test, type Route } from '@playwright/test';

import { CONSOLE_PATHS } from '../../src/app/routes';
import { FIXTURE_IDS, installOperatorFixture } from './operatorFixture';

const CONFIGURATION_ID = 'configuration-accessible';

test('keeps loaded Build branches stable while expanding and selecting', async ({ page }) => {
  await installOperatorFixture(page);
  let configurationReads = 0;
  let buildReads = 0;
  let buildDetailReads = 0;
  let releaseSecondBuildRead: () => void = () => undefined;
  const secondBuildRead = new Promise<void>((resolve) => {
    releaseSecondBuildRead = resolve;
  });

  await page.route(
    `**/api/v1/projects/${FIXTURE_IDS.project}/build-configurations*`,
    async (route) => {
      configurationReads += 1;
      await fulfillPage(route, [configurationSummary()]);
    },
  );
  await page.route(`**/api/v1/projects/${FIXTURE_IDS.project}/builds*`, async (route) => {
    buildReads += 1;
    await fulfillPage(route, [buildSummary()]);
  });
  await page.route(`**/api/v1/build-configurations/${CONFIGURATION_ID}/versions/1`, async (route) =>
    fulfill(route, configurationSummary()),
  );
  await page.route(
    new RegExp(`/api/v1/builds/${FIXTURE_IDS.build}(?:\\?.*)?$`, 'u'),
    async (route) => {
      buildDetailReads += 1;
      if (buildDetailReads > 1) await secondBuildRead;
      await route.fallback();
    },
  );

  await page.goto(CONSOLE_PATHS.builds);
  const explorer = page.getByRole('complementary', { name: 'Builds explorer' });
  await explorer.getByRole('button', { name: 'Expand Accessible Project' }).click();
  await explorer.getByRole('button', { name: 'Expand Accessible Configuration' }).click();
  const buildLink = explorer.getByRole('link', { name: FIXTURE_IDS.build });
  await expect(buildLink).toBeVisible();

  await explorer.getByRole('button', { name: 'Collapse Accessible Project' }).click();
  await explorer.getByRole('button', { name: 'Expand Accessible Project' }).click();
  await expect(buildLink).toBeVisible();
  expect(configurationReads).toBe(1);
  expect(buildReads).toBe(1);

  const tree = explorer.getByRole('tree', { name: 'Build hierarchy' });
  await tree.evaluate((element) => {
    element.dataset.stabilityMarker = 'original-tree';
  });
  await buildLink.click();
  await expect(page).toHaveURL(`/builds/${FIXTURE_IDS.build}`);
  await expect(explorer.locator('[data-stability-marker="original-tree"]')).toHaveCount(1);
  expect(configurationReads).toBe(1);
  expect(buildReads).toBe(1);

  await page
    .getByRole('navigation', { name: 'Primary sections' })
    .getByRole('link', { name: 'Builds' })
    .click();
  await buildLink.click();
  await expect.poll(() => buildDetailReads).toBe(2);
  await expect(explorer.locator('[data-stability-marker="original-tree"]')).toHaveCount(1);
  await expect(explorer.getByText(/Refreshing selected Build path/u)).toHaveCount(0);
  releaseSecondBuildRead();
});

function configurationSummary() {
  return {
    id: CONFIGURATION_ID,
    name: 'Accessible Configuration',
    project_id: FIXTURE_IDS.project,
    version: 1,
  };
}

function buildSummary() {
  return {
    configuration_id: CONFIGURATION_ID,
    id: FIXTURE_IDS.build,
    project_id: FIXTURE_IDS.project,
    state: 'succeeded',
  };
}

async function fulfillPage(route: Route, items: unknown[]) {
  await fulfill(route, { items, next_cursor: null });
}

async function fulfill(route: Route, body: unknown) {
  await route.fulfill({
    body: JSON.stringify(body),
    contentType: 'application/json',
    status: 200,
  });
}
