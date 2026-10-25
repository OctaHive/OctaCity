import { expect, test } from '@playwright/test';
import type { components } from '../../.generated/api/schema';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';
import { enMessages } from '../../src/app/presentation/messages.en';
import { ruMessages } from '../../src/app/presentation/messages.ru';
import { installOperatorFixture } from './operatorFixture';
import { factoryRoleFixture } from '../fixtures/factoryRoles';

for (const language of ['en', 'ru'] as const) {
  test(`${language}: reviews Runtime configuration by keyboard under Project context`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    await page.setViewportSize({ width: language === 'ru' ? 390 : 1280, height: 844 });
    await page.addInitScript(
      ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
      {
        key: PREFERENCES_KEY,
        preferences: { ...DEFAULT_PREFERENCES, language, theme: 'light' },
      },
    );
    const m = language === 'en' ? enMessages : ruMessages;
    const { profile } = factoryRoleFixture();
    const project = profile.role.project_id;
    const choice: components['schemas']['FactoryRuntimeChoice'] = {
      alias: 'installed-runtime',
      toolchain: profile.definition.toolchain,
      permissions: { ...profile.definition.permissions, network_hosts: ['mcp.example'] },
      enforcement: [
        'plugin_identity',
        'executable_identity',
        'tool_identity',
        'command_arguments',
        'descendant_processes',
        'filesystem_paths',
        'mount_modes',
        'network_hosts',
        'secret_profiles',
        'workload_identity_profiles',
        'cpu',
        'memory',
        'disk',
        'process_count',
        'elapsed_time',
        'outputs',
      ],
      environment_names: [],
      secret_environment_names: [],
      support: { mcp_transports: ['http'], mcp_executables: {} },
    };
    let settings: components['schemas']['FactoryRuntimeSettings'] | null = null;
    const publications: components['schemas']['FactoryRuntimeSettingsRequest'][] = [];
    await page.route('**/api/v1/projects/*/factory-runtime**', async (route) => {
      const path = new URL(route.request().url()).pathname;
      if (path.endsWith('/factory-runtime-choices'))
        return route.fulfill({ json: { items: [choice], total: 1, next_cursor: null } });
      if (route.request().method() === 'GET') return route.fulfill({ json: { settings } });
      const request = route
        .request()
        .postDataJSON() as components['schemas']['FactoryRuntimeSettingsRequest'];
      const reviewed = {
        project_id: project,
        runtime: choice.alias,
        version: (request.expected_version ?? 0) + 1,
        definition: request.definition,
        digest: 'b'.repeat(64),
      };
      if (path.endsWith('/validate')) return route.fulfill({ json: { settings: reviewed } });
      expect(route.request().headers()['idempotency-key']).toBeTruthy();
      expect(route.request().headers()['if-match']).toBeUndefined();
      publications.push(request);
      settings = reviewed;
      return route.fulfill({ json: { settings, disposition: 'applied', published_at: 100 } });
    });
    await page.goto(`/ai-settings?project_id=${project}&view=runtimes`);
    await expect(
      page.getByRole('heading', { name: m['section.aiSettings'], exact: true }),
    ).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Factory Project', exact: true })).toBeVisible();
    await page.getByRole('button', { name: choice.alias, exact: true }).click();
    await page.getByRole('button', { name: m['runtime.addMcp'] }).click();
    await page.getByRole('textbox', { name: m['runtime.mcpName'], exact: true }).fill('reader');
    await page
      .getByRole('textbox', { name: m['runtime.endpoint'], exact: true })
      .fill('https://mcp.example/tools');
    await page.getByRole('button', { name: m['runtime.review'], exact: true }).focus();
    await page.keyboard.press('Enter');
    const review = page.getByRole('dialog', { name: m['runtime.review'] });
    await expect(review).toBeVisible();
    expect(publications).toHaveLength(0);
    await expect(review.getByRole('button', { name: m['runtime.publish'] })).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(review.getByText(m['runtime.published'])).toBeVisible();
    expect(publications).toHaveLength(1);
    expect(publications[0]?.definition.mcp_servers.reader?.server).toEqual({
      transport: 'http',
      url: 'https://mcp.example/tools',
    });
    expect(page.url()).not.toContain('mcp.example');
    await review.getByRole('button', { name: m['common.close'], exact: true }).click();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
    await page.screenshot({ path: `/tmp/runtime-31-${language}.png`, fullPage: true });
  });
}
