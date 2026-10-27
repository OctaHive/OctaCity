import { expect } from '@playwright/test';
import { writeFile } from 'node:fs/promises';
import { test } from './sourceTest';

test('explicitly launches reviewed Issue requirements and observes configured pipeline disposition', async ({
  page,
}) => {
  test.setTimeout(90_000);
  const project = process.env.OCTACITY_RELEASE_PROJECT_ID!;
  const base = `/api/v1/projects/${project}`;
  const issue = (await (await page.request.get(`${base}/issues`)).json()).items[0];
  await page.goto(`/projects/${project}/issues/${issue.id}`);
  await page.getByRole('button', { name: 'Edit requirements', exact: true }).click();
  await page
    .getByRole('textbox', { name: 'Acceptance criteria', exact: true })
    .fill('Preserve the reviewed native task identity.');
  await page.getByRole('button', { name: 'Review task', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Save task', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Close', exact: true }).click();
  await page.getByRole('button', { name: 'Start pipeline', exact: true }).click();
  await page.getByRole('combobox', { name: 'Factory Configurations', exact: true }).click();
  await page
    .getByRole('option', {
      name: `Factory Configuration · v1 · ${process.env.OCTACITY_RELEASE_CONFIGURATION_ID}`,
      exact: true,
    })
    .click();
  await page.getByRole('textbox', { name: 'Exact revision', exact: true }).fill('a'.repeat(40));
  await page.getByRole('button', { name: 'Review pipeline launch', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Confirm pipeline launch', exact: true })
    .click();
  const workflow = (await (await page.request.get(`${base}/issue-workflow-default`)).json())
    .workflow;
  await expect(async () => {
    const response = await page.request.get(`${base}/issues/${issue.id}`);
    expect(response.status()).toBe(200);
    expect((await response.json()).status_id).toBe(workflow.definition.terminal_status_ids[0]);
  }).toPass({ timeout: 30_000 });
  await expect(async () => {
    const response = await page.request.get(
      `${base}/issues/${issue.id}/synchronization/projections`,
    );
    expect(response.status()).toBe(200);
    const operations = await response.json();
    expect(operations.items).toHaveLength(1);
    expect(operations.items[0].pipeline).not.toBeNull();
    expect(operations.items[0].state).toBe('delivered');
  }).toPass({ timeout: 30_000 });
  const operations = await (
    await page.request.get(`${base}/issues/${issue.id}/synchronization/projections`)
  ).json();
  const executions = await (await page.request.get(`${base}/issues/${issue.id}/executions`)).json();
  expect(executions.items).toHaveLength(1);
  expect(executions.items[0].association.issue_id).toBe(issue.id);
  expect(executions.items[0].association.requirements_version).toBe(issue.version + 1);
  const frozen = executions.items[0];
  await page.goto(`/projects/${project}/issues/${issue.id}`);
  await page.getByRole('button', { name: 'Edit requirements', exact: true }).click();
  await page
    .getByRole('textbox', { name: 'Acceptance criteria', exact: true })
    .fill('Later acceptance belongs to a later execution.');
  await page.getByRole('button', { name: 'Review task', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Save task', exact: true }).click();
  const retained = (await (await page.request.get(`${base}/issues/${issue.id}/executions`)).json())
    .items;
  expect(retained).toHaveLength(1);
  expect(retained[0].intent).toEqual(frozen.intent);
  expect(retained[0].task_digest).toBe(frozen.task_digest);
  expect(retained[0].acceptance_digest).toBe(frozen.acceptance_digest);
  expect(retained[0].association).toEqual(frozen.association);
  const evidence = test.info().outputPath('factory-execution.json');
  await writeFile(
    evidence,
    JSON.stringify({
      project_id: project,
      issue_id: issue.id,
      workflow: { id: issue.workflow_id, version: issue.workflow_version },
      execution: {
        id: frozen.id,
        task_digest: frozen.task_digest,
        acceptance_digest: frozen.acceptance_digest,
        association: frozen.association,
      },
      operation_id: operations.items[0].operation_id,
    }),
  );
  await test
    .info()
    .attach('factory-execution.json', { path: evidence, contentType: 'application/json' });
});
