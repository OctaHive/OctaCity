import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import { issueFixture } from '../fixtures/issues';
import { issueSourceChoice, issueSourceReview } from '../fixtures/issueSources';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';

for (const language of ['en', 'ru']) {
  test(`retains independent pending Reporter feedback at narrow width in ${language}`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    await page.addInitScript(
      ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
      { key: PREFERENCES_KEY, preferences: { ...DEFAULT_PREFERENCES, language } },
    );
    await page.setViewportSize({ width: 390, height: 844 });
    const issue = issueFixture(FIXTURE_IDS.project);
    const sync = issueSourceReview(true);
    const base = `/api/v1/projects/${issue.project_id}/issues/${issue.id}`;
    const commands: string[] = [];
    page.on('request', (request) => {
      if (request.method() !== 'GET') commands.push(request.url());
    });
    await page.route(`**${base}/synchronization`, (route) => route.fulfill({ json: sync }));
    await page.route(`**${base}/synchronization/projections*`, (route) =>
      route.fulfill({
        json: {
          items: [
            {
              operation_id: '11'.repeat(32),
              record_id: '22'.repeat(32),
              issue_version: 1,
              mapping: sync.link!.selection.source.configuration,
              native_correlation: sync.link!.native_correlation,
              workflow: sync.workflow,
              status_id: issue.status_id,
              effects: { status: issue.status_id, add: ['custom-ready'], remove: [], state: null },
              pipeline: null,
              resolves: null,
              accepted_at: 1700000000000,
              state: 'pending',
              failure: null,
              attempt: 0,
              recorded_at: 1700000000000,
              available_at: 1700000001000,
            },
          ],
          next_cursor: null,
          observation: 'live-reporter-v1',
          observed_at: 1700000002000,
        },
      }),
    );
    await page.goto(`/projects/${issue.project_id}/issues/${issue.id}`);
    await page
      .getByRole('button', { name: language === 'en' ? 'Properties' : 'Свойства', exact: true })
      .click();
    const section = page.getByRole('region', {
      name: language === 'en' ? 'Outgoing synchronization' : 'Исходящая синхронизация',
    });
    await expect(
      section.getByText(language === 'en' ? 'Pending delivery' : 'Ожидает доставки', {
        exact: true,
      }),
    ).toBeVisible();
    await expect(section.getByText(/custom-ready/)).toBeVisible();
    const details = section.getByText(
      language === 'en' ? 'Exact synchronization provenance' : 'Точное происхождение синхронизации',
    );
    await details.focus();
    await page.keyboard.press('Enter');
    await expect(section.getByText(/1111111111111111/)).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    ).toBe(true);
    expect(commands).toEqual([]);
  });
}

test('reviews source ownership with keyboard at 390px without provider discovery or workflow edits', async ({
  page,
}) => {
  await installOperatorFixture(page);
  await page.setViewportSize({ width: 390, height: 844 });
  const issue = issueFixture(FIXTURE_IDS.project);
  const base = `/api/v1/projects/${issue.project_id}/issues/${issue.id}`;
  const sync = issueSourceReview(true);
  sync.link!.selection.project_id = issue.project_id;
  const commands: { body: unknown; headers: Record<string, string> }[] = [];
  const unexpected: string[] = [];
  page.on('request', (r) => {
    if (r.method() !== 'GET' && /issue-workflows|candidates|refresh|integrations/u.test(r.url()))
      unexpected.push(r.url());
  });
  await page.route(`**${base}/synchronization`, (route) => route.fulfill({ json: sync }));
  await page.route(`**${base}/source-choices`, (route) =>
    route.fulfill({ json: { issue_version: 1, items: [issueSourceChoice()] } }),
  );
  await page.route(`**${base}/source-link/mappings`, async (route) => {
    const body = route.request().postDataJSON();
    commands.push({ body, headers: route.request().headers() });
    await route.fulfill({
      status: 201,
      json: { disposition: 'applied', resource: { version: 1 }, mapping_reference: null },
    });
  });
  await page.goto(`/projects/${issue.project_id}/issues/${issue.id}`);
  await page.getByRole('button', { name: 'Properties', exact: true }).click();
  const source = page.getByRole('region', { name: 'Source and synchronization' });
  await expect(source.getByText('opaque-7', { exact: true })).toBeVisible();
  await source.getByRole('button', { name: 'Configure source' }).click();
  const authority = source.getByRole('combobox', { name: 'Description authority' });
  await authority.focus();
  await page.keyboard.press('Enter');
  await page.getByRole('option', { name: 'Source owns · pull' }).click();
  const review = source.getByRole('button', { name: 'Review field ownership and mapping' });
  await review.click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('button', { name: 'Publish mapping' })).toBeFocused();
  expect(commands).toHaveLength(0);
  await page.keyboard.press('Escape');
  await expect(review).toBeFocused();
  await review.click();
  await dialog.getByRole('button', { name: 'Publish mapping' }).click();
  await expect(dialog.getByText('Mapping version 1 published.')).toBeVisible();
  expect(commands).toHaveLength(1);
  expect(commands[0]!.headers['if-match']).toBe('"1"');
  expect(commands[0]!.body).toMatchObject({
    expected_mapping: null,
    definition: { fields: { description: 'source_owned_pull' }, status: null, push_enabled: false },
  });
  expect(unexpected).toEqual([]);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
    ),
  ).toBe(true);
});
