import { expect } from '@playwright/test';
import { test } from './sourceTest';
import type { components } from '../../.generated/api/schema';

test('delivers the reviewed manual status effect through the native adapter without Work or Run', async ({
  page,
}) => {
  test.setTimeout(60_000);
  const project = process.env.OCTACITY_RELEASE_PROJECT_ID!;
  const base = `/api/v1/projects/${project}`;
  const list: components['schemas']['IssueSummaryPage'] = await (
    await page.request.get(`${base}/issues`)
  ).json();
  const issue = list.items[0]!;
  await page.goto(`/projects/${project}/issues/${issue.id}`);
  const projections = `${base}/issues/${issue.id}/synchronization/projections`;
  expect((await (await page.request.get(projections)).json()).items).toEqual([]);
  await page.getByRole('link', { name: 'Actions', exact: true }).click();
  await page.getByRole('button', { name: 'Authorize disposition', exact: true }).click();
  const review = page.getByRole('dialog', { name: 'Available status moves' });
  await expect(review).toContainText('Arrivée / Inbox → Décision / Accepted');
  await review.getByRole('button', { name: 'Move task', exact: true }).click();
  await expect(review).toContainText('Status recorded in version 2.');
  await review.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(async () => {
    const progress: components['schemas']['IssueSourceProjectionPage'] = await (
      await page.request.get(projections)
    ).json();
    expect(progress.items.some((record) => record.state === 'delivered')).toBe(true);
  }).toPass({ timeout: 25_000 });
  await page.getByRole('link', { name: 'Overview', exact: true }).click();
  await page.reload();
  await expect(page.getByRole('region', { name: 'Outgoing synchronization' })).toContainText(
    'Delivery observed',
  );
  const progress: components['schemas']['IssueSourceProjectionPage'] = await (
    await page.request.get(projections)
  ).json();
  const operations = new Set(progress.items.map((row) => row.operation_id));
  expect(operations.size).toBe(1);
  const delivered = progress.items.find((row) => row.state === 'delivered')!;
  expect(delivered.pipeline).toBeNull();
  expect(delivered.effects).toEqual({
    status: delivered.status_id,
    add: [],
    remove: [],
    state: 'closed',
  });
  expect(delivered.workflow).toEqual({ id: issue.workflow_id, version: issue.workflow_version });
  expect((await (await page.request.get(`${base}/factory-runs`)).json()).items).toEqual([]);
  expect((await (await page.request.get(`${base}/factory-configurations`)).json()).items).toEqual(
    [],
  );
  expect(
    (await (await page.request.get(`${base}/issues/${issue.id}/executions`)).json()).items,
  ).toEqual([]);
});
