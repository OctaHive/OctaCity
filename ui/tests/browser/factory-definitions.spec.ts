import { expect, test, type Page } from '@playwright/test';
import { configurationFixture } from '../fixtures/factory';
import { installOperatorFixture, isLegacyPolicyRead } from './operatorFixture';
import { translate, type MessageKey } from '../../src/app/presentation/messages';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';

import { emptyFactoryPermissions } from '../../src/features/factory/emptyFlowDraft';
import type { components } from '../../.generated/api/schema';

async function installPolicyChoices(page: Page, project: string) {
  const reference = (id: string, digest: string, version: number) => ({
    project_id: project,
    id,
    digest: digest.repeat(64),
    version,
  });
  const launch = {
    kind: 'launch',
    policy: {
      reference: reference('e45ef94b-52a8-48f5-ae4f-9a9bb7f3a3ed', 'a', 1),
      definition: { name: 'Standard launch', allow_manual: true, allow_automatic_source: false },
    },
  } as const;
  const alternate = {
    kind: 'launch',
    policy: {
      reference: { ...launch.policy.reference, version: 2, digest: 'b'.repeat(64) },
      definition: { name: 'Source only', allow_manual: false, allow_automatic_source: true },
    },
  } as const;
  const access = {
    kind: 'access',
    policy: {
      reference: reference('3e1b09d1-9265-4037-af6f-60558f192390', 'c', 1),
      definition: { name: 'Standard access', permissions: emptyFactoryPermissions() },
    },
  } as const;
  const defaults = {
    project_id: project,
    version: 1,
    launch_policy: launch.policy.reference,
    access_profile: access.policy.reference,
  };
  const path = `/api/v1/projects/${project}/factory-policies`;
  await page.route(`**/api/v1/projects/${project}/factory-configuration-choices*`, (route) =>
    route.fulfill({
      json: { items: [], next_cursor: null, total: 0, project_policy_defaults: defaults },
    }),
  );
  await page.route(`**${path}**`, (route) => {
    const url = new URL(route.request().url());
    if (url.pathname === path)
      return route.fulfill({
        json: {
          items: [alternate, access].map(({ kind, policy }) => ({
            name: policy.definition.name,
            reference: { kind, reference: policy.reference },
          })),
          next_cursor: null,
        },
      });
    if (url.pathname === `${path}/defaults`) return route.fulfill({ json: { defaults } });
    const policy = [launch, alternate, access].find(
      (record) => record.policy.reference.digest === url.searchParams.get('digest'),
    );
    return route.fulfill(
      policy ? { json: { policy } } : { status: 404, json: { code: 'not_found' } },
    );
  });
  return { launch, alternate, access };
}

