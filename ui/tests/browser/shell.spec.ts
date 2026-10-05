import { expect, test, type Locator, type Page } from '@playwright/test';

import { CONSOLE_PATHS } from '../../src/app/routes';

async function openReadyConsole(page: Page, path: string, attentionItems: readonly object[] = []) {
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
        ? [browserProject('platform', 'Platform', null, true)]
        : parentId === 'platform'
          ? [browserProject('delivery', 'Delivery', 'platform', false)]
          : [];
    await route.fulfill({
      body: JSON.stringify({ items, next_cursor: null }),
      contentType: 'application/json',
      status: 200,
    });
  });
  await page.route('**/api/v1/operator-attention*', async (route) => {
    await route.fulfill({
      body: JSON.stringify({ items: attentionItems, next_cursor: null }),
      contentType: 'application/json',
      status: 200,
    });
  });
  await page.goto(path);
  await expect(page.getByRole('status', { name: 'Server ready' })).toBeVisible();
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
  await expect(explorer.getByRole('button', { name: 'Expand Delivery' })).toHaveCount(0);
  await expect(page.getByRole('main').getByRole('link', { name: 'Platform' })).toHaveCount(0);
  await expect(page.locator('#console-content')).toBeVisible();
  await expect(page.getByLabel('Security notice')).toContainText('Trusted network only');
});

