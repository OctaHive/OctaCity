import { expect, test } from '@playwright/test';
import { configurationFixture } from '../fixtures/factory';
import { installOperatorFixture } from './operatorFixture';
import { translate, type MessageKey } from '../../src/app/presentation/messages';

for (const language of ['en', 'ru'] as const) {
  for (const width of [390, 1280]) {
    test(`${language}: reads a spacious configuration and selects historical versions inside its dropdown at ${width}px`, async ({
      page,
    }, info) => {
      await installOperatorFixture(page);
      await page.setViewportSize({ width, height: 900 });
      if (language === 'ru')
        await page.addInitScript(() =>
          localStorage.setItem(
            'octacity.console.preferences',
            JSON.stringify({
              version: 2,
              language: 'ru',
              theme: 'light',
              favorites: [],
              recents: [],
              expanded: [],
              explorerOpen: false,
              explorerWidth: 288,
              notificationLastOpenedAt: null,
            }),
          ),
        );
      const t = (key: MessageKey, values = {}) => translate(language, key, values);
      const configuration = configurationFixture();
      const reference = configuration.configuration.reference;
      configuration.configuration.hard_budget = {
        max_attempts: 8,
        max_elapsed_millis: 600000,
        max_tokens: 300000,
        max_cost_micro_units: 2000000,
        max_output_bytes: 134217728,
      };
      const versions: number[] = [];
      const mutations: string[] = [];
      page.on('request', (request) => {
        if (request.method() !== 'GET') mutations.push(request.url());
      });
      await page.route('**/api/v1/projects/*/factory-configurations*', (route) =>
        route.fulfill({
          json: {
            items: [
              {
                ...reference,
                version: 205,
                enabled: true,
                created_at_unix_ms: 1,
                published_at_unix_ms: 1,
              },
            ],
            total: 1,
            next_cursor: null,
          },
        }),
      );
      await page.route(`**/api/v1/factory-configurations/${reference.id}/versions/*`, (route) => {
        const version = Number(new URL(route.request().url()).pathname.split('/').at(-1));
        versions.push(version);
        return route.fulfill({
          json: {
            ...configuration,
            configuration: { ...configuration.configuration, reference: { ...reference, version } },
          },
        });
      });
      await page.goto(`/factory?project_id=${reference.project_id}`);
      await page
        .getByRole('button', {
          name: t('factory.inspectConfiguration', { version: 205 }),
          exact: true,
        })
        .click();
      let card = page.getByRole('region', {
        name: t('factory.configurationVersion', { version: 205 }),
        exact: true,
      });
      await expect(
        card.getByText(t('factory.elapsedSeconds', { value: 600 }), { exact: true }),
      ).toBeVisible();
      await expect(
        card.getByText(language === 'ru' ? '128,0 MiB' : '128.0 MiB', { exact: true }),
      ).toBeVisible();
      await expect(
        page.getByRole('button', { name: t('factory.earlierVersions'), exact: true }),
      ).toHaveCount(0);
      await expect(
        page.getByRole('button', { name: t('factory.laterVersions'), exact: true }),
      ).toHaveCount(0);
      const policy = card.getByText(
        `${configuration.configuration.admission_policy.identity} · ${configuration.configuration.admission_policy.version}`,
        { exact: true },
      );
      await expect(policy).toBeHidden();
      const first = (await card
        .getByRole('button', { name: t('factory.admitManual'), exact: true })
        .boundingBox())!;
      const second = (await card
        .getByRole('button', { name: t('factory.configureAutomatic'), exact: true })
        .boundingBox())!;
      const explanation = (await card
        .getByText(t('factory.inputsHelp'), { exact: true })
        .boundingBox())!;
      expect(first.y - explanation.y - explanation.height).toBeGreaterThanOrEqual(8);
      expect(
        second.y > first.y ? second.y - first.y - first.height : second.x - first.x - first.width,
      ).toBeGreaterThanOrEqual(8);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
        width,
      );
      await card.screenshot({ path: info.outputPath('configuration-summary.png') });
      await card.getByText(t('factory.definitionDetails'), { exact: true }).click();
      await expect(policy).toBeVisible();
      await expect(card.getByText('600000', { exact: true })).toBeVisible();
      const picker = card.getByRole('combobox', { name: t('factory.version'), exact: true });
      await picker.click();
      await page.getByRole('searchbox', { name: t('factory.findVersion'), exact: true }).fill('1');
      expect(await page.getByRole('option').count()).toBeLessThanOrEqual(101);
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      card = page.getByRole('region', {
        name: t('factory.configurationVersion', { version: 1 }),
        exact: true,
      });
      await expect(card).toBeVisible();
      expect(versions).toContain(1);
      await card.getByRole('combobox', { name: t('factory.version'), exact: true }).click();
      await page
        .getByRole('searchbox', { name: t('factory.findVersion'), exact: true })
        .fill('205');
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      await expect(
        page.getByRole('region', {
          name: t('factory.configurationVersion', { version: 205 }),
          exact: true,
        }),
      ).toBeVisible();
      expect(mutations).toEqual([]);
    });
  }
}
