import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';
import { translate, type MessageKey } from '../../src/app/presentation/messages';

for (const language of ['en', 'ru'] as const) {
  for (const width of [390, 1280]) {
    test(`reads and edits Project documents in ${language} at ${width}px without cramped controls or implicit writes`, async ({
      page,
    }, testInfo) => {
      await installOperatorFixture(page);
      await page.addInitScript(
        ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
        {
          key: PREFERENCES_KEY,
          preferences: { ...DEFAULT_PREFERENCES, language, explorerOpen: false },
        },
      );
      await page.setViewportSize({ width, height: 900 });
      const t = (key: MessageKey, values = {}) => translate(language, key, values);
      const resource = {
        project_id: FIXTURE_IDS.project,
        name: 'Project goals',
        published_at: 1_700_000_000_000,
        expires_at: 4_000_000_000_000,
        artifact: {
          artifact_id: 'goals-original',
          content_digest: 'a'.repeat(64),
          encoded_size: 512,
        },
      };
      const original =
        '# Project direction\n\nBuild **clear interfaces** for operators.\n\n- Keep step inputs explicit\n- Retain results\n\n```text\n' +
        'long-code-line-'.repeat(35) +
        '\n```\n\n[Unsafe](javascript:alert(1))\n\n![Remote](https://document-tracker.invalid/pixel)\n\n<img src="https://document-tracker.invalid/raw" onerror="alert(1)">';
      const revised = '# Reviewed direction\n\nMake documents comfortable to read and edit.';
      const publications: unknown[] = [];
      const remote: string[] = [];
      await page.route('https://document-tracker.invalid/**', (route) => {
        remote.push(route.request().url());
        return route.abort();
      });
      await page.route(`**/api/v1/projects/${resource.project_id}/input-documents**`, (route) => {
        if (route.request().method() === 'POST') {
          publications.push(route.request().postDataJSON());
          return route.fulfill({
            json: {
              ...resource,
              artifact: {
                ...resource.artifact,
                artifact_id: 'goals-revised',
                content_digest: 'b'.repeat(64),
              },
            },
          });
        }
        const path = new URL(route.request().url()).pathname;
        return route.fulfill({
          json: path.endsWith('/input-documents')
            ? { items: [resource], next_cursor: null }
            : { project_id: resource.project_id, artifact: resource.artifact, content: original },
        });
      });
      await page.goto(`/projects/${resource.project_id}`);
      await page.getByRole('button', { name: t('projectDocument.manage'), exact: true }).click();
      await page
        .getByRole('button', {
          name: t('projectDocument.view', { name: resource.name }),
          exact: true,
        })
        .press('Enter');
      const view = page.getByRole('article', { name: resource.name, exact: true });
      await expect(view.getByRole('heading', { name: 'Project direction' })).toBeVisible();
      await expect(view.locator('strong')).toHaveText('clear interfaces');
      await view.screenshot({ path: testInfo.outputPath('document-view.png') });
      await expect(view.locator('img,script,iframe,a[href^="javascript:"]')).toHaveCount(0);
      expect(remote).toEqual([]);
      await expect(
        page.getByRole('textbox', { name: t('projectDocument.content'), exact: true }),
      ).toHaveCount(0);
      await view
        .getByRole('button', { name: t('projectDocument.edit'), exact: true })
        .press('Enter');
      const name = page.getByRole('textbox', { name: t('projectDocument.name'), exact: true });
      const text = page.getByRole('textbox', { name: t('projectDocument.content'), exact: true });
      await expect(name).toBeFocused();
      await expect(text).toHaveValue(original);
      const nameBox = (await name.boundingBox())!;
      const textBox = (await text.boundingBox())!;
      expect(textBox.width).toBeGreaterThan(width < 600 ? 250 : 600);
      expect(textBox.height).toBeGreaterThanOrEqual(320);
      expect(textBox.y).toBeGreaterThan(nameBox.y + nameBox.height);
      await text.fill(revised);
      await page
        .getByRole('region', { name: t('projectDocument.manage'), exact: true })
        .screenshot({ path: testInfo.outputPath('document-editor.png') });
      await page
        .getByRole('button', { name: t('projectDocument.preview'), exact: true })
        .press('Enter');
      await expect(
        page
          .getByRole('region', { name: t('projectDocument.previewTitle'), exact: true })
          .getByRole('heading', { name: 'Reviewed direction' }),
      ).toBeVisible();
      expect(publications).toEqual([]);
      await page
        .getByRole('button', { name: t('projectDocument.cancel'), exact: true })
        .press('Enter');
      await expect(view.getByRole('heading', { name: 'Project direction' })).toBeVisible();
      await view
        .getByRole('button', { name: t('projectDocument.edit'), exact: true })
        .press('Enter');
      await text.fill(revised);
      await page
        .getByRole('button', { name: t('projectDocument.reviewChanges'), exact: true })
        .press('Enter');
      const dialog = page.getByRole('dialog');
      await dialog
        .getByRole('button', { name: t('projectDocument.publish'), exact: true })
        .press('Enter');
      await expect(dialog).toContainText(t('projectDocument.published'));
      await dialog.getByRole('button', { name: t('common.close'), exact: true }).press('Enter');
      await expect(view.getByRole('heading', { name: 'Reviewed direction' })).toBeVisible();
      expect(publications).toEqual([{ name: resource.name, content: revised }]);
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth - innerWidth),
      ).toBeLessThanOrEqual(1);
      expect(
        await page.evaluate(() => JSON.stringify({ ...localStorage, ...sessionStorage })),
      ).not.toContain(revised);
      expect(remote).toEqual([]);
    });
  }
}
