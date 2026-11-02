import { expect } from '@playwright/test';
import { test } from './sourceTest';
import { translate, type MessageKey } from '../../src/app/presentation/messages';

// Trust only the disposable gateway's selected public key, without modifying
// workstation trust or accepting an unrelated self-signed certificate.
const spki = process.env.OCTACITY_RELEASE_TLS_SPKI;
test.use({
  launchOptions: { args: spki ? [`--ignore-certificate-errors-spki-list=${spki}`] : [] },
});

for (const language of ['en', 'ru'] as const) {
  test(`default stand goals remain readable and publish immutable edits (${language})`, async ({
    page,
  }) => {
    const project = process.env.OCTACITY_RELEASE_PROJECT_ID!;
    const t = (key: MessageKey, values = {}) => translate(language, key, values);
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
    await page.goto(`/projects/${project}`);
    await page
      .getByRole('button', { name: t('projectDocument.manage'), exact: true })
      .press('Enter');
    await page
      .getByRole('button', {
        name: t('projectDocument.view', { name: 'OctaCity project goals' }),
        exact: true,
      })
      .first()
      .press('Enter');
    await page.getByRole('button', { name: t('projectDocument.edit'), exact: true }).press('Enter');
    const editor = page.getByRole('textbox', { name: t('projectDocument.content'), exact: true });
    await expect(editor).toHaveValue(/Retain step states and results internally/);
    const revised = `# Console goals ${language}\nKeep accepted stage inputs immutable.`;
    await editor.fill(revised);
    await page
      .getByRole('textbox', {
        name: language === 'en' ? 'Document name' : 'Название документа',
        exact: true,
      })
      .fill(`Console revised goals ${language}`);
    await page
      .getByRole('button', { name: t('projectDocument.reviewChanges'), exact: true })
      .press('Enter');
    const published = page.waitForResponse(
      (response) =>
        response.request().method() === 'POST' &&
        new URL(response.url()).pathname === `/api/v1/projects/${project}/input-documents`,
    );
    await page
      .getByRole('dialog')
      .getByRole('button', { name: t('projectDocument.publish'), exact: true })
      .press('Enter');
    const response = await published;
    expect(response.ok()).toBe(true);
    const result = await response.json();
    const read = await page.request.get(
      `/api/v1/projects/${project}/input-documents/${result.artifact.artifact_id}`,
    );
    expect((await read.json()).content).toBe(revised);
    await page
      .getByRole('dialog')
      .getByRole('button', { name: t('common.close'), exact: true })
      .press('Enter');
    expect(
      await page.evaluate(() => JSON.stringify({ ...localStorage, ...sessionStorage })),
    ).not.toContain(revised);
    const runs = await page.request.get(`/api/v1/projects/${project}/factory-runs?limit=10`);
    expect((await runs.json()).items).toEqual([]);
  });
}
