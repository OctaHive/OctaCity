import { expect, test } from '@playwright/test';

import { installOperatorFixture } from './operatorFixture';
import { PRIMARY_ROUTES } from './primaryRoutes';

for (const route of PRIMARY_ROUTES) {
  test(`${route.path} retains rendered data when a background refresh fails`, async ({ page }) => {
    const fixture = await installOperatorFixture(page);
    await page.goto(route.path);
    await expect(
      page.getByRole('main').getByRole('heading', {
        exact: true,
        level: 1,
        name: route.heading,
      }),
    ).toBeVisible();
    await expect(page.getByText(route.retainedText, { exact: false }).first()).toBeVisible();
    await page.waitForLoadState('networkidle');

    await page.evaluate(() => globalThis.dispatchEvent(new Event('offline')));
    fixture.failApiReads();
    await page.evaluate(() => globalThis.dispatchEvent(new Event('online')));

    await expect(page.getByText(/Refresh failed\. Showing the last loaded/u).first()).toBeVisible();
    await expect(page.getByText(route.retainedText, { exact: false }).first()).toBeVisible();
  });
}
