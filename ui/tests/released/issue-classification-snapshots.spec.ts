import { expect, type Page } from '@playwright/test';
import { test } from './sourceTest';
import { expectForeignMaterialRejected, reviewIssueSnapshotLaunch } from './issueMaterialChecks';

test('prepares independent Issue snapshot inputs through the packaged Console', async ({
  page,
}) => {
  test.setTimeout(120_000);
  page.setDefaultTimeout(10_000);
  const project = process.env.OCTACITY_RELEASE_PROJECT_ID!;
  const phase = process.env.OCTACITY_RELEASE_MATERIAL_PHASE!;
  await page.addInitScript(() => {
    localStorage.setItem(
      'octacity.console.preferences',
      JSON.stringify({
        version: 2,
        language: 'en',
        theme: 'light',
        favorites: [],
        recents: [],
        expanded: [],
        explorerOpen: false,
        explorerWidth: 288,
        notificationLastOpenedAt: null,
      }),
    );
  });
  if (phase === 'fields') {
    await page.goto(`/projects/${project}`);
    await page.getByRole('button', { name: 'Issue field settings', exact: true }).click();
    const components = page.getByRole('group', { name: 'Components', exact: true });
    await components.getByRole('textbox', { name: 'New option name', exact: true }).fill('API');
    await components
      .getByRole('textbox', { name: 'New option ID (optional)', exact: true })
      .fill('api');
    await components.getByRole('button', { name: 'Add option', exact: true }).click();
    const fields = page.getByRole('group', { name: 'Custom Issue fields', exact: true });
    for (const [name, type] of [
      ['Material environment', 'Text'],
      ['Assessment severity', 'Choice'],
    ]) {
      await fields.getByRole('textbox', { name: 'New field name', exact: true }).fill(name!);
      await fields.getByRole('combobox', { name: 'Field type', exact: true }).click();
      await page.getByRole('option', { name: type!, exact: true }).click();
      await fields.getByRole('button', { name: 'Add field', exact: true }).click();
    }
    await fields
      .getByRole('group', { name: 'Material environment', exact: true })
      .getByRole('spinbutton', { name: 'Maximum UTF-8 bytes', exact: true })
      .fill('128');
    const severity = fields.getByRole('group', { name: 'Assessment severity', exact: true });
    await severity
      .getByRole('textbox', { name: 'New option name', exact: true })
      .fill('Significant');
    await severity
      .getByRole('textbox', { name: 'New option ID (optional)', exact: true })
      .fill('significant');
    await severity.getByRole('button', { name: 'Add option', exact: true }).click();
    await page.getByRole('combobox', { name: 'Severity field', exact: true }).click();
    await page.getByRole('option', { name: 'Assessment severity', exact: true }).click();
    await page.getByRole('button', { name: 'Review field definitions', exact: true }).click();
    await page
      .getByRole('dialog')
      .getByRole('button', { name: 'Publish field definitions', exact: true })
      .click();
    await expect(page.getByRole('dialog')).toContainText('Field definitions published.');
    const catalog = await (
      await page.request.get(`/api/v1/projects/${project}/issue-property-definitions`)
    ).json();
    expect(catalog.version).toBe(1);
    expect(catalog.definition.properties.map((p: { name: string }) => p.name)).toEqual([
      'Material environment',
      'Assessment severity',
    ]);
  } else if (phase === 'launch') {
    const issue = process.env.OCTACITY_RELEASE_ISSUE_ID!;
    const base = `/api/v1/projects/${project}`;
    const configurations = (await (await page.request.get(`${base}/factory-configurations`)).json())
      .items;
    expect(configurations).toHaveLength(1);
    await page.goto(`/projects/${project}/issues/${issue}`);
    await page.getByRole('button', { name: 'Start pipeline', exact: true }).click();
    await choose(
      page,
      'Factory Configurations',
      `Factory Configuration · v1 · ${configurations[0].id}`,
    );
    await choose(page, 'Source', 'Exact revision');
    await page
      .getByRole('textbox', { name: 'Exact revision', exact: true })
      .fill(process.env.OCTACITY_RELEASE_CODE_REVISION!);
    const dialog = await reviewIssueSnapshotLaunch(page);
    for (const text of [
      'ISSUE_TEXT_SELECTED_v1',
      'PROJECT_TEXT_SELECTED_v1',
      'COMMENT_TEXT_SELECTED_v1',
    ]) {
      await expect(dialog).toContainText(text);
    }
    await expect(dialog).not.toContainText('ISSUE_TEXT_UNSELECTED_v2');
    expect(
      (await (await page.request.get(`${base}/issues/${issue}/executions`)).json()).items,
    ).toHaveLength(0);
    await confirm(page, 'Confirm pipeline launch', 'Work and Run admitted.');
  } else {
    expect(phase).toBe('materials');
    const issue = process.env.OCTACITY_RELEASE_ISSUE_ID!;
    const base = `/api/v1/projects/${project}`;
    const path = `${base}/issues/${issue}`;
    const read = async (suffix = '') => await (await page.request.get(path + suffix)).json();
    const empty = await read();
    expect(empty.materials ?? []).toEqual([]);
    await page.goto(`/projects/${project}/issues/${issue}`);
    await page.getByRole('button', { name: 'Edit requirements', exact: true }).click();
    await choose(page, 'Priority', 'High');
    await page.getByRole('button', { name: 'Review task', exact: true }).click();
    await confirm(page, 'Save task', 'Saved task version');
    const properties = page.getByRole('complementary', { name: 'Properties', exact: true });
    await properties.getByRole('button', { name: 'Edit properties', exact: true }).click();
    await choose(page, 'Issue component', 'API');
    await choose(page, 'Issue type', 'Feature request');
    await choose(page, 'Issue size', 'Small');
    await properties
      .getByRole('textbox', { name: 'Material environment', exact: true })
      .fill('staging-arm64');
    await choose(page, 'Assessment severity', 'Significant');
    await properties.getByRole('button', { name: 'Review properties', exact: true }).click();
    await confirm(page, 'Save properties', 'Saved task version');
    await page.getByRole('link', { name: 'Documents', exact: true }).click();
    await page.getByRole('button', { name: 'New Issue document', exact: true }).click();
    await page
      .getByRole('textbox', { name: 'Document name', exact: true })
      .fill('Task investigation');
    await page
      .getByRole('textbox', { name: 'Document text', exact: true })
      .fill('ISSUE_TEXT_SELECTED_v1');
    await page.getByRole('button', { name: 'Review Issue publication', exact: true }).click();
    await confirm(page, 'Publish Issue document', 'Issue document published');
    const first = (await read('/input-documents')).items[0];
    expect((await read()).materials ?? []).toEqual([]);
    await page.getByRole('button', { name: 'Preview Task investigation', exact: true }).click();
    await page.getByRole('button', { name: 'Publish new Issue version', exact: true }).click();
    await page
      .getByRole('textbox', { name: 'Document text', exact: true })
      .fill('ISSUE_TEXT_UNSELECTED_v2');
    await page.getByRole('button', { name: 'Review Issue publication', exact: true }).click();
    await confirm(page, 'Publish Issue document', 'Issue document published');
    const documents = (await read('/input-documents')).items;
    expect(documents).toHaveLength(2);
    expect((await read()).materials ?? []).toEqual([]);
    await page
      .getByRole('listitem')
      .filter({ hasText: first.artifact.artifact_id })
      .getByRole('button', { name: 'Select Task investigation as material', exact: true })
      .click();
    await confirm(page, 'Save material selection', 'Material selection saved');
    const sharedResponse = await page.request.post(`${base}/input-documents`, {
      headers: { 'Idempotency-Key': 'qualification-shared-guide' },
      data: { name: 'Shared delivery guide', content: 'PROJECT_TEXT_SELECTED_v1' },
    });
    expect(sharedResponse.ok()).toBe(true);
    const shared = await sharedResponse.json();
    const projectDocuments = (await (await page.request.get(`${base}/input-documents`)).json())
      .items;
    for (const document of documents) {
      expect(
        projectDocuments.map((d: { artifact: { artifact_id: string } }) => d.artifact.artifact_id),
      ).not.toContain(document.artifact.artifact_id);
    }
    await page.getByRole('button', { name: 'Link existing Project document', exact: true }).click();
    const picker = page.getByRole('region', {
      name: 'Link existing Project document',
      exact: true,
    });
    await expect(picker.getByText('Task investigation', { exact: true })).toHaveCount(0);
    await picker.getByRole('button', { name: 'View Shared delivery guide', exact: true }).click();
    await picker
      .getByRole('button', { name: 'Link this exact Project publication', exact: true })
      .click();
    await confirm(page, 'Save material selection', 'Material selection saved');
    await page.getByRole('link', { name: 'Activity', exact: true }).click();
    await page
      .getByRole('textbox', { name: 'Comment', exact: true })
      .fill('COMMENT_TEXT_SELECTED_v1');
    await page.getByRole('button', { name: 'Review comment', exact: true }).click();
    await confirm(page, 'Append comment', 'Operator comment recorded');
    expect((await read()).materials).toHaveLength(2);
    await page
      .getByRole('button', { name: 'Include comment 1 as Issue material', exact: true })
      .click();
    await confirm(page, 'Save material selection', 'Material selection saved');
    const current = await read();
    expect(current.content.priority).toBe('high');
    expect(current.planning).toMatchObject({
      component: 'api',
      type: 'feature_request',
      size: 'small',
    });
    expect(current.materials.map((m: { reference: { kind: string } }) => m.reference.kind)).toEqual(
      ['issue_document', 'project_document', 'comment'],
    );
    expect(current.materials[0].reference.artifact).toEqual(first.artifact);
    expect(current.materials[1].reference.artifact).toEqual(shared.artifact);
    expect((await read('/executions')).items).toEqual([]);
    const otherResponse = await page.request.post(`${base}/issues`, {
      headers: { 'Idempotency-Key': 'qualification-other-issue' },
      data: { ...current.content, title: 'Another owner', planning: current.planning },
    });
    expect(otherResponse.ok()).toBe(true);
    const other = (await otherResponse.json()).resource;
    const foreign = `${base}/issues/${other.id}`;
    for (const owner of [foreign, base]) {
      const response = await page.request.get(
        `${owner}/input-documents/${first.artifact.artifact_id}`,
      );
      expect(response.status()).toBe(404);
      expect(await response.text()).not.toContain('ISSUE_TEXT_SELECTED');
    }
    const ownPublication = await page.request.post(`${foreign}/input-documents`, {
      headers: { 'Idempotency-Key': 'qualification-sibling-document' },
      data: { name: 'Sibling investigation', content: 'OTHER_ISSUE_PRIVATE' },
    });
    expect(ownPublication.ok()).toBe(true);
    const ownDocument = await ownPublication.json();
    const ownSelection = await page.request.patch(`${foreign}/materials`, {
      headers: {
        'Idempotency-Key': 'qualification-sibling-selection',
        'If-Match': `"${other.version}"`,
      },
      data: {
        references: [
          {
            kind: 'issue_document',
            project_id: project,
            issue_id: other.id,
            artifact: ownDocument.artifact,
          },
        ],
      },
    });
    expect(ownSelection.status()).toBe(200);
    const beforeRejection = (await ownSelection.json()).resource;
    expect(beforeRejection.materials[0].reference.artifact).toEqual(ownDocument.artifact);
    // The route and current authority just succeeded; this publication belongs to another Issue.
    const stolen = await page.request.patch(`${foreign}/materials`, {
      headers: {
        'Idempotency-Key': 'qualification-forged-owner',
        'If-Match': ownSelection.headers().etag!,
      },
      data: { references: [{ ...current.materials[0].reference, issue_id: other.id }] },
    });
    await expectForeignMaterialRejected(stolen);
    const unchanged = await (await page.request.get(foreign)).json();
    expect(unchanged.version).toBe(beforeRejection.version);
    expect(unchanged.materials).toEqual(beforeRejection.materials);
    expect((await (await page.request.get(`${foreign}/executions`)).json()).items).toEqual([]);
    await page.goto(`/projects/${project}`);
    await page.getByRole('button', { name: 'Manage Project documents', exact: true }).click();
    await expect(
      page.getByRole('button', { name: 'View Shared delivery guide', exact: true }),
    ).toBeVisible();
    await expect(page.getByText('Task investigation', { exact: true })).toHaveCount(0);
    await page.goto(`/factory?project_id=${project}`);
    await page.getByRole('button', { name: 'Add pipeline', exact: true }).click();
    await page.getByRole('button', { name: 'Add step', exact: true }).click();
    await choose(page, 'Step type', 'AI agent');
    const stepDocuments = page.getByRole('group', { name: 'Documents', exact: true });
    await expect(
      stepDocuments.getByRole('checkbox', { name: 'Shared delivery guide', exact: true }),
    ).not.toBeChecked();
    await expect(
      stepDocuments.getByRole('checkbox', { name: 'Task investigation', exact: true }),
    ).toHaveCount(0);
  }
});

async function choose(page: Page, name: string, option: string) {
  await page.getByRole('combobox', { name, exact: true }).click();
  await page.getByRole('option', { name: option, exact: true }).click();
}

async function confirm(page: Page, button: string, message: string) {
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: button, exact: true }).click();
  await expect(dialog).toContainText(message);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
}
