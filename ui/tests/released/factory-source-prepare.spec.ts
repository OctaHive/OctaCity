import { expect } from '@playwright/test';
import { test } from './sourceTest';
import { importNativeTask, publishWorkflow, reviewStatusPush } from './sourceJourney';

test('reviews a pipeline transition before separately enabling execution', async ({ page }) => {
  test.setTimeout(90_000);
  const project = process.env.OCTACITY_RELEASE_PROJECT_ID!;
  const workflow = await publishWorkflow(
    page,
    project,
    'Factory editorial / 研究',
    'Arrivée / Inbox',
    'Décision / Accepted',
    false,
    true,
  );
  expect(workflow.definition.transitions[0]!.origins).toContain('pipeline');
  const issue = await importNativeTask(page, project);
  expect(issue.workflow_id).toBe(workflow.id);
  await reviewStatusPush(page, 'Décision / Accepted');
  expect(
    (await (await page.request.get(`/api/v1/projects/${project}/factory-runs`)).json()).items,
  ).toEqual([]);
});
