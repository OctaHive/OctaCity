import { expect, test, type Page } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import {
  factoryRoleFixture,
  factoryProfileConfigurationFixture,
  pendingProfileFixture,
} from '../fixtures/factoryRoles';
import { enMessages } from '../../src/app/presentation/messages.en';
import { ruMessages } from '../../src/app/presentation/messages.ru';

test.use({ trace: 'off', screenshot: 'off', video: 'off' });
test('AI-agent avatar upload is previewed and retained with its identity and instructions', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const { role } = factoryRoleFixture();
  const owner = role.reference.project_id;
  let published: unknown;
  await page.route('**/api/v1/projects/*/factory-**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/factory-roles/validate')) {
      await route.fulfill({
        json: { reference: role.reference, definition: route.request().postDataJSON().definition },
      });
    } else if (path.endsWith('/factory-role-publications')) {
      published = route.request().postDataJSON();
      await route.fulfill({
        json: { disposition: 'applied', role, profiles: [], published_at: 100 },
      });
    } else await route.fulfill({ json: { items: [], total: 0, next_cursor: null } });
  });
  await page.goto(`/factory?project_id=${owner}`);
  await page.getByRole('link', { name: 'AI agents', exact: true }).click();
  await page.getByRole('button', { name: 'Create AI agent', exact: true }).click();
  await page.getByRole('textbox', { name: 'Agent name', exact: true }).fill('Reviewer');
  await page
    .getByRole('textbox', { name: 'Description', exact: true })
    .fill('Checks the implementation');
  await page
    .getByRole('textbox', { name: 'Shared instruction 1', exact: true })
    .fill('Review the change.');
  await page.getByLabel('Avatar', { exact: true }).setInputFiles({
    name: 'avatar.png',
    mimeType: 'image/png',
    buffer: Buffer.from(
      'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgaPj/HwAEggJ/59habAAAAABJRU5ErkJggg==',
      'base64',
    ),
  });
  const preview = page.getByRole('img', { name: 'Agent avatar', exact: true });
  await expect(preview).toBeVisible();
  await expect
    .poll(() => preview.evaluate((image: HTMLImageElement) => image.naturalWidth))
    .toBeGreaterThan(0);
  await page.getByRole('button', { name: 'Review AI agent', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Confirm publish', exact: true })
    .click();
  await expect(page.getByRole('dialog').getByText('Immutable version published.')).toBeVisible();
  expect(published).toMatchObject({
    role: {
      definition: {
        name: 'Reviewer',
        description: 'Checks the implementation',
        instructions: ['Review the change.'],
        avatar: expect.stringMatching(/^data:image\/png;base64,/),
      },
    },
    profiles: [],
  });
});
async function preferences(page: Page, language: string) {
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
  await page.setViewportSize({ width: 390, height: 844 });
}
for (const language of ['en', 'ru'] as const) {
  const m = language === 'en' ? enMessages : ruMessages;
  test(`${language}: AI-agent authoring imports a shared skill in the open draft`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    await preferences(page, language);
    const { role, profile, choice, capability } = factoryRoleFixture();
    const owner = role.reference.project_id;
    const base = `/api/v1/projects/${owner}`;
    let published: unknown;
    let retainedRole = role;
    const pageOf = (items: unknown[]) => ({ items, total: items.length, next_cursor: null });
    await page.route('**/api/v1/projects/*/factory-**', async (route) => {
      const path = new URL(route.request().url()).pathname;
      let body: unknown;
      if (path === `${base}/factory-roles`)
        body = pageOf([
          {
            reference: retainedRole.reference,
            name: retainedRole.definition.name,
            icon: 'bot',
            concurrency_limit: 2,
            template: null,
          },
        ]);
      else if (
        path === `${base}/factory-role-templates` ||
        (path === `${base}/factory-skills` && route.request().method() === 'GET')
      )
        body = pageOf([]);
      else if (path === `${base}/factory-roles/${role.reference.id}/profiles`)
        body = pageOf([
          {
            reference: profile.reference,
            role: profile.role,
            name: 'default',
            toolchain: profile.definition.toolchain,
            template: null,
          },
        ]);
      else if (path.startsWith(`${base}/factory-roles/${role.reference.id}/versions/`))
        body = retainedRole;
      else if (path.includes('/factory-role-profiles/') && path.includes('/versions/'))
        body = profile;
      else if (path === `${base}/factory-profile-capabilities`) body = pageOf([choice]);
      else if (path.startsWith(`${base}/factory-profile-capabilities/`)) body = capability;
      else if (path === `${base}/factory-roles/validate`)
        body = { ...role, definition: route.request().postDataJSON().definition };
      else if (path === `${base}/factory-skills`) {
        const request = route.request().postDataJSON();
        expect(request.files.map((item: { path: string }) => item.path)).toEqual(['SKILL.md']);
        body = {
          bundle: { identity: request.identity, version: 'v1', digest: 'a'.repeat(64) },
          published_at: 100,
        };
      } else if (path === `${base}/factory-role-publications`) {
        const request = route.request().postDataJSON();
        published = request;
        retainedRole = {
          ...role,
          reference: { ...role.reference, version: 2 },
          definition: request.role.definition,
        };
        body = { disposition: 'applied', role: retainedRole, profiles: [], published_at: 100 };
      } else return route.fallback();
      await route.fulfill({ json: body });
    });
    await page.goto(`/factory?project_id=${owner}`);
    await page.getByRole('link', { name: m['factory.agents'], exact: true }).click();
    await expect(
      page.getByRole('button', { name: /Enroll agent|Зарегистрировать агента/ }),
    ).toHaveCount(0);
    await page.getByRole('button', { name: 'Code specialist', exact: true }).click();
    await expect(page.getByRole('img', { name: 'bot', exact: true })).toBeVisible();
    await page.getByRole('button', { name: m['factory.editAgent'] }).click();
    const name = page.getByRole('textbox', { name: m['factory.agentName'], exact: true });
    await name.fill('private agent draft');
    await page
      .getByLabel(m['factory.importFolder'], { exact: true })
      .setInputFiles('tests/fixtures/skill-folder');
    await page.getByRole('button', { name: m['factory.reviewImport'] }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByRole('button', { name: m['factory.confirmImport'] })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(name).toHaveValue('private agent draft');
    await expect(page.getByRole('button', { name: m['factory.reviewImport'] })).toBeFocused();
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    await expect(dialog.getByText(m['factory.skillImported'])).toBeVisible();
    await dialog.getByRole('button', { name: m['common.close'], exact: true }).click();
    await expect(page.getByRole('checkbox', { name: /skill-folder.*v1/ })).toBeChecked();
    await page.getByRole('button', { name: m['factory.reviewAgent'] }).click();
    await dialog.getByRole('button', { name: m['factory.confirmPublish'] }).click();
    await expect(dialog.getByText(m['factory.published'])).toBeVisible();
    expect(published).toMatchObject({
      role: {
        id: role.reference.id,
        expected_current_version: 1,
        definition: {
          name: 'private agent draft',
          skills: {
            'skill-folder': { identity: 'skill-folder', version: 'v1', digest: 'a'.repeat(64) },
          },
        },
      },
      profiles: [],
    });
    await dialog.getByRole('button', { name: m['common.close'], exact: true }).click();
    await page.reload();
    await expect(
      page.getByRole('heading', { name: m['section.aiSettings'], exact: true }),
    ).toBeVisible();
    await page.getByRole('button', { name: 'private agent draft', exact: true }).click();
    await expect(
      page.getByRole('button', {
        name: m['factory.viewSkill'].replace('{name}', 'skill-folder · v1'),
      }),
    ).toBeVisible();
    expect(page.url()).not.toContain('private');
    expect(
      await page.evaluate(() => JSON.stringify(localStorage) + JSON.stringify(sessionStorage)),
    ).not.toContain('private agent draft');
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  });
  test(`${language}: pending profile confirmation records intent and refreshes retained history`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    await preferences(page, language);
    const selection = pendingProfileFixture();
    let accepted = false;
    let calls = 0;
    await page.route(
      `**/api/v1/factory-runs/${FIXTURE_IDS.factoryRun}/profile-selections**`,
      async (route) => {
        if (route.request().method() === 'POST') {
          calls++;
          expect(route.request().headers()['if-match']).toBe('"2"');
          expect(route.request().postDataJSON()).toEqual({
            profile: selection.eligible_candidates[0]!.profile,
          });
          accepted = true;
          await route.fulfill({
            json: {
              run_id: FIXTURE_IDS.factoryRun,
              state: 'waiting',
              version: 3,
              cancellation_requested: false,
              disposition: 'applied',
            },
          });
        } else
          await route.fulfill({
            json: {
              items: [
                {
                  kind: 'profile_selection',
                  record: {
                    ...selection,
                    ...(accepted
                      ? {
                          pending_control: {
                            id: '4'.repeat(64),
                            profile: selection.eligible_candidates[0]!.profile,
                            version: 3,
                            recorded_at: 2,
                          },
                        }
                      : {}),
                  },
                },
              ],
              next_cursor: null,
            },
          });
      },
    );
    const config = factoryProfileConfigurationFixture();
    await page.route(
      `**/api/v1/factory-configurations/${config.configuration.reference.id}/versions/1`,
      (route) => route.fulfill({ json: config }),
    );
    await page.goto(`/factory/${FIXTURE_IDS.factoryRun}?diagnostic=profile-selections`);
    const review = page.getByRole('button', { name: m['factory.reviewProfileChoice'] });
    await review.focus();
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByRole('button', { name: m['factory.confirmCommand'] })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(review).toBeFocused();
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    await expect(dialog.getByText(m['factory.commandRecorded'])).toBeVisible();
    expect(calls).toBe(1);
    await dialog.getByRole('button', { name: m['common.close'], exact: true }).click();
    await expect(review).toHaveCount(0);
    await page.reload();
    await expect(review).toHaveCount(0);
    await page.getByRole('button', { name: new RegExp(selection.id) }).click();
    await expect(page.getByText(selection.input_digest, { exact: true }).first()).toBeVisible();
    expect(page.url()).not.toContain(selection.eligible_candidates[0]!.profile.id);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(
      true,
    );
  });
}
