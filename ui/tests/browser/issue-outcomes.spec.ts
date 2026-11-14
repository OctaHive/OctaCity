import { expect, test } from '@playwright/test';
import type { components } from '../../.generated/api/schema';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';
import { reviewFixture } from '../fixtures/issueCodeReviews';
import { issueFixture } from '../fixtures/issues';
import { FIXTURE_IDS, installOperatorFixture } from './operatorFixture';

const issue = issueFixture(FIXTURE_IDS.project);
const issueUrl = `/projects/${issue.project_id}/issues/${issue.id}`;
const runId = '44000000-0000-4000-8000-000000000001';
const newerRun = '44000000-0000-4000-8000-000000000002';
const triage: components['schemas']['IssueOutcomeCard'] = {
  slot: 'triage',
  run_id: runId,
  conclusion: 'The checkout change fits the project. The duplicate search could not run.',
  presentation: {
    kind: 'triage',
    classification: { status: 'recommended' },
    decision: 'eligible',
    duplicate_coverage: 'unknown',
    recommended_priority: 2,
    recommended_size: 'small',
    component: 'console',
  },
  links: [],
};

test('links to a confirmed pull request on browsers without URL.parse and keeps the original Run accessible', async ({
  page,
}) => {
  await installOperatorFixture(page);
  await page.addInitScript(() => Reflect.deleteProperty(URL, 'parse'));
  const confirmed = 'https://forge.example/team/repository/pull/23';
  await page.route(`**/api/v1${issueUrl}/outcomes`, (route) =>
    route.fulfill({
      json: {
        items: [
          { ...triage, conclusion: 'Unverified URL: https://untrusted.example/pull/7' },
          {
            slot: 'publication',
            run_id: newerRun,
            conclusion: 'Published for manual merge.',
            presentation: { kind: 'publication' },
            links: [
              { kind: 'pull_request', url: confirmed },
              { kind: 'pull_request', url: 'javascript:alert(1)' },
            ],
          },
        ],
      },
    }),
  );
  await page.goto(issueUrl);
  const cards = page.getByRole('region', { name: 'Outcomes' });
  const card = cards.getByRole('article', { name: 'Pull request' });
  await expect(card.getByText('Published for manual merge.')).toBeVisible();
  await expect(card.getByRole('link', { name: 'Open pull request' })).toHaveAttribute(
    'href',
    confirmed,
  );
  await expect(card.getByRole('link', { name: 'Open Run' })).toHaveAttribute(
    'href',
    `/factory/${newerRun}`,
  );
  await expect(cards.locator('a[href*="untrusted.example"], a[href^="javascript:"]')).toHaveCount(
    0,
  );
});

for (const language of ['en', 'ru'] as const) {
  test(`shows current triage advice on the task at phone width in ${language}`, async ({
    page,
  }) => {
    const ru = language === 'ru';
    await installOperatorFixture(page);
    await page.addInitScript(
      ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
      { key: PREFERENCES_KEY, preferences: { ...DEFAULT_PREFERENCES, language } },
    );
    await page.setViewportSize({ width: 390, height: 844 });
    let card = triage;
    const requests: string[] = [];
    await page.route(`**/api/v1${issueUrl}/outcomes`, (route) => {
      requests.push(route.request().method());
      return route.fulfill({ json: { items: [card] } });
    });

    await page.goto(issueUrl);
    const outcomes = page.getByRole('region', { name: ru ? 'Результаты работы' : 'Outcomes' });
    const current = outcomes.getByRole('article', { name: ru ? 'Триаж' : 'Triage' });
    await expect(current.getByText(triage.conclusion)).toBeVisible();
    await expect(current.getByText(ru ? 'Допустимо' : 'Eligible', { exact: true })).toBeVisible();
    await expect(
      current.getByText(ru ? 'Покрытие неизвестно' : 'Coverage unknown', { exact: true }),
    ).toBeVisible();
    await expect(
      current.getByText(ru ? 'Рекомендуемый приоритет' : 'Recommended priority', { exact: true }),
    ).toBeVisible();
    await expect(current.getByText('High', { exact: true })).toBeVisible();
    await expect(current.getByText(ru ? 'Малый' : 'Small', { exact: true })).toBeVisible();
    await expect(current.getByText(ru ? 'Консоль' : 'Console', { exact: true })).toBeVisible();

    const run = current.getByRole('link', { name: ru ? 'Открыть Run' : 'Open Run' });
    await expect(run).toHaveAttribute('href', `/factory/${runId}`);
    card = {
      ...triage,
      run_id: newerRun,
      conclusion: 'Updated triage conclusion',
      presentation: {
        kind: 'triage',
        classification: { status: 'recommended' },
        decision: 'needs_input',
        duplicate_coverage: 'not_checked',
        component: 'duplicate',
        recommended_size: 'approved',
      },
    };
    await page.getByRole('button', { name: ru ? 'Обновить задачу' : 'Refresh task' }).click();
    await expect(current.getByText(card.conclusion)).toBeVisible();
    await expect(
      current.getByText(ru ? 'Поиск не подтверждён' : 'Search not verified', { exact: true }),
    ).toBeVisible();
    await expect(current.getByText('duplicate', { exact: true })).toBeVisible();
    await expect(current.getByText('approved', { exact: true })).toBeVisible();
    await expect(outcomes.getByRole('article')).toHaveCount(1);
    await expect(outcomes.getByText(triage.conclusion)).toHaveCount(0);
    await expect(run).toHaveAttribute('href', `/factory/${newerRun}`);
    await run.focus();
    await page.keyboard.press('Enter');
    await expect(page).toHaveURL(new RegExp(`/factory/${newerRun}$`));
    expect(requests).toEqual(['GET', 'GET']);
  });
}

