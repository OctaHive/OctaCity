import { expect, test } from '@playwright/test';
import type { components } from '../../.generated/api/schema';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';
import { enMessages } from '../../src/app/presentation/messages.en';
import { ruMessages } from '../../src/app/presentation/messages.ru';
import { installOperatorFixture } from './operatorFixture';
import { factoryRoleFixture } from '../fixtures/factoryRoles';

for (const language of ['en', 'ru'] as const) {
  test(`${language}: configures Runtime and explicitly selects its MCP in a profile by keyboard`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.addInitScript(
      ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
      {
        key: PREFERENCES_KEY,
        preferences: { ...DEFAULT_PREFERENCES, language, theme: 'light' },
      },
    );
    const m = language === 'en' ? enMessages : ruMessages;
    const { role, profile, capability, choice: profileChoice } = factoryRoleFixture();
    const project = profile.role.project_id;
    const authorization: components['schemas']['FactoryMcpAuthorization'] = {
      credential_profile: 'mcp-read',
      secret: { provider: 'agent-private', reference: 'project/issues-read' },
      environment: 'ISSUE_TOKEN',
      purpose: 'mcp_read',
      resource: { identity: 'issues-repository', version: 'v1', digest: 'e'.repeat(64) },
    };
    const server: components['schemas']['FactoryMcpServer'] = {
      transport: 'http',
      url: 'https://mcp.example/tools',
      bearer_token_env_var: 'ISSUE_TOKEN',
      enabled_tools: ['read'],
    };
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
      environment_names: ['ISSUE_TOKEN'],
      secret_environment_names: ['ISSUE_TOKEN'],
      support: {
        mcp_transports: ['http'],
        mcp_executables: {},
        authenticated_mcp: {
          reader: {
            reference: { identity: 'installed-reader', version: 'v1', digest: 'f'.repeat(64) },
            server,
            authorization,
          },
        },
      },
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
    const profilePublications: components['schemas']['PublishFactoryRoleProfilesRequest'][] = [];
    let latestProfile = profile;
    const reader = {
      configuration: { identity: 'configured-reader', version: 'v1', digest: 'c'.repeat(64) },
      integration: null,
    };
    await page.route('**/api/v1/projects/*/factory-role**', async (route) => {
      const path = new URL(route.request().url()).pathname;
      const pageOf = (items: unknown[]) => ({ items, total: items.length, next_cursor: null });
      if (path.endsWith('/factory-roles'))
        return route.fulfill({ json: pageOf([{ ...role.definition, reference: role.reference }]) });
      if (path.endsWith('/factory-role-templates')) return route.fulfill({ json: pageOf([]) });
      if (path.endsWith('/profiles'))
        return route.fulfill({
          json: pageOf([
            {
              reference: latestProfile.reference,
              role: latestProfile.role,
              name: latestProfile.definition.name,
              toolchain: latestProfile.definition.toolchain,
            },
          ]),
        });
      if (path.includes('/factory-roles/') && path.includes('/versions/'))
        return route.fulfill({ json: role });
      if (path.includes('/factory-role-profiles/') && path.includes('/versions/'))
        return route.fulfill({ json: path.endsWith('/versions/1') ? profile : latestProfile });
      if (path.endsWith('/factory-role-profiles/validate')) {
        const request = route.request().postDataJSON();
        return route.fulfill({
          json: { profile: { ...profile, definition: request.definition }, incompatibility: null },
        });
      }
      if (path.endsWith('/factory-role-publications')) {
        const request = route
          .request()
          .postDataJSON() as components['schemas']['PublishFactoryRoleProfilesRequest'];
        profilePublications.push(request);
        const change = request.profiles?.[0];
        expect(change).toBeTruthy();
        if (!change) return route.abort();
        latestProfile = {
          ...profile,
          reference: { ...profile.reference, version: 2, digest: 'd'.repeat(64) },
          definition: change.definition,
        };
        return route.fulfill({
          json: {
            disposition: 'applied',
            published_at: 101,
            role: null,
            profiles: [latestProfile],
          },
        });
      }
      return route.fallback();
    });
    await page.route('**/api/v1/projects/*/factory-profile-capabilities**', (route) => {
      const path = new URL(route.request().url()).pathname;
      const mcpServers = settings ? { reader } : {};
      if (path.endsWith('/factory-profile-capabilities'))
        return route.fulfill({
          json: {
            items: [{ ...profileChoice, mcp_servers: mcpServers }],
            total: 1,
            next_cursor: null,
          },
        });
      return route.fulfill({
        json: {
          ...capability,
          mcp_configurations: settings?.definition.mcp_servers.reader
            ? {
                reader: {
                  reference: reader.configuration,
                  ...settings.definition.mcp_servers.reader,
                },
              }
            : {},
          model_catalog: { available: true, manual_model: true, models: [] },
          definition: {
            ...capability.definition,
            permissions: settings?.definition.permissions ?? capability.definition.permissions,
            execution: {
              ...capability.definition.execution,
              mcp_servers: mcpServers,
            },
          },
        },
      });
    });
    await page.goto(`/ai-settings?project_id=${project}&view=agents`);
    const storageBefore = await page.evaluate(() =>
      JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
    );
    await page.getByRole('button', { name: role.definition.name, exact: true }).click();
    await page.getByRole('button', { name: 'default', exact: true }).click();
    await page.getByRole('button', { name: m['factory.editProfile'], exact: true }).click();
    const editor = page.getByRole('region', { name: m['factory.profileEditor'], exact: true });
    await editor
      .getByRole('textbox', { name: m['factory.profileName'], exact: true })
      .fill('My pro');
    await editor
      .getByRole('textbox', { name: m['factory.modelIdentifier'], exact: true })
      .fill('Vendor/future');
    await editor
      .getByRole('textbox', { name: `${m['factory.profileInstruction']} 1`, exact: true })
      .fill('Retain my instructions.');
    await editor.getByRole('link', { name: m['factory.configureRuntime'] }).click();
    await expect(
      page.getByRole('heading', { name: m['section.aiSettings'], exact: true }),
    ).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Factory Project', exact: true })).toBeVisible();
    await page.getByRole('button', { name: choice.alias, exact: true }).click();
    await page.getByRole('button', { name: m['runtime.addMcp'] }).click();
    await page.getByRole('combobox', { name: m['runtime.installedMcp'], exact: true }).click();
    await page.getByRole('option', { name: 'reader', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect(page.getByText('agent-private · project/issues-read')).toBeVisible();
    await page.getByText(m['runtime.installationPrerequisites'], { exact: true }).click();
    await expect(page.getByText(m['runtime.pluginQualification'], { exact: true })).toBeVisible();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
    await page
      .getByRole('region', { name: m['runtime.editor'], exact: true })
      .screenshot({ path: `/tmp/runtime-settings-editor-${language}.png` });
    await page.getByRole('button', { name: m['runtime.review'], exact: true }).focus();
    await page.keyboard.press('Enter');
    const review = page.getByRole('dialog', { name: m['runtime.review'] });
    await expect(review).toBeVisible();
    expect(publications).toHaveLength(0);
    await expect(review.getByRole('button', { name: m['runtime.publish'] })).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(review.getByText(m['runtime.published'])).toBeVisible();
    expect(publications).toHaveLength(1);
    expect(publications[0]?.definition.mcp_servers.reader).toEqual({
      server,
      authorization,
      integration: null,
    });
    expect(page.url()).not.toContain('mcp.example');
    await review.getByRole('button', { name: m['common.close'], exact: true }).click();
    await page.getByRole('link', { name: m['runtime.configureProfile'], exact: true }).focus();
    await page.keyboard.press('Enter');
    expect(new URL(page.url()).searchParams.get('project_id')).toBe(project);
    const selection = editor.getByRole('checkbox', { name: 'reader', exact: true });
    await expect(selection).not.toBeChecked();
    await expect(editor.getByText('issues-repository · v1')).toBeVisible();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
    await editor.screenshot({ path: `/tmp/runtime-profile-editor-${language}.png` });
    await expect(
      editor.getByRole('textbox', { name: m['factory.profileName'], exact: true }),
    ).toHaveValue('My pro');
    await expect(
      editor.getByRole('textbox', { name: m['factory.modelIdentifier'], exact: true }),
    ).toHaveValue('Vendor/future');
    await selection.focus();
    await page.keyboard.press('Space');
    await expect(selection).toBeChecked();
    await editor.getByRole('button', { name: m['factory.reviewProfile'] }).click();
    const profileReview = page.getByRole('dialog', { name: m['factory.reviewProfile'] });
    expect(profilePublications).toHaveLength(0);
    await profileReview.getByRole('button', { name: m['factory.confirmPublish'] }).focus();
    await page.keyboard.press('Enter');
    await expect(profileReview.getByText(m['factory.published'])).toBeVisible();
    expect(profilePublications).toHaveLength(1);
    expect(profilePublications[0]?.profiles?.[0]?.definition).toMatchObject({
      name: 'My pro',
      model_identifier: 'Vendor/future',
      instructions: ['Retain my instructions.'],
      execution: { mcp_servers: { reader } },
    });
    await profileReview.getByRole('button', { name: m['common.close'], exact: true }).click();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
    expect(
      await page.evaluate(() =>
        JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
      ),
    ).toBe(storageBefore);
    await page.screenshot({
      path: `/tmp/runtime-settings-profile-${language}.png`,
      fullPage: true,
    });
  });
}
