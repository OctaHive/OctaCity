import { expect } from '@playwright/test';
import { test } from './sourceTest';

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
    const labels =
      language === 'en'
        ? ([
            'Set up triage',
            'Review triage prerequisites',
            'Retained Project document',
            'Open document for viewing or editing',
            'Document text',
            'Review document publication',
            'Publish document',
            'Close',
          ] as const)
        : ([
            'Настроить триаж',
            'Проверить условия триажа',
            'Сохранённый документ проекта',
            'Открыть документ для просмотра или правки',
            'Текст документа',
            'Проверить публикацию документа',
            'Опубликовать документ',
            'Закрыть',
          ] as const);
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
    await page.goto(`/factory?project_id=${project}`);
    await page.getByRole('button', { name: labels[0], exact: true }).press('Enter');
    await page.getByRole('button', { name: labels[1], exact: true }).press('Enter');
    await page.getByRole('combobox', { name: labels[2], exact: true }).press('Enter');
    await page
      .getByRole('option', { name: /OctaCity project goals/ })
      .first()
      .press('Enter');
    await page.getByRole('button', { name: labels[3], exact: true }).press('Enter');
    const editor = page.getByRole('textbox', { name: labels[4], exact: true });
    await expect(editor).toHaveValue(/Retain step states and results internally/);
    const revised = `# Console goals ${language}\nKeep accepted stage inputs immutable.`;
    await editor.fill(revised);
    await page
      .getByRole('textbox', {
        name: language === 'en' ? 'Document name' : 'Название документа',
        exact: true,
      })
      .fill(`Console revised goals ${language}`);
    await page.getByRole('button', { name: labels[5], exact: true }).press('Enter');
    const published = page.waitForResponse(
      (response) =>
        response.request().method() === 'POST' &&
        new URL(response.url()).pathname === `/api/v1/projects/${project}/input-documents`,
    );
    await page
      .getByRole('dialog')
      .getByRole('button', { name: labels[6], exact: true })
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
      .getByRole('button', { name: labels[7], exact: true })
      .press('Enter');
    expect(
      await page.evaluate(() => JSON.stringify({ ...localStorage, ...sessionStorage })),
    ).not.toContain(revised);
    const runs = await page.request.get(`/api/v1/projects/${project}/factory-runs?limit=10`);
    expect((await runs.json()).items).toEqual([]);
  });
}
