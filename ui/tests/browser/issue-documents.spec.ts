import { expect, test } from '@playwright/test';
import type { components } from '../../.generated/api/schema';
import { installOperatorFixture } from './operatorFixture';
import { workflowFixture } from '../fixtures/issueWorkflows';
import { configurationFixture } from '../fixtures/factory';
import { timelineFixture } from '../fixtures/issueActivityData';
import { issueClassificationFixture } from '../fixtures/issueClassification';
import { issueFixture, issuePropertyChoicesFixture } from '../fixtures/issues';
import { DEFAULT_PREFERENCES, PREFERENCES_KEY } from '../../src/app/presentation/preferences';
import { translate, type MessageKey } from '../../src/app/presentation/messages';
type S = components['schemas'];
test.use({ trace: 'off', screenshot: 'off', video: 'off' });
for (const language of ['en', 'ru'] as const)
  for (const width of [390, 1280]) {
    test(`${language}: Issue documents create/version/link/select/replace/remove with exact review at ${width}px`, async ({
      page,
    }) => {
      await installOperatorFixture(page);
      await page.addInitScript(
        ({ key, preferences }) => localStorage.setItem(key, JSON.stringify(preferences)),
        {
          key: PREFERENCES_KEY,
          preferences: { ...DEFAULT_PREFERENCES, language, explorerOpen: false },
        },
      );
      await page.setViewportSize({ width, height: 900 });
      const t = (key: MessageKey, values = {}) => translate(language, key, values);
      const project = configurationFixture().configuration.reference.project_id;
      let issue = { ...issueFixture(project), version: 2 };
      const base = `/api/v1/projects/${project}/issues/${issue.id}`;
      await page.route(
        `**/api/v1/projects/${project}/issue-workflows/${issue.workflow_id}/versions/${issue.workflow_version}`,
        (route) => route.fulfill({ json: workflowFixture(project) }),
      );
      const history = new Map<number, S['IssueResource']>([[2, issue]]);
      await page.route(`**${base}/revisions/*/classification`, (route) =>
        route.fulfill({
          json: {
            ...issueClassificationFixture(project).review,
            version: Number(new URL(route.request().url()).pathname.split('/').at(-2)),
          },
        }),
      );
      const docs: S['IssueDocumentResource'][] = [];
      const bodies = new Map<string, string>();
      const receipts = new Map<string, S['IssueDocumentResource']>();
      const publishes: { body: S['PublishIssueDocumentRequest']; key: string | undefined }[] = [];
      const selections: S['EditIssueMaterialsRequest'][] = [];
      const executionWrites: string[] = [];
      const contentReads: string[] = [];
      let unavailable = false;
      let sharedUnavailable = false;
      let sharedReadGate: Promise<void> | null = null;
      let selectionConflict = true;
      const shared: S['ProjectDocumentResource'] = {
        project_id: project,
        name: 'Shared guide',
        published_at: 1,
        expires_at: 4102444800000,
        artifact: {
          artifact_id: 'shared-publication',
          content_digest: 'e'.repeat(64),
          encoded_size: 13,
        },
      };
      const secondShared = {
        ...shared,
        name: 'Second shared guide',
        artifact: {
          ...shared.artifact,
          artifact_id: 'second-shared-publication',
          content_digest: 'f'.repeat(64),
          encoded_size: 21,
        },
      };
      history.set(1, {
        ...issue,
        version: 1,
        materials: [
          {
            reference: { kind: 'project_document', project_id: project, artifact: shared.artifact },
            selected_by: { kind: 'unauthenticated_management', identity: null },
            selected_at: 1,
          },
        ],
      });
      await page.route(
        (u) => u.pathname === base,
        (route) => route.fulfill({ json: issue }),
      );
      await page.route(
        (u) => u.pathname === `${base}/transitions`,
        (route) =>
          route.fulfill({
            json: {
              issue_version: issue.version,
              workflow_id: issue.workflow_id,
              workflow_version: issue.workflow_version,
              status_id: issue.status_id,
              items: [],
              field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
            },
          }),
      );
      await page.route(`**/api/v1/projects/${project}/issue-property-choices`, (route) =>
        route.fulfill({ json: issuePropertyChoicesFixture(project) }),
      );
      await page.route(
        (u) => u.pathname.startsWith(`${base}/executions`),
        (route) => {
          if (route.request().method() !== 'GET') executionWrites.push(route.request().url());
          return route.fulfill({ json: { items: [], next_cursor: null } });
        },
      );
      await page.route(
        (u) => u.pathname === `${base}/input-documents`,
        async (route) => {
          if (route.request().method() === 'GET')
            return route.fulfill({ json: { items: docs, next_cursor: null } });
          const body: S['PublishIssueDocumentRequest'] = route.request().postDataJSON();
          const key = route.request().headers()['idempotency-key'];
          publishes.push({ body, key });
          expect(route.request().headers()['if-match']).toBeUndefined();
          let doc = receipts.get(key!);
          if (!doc) {
            doc = {
              project_id: project,
              issue_id: issue.id,
              name: body.name,
              actor: { kind: 'unauthenticated_management', identity: null },
              published_at: docs.length + 1,
              expires_at: 4102444800000,
              artifact: {
                artifact_id: `issue-publication-${docs.length + 1}`,
                content_digest: String(docs.length + 1).repeat(64),
                encoded_size: new TextEncoder().encode(body.content).length,
              },
            };
            docs.push(doc);
            bodies.set(doc.artifact.artifact_id, body.content);
            receipts.set(key!, doc);
          }
          if (publishes.length === 1) return route.abort('connectionfailed');
          return route.fulfill({ status: 201, json: doc });
        },
      );
      await page.route(
        (u) => u.pathname.startsWith(`${base}/input-documents/`),
        (route) => {
          const id = route.request().url().split('/').at(-1);
          contentReads.push(id!);
          if (unavailable)
            return route.fulfill({
              status: 404,
              json: {
                code: 'not_found',
                message: 'Publication unavailable',
                request_id: 'missing',
              },
            });
          const doc = docs.find((d) => d.artifact.artifact_id === id);
          return doc
            ? route.fulfill({ json: { document: doc, content: bodies.get(id!) } })
            : route.fulfill({
                status: 404,
                json: {
                  code: 'not_found',
                  message: 'Publication unavailable',
                  request_id: 'missing',
                },
              });
        },
      );
      await page.route(
        (u) => u.pathname === `${base}/materials`,
        (route) => {
          const body: S['EditIssueMaterialsRequest'] = route.request().postDataJSON();
          expect(route.request().headers()['if-match']).toBe(`"${issue.version}"`);
          if (selectionConflict) {
            selectionConflict = false;
            issue = { ...issue, version: issue.version + 1 };
            history.set(issue.version, issue);
            return route.fulfill({
              status: 409,
              json: { code: 'conflict', message: 'Issue changed', request_id: 'changed' },
            });
          }
          selections.push(body);
          issue = {
            ...issue,
            version: issue.version + 1,
            materials: body.references.map((reference) => ({
              reference,
              selected_by: { kind: 'unauthenticated_management', identity: null },
              selected_at: 10,
            })),
          };
          history.set(issue.version, issue);
          return route.fulfill({ json: { disposition: 'applied', resource: issue } });
        },
      );
      await page.route(
        (u) =>
          u.pathname.startsWith(`${base}/revisions/`) && !u.pathname.endsWith('/classification'),
        (route) =>
          route.fulfill({ json: history.get(Number(route.request().url().split('/').at(-1))) }),
      );
      await page.route(
        (u) => u.pathname === `${base}/timeline`,
        (route) =>
          route.fulfill({ json: timelineFixture([{ kind: 'revision', version: 1, at: 1 }]) }),
      );
      await page.route(
        (u) => u.pathname === `${base}/revisions`,
        (route) =>
          route.fulfill({
            json: {
              items: [{ version: 1, applicability_version: 1, updated_at_unix_ms: 1 }],
              next_cursor: null,
            },
          }),
      );
      const sharedBase = `/api/v1/projects/${project}/input-documents`;
      await page.route(
        (u) => u.pathname === sharedBase,
        (route) => {
          expect(route.request().method()).toBe('GET');
          return route.fulfill({ json: { items: [shared, secondShared], next_cursor: null } });
        },
      );
      await page.route(
        (u) => u.pathname === `${sharedBase}/shared-publication`,
        async (route) => {
          if (sharedReadGate) await sharedReadGate;
          return sharedUnavailable
            ? route.fulfill({
                status: 403,
                json: {
                  code: 'forbidden',
                  message: 'Shared source unavailable',
                  request_id: 'denied',
                },
              })
            : route.fulfill({
                json: {
                  project_id: project,
                  artifact: shared.artifact,
                  expires_at: shared.expires_at,
                  content: '# Shared guide',
                },
              });
        },
      );
      await page.route(
        (u) => u.pathname === `${sharedBase}/second-shared-publication`,
        (route) =>
          route.fulfill({
            json: {
              project_id: project,
              artifact: secondShared.artifact,
              expires_at: secondShared.expires_at,
              content: '# Second shared guide',
            },
          }),
      );
      await page.goto(`/projects/${project}/issues/${issue.id}?section=documents`);
      await page.getByRole('button', { name: t('issueDocuments.new'), exact: true }).press('Enter');
      await page
        .getByRole('textbox', { name: t('projectDocument.name'), exact: true })
        .fill('Task notes');
      await page
        .getByRole('textbox', { name: t('projectDocument.content'), exact: true })
        .fill('# PRIVATE_DOC_TEXT_v1');
      await page.getByRole('link', { name: t('issues.overview'), exact: true }).press('Enter');
      await page.getByRole('link', { name: t('issues.documents'), exact: true }).press('Enter');
      await expect(
        page.getByRole('textbox', { name: t('projectDocument.content'), exact: true }),
      ).toHaveValue('# PRIVATE_DOC_TEXT_v1');
      await page.getByRole('link', { name: t('issues.activity'), exact: true }).press('Enter');
      await page
        .getByRole('region', { name: t('issues.taskActivity'), exact: true })
        .getByRole('link', { name: t('issues.readRevision', { version: 1 }), exact: true })
        .press('Enter');
      await expect(
        page.getByRole('region', { name: t('issues.historical', { version: 1 }), exact: true }),
      ).toBeVisible();
      await page.getByRole('link', { name: t('issues.documents'), exact: true }).press('Enter');
      await expect(
        page.getByText(t('issueDocuments.historical', { version: 1 }), { exact: true }),
      ).toBeVisible();
      await expect(
        page.getByRole('textbox', { name: t('projectDocument.content'), exact: true }),
      ).toHaveCount(0);
      await page
        .getByRole('region', { name: t('issueDocuments.selected'), exact: true })
        .getByRole('button', {
          name: t('issueDocuments.preview', { name: shared.artifact.artifact_id }),
          exact: true,
        })
        .press('Enter');
      await expect(page.getByRole('heading', { name: 'Shared guide', exact: true })).toBeVisible();
      await page.getByRole('link', { name: t('issues.activity'), exact: true }).press('Enter');
      await page.getByRole('link', { name: t('issues.closeRevision'), exact: true }).press('Enter');
      await expect(
        page.getByRole('region', { name: t('issues.historical', { version: 1 }), exact: true }),
      ).toHaveCount(0);
      await page.getByRole('link', { name: t('issues.documents'), exact: true }).press('Enter');
      await expect(
        page.getByRole('textbox', { name: t('projectDocument.content'), exact: true }),
      ).toHaveValue('# PRIVATE_DOC_TEXT_v1');

      await page
        .getByRole('button', { name: t('issueDocuments.reviewPublication'), exact: true })
        .press('Enter');
      let dialog = page.getByRole('dialog');
      await expect(
        dialog.getByRole('button', { name: t('issueDocuments.publish'), exact: true }),
      ).toBeFocused();
      await dialog
        .getByRole('button', { name: t('issueDocuments.publish'), exact: true })
        .press('Enter');
      await dialog
        .getByRole('button', { name: t('command.retrySame'), exact: true })
        .press('Enter');
      await expect(dialog.getByText(t('issueDocuments.published'), { exact: true })).toBeVisible();
      await dialog.getByRole('button', { name: t('common.close'), exact: true }).press('Enter');
      expect(publishes[0]).toEqual(publishes[1]);
      expect(docs).toHaveLength(1);
      expect(selections).toHaveLength(0);
      async function assertDocumentLayout() {
        const view = page.getByRole('region', { name: t('issueDocuments.title'), exact: true });
        for (const button of await view.getByRole('button').all()) {
          if (await button.isVisible())
            expect((await button.boundingBox())!.height).toBeGreaterThanOrEqual(44);
        }
        expect(
          await page.evaluate(() => document.documentElement.scrollWidth - innerWidth),
        ).toBeLessThanOrEqual(1);
      }
      await assertDocumentLayout();
      await page.getByRole('button', { name: t('issueDocuments.new'), exact: true }).press('Enter');
      await page
        .getByRole('textbox', { name: t('projectDocument.name'), exact: true })
        .fill('Unpublished');
      await page
        .getByRole('textbox', { name: t('projectDocument.content'), exact: true })
        .fill('PRIVATE_TRANSIENT_DRAFT');
      const row = page.getByRole('listitem').filter({ hasText: 'issue-publication-1' });
      await row
        .getByRole('button', {
          name: t('issueDocuments.select', { name: 'Task notes' }),
          exact: true,
        })
        .press('Enter');
      await page
        .getByRole('dialog')
        .getByRole('button', { name: t('issueDocuments.saveSelection'), exact: true })
        .press('Enter');
      await page
        .getByRole('dialog')
        .getByRole('button', { name: t('command.refreshCurrent'), exact: true })
        .press('Enter');
      await expect(
        page.getByRole('textbox', { name: t('projectDocument.content'), exact: true }),
      ).toHaveValue('PRIVATE_TRANSIENT_DRAFT');
      await expect(page.getByRole('dialog')).toHaveCount(0);
      await page
        .getByRole('button', { name: t('projectDocument.cancel'), exact: true })
        .press('Enter');
      await expect(
        page.getByRole('button', { name: t('issueDocuments.new'), exact: true }),
      ).toBeVisible();
      const selectAgain = row.getByRole('button', {
        name: t('issueDocuments.select', { name: 'Task notes' }),
        exact: true,
      });
      await expect(selectAgain).toBeEnabled();
      await selectAgain.press('Enter');
      async function confirm() {
        const d = page.getByRole('dialog');
        await d
          .getByRole('button', { name: t('issueDocuments.saveSelection'), exact: true })
          .press('Enter');
        await expect(
          d.getByText(t('issueDocuments.selectionSaved'), { exact: true }),
        ).toBeVisible();
        await d.getByRole('button', { name: t('common.close'), exact: true }).press('Enter');
        await expect(d).toHaveCount(0);
      }
      await confirm();
      const selectedRevision = issue.version;
      await row
        .getByRole('button', {
          name: t('issueDocuments.preview', { name: 'Task notes' }),
          exact: true,
        })
        .press('Enter');
      await expect(page.getByRole('heading', { name: 'PRIVATE_DOC_TEXT_v1' })).toBeVisible();
      await page
        .getByRole('button', { name: t('issueDocuments.version'), exact: true })
        .press('Enter');
      await page
        .getByRole('textbox', { name: t('projectDocument.content'), exact: true })
        .fill('# PRIVATE_DOC_TEXT_v2');
      await page
        .getByRole('button', { name: t('issueDocuments.reviewPublication'), exact: true })
        .press('Enter');
      dialog = page.getByRole('dialog');
      await dialog
        .getByRole('button', { name: t('issueDocuments.publish'), exact: true })
        .press('Enter');
      await expect(dialog.getByText(t('issueDocuments.published'), { exact: true })).toBeVisible();
      await dialog.getByRole('button', { name: t('common.close'), exact: true }).press('Enter');
      expect(selections).toHaveLength(1);
      expect(selections[0]?.references[0]).toMatchObject({
        kind: 'issue_document',
        artifact: docs[0]!.artifact,
      });
      await page
        .getByRole('button', {
          name: t('issueDocuments.replace', { id: 'issue-publication-1' }),
          exact: true,
        })
        .press('Enter');
      await page
        .getByRole('listitem')
        .filter({ hasText: 'issue-publication-2' })
        .getByRole('button', {
          name: t('issueDocuments.replaceWith', { name: 'Task notes' }),
          exact: true,
        })
        .press('Enter');
      await confirm();
      await page
        .getByRole('button', {
          name: t('issueDocuments.replace', { id: 'issue-publication-2' }),
          exact: true,
        })
        .press('Enter');
      await page.getByRole('link', { name: t('issues.overview'), exact: true }).press('Enter');
      await expect(
        page.getByText(t('issues.revision', { version: issue.version }), { exact: true }),
      ).toBeVisible();
      issue = { ...issue, version: issue.version + 1, materials: [] };
      history.set(issue.version, issue);
      await page.getByRole('button', { name: t('issues.refresh'), exact: true }).press('Enter');
      await expect(
        page.getByText(t('issues.revision', { version: issue.version }), { exact: true }),
      ).toBeVisible();
      await page.getByRole('link', { name: t('issues.documents'), exact: true }).press('Enter');
      await expect(
        page.getByText(t('issueDocuments.replacementMissing'), { exact: true }),
      ).toBeVisible();
      const replacement = page.getByRole('listitem').filter({ hasText: 'issue-publication-2' });
      await expect(
        replacement.getByRole('button', {
          name: t('issueDocuments.replaceWith', { name: 'Task notes' }),
          exact: true,
        }),
      ).toBeDisabled();
      expect(selections).toHaveLength(2);
      await page
        .getByRole('button', { name: t('projectDocument.cancel'), exact: true })
        .press('Enter');
      await replacement
        .getByRole('button', {
          name: t('issueDocuments.select', { name: 'Task notes' }),
          exact: true,
        })
        .press('Enter');
      await confirm();
      expect(selections.at(-1)?.references).toEqual([
        {
          kind: 'issue_document',
          project_id: project,
          issue_id: issue.id,
          artifact: docs[1]!.artifact,
        },
      ]);
      await page
        .getByRole('button', { name: t('issueDocuments.link'), exact: true })
        .press('Enter');
      const sharedPicker = page.getByRole('region', {
        name: t('issueDocuments.link'),
        exact: true,
      });
      await expect(sharedPicker.getByText('Task notes', { exact: true })).toHaveCount(0);
      let releaseShared!: () => void;
      sharedReadGate = new Promise<void>((resolve) => {
        releaseShared = resolve;
      });
      await page
        .getByRole('button', {
          name: t('projectDocument.view', { name: 'Shared guide' }),
          exact: true,
        })
        .press('Enter');
      await expect(
        sharedPicker.getByRole('button', {
          name: t('projectDocument.view', { name: 'Second shared guide' }),
          exact: true,
        }),
      ).toBeDisabled();
      await expect(
        sharedPicker.getByRole('button', { name: t('issueDocuments.link'), exact: true }),
      ).toBeDisabled();
      releaseShared();
      sharedReadGate = null;
      await expect(
        sharedPicker
          .getByRole('article', { name: 'Shared guide', exact: true })
          .getByRole('heading', { name: 'Shared guide', exact: true }),
      ).toBeVisible();
      const openSecond = sharedPicker.getByRole('button', {
        name: t('projectDocument.view', { name: 'Second shared guide' }),
        exact: true,
      });
      await expect(openSecond).toBeEnabled();
      await openSecond.press('Enter');
      await expect(
        sharedPicker
          .getByRole('article', { name: 'Second shared guide', exact: true })
          .getByRole('heading', { name: 'Second shared guide', exact: true }),
      ).toBeVisible();
      await sharedPicker
        .getByRole('button', { name: t('issueDocuments.linkExact'), exact: true })
        .press('Enter');
      await expect(
        page.getByRole('dialog').getByText(secondShared.artifact.artifact_id, { exact: true }),
      ).toBeVisible();
      await page.keyboard.press('Escape');
      await expect(page.getByRole('dialog')).toHaveCount(0);
      await sharedPicker
        .getByRole('button', {
          name: t('projectDocument.view', { name: 'Shared guide' }),
          exact: true,
        })
        .press('Enter');
      await expect(
        sharedPicker
          .getByRole('article', { name: 'Shared guide', exact: true })
          .getByRole('heading', { name: 'Shared guide', exact: true }),
      ).toBeVisible();
      sharedUnavailable = true;
      await sharedPicker
        .getByRole('button', {
          name: t('projectDocument.view', { name: 'Shared guide' }),
          exact: true,
        })
        .press('Enter');
      await expect(sharedPicker.getByRole('alert')).toHaveAttribute('data-state', 'forbidden');
      await expect(
        sharedPicker
          .getByRole('article', { name: 'Shared guide', exact: true })
          .getByRole('heading', { name: 'Shared guide', exact: true }),
      ).toHaveCount(0);
      await expect(
        sharedPicker.getByRole('button', { name: t('issueDocuments.linkExact'), exact: true }),
      ).toHaveCount(0);
      sharedUnavailable = false;
      await sharedPicker
        .getByRole('button', {
          name: t('projectDocument.view', { name: 'Shared guide' }),
          exact: true,
        })
        .press('Enter');
      await expect(
        sharedPicker
          .getByRole('article', { name: 'Shared guide', exact: true })
          .getByRole('heading', { name: 'Shared guide', exact: true }),
      ).toBeVisible();
      await page
        .getByRole('button', { name: t('issueDocuments.linkExact'), exact: true })
        .press('Enter');
      await confirm();
      expect(selections.at(-1)?.references.map((r) => r.kind)).toEqual([
        'issue_document',
        'project_document',
      ]);
      // A selected row can disappear while its independent body read is still in flight.
      sharedReadGate = new Promise<void>((resolve) => {
        releaseShared = resolve;
      });
      await page
        .getByRole('region', { name: t('issueDocuments.selected'), exact: true })
        .getByRole('button', {
          name: t('issueDocuments.preview', { name: shared.artifact.artifact_id }),
          exact: true,
        })
        .press('Enter');
      const ownPreview = page
        .getByRole('listitem')
        .filter({ hasText: 'issue-publication-2' })
        .getByRole('button', {
          name: t('issueDocuments.preview', { name: 'Task notes' }),
          exact: true,
        });
      await expect(ownPreview).toBeDisabled();
      await page
        .getByRole('button', {
          name: t('issueDocuments.remove', { id: shared.artifact.artifact_id }),
          exact: true,
        })
        .press('Enter');
      await confirm();
      releaseShared();
      sharedReadGate = null;
      await expect(ownPreview).toBeEnabled();
      await page
        .getByRole('button', {
          name: t('issueDocuments.remove', { id: 'issue-publication-2' }),
          exact: true,
        })
        .press('Enter');
      await confirm();
      expect(selections.at(-1)?.references).toEqual([]);
      expect(executionWrites).toEqual([]);
      await page.goto(
        `/projects/${project}/issues/${issue.id}?section=documents&revision=${selectedRevision}`,
      );
      await expect(
        page.getByText(t('issueDocuments.historical', { version: selectedRevision }), {
          exact: true,
        }),
      ).toBeVisible();
      const historicalSelection = page.getByRole('region', {
        name: t('issueDocuments.selected'),
        exact: true,
      });
      await expect(
        historicalSelection.getByText('issue-publication-1', { exact: true }),
      ).toBeVisible();
      await expect(
        historicalSelection.getByText('issue-publication-2', { exact: true }),
      ).toHaveCount(0);
      await expect(
        page.getByRole('button', { name: t('issueDocuments.new'), exact: true }),
      ).toHaveCount(0);
      await expect(
        page.getByRole('button', {
          name: t('issueDocuments.remove', { id: 'issue-publication-1' }),
          exact: true,
        }),
      ).toHaveCount(0);
      await historicalSelection
        .getByRole('button', {
          name: t('issueDocuments.preview', { name: 'issue-publication-1' }),
          exact: true,
        })
        .press('Enter');
      await expect(page.getByRole('heading', { name: 'PRIVATE_DOC_TEXT_v1' })).toBeVisible();
      await assertDocumentLayout();
      unavailable = true;
      await historicalSelection
        .getByRole('button', {
          name: t('issueDocuments.preview', { name: 'issue-publication-1' }),
          exact: true,
        })
        .press('Enter');
      await expect(
        page
          .getByRole('region', { name: t('issueDocuments.title'), exact: true })
          .getByRole('alert'),
      ).toHaveAttribute('data-state', 'not-found');
      await expect(page.getByRole('heading', { name: 'PRIVATE_DOC_TEXT_v1' })).toHaveCount(0);
      expect(contentReads.at(-1)).toBe('issue-publication-1');
      expect(selections).toHaveLength(6);
      await page.goto(`/projects/${project}`);
      await page
        .getByRole('button', { name: t('projectDocument.manage'), exact: true })
        .press('Enter');
      await expect(
        page.getByRole('button', {
          name: t('projectDocument.view', { name: 'Shared guide' }),
          exact: true,
        }),
      ).toBeVisible();
      await expect(page.getByText('Task notes', { exact: true })).toHaveCount(0);
      await page.goto(`/factory?project_id=${project}`);
      await page
        .getByRole('button', { name: t('factory.addPipeline'), exact: true })
        .press('Enter');
      await page.getByRole('button', { name: t('factory.addNode'), exact: true }).press('Enter');
      await page.getByRole('combobox', { name: t('factory.nodeKind'), exact: true }).click();
      await page
        .getByRole('option', { name: t('factory.kind.reasoning'), exact: true })
        .press('Enter');
      const stepDocuments = page.getByRole('group', { name: t('factory.documents'), exact: true });
      await expect(
        stepDocuments.getByRole('checkbox', { name: 'Shared guide', exact: true }),
      ).not.toBeChecked();
      await expect(
        stepDocuments.getByRole('checkbox', { name: 'Task notes', exact: true }),
      ).toHaveCount(0);
      expect(
        await page.evaluate(() => JSON.stringify(localStorage) + JSON.stringify(sessionStorage)),
      ).not.toContain('PRIVATE_DOC_TEXT');
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth - innerWidth),
      ).toBeLessThanOrEqual(1);
    });
  }
