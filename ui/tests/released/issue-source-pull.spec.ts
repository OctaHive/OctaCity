import { expect } from '@playwright/test';
import { test } from './sourceTest';
import { writeFile } from 'node:fs/promises';
import type { components } from '../../.generated/api/schema';

test('pulls a source-owned status once, retains its workflow pin and leaves echo and unsynchronized moves local', async ({
  page,
}) => {
  test.setTimeout(60_000);
  const project = process.env.OCTACITY_RELEASE_PROJECT_ID!;
  const base = `/api/v1/projects/${project}`;
  const list: components['schemas']['IssueSummaryPage'] = await (
    await page.request.get(`${base}/issues`)
  ).json();
  const issue = list.items[0]!;
  const path = `${base}/issues/${issue.id}`;
  const workflow: components['schemas']['IssueWorkflowResource'] = await (
    await page.request.get(`${base}/issue-workflows/${issue.workflow_id}/versions/1`)
  ).json();
  await page.goto(`/projects/${project}/issues/${issue.id}`);
  await page.getByRole('button', { name: 'Configure source', exact: true }).click();
  const mapping = page.getByRole('region', {
    name: 'Field ownership and native mapping',
    exact: true,
  });
  await mapping.getByRole('combobox', { name: 'Status authority', exact: true }).click();
  await page.getByRole('option', { name: 'Source owns · pull', exact: true }).click();
  await mapping.getByRole('button', { name: 'Add pull rule', exact: true }).click();
  await mapping.getByRole('combobox', { name: 'Workflow status', exact: true }).click();
  await page.getByRole('option', { name: 'Arrivée / Inbox', exact: true }).click();
  await mapping.getByRole('combobox', { name: 'Native state', exact: true }).click();
  await page.getByRole('option', { name: 'closed', exact: true }).click();
  await mapping
    .getByRole('button', { name: 'Review field ownership and mapping', exact: true })
    .click();
  let review = page.getByRole('dialog');
  await review.getByRole('button', { name: 'Publish mapping', exact: true }).click();
  await expect(review).toContainText('Mapping version 2 published.');
  await review.getByRole('button', { name: 'Close', exact: true }).click();
  await page.reload();
  await expect(async () => {
    const moved: components['schemas']['IssueResource'] = await (
      await page.request.get(path)
    ).json();
    expect(moved.status_id).toBe(workflow.definition.initial_status_id);
    expect(moved.version).toBe(3);
  }).toPass({ timeout: 15_000 });
  await expect(async () => {
    const sync: components['schemas']['IssueSynchronization'] = await (
      await page.request.get(`${path}/synchronization`)
    ).json();
    expect(sync.observation?.status?.condition).toBe('echo');
    expect(sync.mapping?.definition.status?.workflow).toEqual({ id: workflow.id, version: 1 });
  }).toPass({ timeout: 10_000 });
  const history: components['schemas']['IssueStatusHistoryPage'] = await (
    await page.request.get(`${path}/status-history`)
  ).json();
  expect(history.items).toHaveLength(3);
  const sourceMoves = history.items.filter((row) => row.origin === 'source' && row.from !== null);
  expect(sourceMoves).toHaveLength(1);
  expect(sourceMoves[0]?.transition_id).toBe(
    workflow.definition.transitions.find((edge) => edge.name === 'Reopen discussion')?.id,
  );
  expect((await (await page.request.get(path)).json()).version).toBe(3);
  await page.reload();
  await expect(
    page.getByRole('region', { name: 'Source and synchronization', exact: true }),
  ).toContainText('Native echo · no business status change');
  await page.getByRole('button', { name: 'Configure source', exact: true }).click();
  await mapping.getByRole('combobox', { name: 'Status authority', exact: true }).click();
  await page.getByRole('option', { name: 'Not synchronized', exact: true }).click();
  await mapping
    .getByRole('button', { name: 'Review field ownership and mapping', exact: true })
    .click();
  review = page.getByRole('dialog');
  await review.getByRole('button', { name: 'Publish mapping', exact: true }).click();
  await expect(review).toContainText('Mapping version 3 published.');
  await review.getByRole('button', { name: 'Close', exact: true }).click();
  await page.getByRole('link', { name: 'Actions', exact: true }).click();
  await page.getByRole('button', { name: 'Authorize disposition', exact: true }).click();
  await review.getByRole('button', { name: 'Move task', exact: true }).click();
  await expect(review).toContainText('Status recorded in version 4.');
  await review.getByRole('button', { name: 'Close', exact: true }).click();
  const local: components['schemas']['IssueResource'] = await (await page.request.get(path)).json();
  expect(local.workflow_id).toBe(workflow.id);
  expect(local.status_id).toBe(workflow.definition.terminal_status_ids[0]);
  const currentDefault: components['schemas']['IssueWorkflowDefault'] = await (
    await page.request.get(`${base}/issue-workflow-default`)
  ).json();
  expect(currentDefault.workflow.id).not.toBe(workflow.id);
  const projections: components['schemas']['IssueSourceProjectionPage'] = await (
    await page.request.get(`${path}/synchronization/projections`)
  ).json();
  expect(new Set(projections.items.map((row) => row.operation_id)).size).toBe(1);
  expect(projections.items.every((row) => row.state === 'delivered')).toBe(true);
  expect((await (await page.request.get(`${path}/executions`)).json()).items).toEqual([]);
  expect((await (await page.request.get(`${base}/factory-runs`)).json()).items).toEqual([]);
  const observation = test.info().outputPath('status-qualification.json');
  await writeFile(
    observation,
    JSON.stringify({
      project_id: project,
      issue_id: issue.id,
      workflow: { id: workflow.id, version: 1 },
      default_workflow: currentDefault.workflow.id,
      source_transition: sourceMoves[0],
      final_status: local.status_id,
      final_version: local.version,
      delivered_operation: projections.items[0]?.operation_id,
    }),
  );
  await test
    .info()
    .attach('status-qualification.json', { path: observation, contentType: 'application/json' });
});