for (const language of ['en', 'ru'] as const) {
  test(`opens exact retained outcome resources by keyboard in ${language}`, async ({ page }) => {
    const ru = language === 'ru';
    await installOperatorFixture(page);
    await page.addInitScript(
      ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
      { key: PREFERENCES_KEY, preferences: { ...DEFAULT_PREFERENCES, language } },
    );
    await page.setViewportSize({ width: 390, height: 844 });
    const documentId = '55000000-0000-4000-8000-000000000001';
    const review = reviewFixture(issue.project_id);
    const items: components['schemas']['IssueOutcomeCard'][] = [
      {
        slot: 'specifications',
        run_id: runId,
        conclusion: 'Checkout specifications are ready.',
        presentation: { kind: 'specifications' },
        links: [
          { kind: 'document', artifact_id: documentId, title: 'Checkout behavior' },
          { kind: 'unavailable', title: 'Expired draft' },
        ],
      },
      {
        slot: 'review',
        run_id: runId,
        conclusion: 'The checkout change needs revision.',
        presentation: { kind: 'review', verdict: 'changes_required' },
        links: [{ kind: 'review', review_id: review.association.id, title: 'Checkout review' }],
      },
      {
        slot: 'followup',
        run_id: runId,
        conclusion: '<img src="private"> Follow-up recorded.',
        presentation: { kind: 'custom', title: 'Follow-up' },
        links: [],
      },
    ];
    const requests: string[] = [];
    const writes: string[] = [];
    page.on('request', (request) => {
      if (request.method() !== 'GET') writes.push(request.url());
    });
    await page.route(`**/api/v1${issueUrl}/outcomes`, (route) =>
      route.fulfill({ json: { items } }),
    );
    await page.route(
      `**/api/v1/projects/${issue.project_id}/input-documents/${documentId}`,
      (route) => {
        requests.push('document');
        return route.fulfill({
          json: {
            project_id: issue.project_id,
            artifact: { artifact_id: documentId, content_digest: 'd'.repeat(64), encoded_size: 32 },
            expires_at: 4102444800000,
            content: 'Exact retained checkout contract',
          },
        });
      },
    );
    await page.route(
      `**/api/v1${issueUrl}/code-reviews/${review.association.id}/observations?*`,
      (route) => {
        requests.push('review');
        return route.fulfill({
          json: {
            association: review.association,
            availability: review.availability,
            next_cursor: null,
            items: [
              {
                id: 'a'.repeat(64),
                association_id: review.association.id,
                sequence: 1,
                observation: {
                  ...review.observation!,
                  author: 'Exact retained checkout reviewer',
                },
              },
            ],
          },
        });
      },
    );
    await page.goto(issueUrl);
    const outcomes = page.getByRole('region', { name: ru ? 'Результаты работы' : 'Outcomes' });
    const specifications = outcomes.getByRole('article', {
      name: ru ? 'Спецификации' : 'Specifications',
    });
    const codeReview = outcomes.getByRole('article', { name: ru ? 'Ревью кода' : 'Code review' });
    await expect(specifications.getByText(items[0]!.conclusion)).toBeVisible();
    await expect(codeReview.getByText(ru ? 'Нужны изменения' : 'Changes required')).toBeVisible();
    const followup = outcomes.getByRole('article', { name: 'Follow-up' });
    await expect(followup.getByText(items[2]!.conclusion)).toBeVisible();
    await expect(followup.locator('img')).toHaveCount(0);
    await expect(
      specifications.getByText(ru ? 'Недоступно' : 'Unavailable', { exact: true }),
    ).toBeVisible();
    await expect(specifications.getByRole('link', { name: 'Expired draft' })).toHaveCount(0);
    expect(requests).toEqual([]);

    await specifications.getByRole('link', { name: 'Checkout behavior' }).focus();
    await page.keyboard.press('Enter');
    await expect(outcomes.getByText('Exact retained checkout contract')).toBeVisible();
    await expect(outcomes.getByRole('heading', { name: 'Checkout behavior' })).toBeFocused();
    await codeReview.getByRole('link', { name: 'Checkout review' }).focus();
    await page.keyboard.press('Enter');
    await expect(outcomes.getByText('Exact retained checkout reviewer')).toBeVisible();
    await expect(outcomes.getByRole('heading', { name: 'Checkout review' })).toBeFocused();
    await codeReview.getByRole('link', { name: ru ? 'Закрыть' : 'Close', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect(codeReview.getByRole('link', { name: 'Checkout review' })).toBeFocused();
    expect(requests).toEqual(['document', 'review']);
    expect(writes).toEqual([]);
    // The scrollbar gutter can reduce the available width below the viewport width.
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      ),
    ).toBeLessThanOrEqual(0);
  });
}

