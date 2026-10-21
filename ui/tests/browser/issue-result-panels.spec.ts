import { resultsFixture } from '../fixtures/issueActivityData';
import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { issueFixture } from '../fixtures/issues';
import {
  record,
  execution,
  subject,
  checksPage,
  artifactsPage,
} from '../fixtures/issueResultPanels';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';

for (const language of ['en', 'ru'] as const)
  for (const width of [390, 1280]) {
    test(`inspects exact checks and safe artifacts with keyboard at ${width}px in ${language}`, async ({
      page,
    }) => {
      await installOperatorFixture(page);
      await page.addInitScript(
        ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
        { key: PREFERENCES_KEY, preferences: { ...DEFAULT_PREFERENCES, language } },
      );
      await page.setViewportSize({ width, height: 844 });
      const issue = issueFixture(FIXTURE_IDS.project);
      const base = `/api/v1/projects/${issue.project_id}/issues/${issue.id}`;
      const kind = width === 390 ? 'candidate' : 'changeset';
      const publication = resultsFixture([
        {
          kind: 'domain_fact',
          record: {
            id: record,
            flow_run_id: 'flow',
            cycle_id: 'cycle',
            node_attempt_id: 'node',
            input_digest: 'ef'.repeat(32),
            observed_at: 1,
            facts:
              kind === 'candidate'
                ? { kind, subject }
                : {
                    kind,
                    subject,
                    id: '77777777-7777-4777-8777-777777777777',
                    node_attempt_id: 'node',
                  },
          },
        },
      ]);
      await page.route(`**${base}/results?*`, (route) =>
        route.fulfill({
          json:
            new URL(route.request().url()).searchParams.get('collection') === 'candidates'
              ? publication
              : resultsFixture(),
        }),
      );
      const contents: URL[] = [];
      const writes: string[] = [];
      page.on('request', (request) => {
        if (request.method() !== 'GET') writes.push(request.url());
      });
      await page.route(`**${base}/executions/old-request`, (route) =>
        route.fulfill({ json: execution }),
      );
      await page.route(`**${base}/results/checks?*`, (route) =>
        route.fulfill({ json: checksPage() }),
      );
      const artifacts = artifactsPage();
      artifacts.next_cursor = null;
      artifacts.items[0]!.metadata!.expires_at = null;
      const screenshot = structuredClone(artifacts.items[0]!);
      screenshot.group = 'screenshot';
      screenshot.artifact.artifact_id = 'screenshot-one';
      screenshot.metadata!.name = 'Safe screenshot';
      screenshot.metadata!.media_type = 'image/png';
      artifacts.items.push(screenshot);
      await page.route(`**${base}/results/artifacts?*`, (route) =>
        route.fulfill({ json: artifacts }),
      );
      await page.route(`**${base}/results/artifacts/*/content?*`, async (route) => {
        const url = new URL(route.request().url());
        contents.push(url);
        if (url.pathname.includes('screenshot-one'))
          await route.fulfill({
            contentType: 'image/png',
            body: Buffer.from(
              'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=',
              'base64',
            ),
          });
        else
          await route.fulfill({
            contentType:
              url.searchParams.get('mode') === 'preview'
                ? 'text/plain; charset=utf-8'
                : 'application/octet-stream',
            body: '<script>inert report</script>\n' + 'long-report-'.repeat(100),
          });
      });
      await page.goto(`/projects/${issue.project_id}/issues/${issue.id}?execution=old-request`);
      if (width === 390)
        await page
          .getByRole('button', { name: language === 'en' ? 'Properties' : 'Свойства', exact: true })
          .click();
      const properties = page.getByRole('complementary', {
        name: language === 'en' ? 'Properties' : 'Свойства',
      });
      const summary = properties.getByRole('link', {
        name: language === 'en' ? 'Artifacts' : 'Артефакты',
        exact: true,
      });
      await expect(summary).toBeVisible({ timeout: 5000 });
      await summary.focus();
      await page.keyboard.press('Enter');
      const section = page.getByRole('region', {
        name: language === 'en' ? 'Execution artifacts' : 'Артефакты выполнения',
      });
      await expect(section.getByRole('heading', { name: 'Original task' })).toBeVisible();
      expect(contents).toEqual([]);
      const input = section
        .getByRole('listitem')
        .filter({ has: page.getByRole('heading', { name: 'Original task', exact: true }) });
      await expect(
        input.getByText(language === 'en' ? 'No expiry' : 'Без срока удаления', { exact: true }),
      ).toBeVisible();
      const preview = input.getByRole('button', {
        name: language === 'en' ? 'Preview' : 'Предпросмотр',
        exact: true,
      });
      await preview.focus();
      await page.keyboard.press('Enter');
      await expect(input.locator('pre')).toContainText('<script>inert report</script>');
      await expect(input.locator('script')).toHaveCount(0);
      await input
        .getByRole('button', { name: language === 'en' ? 'Download' : 'Скачать', exact: true })
        .click();
      const downloadEvent = page.waitForEvent('download');
      await input
        .getByRole('link', { name: language === 'en' ? 'Save file' : 'Сохранить файл' })
        .click();
      expect((await downloadEvent).suggestedFilename()).toBe('Original task');
      const image = section
        .getByRole('listitem')
        .filter({ has: page.getByRole('heading', { name: 'Safe screenshot' }) });
      await image
        .getByRole('button', { name: language === 'en' ? 'Preview' : 'Предпросмотр', exact: true })
        .click();
      await expect(image.getByRole('img', { name: 'Safe screenshot' })).toBeVisible();
      expect(
        await image.getByRole('img').evaluate((element: HTMLImageElement) => element.naturalWidth),
      ).toBe(1);
      await expect(
        section.getByText(language === 'en' ? 'Expired' : 'Срок хранения истёк', { exact: true }),
      ).toBeVisible();
      expect(contents.map((url) => url.searchParams.get('mode'))).toEqual([
        'preview',
        'download',
        'preview',
      ]);
      expect(contents.every((url) => url.searchParams.get('work_id') === 'retained-work')).toBe(
        true,
      );
      const nav = page.getByRole('navigation', {
        name: language === 'en' ? 'Result panels' : 'Разделы результатов',
      });
      await nav
        .getByRole('link', { name: language === 'en' ? 'Checks' : 'Проверки', exact: true })
        .click();
      const candidates = page.getByRole('region', {
        name: language === 'en' ? 'Retained candidates' : 'Сохранённые кандидаты',
      });
      await candidates.getByRole('link', { name: 'historical-head', exact: true }).click();
      const checks = page.getByRole('region', {
        name: language === 'en' ? 'Factory checks' : 'Проверки Factory',
      });
      await expect(
        checks.getByText(language === 'en' ? 'Missing' : 'Отсутствует', { exact: true }),
      ).toBeVisible();
      await expect(checks.getByText('historical-head', { exact: true }).first()).toBeVisible();
      expect(new URL(page.url()).searchParams.get('check_record')).toBe(record);
      await page.reload();
      await expect(checks.getByText('Required unit tests <script>')).toBeVisible();
      expect(writes).toEqual([]);
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
        ),
      ).toBe(true);
      const storage = await page.evaluate(() =>
        JSON.stringify({ local: { ...localStorage }, session: { ...sessionStorage } }),
      );
      expect(storage).not.toContain('inert report');
      expect(storage).not.toContain('blob:');
    });
  }
