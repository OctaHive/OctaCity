import { expect, test } from '@playwright/test';
import { installOperatorFixture } from './operatorFixture';
import { configurationFixture } from '../fixtures/factory';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';
import { translate, type MessageKey } from '../../src/app/presentation/messages';

for (const [language, width] of [
  ['en', 1280],
  ['ru', 390],
] as const) {
  test(`${language}: keeps complete published pipeline goals and criteria within the page at ${width}px`, async ({
    page,
  }, info) => {
    await installOperatorFixture(page);
    await page.addInitScript(
      ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
      {
        key: PREFERENCES_KEY,
        preferences: { ...DEFAULT_PREFERENCES, language, explorerOpen: false },
      },
    );
    await page.setViewportSize({ width, height: 900 });
    const t = (key: MessageKey) => translate(language, key);
    const project = configurationFixture().configuration.reference.project_id;
    const publications: { name: string; content: string }[] = [];
    await page.route(`**/api/v1/projects/${project}/input-documents**`, (route) => {
      if (route.request().method() !== 'POST')
        return route.fulfill({ json: { items: [], next_cursor: null } });
      const request = route.request().postDataJSON();
      publications.push(request);
      return route.fulfill({
        json: {
          project_id: project,
          name: request.name,
          published_at: Date.now(),
          expires_at: Date.now() + 3600000,
          artifact: {
            artifact_id: `process-${publications.length}`,
            content_digest: String(publications.length).repeat(64),
            encoded_size: new TextEncoder().encode(request.content).length,
          },
        },
      });
    });
    await page.goto(`/factory?project_id=${project}`);
    await page.getByRole('button', { name: t('factory.addPipeline'), exact: true }).click();
    const purpose = page.getByRole('group', { name: t('factory.processPurpose'), exact: true });
    for (const kind of ['goals', 'criteria'] as const) {
      const label = t(kind === 'goals' ? 'factory.pipelineGoals' : 'factory.completionCriteria');
      const content = `${label}\n\n${'Сохрани решение, найденные сведения и оставшиеся вопросы. '.repeat(12)}\n\nhttps://example.invalid/${'длинная-ссылка-'.repeat(40)}\nКонец документа.`;
      await purpose
        .getByRole('button', {
          name: t(
            kind === 'goals' ? 'factory.editPipelineGoals' : 'factory.editCompletionCriteria',
          ),
          exact: true,
        })
        .click();
      await purpose.getByRole('textbox', { name: label, exact: true }).fill(content);
      await purpose
        .getByRole('button', {
          name: t(
            kind === 'goals'
              ? 'factory.reviewGoalsPublication'
              : 'factory.reviewCriteriaPublication',
          ),
          exact: true,
        })
        .click();
      const dialog = page.getByRole('dialog');
      await dialog
        .getByRole('button', {
          name: t(kind === 'goals' ? 'factory.saveGoalsInput' : 'factory.saveCriteriaInput'),
          exact: true,
        })
        .click();
      await expect(dialog).toContainText(t('factory.processInputSaved'));
      await dialog.getByRole('button', { name: t('common.close'), exact: true }).click();
      const preview = purpose.getByRole('region', { name: label, exact: true }).locator('pre');
      expect(await preview.textContent()).toBe(content);
      expect(
        await preview.evaluate((element) => element.scrollWidth - element.clientWidth),
      ).toBeLessThanOrEqual(1);
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth - innerWidth),
      ).toBeLessThanOrEqual(1);
    }
    expect(publications).toHaveLength(2);
    await purpose.screenshot({ path: info.outputPath('pipeline-purpose.png') });
  });
}
