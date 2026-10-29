import { expect, test } from '@playwright/test';
import { configurationFixture } from '../fixtures/factory';
import { installOperatorFixture } from './operatorFixture';

test.use({ trace: 'off', screenshot: 'off', video: 'off' });

for (const language of ['en', 'ru'] as const) {
  test(`triage first setup keeps stage documents explicit and private with keyboard at narrow width (${language})`, async ({
    page,
  }, testInfo) => {
    const text =
      language === 'en'
        ? {
            open: 'Set up triage',
            review: 'Review triage prerequisites',
            name: 'Document name',
            content: 'Document text',
            publication: 'Review document publication',
            confirm: 'Publish document',
            close: 'Close',
            project: 'Project analyst',
            analyst: 'Systems analyst',
            goals: 'Project goals',
            none: 'None',
            resolve: 'Resolve reviewed triage',
            discard: 'Close triage setup',
            runtime: 'Configure Runtime',
          }
        : {
            open: 'Настроить триаж',
            review: 'Проверить условия триажа',
            name: 'Название документа',
            content: 'Текст документа',
            publication: 'Проверить публикацию документа',
            confirm: 'Опубликовать документ',
            close: 'Закрыть',
            project: 'Аналитик проекта',
            analyst: 'Системный аналитик',
            goals: 'Цели проекта',
            none: 'Нет',
            resolve: 'Собрать проверенный triage',
            discard: 'Закрыть настройку триажа',
            runtime: 'Настроить Runtime',
          };
    await page.addInitScript(
      (locale) =>
        localStorage.setItem(
          'octacity.console.preferences',
          JSON.stringify({
            version: 2,
            language: locale,
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
    await installOperatorFixture(page);
    const owner = configurationFixture().configuration.reference.project_id;
    const preset = { identity: 'triage', version: 'v1', digest: 'a'.repeat(64) };
    const reference = {
      artifact_id: '12345678-1234-4234-8234-123456789012',
      content_digest: 'b'.repeat(64),
      encoded_size: 32,
    };
    const documents: unknown[] = [];
    const publications: { body: unknown; key: string | undefined }[] = [];
    const resolutions: Record<string, unknown>[] = [];
    const forbidden: string[] = [];
    let buildReads = 0;
    await page.route(`**/api/v1/projects/${owner}/**`, async (route) => {
      const request = route.request();
      const path = new URL(request.url()).pathname;
      if (path.endsWith('/factory-presets/resolve')) {
        resolutions.push(request.postDataJSON());
        await route.fulfill({
          json: {
            preset,
            completion: {
              kind: 'draft',
              project_id: owner,
              preset,
              content: 'Frozen reviewed process criteria',
            },
            diagnostics: [{ path: '/profiles/eligibility', reason: 'required' }],
            flow: null,
            configuration: null,
            input: null,
            dependencies: [],
          },
        });
      } else if (path.endsWith('/factory-presets')) {
        await route.fulfill({
          json: {
            items: [
              {
                reference: preset,
                profile_aliases: ['eligibility', 'classification'],
                parameters: [],
                required_node_kinds: ['reasoning'],
              },
            ],
            next_cursor: null,
          },
        });
      } else if (path.endsWith('/input-documents')) {
        if (request.method() === 'POST') {
          const body = request.postDataJSON();
          publications.push({ body, key: request.headers()['idempotency-key'] });
          const resource = {
            project_id: owner,
            name: body.name,
            artifact: reference,
            published_at: Date.now(),
            expires_at: Date.now() + 86_400_000,
          };
          documents.push(resource);
          await route.fulfill({ status: 201, json: resource });
        } else await route.fulfill({ json: { items: documents, next_cursor: null } });
      } else if (request.method() !== 'GET') {
        forbidden.push(`${request.method()} ${path}`);
        await route.abort();
      } else if (
        ['/factory-roles', '/flow-definitions', '/factory-configurations', '/builds'].some(
          (suffix) => path.endsWith(suffix),
        )
      ) {
        if (path.endsWith('/builds')) buildReads += 1;
        await route.fulfill({ json: { items: [], total: 0, next_cursor: null } });
      } else await route.fallback();
    });
    await page.setViewportSize({ width: language === 'en' ? 390 : 640, height: 900 });
    await page.goto(`/factory?project_id=${owner}`);
    const open = page.getByRole('button', { name: text.open, exact: true });
    await open.focus();
    await page.keyboard.press('Enter');
    await page.getByRole('button', { name: text.review, exact: true }).focus();
    await page.keyboard.press('Enter');
    const project = page.getByRole('region', { name: text.project, exact: true });
    const analyst = page.getByRole('region', { name: text.analyst, exact: true });
    await expect(project).toBeVisible();
    await expect(project.getByText(text.goals, { exact: true })).toBeVisible();
    await expect(project.getByRole('link', { name: text.runtime })).toHaveAttribute(
      'target',
      '_blank',
    );
    const privateDraft = `private-triage-draft-${language}`;
    await page.getByRole('textbox', { name: text.name, exact: true }).fill(privateDraft);
    await page
      .getByRole('textbox', { name: text.content, exact: true })
      .fill('Exact reviewed goals\n');
    const review = page.getByRole('button', { name: text.publication, exact: true });
    await review.focus();
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog', { name: privateDraft });
    await expect(dialog.getByRole('button', { name: text.confirm, exact: true })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(review).toBeFocused();
    expect(publications).toHaveLength(0);
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    await dialog.getByRole('button', { name: text.close, exact: true }).press('Enter');
    expect(publications).toEqual([
      { body: { name: privateDraft, content: 'Exact reviewed goals\n' }, key: expect.any(String) },
    ]);
    const goals = project.getByRole('combobox', { name: text.goals, exact: true });
    await expect(goals).toContainText(text.none);
    await expect(analyst.getByRole('combobox', { name: text.goals, exact: true })).toContainText(
      text.none,
    );
    await goals.focus();
    await page.keyboard.press('Enter');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expect(goals).toContainText(privateDraft);
    await page.getByRole('button', { name: text.resolve, exact: true }).press('Enter');
    await expect
      .poll(() => resolutions.at(-1)?.context)
      .toEqual({
        eligibility: { project_id: owner, goals: reference, rules: null, documents: {} },
      });
    expect(
      await page.evaluate(() => `${JSON.stringify(localStorage)}${JSON.stringify(sessionStorage)}`),
    ).not.toContain(privateDraft);
    expect(forbidden).toEqual([]);
    expect(buildReads).toBe(0);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth),
    ).toBeLessThanOrEqual(0);
    await page.getByRole('textbox', { name: text.name, exact: true }).scrollIntoViewIfNeeded();
    await page.screenshot({ path: testInfo.outputPath(`triage-${language}.png`) });
    await page.getByRole('button', { name: text.discard, exact: true }).press('Enter');
    await expect(open).toBeFocused();
  });
}
