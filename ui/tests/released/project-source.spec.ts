import { expect } from '@playwright/test';
import { test } from './sourceTest';
import { writeFile } from 'node:fs/promises';
import { importNativeTask } from './sourceJourney';
import type { components } from '../../.generated/api/schema';
import { translate } from '../../src/app/presentation/messages';

test('imports one native task through the packaged Source-only installation', async ({ page }) => {
  test.setTimeout(60_000);
  const project = process.env.OCTACITY_RELEASE_PROJECT_ID;
  expect(project).toBeTruthy();
  const t = (key: Parameters<typeof translate>[1]) => translate('en', key);
  const base = `/api/v1/projects/${project}`;
  const defaultWorkflow: components['schemas']['IssueWorkflowDefault'] = await (
    await page.request.get(`${base}/issue-workflow-default`)
  ).json();
  let continuationPages = 0;
  page.on('request', (request) => {
    const url = new URL(request.url());
    if (url.pathname === '/api/v1/integrations' && url.searchParams.has('after'))
      continuationPages += 1;
  });
  const issue = await importNativeTask(page, project!);
  expect(continuationPages).toBeGreaterThan(0);
  expect(issue.workflow_id).toBe(defaultWorkflow.workflow.id);
  expect(issue.workflow_version).toBe(defaultWorkflow.workflow.version);
  expect(issue.status_id).toBe(defaultWorkflow.workflow.definition.initial_status_id);
  const detail: components['schemas']['IssueResource'] = await (
    await page.request.get(`${base}/issues/${issue.id}`)
  ).json();
  expect(detail.content.acceptance).toBeNull();
  const synchronization: components['schemas']['IssueSynchronization'] = await (
    await page.request.get(`${base}/issues/${issue.id}/synchronization`)
  ).json();
  expect(synchronization.link?.selection.item.item).toBe('9001');
  expect(synchronization.link?.selection.item.resource).toBe('1234');
  expect(
    (await (await page.request.get(`${base}/issues/${issue.id}/executions`)).json()).items,
  ).toEqual([]);
  await expect(page.getByRole('region', { name: t('issueSource.title') })).toContainText('9001');
  for (let read = 0; read < 3; read += 1) {
    await page.reload();
    await expect(
      page.getByRole('heading', { name: /Native task without execution acceptance/ }),
    ).toBeVisible();
    expect(
      (await (await page.request.get(`${base}/issues`)).json()).items.map(
        (item: { id: string }) => item.id,
      ),
    ).toEqual([issue.id]);
  }
  expect((await (await page.request.get(`${base}/factory-configurations`)).json()).items).toEqual(
    [],
  );
  expect((await (await page.request.get(`${base}/factory-runs`)).json()).items).toEqual([]);
  await page.getByRole('button', { name: 'Configure source', exact: true }).click();
  const mapping = page.getByRole('region', {
    name: 'Field ownership and native mapping',
    exact: true,
  });
  await mapping.getByRole('combobox', { name: 'Status authority', exact: true }).click();
  await expect(page.getByRole('option', { name: 'OctaCity owns · push', exact: true })).toHaveCount(
    0,
  );
  await page.getByRole('option', { name: 'Not synchronized', exact: true }).click();
  await mapping
    .getByRole('button', { name: 'Review field ownership and mapping', exact: true })
    .click();
  const review = page.getByRole('dialog');
  await review.getByRole('button', { name: 'Publish mapping', exact: true }).click();
  await expect(review).toContainText('Mapping version 1 published.');
  await review.getByRole('button', { name: 'Close', exact: true }).click();
  const moves: components['schemas']['IssueTransitionChoices'] = await (
    await page.request.get(`${base}/issues/${issue.id}/transitions`)
  ).json();
  const move = moves.items.find((item) => item.available)!;
  await page.getByRole('link', { name: 'Actions', exact: true }).click();
  await page.getByRole('button', { name: move.transition.name, exact: true }).click();
  await review.getByRole('button', { name: 'Move task', exact: true }).click();
  await expect(review).toContainText('Status recorded in version 2.');
  await review.getByRole('button', { name: 'Close', exact: true }).click();
  const localMove: components['schemas']['IssueResource'] = await (
    await page.request.get(`${base}/issues/${issue.id}`)
  ).json();
  expect(localMove.status_id).toBe(move.transition.to);
  expect(localMove.workflow_id).toBe(issue.workflow_id);
  expect(
    (
      await (
        await page.request.get(`${base}/issues/${issue.id}/synchronization/projections`)
      ).json()
    ).items,
  ).toEqual([]);
  expect((await (await page.request.get(`${base}/factory-runs`)).json()).items).toEqual([]);
  const observation = test.info().outputPath('canonical-issue.json');
  await writeFile(
    observation,
    JSON.stringify({
      project_id: project,
      issue_id: issue.id,
      workflow_id: issue.workflow_id,
      workflow_version: issue.workflow_version,
      initial_status_id: issue.status_id,
      native_item: synchronization.link?.selection.item,
    }),
  );
  await test.info().attach('canonical-issue.json', {
    path: observation,
    contentType: 'application/json',
  });
});