test('persists Russian presentation without storing resource content', async ({ page }) => {
  await page.setViewportSize({ height: 900, width: 1_440 });
  await openReadyConsole(page, CONSOLE_PATHS.projects);

  await page.getByLabel('Operator menu').click();
  await page.getByRole('combobox', { name: 'Language' }).selectOption('ru');
  await expect(page.locator('html')).toHaveAttribute('lang', 'ru');
  await expect(page.getByRole('link', { name: 'Проекты', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(page.getByText('Выберите проект в иерархии слева.')).toBeVisible();

  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('lang', 'ru');
  const record = await page.evaluate(() => localStorage.getItem('octacity.console.preferences'));
  expect(record).not.toMatch(/Platform|Delivery|payload|requestId|credential/i);
});

test('keeps relevant notifications browser-local and rebuilds attention after reload', async ({
  page,
}) => {
  const favoriteBuildId = '77777777-7777-4777-8777-777777777777';
  const unrelatedBuildId = '88888888-8888-4888-8888-888888888888';
  const attentionItems = [
    browserAttention('favorite-build-event', 'build', favoriteBuildId, null),
    browserAttention('unrelated-build-event', 'build', unrelatedBuildId, null),
    browserAttention('critical-active', 'critical_system', null, null),
    browserAttention('critical-resolved', 'critical_system', null, 1_700_000_001_000),
  ];
  await page.setViewportSize({ height: 900, width: 1_440 });
  await openReadyConsole(page, CONSOLE_PATHS.projects, attentionItems);
  await page.evaluate((buildId) => {
    localStorage.setItem(
      'octacity.console.preferences',
      JSON.stringify({
        expanded: [],
        explorerOpen: true,
        explorerWidth: 288,
        favorites: [{ id: buildId, kind: 'build' }],
        language: 'en',
        notificationLastOpenedAt: null,
        recents: [],
        theme: 'system',
        version: 2,
      }),
    );
  }, favoriteBuildId);
  await page.reload();

  const bell = page.getByRole('button', { name: 'Notifications, 3 unseen' });
  await expect(bell).toBeVisible();
  await bell.click();
  const center = page.getByRole('dialog', { name: 'Notification center' });
  await expect(center.getByText('favorite-build-event summary')).toBeVisible();
  await expect(center.getByText('critical-active summary')).toBeVisible();
  await expect(center.getByText('critical-resolved summary')).toBeVisible();
  await expect(center.getByText('unrelated-build-event summary')).toHaveCount(0);
  await expect(center.getByText('Resolved', { exact: true })).toBeVisible();
  await expect(center.getByRole('link', { name: /favorite-build-event summary/u })).toHaveAttribute(
    'href',
    `/builds/${favoriteBuildId}`,
  );
  await expect(page.getByRole('button', { name: 'Close notification center' })).toBeFocused();
  const stored = await page.evaluate(() => localStorage.getItem('octacity.console.preferences'));
  expect(stored ?? '').not.toMatch(/favorite-build-event|critical-active|request[_-]?id/iu);

  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Notifications' })).toBeFocused();
  await page.reload();
  await expect(page.getByRole('button', { name: 'Notifications' })).toBeVisible();
  await page.getByRole('button', { name: 'Notifications' }).click();
  await expect(page.getByText('No commands were completed in this browser session.')).toBeVisible();
  await expect(page.getByText('favorite-build-event summary')).toBeVisible();
});

test('uses a contextual Audit filter panel without generic navigation groups', async ({ page }) => {
  await page.setViewportSize({ height: 900, width: 1_440 });
  await openReadyConsole(page, CONSOLE_PATHS.audit);

  await expect(page.getByRole('heading', { exact: true, level: 1, name: 'Audit' })).toBeVisible();
  const explorer = page.getByRole('complementary', { name: 'Audit explorer' });
  await expect(explorer.getByRole('form', { name: 'Audit filters' })).toBeVisible();
  await expect(explorer.getByRole('heading', { name: 'Favorites' })).toHaveCount(0);
  await expect(explorer.getByRole('heading', { name: 'Browse' })).toHaveCount(0);
  await expect(page.getByRole('main').getByRole('form', { name: 'Audit filters' })).toHaveCount(0);
  await expect(page.getByRole('separator', { name: 'Resize Audit explorer' })).toBeVisible();

  await page.setViewportSize({ height: 900, width: 640 });
  const open = page.getByRole('button', { name: 'Open Audit explorer' });
  await expect(open).toBeVisible();
  await open.click();
  const dialog = page.getByRole('dialog', { name: 'Audit explorer' });
  await expect(dialog.getByRole('form', { name: 'Audit filters' })).toBeVisible();
  await dialog.getByRole('button', { name: 'Collapse Audit explorer' }).click();
  await expect(open).toBeFocused();
});

test('keeps Audit outcome labels on one line', async ({ page }) => {
  await page.route('**/api/v1/audit-facts*', async (route) => {
    await route.fulfill({
      body: JSON.stringify({ items: [browserAuditFact()], next_cursor: null }),
      contentType: 'application/json',
      status: 200,
    });
  });
  await page.setViewportSize({ height: 900, width: 900 });
  await openReadyConsole(page, CONSOLE_PATHS.audit);

  const outcome = page.getByText('Accepted', { exact: true });
  await expect(outcome).toBeVisible();
  expect(
    await outcome.evaluate((element) => {
      const range = document.createRange();
      range.selectNodeContents(element);
      return range.getClientRects().length;
    }),
  ).toBe(1);
});

function browserProject(id: string, name: string, parentId: string | null, hasChildren: boolean) {
  return {
    created_at_unix_ms: 1_700_000_000_000,
    has_children: hasChildren,
    id,
    name,
    parent_id: parentId,
    updated_at_unix_ms: 1_700_000_000_000,
    version: 1,
  };
}

function browserAuditFact() {
  return {
    actor: { identity: 'agent-layout', kind: 'agent' },
    id: 'audit-layout',
    idempotency_key: null,
    metadata: {},
    occurred_at_unix_ms: 1_700_000_000_000,
    operation: 'append-job-events',
    outcome: 'accepted',
    request_identity: `append-job-events:${'1'.repeat(64)}:25-25`,
    target_identity: 'job-layout',
    target_kind: 'job',
  };
}

function browserAttention(
  id: string,
  category: 'build' | 'critical_system',
  targetId: string | null,
  resolvedAt: number | null,
) {
  return {
    category,
    code: category === 'critical_system' ? 'required_dependency_unavailable' : 'build_failed',
    id,
    occurred_at_unix_ms: 1_700_000_000_000,
    resolved_at_unix_ms: resolvedAt,
    severity: category === 'critical_system' ? 'critical' : 'warning',
    summary: `${id} summary`,
    target: targetId === null ? null : { id: targetId, kind: category },
  };
}

test('resizes and restores the desktop explorer within its bounds', async ({ page }) => {
  await page.setViewportSize({ height: 900, width: 1_440 });
  await openReadyConsole(page, CONSOLE_PATHS.projects);

  const separator = page.getByRole('separator', { name: 'Resize Projects explorer' });
  await expect(separator).toHaveAttribute('aria-valuenow', '288');
  await separator.focus();
  await separator.press('End');
  await expect(separator).toHaveAttribute('aria-valuenow', '480');

  await page.reload();
  await expect(page.getByRole('status', { name: 'Server ready' })).toBeVisible();
  const restored = page.getByRole('separator', { name: 'Resize Projects explorer' });
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
  await expect(page.getByRole('separator', { name: 'Resize Projects explorer' })).toHaveAttribute(
    'aria-valuenow',
    '240',
  );

  await page.setViewportSize({ height: 900, width: 640 });
  await expect(page.getByRole('button', { name: 'Open Projects explorer' })).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Projects explorer' })).toBeHidden();
});

test('uses a focus-managed explorer overlay at the narrow supported width', async ({ page }) => {
  await page.setViewportSize({ height: 900, width: 640 });
  await openReadyConsole(page, CONSOLE_PATHS.projects);

  const content = page.locator('#console-content');
  await expect(content).toBeVisible();
  await expect(
    page.getByRole('heading', { exact: true, level: 1, name: 'Projects' }),
  ).toBeVisible();
  await expect(page.getByRole('combobox', { name: 'Theme' })).toBeVisible();
  await expect(page.getByLabel('Operator menu')).toBeVisible();

  const reopen = page.getByRole('button', { name: 'Open Projects explorer' });
  await expect(reopen).toBeVisible();
  await expect(page.getByRole('dialog', { name: 'Projects explorer' })).toBeHidden();

  await reopen.press('Enter');
  const explorer = page.getByRole('dialog', { name: 'Projects explorer' });
  await expect(explorer).toBeVisible();
  await expect(explorer.getByRole('link', { name: 'Platform' })).toBeVisible();
  await expect(
    explorer.getByRole('heading', { exact: true, level: 2, name: 'Projects' }),
  ).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('button', { name: 'Search Projects' })).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(page.getByRole('link', { name: 'Platform' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Open Projects explorer' })).toBeFocused();

  const viewport = await page.evaluate(() => ({
    client: document.documentElement.clientWidth,
    scroll: document.documentElement.scrollWidth,
  }));
  expect(viewport.scroll).toBe(viewport.client);
});

test('opens global and explorer search with focus restoration', async ({ page }) => {
  const searchableBuildId = '22222222-2222-4222-8222-222222222222';
  await page.route('**/api/v1/search*', async (route) => {
    await route.fulfill({
      body: JSON.stringify({
        items: [
          {
            context: 'Succeeded',
            id: searchableBuildId,
            kind: 'build',
            label: 'Alpha Build',
          },
        ],
        next_cursor: null,
      }),
      contentType: 'application/json',
      status: 200,
    });
  });
  await page.setViewportSize({ height: 900, width: 1_280 });
  await openReadyConsole(page, CONSOLE_PATHS.projects);

  const globalSearch = page.getByRole('button', { name: 'Search resources' });
  await globalSearch.click();
  await expect(page.getByRole('dialog', { name: 'Search resources' })).toBeVisible();
  await expect(page.getByRole('searchbox', { name: 'Search query' })).toBeFocused();
  await expect(page.getByText('Scope: All resources')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Navigate' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Presentation commands' })).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Close command center' }).getByText('Esc', { exact: true }),
  ).toBeVisible();
  await page.getByRole('searchbox', { name: 'Search query' }).fill('alpha');
  await expect(page.getByRole('button', { name: 'Open Alpha Build' })).toBeVisible();
  await page.getByRole('button', { name: 'Add Alpha Build to favorites' }).click();
  await page.keyboard.press('Escape');
  await expect(globalSearch).toBeFocused();

  await page.getByRole('link', { name: 'Builds' }).focus();
  await page.keyboard.press('Control+k');
  await expect(page.getByRole('searchbox', { name: 'Search query' })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('button', { name: 'Close command center' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('link', { name: 'Builds' })).toBeFocused();

  await page.getByRole('button', { name: 'Search Projects' }).click();
  await expect(page.getByText('Scope: Projects')).toBeVisible();
  await page.getByRole('button', { name: 'Search all resources' }).click();
  await expect(page.getByText('Scope: All resources')).toBeVisible();
  await page.keyboard.press('Escape');

  await page.getByRole('link', { name: 'Builds' }).click();
  await expect(page.getByRole('link', { name: `Open Builds ${searchableBuildId}` })).toBeVisible();
  await page.getByRole('button', { name: 'Search Builds' }).click();
  await expect(page.getByText('Scope: Builds')).toBeVisible();
  await page.getByRole('button', { name: 'Search all resources' }).click();
  await expect(page.getByText('Scope: All resources')).toBeVisible();
});

test('keeps Agent empty-state text inside the panel content inset', async ({ page }) => {
  const agent = browserAgent();
  const pool = browserAgentPool();
  await page.route(/\/api\/v1\/agents(?:\/[^?]+)?(?:\?.*)?$/u, async (route) => {
    const url = new URL(route.request().url());
    await route.fulfill({
      body: JSON.stringify(
        url.pathname === `/api/v1/agents/${agent.id}`
          ? agent
          : { items: [agent], next_cursor: null },
      ),
      contentType: 'application/json',
      status: 200,
    });
  });
  await page.route(/\/api\/v1\/agent-pools(?:\/[^?]+)?(?:\?.*)?$/u, async (route) => {
    const url = new URL(route.request().url());
    await route.fulfill({
      body: JSON.stringify(
        url.pathname === `/api/v1/agent-pools/${pool.id}`
          ? pool
          : { items: [pool], next_cursor: null },
      ),
      contentType: 'application/json',
      status: 200,
    });
  });
  await page.setViewportSize({ height: 900, width: 1_440 });
  await openReadyConsole(page, `/agents/${agent.id}`);

  await expectPanelMessageInset(
    page.getByRole('heading', { name: 'Inventory and capacity' }),
    page.getByText('No execution runtimes were published.'),
  );
  await expectPanelMessageInset(
    page.getByRole('heading', { name: 'Current execution' }),
    page.getByText('Idle — no current execution is published.'),
  );
});

async function expectPanelMessageInset(heading: Locator, message: Locator) {
  const panel = heading.locator('xpath=ancestor::section[1]');
  await expect(message).toBeVisible();
  const [messageBox, panelBox, inset] = await Promise.all([
    message.boundingBox(),
    panel.boundingBox(),
    message.evaluate((element) => {
      const style = getComputedStyle(element);
      return {
        bottom: Number.parseFloat(style.paddingBottom),
        left: Number.parseFloat(style.paddingLeft),
      };
    }),
  ]);
  expect(messageBox).not.toBeNull();
  expect(panelBox).not.toBeNull();
  if (messageBox === null || panelBox === null) return;
  expect(messageBox.x).toBeGreaterThanOrEqual(panelBox.x);
  expect(messageBox.x + messageBox.width).toBeLessThanOrEqual(panelBox.x + panelBox.width);
  expect(inset.left).toBeGreaterThanOrEqual(16);
  expect(inset.bottom).toBeGreaterThanOrEqual(16);
}

function browserAgent() {
  return {
    capacity: {
      logical_cpu_count: 10,
      state_disk_total_bytes: 493_921_239_040,
      total_memory_bytes: 34_359_738_368,
      virtualization_available: true,
      work_disk_total_bytes: 493_921_239_040,
    },
    current_execution: null,
    id: 'agent-layout',
    inventory: {
      agent_version: '0.1.0',
      cache: null,
      coordinator_protocols: [1],
      host_platform: { architecture: 'arm64', os: 'macos' },
      labels: {},
      octa: {
        build_commit: null,
        event_schemas: [1],
        features: [],
        octafile_versions: [1],
        plugin_protocols: [1],
        plugins: [],
        runner_protocols: [1],
        runner_sha256: '1'.repeat(64),
        version: '0.4.0',
      },
      runtimes: [],
      source_plugins: [],
    },
    last_seen_at_unix_ms: 1_700_000_000_000,
    name: 'Layout Agent',
    pool_id: 'pool-layout',
    pool_version: 1,
    status: 'online',
    version: 1,
  };
}

function browserAgentPool() {
  return {
    definition: {
      admission_policy: { mode: 'any' },
      concurrency_limit: 1,
      drain_state: 'accepting',
      enabled: true,
      fairness_policy: 'priority_fifo',
      static_capacity_limit: 1,
    },
    id: 'pool-layout',
    name: 'Layout Pool',
    published_at_unix_ms: 1_700_000_000_000,
    version: 1,
  };
}
