import { expect, test, type Page } from '@playwright/test';

import { installOperatorFixture } from './operatorFixture';
import { primaryRoute } from './primaryRoutes';

const visualCases = [
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
] as const;

for (const visual of visualCases) {
  test(`${visual.name} remains visually stable`, async ({ page }, testInfo) => {
    testInfo.snapshotSuffix = '';
    await installOperatorFixture(page);
    await page.setViewportSize(visual.viewport);
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
    await selectTheme(page, visual.theme);
    if (visual.viewport.width === 640) {
      await expect(page.getByRole('button', { name: /Open .* explorer/u })).toBeVisible();
    }

    await expect(page).toHaveScreenshot(visual.name, { fullPage: true });
  });
}

async function selectTheme(page: Page, theme: 'dark' | 'light') {
  await page.getByRole('combobox', { name: 'Theme' }).click();
  await page.getByRole('option', { name: theme === 'dark' ? 'Dark' : 'Light' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-resolved-theme', theme);
}
