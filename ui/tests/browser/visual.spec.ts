import { expect, test, type Page } from '@playwright/test';

import { configurationFixture } from '../fixtures/factory';
import { installOperatorFixture } from './operatorFixture';
import { primaryRoute } from './primaryRoutes';

const visualCases = [
  {
    ...primaryRoute('factory-run'),
    name: 'factory-run-graph-desktop-light.png',
    theme: 'light',
    openPinnedClosure: true,
    viewport: { height: 900, width: 1_440 },
  },
  {
    ...primaryRoute('factory'),
    path: `/factory?project_id=${configurationFixture().configuration.reference.project_id}`,
    name: 'factory-desktop-light.png',
    theme: 'light',
    viewport: { height: 900, width: 1_440 },
  },
  {
    ...primaryRoute('factory-run'),
    name: 'factory-run-desktop-dark.png',
    theme: 'dark',
    viewport: { height: 900, width: 1_440 },
  },
  {
    ...primaryRoute('factory-run'),
    language: 'ru',
    name: 'factory-run-narrow-russian-light.png',
    theme: 'light',
    viewport: { height: 900, width: 640 },
  },
  {
    ...primaryRoute('projects'),
    illustrationLabel: 'Project workspace illustration',
    name: 'projects-desktop-light.png',
    theme: 'light',
    viewport: { height: 900, width: 1_440 },
  },
  {
    ...primaryRoute('projects'),
    illustrationLabel: 'Project workspace illustration',
    name: 'projects-desktop-dark.png',
    theme: 'dark',
    viewport: { height: 900, width: 1_440 },
  },
  {
    ...primaryRoute('audit'),
    name: 'audit-narrow-light.png',
    theme: 'light',
    viewport: { height: 900, width: 640 },
  },
  {
    ...primaryRoute('audit'),
    name: 'audit-narrow-dark.png',
    theme: 'dark',
    viewport: { height: 900, width: 640 },
  },
  {
    ...primaryRoute('project'),
    name: 'project-desktop-light.png',
    theme: 'light',
    viewport: { height: 900, width: 1_440 },
  },
  {
    ...primaryRoute('project'),
    language: 'ru',
    name: 'project-desktop-russian-light.png',
    theme: 'light',
    viewport: { height: 900, width: 1_440 },
  },
  {
    ...primaryRoute('builds'),
    illustrationLabel: 'Build pipeline illustration',
    name: 'builds-desktop-dark.png',
    theme: 'dark',
    viewport: { height: 900, width: 1_440 },
  },
  {
    ...primaryRoute('build'),
    name: 'build-narrow-light.png',
    theme: 'light',
    viewport: { height: 900, width: 640 },
  },
  {
    ...primaryRoute('agents'),
    illustrationLabel: 'Agent capacity illustration',
    name: 'agents-desktop-light.png',
    theme: 'light',
    viewport: { height: 900, width: 1_440 },
  },
  {
    ...primaryRoute('agent'),
    name: 'agent-narrow-dark.png',
    theme: 'dark',
    viewport: { height: 900, width: 640 },
  },
  {
    ...primaryRoute('agent-pools'),
    illustrationLabel: 'Agent capacity illustration',
    name: 'agent-pools-desktop-dark.png',
    theme: 'dark',
    viewport: { height: 900, width: 1_440 },
  },
  {
    ...primaryRoute('agent-pool'),
    name: 'agent-pool-narrow-light.png',
    theme: 'light',
    viewport: { height: 900, width: 640 },
  },
  {
    ...primaryRoute('projects'),
    colorScheme: 'dark',
    name: 'projects-narrow-overlay-system-dark.png',
    openExplorer: true,
    theme: 'system',
    viewport: { height: 900, width: 640 },
  },
] as const;

for (const visual of visualCases) {
  test(`${visual.name} remains visually stable`, async ({ page }, testInfo) => {
    testInfo.snapshotSuffix = process.platform;
    await installOperatorFixture(page);
    await page.setViewportSize(visual.viewport);
    if ('colorScheme' in visual) await page.emulateMedia({ colorScheme: visual.colorScheme });
    await page.goto(visual.path);
    await expect(page.getByRole('status', { name: 'Server ready' })).toBeVisible();
    await expect(
      page.getByRole('main').getByRole('heading', {
        exact: true,
        level: 1,
        name: visual.heading,
      }),
    ).toBeVisible();
    if ('illustrationLabel' in visual) {
      await expect(page.getByRole('img', { name: visual.illustrationLabel })).toBeVisible();
    }
    await selectTheme(
      page,
      visual.theme,
      'colorScheme' in visual ? visual.colorScheme : visual.theme,
    );
    if (visual.viewport.width === 640) {
      await expect(page.getByRole('button', { name: /Open .* explorer/u })).toBeVisible();
    }
    if ('openExplorer' in visual && visual.openExplorer) {
      await page.getByRole('button', { name: /Open .* explorer/u }).click();
      const explorer = page.getByRole('dialog', { name: /.* explorer/u });
      await expect(explorer).toBeVisible();
      await expect(explorer.getByText(visual.retainedText, { exact: false }).first()).toBeVisible();
    }
    if ('openPinnedClosure' in visual) {
      await page.locator('summary').filter({ hasText: 'Pinned definition closure' }).click();
      await expect(page.getByRole('img', { name: 'Flow graph', exact: true })).toBeVisible();
    }
    if ('language' in visual) await selectRussian(page);
    await expect(page.locator('body')).toHaveCSS('font-family', /ui-sans-serif/u);

    // Retained headings can be visible while another observer refreshes its data.
    // Capture the settled fixture, including after a presentation change.
    await expect(
      page
        .getByRole('status')
        .filter({ hasText: /^(Loading |Refreshing |Загрузка: |Обновление: )/u }),
    ).toHaveCount(0);
    await page.evaluate(() => window.scrollTo({ top: 0, left: 0, behavior: 'instant' }));
    await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(0);
    await expect(page).toHaveScreenshot(visual.name, { fullPage: true });
  });
}

async function selectTheme(
  page: Page,
  theme: 'dark' | 'light' | 'system',
  resolved: 'dark' | 'light',
) {
  await page.getByRole('combobox', { name: 'Theme' }).click();
  const option = { dark: 'Dark', light: 'Light', system: 'System' }[theme];
  await page.getByRole('option', { name: option }).click();
  await expect(page.locator('html')).toHaveAttribute('data-resolved-theme', resolved);
}

async function selectRussian(page: Page) {
  await page.getByLabel('Operator menu').click();
  await page.getByRole('combobox', { name: 'Language' }).click();
  await page.getByRole('option', { name: 'Russian' }).click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'ru');
}