for (const language of ['en', 'ru'] as const) {
  for (const width of [390, 1280]) {
    test(`${language}: uses exact Configuration policies by keyboard at ${width}px`, async ({
      page,
    }) => {
      await installOperatorFixture(page);
      await page.setViewportSize({ width, height: 900 });
      await page.addInitScript(
        ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
        {
          key: PREFERENCES_KEY,
          preferences: { ...DEFAULT_PREFERENCES, language, explorerOpen: false },
        },
      );
      const t = (key: MessageKey) => translate(language, key);
      const configuration = configurationFixture().configuration;
      const project = configuration.reference.project_id;
      const { alternate, access } = await installPolicyChoices(page, project);
      const requests: components['schemas']['CreateFactoryConfigurationRequest'][] = [];
      await page.route('**/api/v1/factory-configurations/validate', (route) => {
        const request = route
          .request()
          .postDataJSON() as components['schemas']['CreateFactoryConfigurationRequest'];
        requests.push(request);
        return route.fulfill({
          json: { configuration: { ...request.definition, reference: configuration.reference } },
        });
      });
      await page.goto(`/factory?project_id=${project}`);
      await page
        .getByRole('button', { name: t('factory.createConfiguration'), exact: true })
        .press('Enter');
      const draft = page.getByRole('group', { name: t('factory.configurationDraft'), exact: true });
      const launch = draft.getByRole('combobox', { name: t('policy.launchLabel'), exact: true });
      await expect(launch).toHaveText('Standard launch · v1');
      await expect(
        draft.getByRole('combobox', { name: t('policy.accessLabel'), exact: true }),
      ).toHaveText('Standard access · v1');
      for (const key of ['policy.launchLabel', 'factory.rootFlow', 'policy.accessLabel'] as const) {
        const label = draft.getByText(t(key), { exact: true });
        await expect(label).toBeVisible();
        const picker = draft.getByRole('combobox', { name: t(key), exact: true });
        const labelBox = (await label.boundingBox())!;
        const pickerBox = (await picker.boundingBox())!;
        expect(pickerBox.y).toBeGreaterThanOrEqual(labelBox.y + labelBox.height);
        await picker.press('ArrowDown');
        await expect(page.getByRole('listbox', { name: t(key), exact: true })).toBeVisible();
        await page.keyboard.press('Escape');
        await expect(picker).toBeFocused();
      }
      await launch.press('ArrowDown');
      await page.getByRole('option', { name: 'Source only · v2' }).press('Enter');
      await expect(launch).toBeFocused();
      await expect(draft.getByText(t('policy.automaticAllowed'), { exact: true })).toBeVisible();
      const runs = draft.getByRole('spinbutton', { name: t('factory.activeRuns'), exact: true });
      await runs.fill('3');
      const manage = draft.getByRole('button', { name: t('policy.manageLaunch'), exact: true });
      await manage.press('Enter');
      const back = page.getByRole('button', { name: t('policy.backToDraft'), exact: true });
      await expect(back).toBeFocused();
      const settings = page.getByRole('region', { name: t('policy.settings'), exact: true });
      await settings.getByRole('button', { name: t('policy.edit'), exact: true }).press('Enter');
      await expect(settings.getByRole('textbox', { name: t('policy.name') })).toHaveValue(
        'Source only',
      );
      await settings.getByRole('button', { name: t('policy.cancel'), exact: true }).press('Enter');
      await back.press('Enter');
      await expect(manage).toBeFocused();
      await expect(runs).toHaveValue('3');
      await expect(launch).toHaveText('Source only · v2');
      await expect(
        draft.getByRole('group', { name: t('factory.hardBudget'), exact: true }),
      ).toBeVisible();
      await expect(
        draft.getByRole('spinbutton', { name: t('factory.activeCalls'), exact: true }),
      ).toBeVisible();
      await draft.getByText(t('policy.bindingDetails'), { exact: true }).first().press('Enter');
      await expect(
        draft.getByText(t('policy.exactReference'), { exact: true }).first(),
      ).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
        width,
      );
      await page
        .getByRole('button', { name: t('factory.validateDraft'), exact: true })
        .press('Enter');
      await expect(page.getByText(t('factory.validationPassed'), { exact: true })).toBeVisible();
      expect(requests).toHaveLength(1);
      expect(requests[0]!.definition).toMatchObject({
        wip_limits: { max_active_runs: 3 },
        admission_policy: { project_policy: alternate.policy.reference },
        permission_ceiling: { project_policy: access.policy.reference },
      });
      await page
        .getByRole('button', { name: t('factory.reviewPublication'), exact: true })
        .press('Enter');
      const dialog = page.getByRole('dialog');
      await expect(dialog).toBeVisible();
      await dialog.getByRole('button', { name: t('command.abandon'), exact: true }).press('Enter');
      await expect(
        page.getByRole('button', { name: t('factory.reviewPublication'), exact: true }),
      ).toBeFocused();
    });

    test(`${language}: keeps the published flow editor padded and its actions separated at ${width}px`, async ({
      page,
    }, info) => {
      await installOperatorFixture(page);
      await page.setViewportSize({ width, height: 900 });
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
      const t = (key: MessageKey) => translate(language, key);
      const configuration = configurationFixture().configuration;
      const flow = configuration.flow;
      const root = flow.closure.definitions.find(
        (definition) => definition.reference.id === flow.closure.root.id,
      )!;
      root.name = null;
      root.process = {
        goals: {
          artifact: { artifact_id: 'goals', content_digest: 'a'.repeat(64), encoded_size: 10 },
          recipients: [],
        },
        criteria: {
          artifact: { artifact_id: 'criteria', content_digest: 'b'.repeat(64), encoded_size: 10 },
          recipients: [],
        },
      };
      await page.route(`**/api/v1/flow-definitions/${flow.closure.root.id}/versions/*`, (route) =>
        route.fulfill({
          json: {
            flow,
            project_id: configuration.reference.project_id,
            published_at_unix_ms: 1,
          },
        }),
      );
      await page.goto(
        `/factory?project_id=${configuration.reference.project_id}&flow_definition_id=${flow.closure.root.id}&flow_version=1`,
      );
      await page.getByRole('button', { name: t('factory.editFlow'), exact: true }).click();
      const heading = page.getByRole('heading', { name: t('factory.immutableFlow'), exact: true });
      const card = heading.locator('..').locator('..');
      const graph = page.getByRole('heading', { name: t('factory.graphEditor'), exact: true });
      const discard = page.getByRole('button', { name: t('factory.discardDraft'), exact: true });
      const memory = page.getByText(t('factory.memoryDraft'), { exact: true });
      if (width >= 1280) {
        expect(
          (await heading.boundingBox())!.x - (await card.boundingBox())!.x,
        ).toBeGreaterThanOrEqual(16);
        expect(
          (await graph.boundingBox())!.x - (await card.boundingBox())!.x,
        ).toBeGreaterThanOrEqual(16);
      }
      const memoryBox = (await memory.boundingBox())!;
      const discardBox = (await discard.boundingBox())!;
      expect(discardBox.y - memoryBox.y - memoryBox.height).toBeGreaterThanOrEqual(8);
      expect(
        (await graph.boundingBox())!.y - discardBox.y - discardBox.height,
      ).toBeGreaterThanOrEqual(8);
      const accessHelp = page.getByText(t('factory.accessRequirementsHelp'), { exact: true });
      const helpBox = (await accessHelp.boundingBox())!;
      const reviewBox = (await page
        .getByRole('button', { name: t('factory.reviewRequiredAccess'), exact: true })
        .boundingBox())!;
      expect(reviewBox.y - helpBox.y - helpBox.height).toBeGreaterThanOrEqual(8);
      for (const kind of ['goals', 'criteria'] as const) {
        const edit = (await page
          .getByRole('button', {
            name: t(
              kind === 'goals' ? 'factory.editPipelineGoals' : 'factory.editCompletionCriteria',
            ),
            exact: true,
          })
          .boundingBox())!;
        const detach = (await page
          .getByRole('button', {
            name: t(
              kind === 'goals' ? 'factory.detachPipelineGoals' : 'factory.detachCompletionCriteria',
            ),
            exact: true,
          })
          .boundingBox())!;
        expect(
          detach.y > edit.y ? detach.y - edit.y - edit.height : detach.x - edit.x - edit.width,
        ).toBeGreaterThanOrEqual(8);
      }
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
        width,
      );
      await heading.evaluate((element) => element.scrollIntoView({ block: 'start' }));
      await page.evaluate(() => window.scrollBy(0, -80));
      await page.screenshot({ path: info.outputPath('flow-editor-spacing.png') });
    });

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
        if (request.method() !== 'GET' && !isLegacyPolicyRead(request, reference.project_id))
          mutations.push(request.url());
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
