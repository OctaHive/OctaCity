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
  await page.goto(path);
  await expect(page.getByRole('status')).toHaveText('Server ready');
}

test('renders the semantic application shell', async ({ page }) => {
  await page.setViewportSize({ height: 900, width: 1_280 });
  await openReadyConsole(page, CONSOLE_PATHS.projects);

  await expect(page.locator('body')).toMatchAriaSnapshot(`
    - link "Skip to content":
      - /url: "#console-content"
    - complementary:
      - link "OctaCity Projects":
        - /url: /projects
        - strong: OctaCity
        - text: Operator Console
      - button "Collapse navigation" [expanded]
      - navigation "Primary navigation":
        - paragraph: Operations
        - link "Projects":
          - /url: /projects
        - link "Agents":
          - /url: /agents
        - link "Agent Pools":
          - /url: /agent-pools
        - link "Audit":
          - /url: /audit
    - banner:
      - text: Operations
      - status: Server ready
    - complementary "Security notice":
      - paragraph:
        - strong: Trusted network only.
        - text: This console is unauthenticated and must not be exposed to an untrusted network.
    - main:
      - region "Projects":
        - paragraph: Operator workspace
        - heading "Projects" [level=1]
        - paragraph: Browse the bounded Project hierarchy and select an operator workspace.
  `);
});

test('preserves navigation and content at the narrow supported width', async ({ page }) => {
  await page.setViewportSize({ height: 900, width: 640 });
  await openReadyConsole(page, CONSOLE_PATHS.audit);

  const navigation = page.getByRole('navigation', { name: 'Primary navigation' });
  const content = page.locator('#console-content');
  await expect(navigation).toBeVisible();
  await expect(content).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Audit' })).toBeVisible();

  const expandedContentWidth = await content.evaluate(
    (element) => element.getBoundingClientRect().width,
  );
  const pageWidth = await page.evaluate(() => ({
    client: document.documentElement.clientWidth,
    scroll: document.documentElement.scrollWidth,
  }));
  expect(pageWidth.scroll).toBe(pageWidth.client);

  const collapse = page.getByRole('button', { name: 'Collapse navigation' });
  await collapse.focus();
  await collapse.press('Enter');
  await expect(page.getByRole('button', { name: 'Expand navigation' })).toHaveAttribute(
    'aria-expanded',
    'false',
  );
  await expect
    .poll(async () => content.evaluate((element) => element.getBoundingClientRect().width))
    .toBeGreaterThan(expandedContentWidth);
});
