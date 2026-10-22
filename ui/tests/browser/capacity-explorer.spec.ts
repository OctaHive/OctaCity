import { expect, test } from '@playwright/test';

import { CONSOLE_PATHS } from '../../src/app/routes';
import { FIXTURE_IDS, installOperatorFixture } from './operatorFixture';

test('shows the Agent tree without a redundant browse heading or capacity notice', async ({
  page,
}) => {
  await installOperatorFixture(page);
  await page.goto(CONSOLE_PATHS.agents);
  const explorer = page.getByRole('complementary', { name: 'Agents explorer' });
  await expect(explorer.getByRole('link', { name: 'Accessible Pool', exact: true })).toBeVisible();
  await expect(explorer.getByRole('heading', { name: 'Favorites', exact: true })).toBeVisible();
  await expect(explorer.getByRole('heading', { name: 'Browse', exact: true })).toHaveCount(0);
  await expect(
    explorer.getByText(
      'Capacity facts are authoritative; workload compatibility is decided by the server.',
      { exact: true },
    ),
  ).toHaveCount(0);
});

test('reviews admission actions in the header while preserving host facts and connection', async ({
  page,
}) => {
  await installOperatorFixture(page);
  await page.goto(`/agents/${FIXTURE_IDS.agent}`);
  await expect(page.getByRole('heading', { name: 'Accessible Agent', exact: true })).toBeVisible();
  let resource = await page.evaluate(async (agentId) => {
    const response = await fetch(`/api/v1/agents/${agentId}`);
    return response.json();
  }, FIXTURE_IDS.agent);
  resource.enabled = true;
  resource.inventory.host_details = {
    hostname: 'builder-1',
    fqdn: 'builder-1.example.test',
    os_version: 'macOS 26',
    ip_addresses: ['10.60.20.111'],
  };
  resource.resources = {
    observed_at: 1_700_000_000_000,
    available_cpu_millis: 1000,
    available_memory_bytes: 1024,
    work_disk_free_bytes: 1_073_741_824,
    state_disk_free_bytes: 2_147_483_648,
  };
  const commands: { enabled: boolean; version: string | undefined }[] = [];
  await page.route(`**/api/v1/agents/${FIXTURE_IDS.agent}`, async (route) => {
    await route.fulfill({ json: resource });
  });
  await page.route(/\/api\/v1\/agents\?.*$/u, async (route) => {
    await route.fulfill({
      json: {
        items: [resource],
        next_cursor: null,
        active_count: resource.enabled ? 1 : 0,
        total_count: 1,
      },
    });
  });
  await page.route(`**/api/v1/agents/${FIXTURE_IDS.agent}/availability`, async (route) => {
    const { enabled } = route.request().postDataJSON();
    commands.push({ enabled, version: route.request().headers()['if-match'] });
    resource = { ...resource, enabled, version: resource.version + 1 };
    await route.fulfill({ json: { disposition: 'applied', resource } });
  });
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await expect(page.getByText('builder-1.example.test', { exact: true })).toBeVisible();
  await expect(page.getByText('1.0 GiB free of 100.0 GiB')).toBeVisible();
  await expect(
    page.getByRole('link', { name: 'Accessible Pool', exact: true }).last(),
  ).toBeVisible();

  const actions = page.getByRole('button', { name: 'Actions', exact: true });
  for (const label of ['Disconnect Agent', 'Connect Agent']) {
    await actions.click();
    await page.getByRole('button', { name: label, exact: true }).click();
    const dialog = page.getByRole('dialog', { name: `${label} Accessible Agent?` });
    await dialog.getByRole('button', { name: label, exact: true }).click();
    await expect(dialog.getByText('Agent availability updated.')).toBeVisible();
    await dialog.getByRole('button', { name: 'Close', exact: true }).click();
    await expect(
      page.getByText(label === 'Disconnect Agent' ? 'New Jobs disabled' : 'Accepting new Jobs', {
        exact: true,
      }),
    ).toBeVisible();
    await expect(
      page.getByRole('heading', { name: 'Accessible Agent', exact: true }),
    ).toBeVisible();
  }
  expect(commands).toEqual([
    { enabled: false, version: '"1"' },
    { enabled: true, version: '"2"' },
  ]);
  await page.setViewportSize({ width: 375, height: 812 });
  await actions.click();
  await expect(page.getByRole('button', { name: 'Graceful drain', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Forced drain', exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
});

for (const target of [
  { name: 'Accessible Pool', path: `/agent-pools/${FIXTURE_IDS.pool}` },
  { name: 'Accessible Agent', path: `/agents/${FIXTURE_IDS.agent}` },
]) {
  test(`selects ${target.name} from the full tree row without changing expansion`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    await page.goto(CONSOLE_PATHS.agents);
    const explorer = page.getByRole('complementary', { name: 'Agents explorer' });
    await explorer.getByRole('button', { name: 'Expand Accessible Pool' }).click();
    await expect(page).toHaveURL(CONSOLE_PATHS.agents);

    const link = explorer.getByRole('link', { name: target.name, exact: true });
    const row = explorer
      .getByRole('treeitem')
      .filter({ has: page.getByRole('link', { name: target.name, exact: true }) })
      .last();
    await expect(link).toBeVisible();
    const bounds = await row.boundingBox();
    const label = await link.boundingBox();
    if (bounds === null || label === null) throw new Error('Tree row must be visible');

    // The trailing row padding is outside both the name and the status badge.
    await row.click({
      position: { x: bounds.width - 2, y: label.y - bounds.y + label.height / 2 },
    });
    await expect(page).toHaveURL(target.path);
    await expect(link).toHaveAttribute('aria-current', 'page');

    await explorer.getByRole('button', { name: 'Collapse Accessible Pool' }).click();
    await expect(page).toHaveURL(target.path);
    await expect(explorer.getByRole('button', { name: 'Expand Accessible Pool' })).toBeVisible();
    await explorer.getByRole('button', { name: 'Expand Accessible Pool' }).click();
    await expect(page).toHaveURL(target.path);
    await expect(link).toBeVisible();
  });
}

test('keeps loaded Agent branches stable while expanding and selecting', async ({ page }) => {
  await installOperatorFixture(page);
  let agentListReads = 0;
  let poolListReads = 0;
  let releaseAgentDetail: () => void = () => undefined;
  const agentDetail = new Promise<void>((resolve) => {
    releaseAgentDetail = resolve;
  });

  await page.route(/\/api\/v1\/agents(?:\?.*)?$/u, async (route) => {
    agentListReads += 1;
    await route.fallback();
  });
  await page.route(/\/api\/v1\/agent-pools(?:\?.*)?$/u, async (route) => {
    poolListReads += 1;
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
  expect(poolListReads).toBe(1);

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
  expect(poolListReads).toBe(1);
});
