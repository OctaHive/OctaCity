import { expect, type Page } from '@playwright/test';
import type { components } from '../../.generated/api/schema';
import { translate } from '../../src/app/presentation/messages';

/** Actual Console import shared by packaged Source journeys; no route interception. */
export async function importNativeTask(
  page: Page,
  project: string,
  options: {
    title?: string;
    language?: 'en' | 'ru';
    existingIssues?: number;
  } = {},
) {
  const base = `/api/v1/projects/${project}`;
  const title = options.title ?? 'Native task without execution acceptance';
  const t = (key: Parameters<typeof translate>[1]) => translate(options.language ?? 'en', key);
  await page.goto(`/projects/${project}/issues`);
  const importsResponse = page.waitForResponse(
    (response) => new URL(response.url()).pathname === `${base}/issue-source-imports`,
  );
  await page.getByRole('button', { name: t('issueImport.open') }).click();
  expect((await importsResponse).status()).toBe(200);
  await page.getByRole('button', { name: t('issueImport.add') }).click();
  const selector = page.getByRole('combobox', { name: t('factory.sourceConnection') });
  const connection = page.getByRole('option', { name: `Packaged tasks ${project}`, exact: true });
  await selector.click();
  while ((await connection.count()) === 0) {
    await page.keyboard.press('Escape');
    const nextPage = page.waitForResponse(
      (response) => new URL(response.url()).pathname === '/api/v1/integrations',
    );
    await page.getByRole('button', { name: t('factory.moreConnections'), exact: true }).click();
    const response = await nextPage;
    expect(response.status()).toBe(200);
    const connections = await response.json();
    await selector.click();
    // Observe the appended page before deciding whether another page is needed.
    await expect(
      page.getByRole('option', { name: connections.items.at(-1).definition.name, exact: true }),
    ).toBeVisible();
  }
  const choiceResponse = page.waitForResponse((response) =>
    response.url().includes(`${base}/issue-source-choices`),
  );
  await connection.click();
  // Selecting a connection requests the installed Source choices through nginx.
  // The actual server response, including missing composition, remains observable.
  expect((await choiceResponse).status()).toBe(200);
  await page.getByRole('combobox', { name: t('issueSource.profile') }).click();
  await page
    .getByRole('option', { name: 'owner/tickets · github-issues / v2', exact: true })
    .click();
  await page.getByRole('button', { name: t('issueImport.preview'), exact: true }).click();
  await expect(page.getByRole('region', { name: t('issueImport.previewTitle') })).toContainText(
    title,
  );
  expect((await (await page.request.get(`${base}/issues`)).json()).items).toHaveLength(
    options.existingIssues ?? 0,
  );
  await page.getByRole('checkbox', { name: t('issueImport.enabled') }).check();
  await page.getByRole('button', { name: t('issueImport.review') }).click();
  const review = page.getByRole('dialog', { name: t('issueImport.publish') });
  await review.getByRole('button', { name: t('issueImport.confirm') }).click();
  await expect(review).toContainText(t('issueImport.published'));
  await page.keyboard.press('Escape');
  await expect(async () => {
    await page.getByRole('button', { name: t('issueImport.refreshTasks') }).click();
    await expect(page.getByRole('link', { name: title })).toHaveCount(1);
  }).toPass({ timeout: 30_000 });
  await page.getByRole('link', { name: title }).click();
  await expect(page.getByRole('heading', { name: title })).toBeVisible();
  const canonicalPath = new URL(page.url()).pathname;
  const list: components['schemas']['IssueSummaryPage'] = await (
    await page.request.get(`${base}/issues`)
  ).json();
  expect(list.items).toHaveLength((options.existingIssues ?? 0) + 1);
  const issue = list.items.find((item) => canonicalPath.includes(item.id))!;
  expect(canonicalPath).toContain(issue.id);
  return issue;
}

