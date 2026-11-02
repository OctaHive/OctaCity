import { expect, test } from '@playwright/test';
import { configurationFixture } from '../fixtures/factory';
import { installOperatorFixture } from './operatorFixture';
import { translate } from '../../src/app/presentation/messages';

test.use({ trace: 'off', screenshot: 'off', video: 'off' });

for (const language of ['en', 'ru'] as const) {
  test(`retained definitions use common authoring without a triage entry at phone width (${language})`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    await page.addInitScript(
      (language) =>
        localStorage.setItem(
          'octacity.console.preferences',
          JSON.stringify({
            version: 2,
            language,
            theme: 'light',
            favorites: [],
            recents: [],
            expanded: [],
            explorerOpen: false,
            explorerWidth: 288,
            notificationLastOpenedAt: null,
          }),
        ),
      language,
    );
    const t = (
      key: Parameters<typeof translate>[1],
      values: Parameters<typeof translate>[2] = {},
    ) => translate(language, key, values);
    const owner = configurationFixture().configuration.reference.project_id;
    const presets: string[] = [];
    const mutations: string[] = [];
    page.on('request', (request) => {
      if (request.url().includes('/factory-presets')) presets.push(request.url());
      if (request.method() !== 'GET') mutations.push(request.url());
    });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(`/factory?project_id=${owner}`);
    await expect(
      page.getByRole('button', { name: t('factory.addPipeline'), exact: true }),
    ).toBeEnabled();
    await expect(
      page.getByRole('button', {
        name: language === 'en' ? 'Set up triage' : 'Настроить триаж',
        exact: true,
      }),
    ).toHaveCount(0);
    await page.getByRole('button', { name: 'implement · v1', exact: true }).press('Enter');
    await page.getByRole('button', { name: t('factory.editFlow'), exact: true }).press('Enter');
    await expect(
      page.getByRole('region', { name: t('factory.stepGraph'), exact: true }),
    ).toBeVisible();
    const name = page.getByRole('textbox', { name: t('factory.nodeName'), exact: true });
    await name.fill('private_migration_draft');
    await page.getByRole('button', { name: t('factory.discardDraft'), exact: true }).press('Enter');
    await page.getByRole('button', { name: t('factory.editFlow'), exact: true }).press('Enter');
    await expect(name).toHaveValue('implement');
    expect(presets).toEqual([]);
    expect(mutations).toEqual([]);
    expect(
      await page.evaluate(() =>
        JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
      ),
    ).not.toContain('private_migration_draft');
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth - innerWidth),
    ).toBeLessThanOrEqual(0);
  });
}
