import { expect } from '@playwright/test';
import { test } from './sourceTest';
import { importNativeTask, publishWorkflow, reviewStatusPush } from './sourceJourney';

test('reviews a workflow-pinned native push mapping without execution or an implicit write', async ({
  page,
}) => {
  test.setTimeout(90_000);
  const project = process.env.OCTACITY_RELEASE_PROJECT_ID!;
  const workflow = await publishWorkflow(
    page,
    project,
    'Editorial / 研究',
    'Arrivée / Inbox',
    'Décision / Accepted',
    true,
  );
  const issue = await importNativeTask(page, project);
  expect(issue.workflow_id).toBe(workflow.id);
  const later = await publishWorkflow(
    page,
    project,
    'Future policy',
    'Future inbox',
    'Future disposition',
  );
  expect(later.id).not.toBe(workflow.id);
  await page.goto(`/projects/${project}/issues/${issue.id}`);
  const retained = await (
    await page.request.get(`/api/v1/projects/${project}/issues/${issue.id}`)
  ).json();
  expect(retained.workflow_id).toBe(workflow.id);
  expect(retained.workflow_version).toBe(1);
  await reviewStatusPush(page, 'Décision / Accepted');
});