/** Publish through the existing canvas, then separately review its default selection. */
export async function publishWorkflow(
  page: Page,
  project: string,
  name: string,
  initial: string,
  terminal: string,
  reversible = false,
  pipeline = false,
) {
  await page.goto(`/projects/${project}/issue-workflows`);
  await page.getByRole('button', { name: 'Create workflow', exact: true }).click();
  const draft = page.getByRole('region', { name: 'Workflow draft' });
  const inspector = draft.getByRole('complementary', { name: 'Workflow properties' });
  await inspector.getByRole('textbox', { name: 'Workflow name' }).fill(name);
  await draft.getByRole('button', { name: 'Add status', exact: true }).click();
  await inspector.getByRole('textbox', { name: 'Status name' }).fill(initial);
  await draft.getByRole('button', { name: 'Add status', exact: true }).click();
  await inspector.getByRole('textbox', { name: 'Status name' }).fill(terminal);
  await inspector.getByRole('checkbox', { name: 'Terminal status' }).check();
  await draft.getByRole('button', { name: `Connect from ${initial}`, exact: true }).focus();
  await page.keyboard.press('Enter');
  await inspector.getByRole('button', { name: 'Connect statuses', exact: true }).click();
  await inspector.getByRole('textbox', { name: 'Transition name' }).fill('Authorize disposition');
  await inspector.getByRole('checkbox', { name: 'Manual move' }).check();
  if (pipeline) await inspector.getByRole('checkbox', { name: 'Pipeline action' }).check();
  if (reversible) {
    await draft.getByRole('button', { name: `Connect from ${terminal}`, exact: true }).focus();
    await page.keyboard.press('Enter');
    await inspector.getByRole('button', { name: 'Connect statuses', exact: true }).click();
    await inspector.getByRole('textbox', { name: 'Transition name' }).fill('Reopen discussion');
    await inspector.getByRole('checkbox', { name: 'Manual move' }).check();
    await inspector.getByRole('checkbox', { name: 'Source synchronization' }).check();
  }
  await page.getByRole('button', { name: 'Validate draft', exact: true }).click();
  await page.getByRole('button', { name: 'Review publication', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText(`${initial} → ${terminal}`);
  await dialog.getByRole('button', { name: 'Publish version', exact: true }).click();
  await expect(dialog).toContainText('Published version 1.');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await page.getByRole('button', { name: `Open ${name}`, exact: true }).click();
  await page.getByRole('button', { name: 'Use for new tasks', exact: true }).click();
  await expect(dialog).toContainText(`${name} · Version 1`);
  await dialog.getByRole('button', { name: 'Select default', exact: true }).click();
  await expect(dialog).toContainText('Default selected for new tasks.');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  const selected: components['schemas']['IssueWorkflowDefault'] = await (
    await page.request.get(`/api/v1/projects/${project}/issue-workflow-default`)
  ).json();
  expect(selected.workflow.definition.name).toBe(name);
  return selected.workflow;
}

/** Explicitly reviewed native state push; publication itself authorizes no effect. */
export async function reviewStatusPush(page: Page, terminal: string) {
  await page.getByRole('button', { name: 'Configure source', exact: true }).click();
  const mapping = page.getByRole('region', {
    name: 'Field ownership and native mapping',
    exact: true,
  });
  await mapping.getByRole('combobox', { name: 'Status authority', exact: true }).click();
  await expect(
    page.getByRole('option', { name: 'OctaCity owns · push', exact: true }),
  ).toBeVisible();
  await page.getByRole('option', { name: 'OctaCity owns · push', exact: true }).click();
  await mapping.getByRole('button', { name: 'Add status mapping', exact: true }).click();
  await mapping.getByRole('combobox', { name: 'Logical status', exact: true }).click();
  await page.getByRole('option', { name: terminal, exact: true }).click();
  await mapping.getByRole('combobox', { name: 'Resulting task state', exact: true }).click();
  await page.getByRole('option', { name: 'closed', exact: true }).click();
  await mapping
    .getByRole('checkbox', { name: 'Enable outbound status updates', exact: true })
    .check();
  await mapping
    .getByRole('button', { name: 'Review field ownership and mapping', exact: true })
    .click();
  const review = page.getByRole('dialog');
  await expect(review).toContainText(terminal);
  await review.getByRole('button', { name: 'Publish mapping', exact: true }).click();
  await expect(review).toContainText('Mapping version 1 published.');
}