test('opens linked Build and Work artifacts through their exact read owners', async ({ page }) => {
  await installOperatorFixture(page);
  const workId = '66000000-0000-4000-8000-000000000001';
  const workArtifact = '77000000-0000-4000-8000-000000000001';
  const buildArtifact = '77000000-0000-4000-8000-000000000002';
  const item: components['schemas']['IssueOutcomeCard'] = {
    slot: 'evidence',
    run_id: runId,
    conclusion: 'Evidence retained.',
    presentation: { kind: 'custom', title: 'Evidence' },
    links: [
      { kind: 'work_artifact', work_id: workId, artifact_id: workArtifact, title: 'Work evidence' },
      { kind: 'artifact', artifact_id: buildArtifact, title: 'Build evidence' },
    ],
  };
  const reads: string[] = [];
  await page.route(`**/api/v1${issueUrl}/outcomes`, (route) =>
    route.fulfill({ json: { items: [item] } }),
  );
  await page.route(`**/api/v1/artifacts/${buildArtifact}`, (route) => {
    reads.push('build');
    return route.fulfill({
      json: {
        id: buildArtifact,
        name: 'Exact Build evidence',
        build_id: FIXTURE_IDS.build,
        media_type: 'text/plain',
        size_bytes: 42,
      },
    });
  });
  await page.route(`**/api/v1${issueUrl}/results/artifacts/${workArtifact}/content?*`, (route) => {
    const query = new URL(route.request().url()).searchParams;
    expect(query.get('work_id')).toBe(workId);
    expect(query.get('mode')).toBe('preview');
    reads.push('work');
    return route.fulfill({ contentType: 'text/plain', body: 'Exact Work evidence' });
  });

  await page.goto(issueUrl);
  const card = page.getByRole('article', { name: 'Evidence', exact: true });
  await expect(card.getByText(item.conclusion)).toBeVisible();
  expect(reads).toEqual([]);
  await card.getByRole('link', { name: 'Work evidence' }).click();
  await card.getByRole('button', { name: 'Preview', exact: true }).click();
  await expect(card.getByText('Exact Work evidence')).toBeVisible();
  await card.getByRole('link', { name: 'Build evidence' }).click();
  await expect(card.getByText('Exact Build evidence')).toBeVisible();
  await expect(card.getByRole('link', { name: 'Open Build' })).toHaveAttribute(
    'href',
    `/builds/${FIXTURE_IDS.build}`,
  );
  expect(reads).toEqual(['work', 'build']);
});

test('leaves the ordinary Overview usable when no result is eligible', async ({ page }) => {
  await installOperatorFixture(page);
  const loaded = page.waitForResponse(
    (response) => new URL(response.url()).pathname === `/api/v1${issueUrl}/outcomes`,
  );
  await page.goto(issueUrl);
  expect(await (await loaded).json()).toEqual({ items: [] });
  await expect(page.getByRole('heading', { name: /Investigate checkout/ })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Outcomes' })).toHaveCount(0);
});

test('separates committed triage assignments from recommendations and conflicts', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const applied = {
    status: 'applied',
    priority: { value: 2, current: false },
    size_label: { name: 'Size small', current: true },
  };
  let classification: object = applied;
  await page.route(`**/api/v1${issueUrl}/outcomes`, (route) =>
    route.fulfill({
      json: {
        items: [
          {
            ...triage,
            presentation: { ...triage.presentation, classification },
          },
        ],
      },
    }),
  );
  await page.goto(issueUrl);
  const card = page.getByRole('article', { name: 'Triage' });
  await expect(card.getByText('Applied to task', { exact: true })).toBeVisible();
  await expect(card.getByText('Applied priority', { exact: true })).toBeVisible();
  await expect(card.getByText('Size small', { exact: true })).toBeVisible();
  await expect(
    card.getByText('The task priority has since changed.', { exact: true }),
  ).toBeVisible();
  classification = { status: 'conflict' };
  await page.getByRole('button', { name: 'Refresh task' }).click();
  await expect(
    card.getByText('Classification could not be applied', { exact: true }),
  ).toBeVisible();
  await expect(card.getByText('Recommended priority', { exact: true })).toBeVisible();
  await expect(card.getByText('Applied priority', { exact: true })).toHaveCount(0);
  await expect(card.getByText('Size small', { exact: true })).toHaveCount(0);
});
