import { expect, test } from '@playwright/test';
import { installOperatorFixture, FIXTURE_IDS } from './operatorFixture';
import type { WorkflowDefinition, WorkflowResource } from '../../src/api/issueWorkflows';
import { workflowFixture } from '../fixtures/issueWorkflows';

test('keeps the line centered through the arrowhead on a curved workflow transition', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  workflow.definition.statuses = workflow.definition.statuses.slice(0, 2);
  workflow.definition.terminal_status_ids = [workflow.definition.statuses[1]!.id];
  workflow.definition.transitions = [
    {
      id: '30000000-0000-4000-8000-000000000001',
      name: 'Complete',
      from: workflow.definition.statuses[0]!.id,
      to: workflow.definition.statuses[1]!.id,
      origins: ['manual'],
      evidence: [],
    },
    {
      id: '30000000-0000-4000-8000-000000000002',
      name: 'Return',
      from: workflow.definition.statuses[1]!.id,
      to: workflow.definition.statuses[0]!.id,
      origins: ['manual'],
      evidence: [],
    },
  ];
  await page.route('**/api/v1/projects/*/issue-workflow-default', (route) =>
    route.fulfill({ json: { version: 7, workflow } }),
  );
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  const arrow = page
    .getByRole('region', { name: 'Default for new tasks' })
    .getByRole('button', { name: 'Return: Done → New' });
  // SVG markerUnits defines whether the renderer scales a head with its line width.
  const headSize = () =>
    arrow
      .locator('path')
      .first()
      .evaluate((element) => {
        const path = element as SVGPathElement;
        const marker = path.ownerSVGElement!.querySelector('marker')!;
        const scale =
          marker.markerUnits.baseVal === SVGMarkerElement.SVG_MARKERUNITS_STROKEWIDTH
            ? Number.parseFloat(getComputedStyle(path).strokeWidth)
            : 1;
        return [
          marker.markerWidth.baseVal.value * scale,
          marker.markerHeight.baseVal.value * scale,
        ];
      });
  const unselectedHead = await headSize();
  const offsets = await arrow
    .locator('path')
    .first()
    .evaluate((element) => {
      const path = element as SVGPathElement;
      const length = path.getTotalLength();
      const end = path.getPointAtLength(length);
      const tangent = path.getPointAtLength(length - 0.1);
      const dx = end.x - tangent.x;
      const dy = end.y - tangent.y;
      return [4, 8, 14].map((distance) => {
        const point = path.getPointAtLength(length - distance);
        return Math.abs((point.x - end.x) * dy - (point.y - end.y) * dx) / Math.hypot(dx, dy);
      });
    });
  for (const offset of offsets) expect(offset).toBeLessThan(0.5);
  await arrow.locator('text, foreignObject').click();
  expect(await headSize()).toEqual(unselectedHead);
  await expect(arrow).toBeFocused();
  await expect(arrow).toHaveCSS('outline-style', 'none');
  await page.keyboard.press('Space');
  await expect(arrow).toBeFocused();
  await expect(arrow).toHaveCSS('outline-style', 'solid');
});

test('authors an empty canvas, retries the frozen publication and separately selects the default over HTTP', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  let created: WorkflowResource | undefined;
  let selected = { version: 7, workflow };
  const commands: {
    path: string;
    body: unknown;
    raw: string | null;
    precondition: string | undefined;
    key: string | undefined;
  }[] = [];
  const forbiddenRequests: string[] = [];
  page.on('request', (request) => {
    if (/\/api\/v1\/(?:integrations|factory)/u.test(request.url()))
      forbiddenRequests.push(request.url());
  });
  await page.route('**/api/v1/projects/*/issue-workflow**', async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (request.method() !== 'POST') {
      await route.fulfill({
        json: path.endsWith('/issue-workflow-default')
          ? selected
          : path.endsWith('/issue-workflows')
            ? {
                items: [workflow, ...(created ? [created] : [])].map((resource) => ({
                  ...resource,
                  name: resource.definition.name,
                })),
                next_cursor: null,
              }
            : created,
      });
      return;
    }
    const body = request.postDataJSON();
    commands.push({
      path,
      body,
      raw: request.postData(),
      precondition: request.headers()['if-match'],
      key: request.headers()['idempotency-key'],
    });
    if (path.endsWith('/validate')) {
      await route.fulfill({ json: body.definition });
    } else if (path.endsWith('/issue-workflow-default')) {
      selected = { version: 8, workflow: created! };
      await route.fulfill({ json: { disposition: 'applied', resource: selected } });
    } else if (!created) {
      // The server applied the command, but its response did not reach the browser.
      created = { ...workflow, id: body.id, version: 1, definition: body.definition };
      await route.abort('failed');
    } else {
      await route.fulfill({ json: { disposition: 'replayed', resource: created } });
    }
  });
  await page.goto(`/projects/${FIXTURE_IDS.project}`);
  await page.getByRole('link', { name: 'Task workflow' }).click();
  await page.getByRole('button', { name: 'Create workflow' }).click();
  const draft = page.getByRole('region', { name: 'Workflow draft' });
  const inspector = draft.getByRole('complementary', { name: 'Workflow properties' });
  await expect(draft.getByRole('article')).toHaveCount(0);
  await inspector.getByRole('textbox', { name: 'Workflow name' }).fill('Support workflow');
  await draft.getByRole('button', { name: 'Add status', exact: true }).click();
  await inspector.getByRole('textbox', { name: 'Status name' }).fill('Received');
  await draft.getByRole('button', { name: 'Add status', exact: true }).click();
  await inspector.getByRole('textbox', { name: 'Status name' }).fill('Answered');
  await inspector.getByRole('checkbox', { name: 'Terminal status' }).check();
  await draft.getByRole('group', { name: 'Workflow graph' }).scrollIntoViewIfNeeded();
  const source = (await draft
    .getByRole('button', { name: 'Connect from Received', exact: true })
    .boundingBox())!;
  const target = (await draft.getByRole('article', { name: 'Answered' }).boundingBox())!;
  await page.mouse.move(source.x + source.width / 2, source.y + source.height / 2);
  await page.mouse.down();
  await page.mouse.move(target.x + target.width / 2, target.y + target.height / 2, { steps: 6 });
  await page.mouse.up();
  await inspector.getByRole('textbox', { name: 'Transition name' }).fill('Answer');
  await inspector.getByRole('checkbox', { name: 'Pipeline action' }).check();
  await inspector.getByRole('checkbox', { name: 'Required checks passed' }).check();
  expect(commands).toEqual([]);
  await page.getByRole('button', { name: 'Validate draft' }).click();
  const review = page.getByRole('button', { name: 'Review publication' });
  await expect(review).toBeEnabled();
  await review.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog');
  const publish = dialog.getByRole('button', { name: 'Publish version' });
  await expect(publish).toBeFocused();
  await expect(dialog).toContainText('Statuses: Received, Answered');
  await expect(dialog).toContainText('Initial status: Received');
  await expect(dialog).toContainText('Terminal statuses: Answered');
  await expect(dialog).toContainText('Answer: Received → Answered');
  await expect(dialog).toContainText('Allowed origins: Manual move, Pipeline action');
  await expect(dialog).toContainText('Required evidence: Required checks passed');
  await page.keyboard.press('Escape');
  await expect(review).toBeFocused();
  await review.click();
  await publish.click();
  await dialog.getByRole('button', { name: 'Retry same command' }).click();
  await expect(dialog).toContainText('Published version 1.');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(review).toBeFocused();
  expect(commands).toHaveLength(3);
  expect(commands[0]?.precondition).toBeUndefined();
  expect(commands[0]?.key).toBeUndefined();
  expect(commands[0]?.body).toEqual({ definition: created!.definition });
  expect(commands[1]?.precondition).toBeUndefined();
  expect(commands[1]?.path).toBe(`/api/v1/projects/${FIXTURE_IDS.project}/issue-workflows`);
  expect(commands[1]?.key).toMatch(/^[0-9a-f-]{36}$/u);
  expect(commands[2]).toEqual(commands[1]);
  const [received, answered] = created!.definition.statuses;
  expect(created!.id).not.toBe(workflow.id);
  expect(commands[1]?.body).toEqual({
    id: expect.stringMatching(/^[0-9a-f-]{36}$/u),
    definition: {
      name: 'Support workflow',
      statuses: [
        { id: expect.stringMatching(/^[0-9a-f-]{36}$/u), name: 'Received' },
        { id: expect.stringMatching(/^[0-9a-f-]{36}$/u), name: 'Answered' },
      ],
      initial_status_id: received!.id,
      terminal_status_ids: [answered!.id],
      transitions: [
        {
          id: expect.stringMatching(/^[0-9a-f-]{36}$/u),
          name: 'Answer',
          from: received!.id,
          to: answered!.id,
          origins: ['manual', 'pipeline'],
          evidence: ['passed_required_checks'],
        },
      ],
    },
  });
  const current = page.getByRole('region', { name: 'Default for new tasks' });
  await expect(current).toContainText('Local workflow');
  await page.getByRole('button', { name: 'Open Support workflow' }).click();
  await page.getByRole('button', { name: 'Use for new tasks' }).click();
  await expect(dialog).toContainText('Support workflow · Version 1');
  await expect(dialog).toContainText('Existing tasks keep their pinned workflow.');
  expect(commands).toHaveLength(3);
  await dialog.getByRole('button', { name: 'Select default', exact: true }).click();
  await expect(dialog).toContainText('Default selected for new tasks.');
  await expect(current).toContainText('Support workflow');
  expect(commands).toHaveLength(4);
  expect(commands[3]?.body).toEqual({ workflow_id: created!.id, workflow_version: 1 });
  expect(commands[3]?.precondition).toBe('"7"');
  expect(commands[3]?.key).toMatch(/^[0-9a-f-]{36}$/u);
  expect(commands[3]?.key).not.toBe(commands[1]?.key);
  expect(forbiddenRequests).toEqual([]);
});

test('shows Russian canvas settings and a selected status inspector at narrow width', async ({
  page,
}) => {
  await installOperatorFixture(page);
  await page.addInitScript(() =>
    localStorage.setItem(
      'octacity.console.preferences',
      JSON.stringify({
        version: 2,
        language: 'ru',
        theme: 'light',
        favorites: [],
        recents: [],
        expanded: [],
        explorerOpen: true,
        explorerWidth: 288,
        notificationLastOpenedAt: null,
      }),
    ),
  );
  await page.setViewportSize({ width: 640, height: 900 });
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Редактировать workflow по умолчанию' }).click();
  await expect(page.getByRole('heading', { name: 'Workflow задач', level: 1 })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Добавить переход' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Проверить публикацию' })).toBeDisabled();
  const draft = page.getByRole('region', { name: 'Черновик workflow' });
  await expect(draft.getByRole('group', { name: 'Граф workflow' })).toBeVisible();
  await draft.getByRole('button', { name: 'Выбрать статус New' }).click();
  await expect(draft.getByRole('radio', { name: 'Начальный статус' })).toBeChecked();
  await expect(draft.getByRole('checkbox', { name: 'Завершающий статус' })).toBeVisible();
  await expect(
    draft
      .getByRole('complementary', { name: 'Свойства workflow' })
      .getByRole('textbox', { name: 'Название статуса' }),
  ).toBeVisible();
  const inspector = draft.getByRole('complementary', { name: 'Свойства workflow' });
  const workflowField = await inspector
    .getByRole('textbox', { name: 'Название workflow' })
    .locator('..')
    .boundingBox();
  const statusField = await inspector
    .getByRole('textbox', { name: 'Название статуса' })
    .locator('..')
    .boundingBox();
  expect(statusField!.y - workflowField!.y - workflowField!.height).toBeGreaterThanOrEqual(8);
  expect(
    await page.evaluate(() =>
      Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth),
    ),
  ).toBe(0);
});

test('retains the draft and presents a denied publication without retrying or falling back to another API', async ({
  page,
}) => {
  await installOperatorFixture(page);
  let publications = 0;
  await page.route('**/api/v1/projects/*/issue-workflows/**', async (route) => {
    if (route.request().url().endsWith('/validate')) {
      await route.fulfill({ json: route.request().postDataJSON().definition });
      return;
    }
    publications++;
    await route.fulfill({
      status: 403,
      json: {
        code: 'forbidden',
        message: 'Workflow administration is not allowed.',
        request_id: 'workflow-denied',
      },
    });
  });
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  await page.getByRole('textbox', { name: 'Workflow name' }).fill('My policy');
  await page.getByRole('button', { name: 'Validate draft' }).click();
  await page.getByRole('button', { name: 'Review publication' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Publish version' }).click();
  await expect(page.getByRole('dialog')).toContainText('forbidden');
  await expect(page.getByRole('button', { name: 'Retry same command' })).toHaveCount(0);
  expect(publications).toBe(1);
  await page.getByRole('dialog').getByRole('button', { name: 'Abandon' }).click();
  await expect(page.getByRole('textbox', { name: 'Workflow name' })).toHaveValue('My policy');
});

test('keeps visible policies editable when the selected default is hidden', async ({ page }) => {
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  workflow.id = '20000000-0000-4000-8000-000000000002';
  workflow.definition.name = 'Visible policy';
  await page.route('**/api/v1/projects/*/issue-workflow-default', (route) =>
    route.fulfill({
      status: 404,
      json: {
        code: 'not_found',
        message: 'Default is not visible.',
        request_id: 'workflow-default-hidden',
      },
    }),
  );
  await page.route('**/api/v1/projects/*/issue-workflows**', (route) =>
    route.fulfill({
      json: new URL(route.request().url()).pathname.endsWith('/issue-workflows')
        ? { items: [{ ...workflow, name: workflow.definition.name }], next_cursor: null }
        : workflow,
    }),
  );
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await expect(page.getByRole('alert')).toContainText('not_found');
  await page.getByRole('button', { name: 'Open Visible policy' }).click();
  await expect(page.getByRole('heading', { name: 'Visible policy' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Use for new tasks' })).toBeDisabled();
  await page.getByRole('button', { name: 'Edit this version' }).click();
  await expect(page.getByRole('textbox', { name: 'Workflow name' })).toHaveValue('Visible policy');
});

test('retains canvas edits after a publication conflict and requires validation and fresh review', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  workflow.definition.transitions = [
    {
      id: '30000000-0000-4000-8000-000000000001',
      name: 'Complete',
      from: workflow.definition.statuses[0]!.id,
      to: workflow.definition.statuses[1]!.id,
      origins: ['manual'],
      evidence: [],
    },
  ];
  await page.route('**/api/v1/projects/*/issue-workflow-default', (route) =>
    route.fulfill({ json: { version: 7, workflow } }),
  );
  const validated: WorkflowDefinition[] = [];
  const publications: {
    body: string | null;
    precondition: string | undefined;
    key: string | undefined;
  }[] = [];
  await page.route('**/api/v1/projects/*/issue-workflows/**', async (route) => {
    const request = route.request();
    if (request.method() === 'GET') {
      await route.fulfill({ json: { ...workflow, version: 5 } });
    } else if (request.url().endsWith('/validate')) {
      validated.push(request.postDataJSON().definition);
      await route.fulfill({ json: request.postDataJSON().definition });
    } else {
      publications.push({
        body: request.postData(),
        precondition: request.headers()['if-match'],
        key: request.headers()['idempotency-key'],
      });
      await route.fulfill(
        publications.length === 1
          ? {
              status: 409,
              json: {
                code: 'conflict',
                message: 'Workflow changed.',
                request_id: 'workflow-stale',
              },
            }
          : {
              json: {
                disposition: 'applied',
                resource: {
                  ...workflow,
                  version: 6,
                  definition: request.postDataJSON().definition,
                },
              },
            },
      );
    }
  });
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  const draft = page.getByRole('region', { name: 'Workflow draft' });
  await page.getByRole('textbox', { name: 'Workflow name' }).fill('Retained draft');
  await draft.getByRole('button', { name: 'Select status Done', exact: true }).click();
  await page.getByRole('textbox', { name: 'Status name' }).fill('Answered');
  await draft.getByRole('button', { name: 'Complete: New → Answered' }).click();
  await page.getByRole('textbox', { name: 'Transition name' }).fill('Answer');
  await page.getByRole('button', { name: 'Validate draft' }).click();
  await page.getByRole('button', { name: 'Review publication' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Publish version' }).click();
  const refresh = page.getByRole('button', { name: 'Refresh current data' });
  await expect(refresh).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByRole('textbox', { name: 'Workflow name' })).toHaveValue('Retained draft');
  await expect(page.getByRole('button', { name: 'Review publication' })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Validate draft' })).toBeFocused();
  await expect(draft.getByRole('article', { name: 'Answered', exact: true })).toBeVisible();
  await expect(draft.getByRole('button', { name: 'Answer: New → Answered' })).toBeVisible();
  expect(validated).toHaveLength(1);
  expect(publications).toHaveLength(1);
  await page.keyboard.press('Enter');
  await page.getByRole('button', { name: 'Review publication' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Based on version 5');
  await expect(dialog).toContainText('Answer: New → Answered');
  expect(publications).toHaveLength(1);
  await dialog.getByRole('button', { name: 'Publish version' }).click();
  await expect(dialog).toContainText('Published version 6.');
  expect(validated).toHaveLength(2);
  expect(validated[1]).toEqual(validated[0]);
  expect(publications).toHaveLength(2);
  expect(publications[0]?.precondition).toBe('"3"');
  expect(publications[1]?.precondition).toBe('"5"');
  expect(publications[1]?.body).toBe(publications[0]?.body);
  expect(publications[0]?.key).toMatch(/^[0-9a-f-]{36}$/u);
  expect(publications[1]?.key).toMatch(/^[0-9a-f-]{36}$/u);
  expect(publications[1]?.key).not.toBe(publications[0]?.key);
});

test('connects nodes by dragging and keyboard controls into the same workflow DTO', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await installOperatorFixture(page);
  const validated: WorkflowDefinition[] = [];
  await page.route('**/api/v1/projects/*/issue-workflows/validate', async (route) => {
    const definition: WorkflowDefinition = route.request().postDataJSON().definition;
    validated.push(definition);
    await route.fulfill({ json: definition });
  });
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  const draft = page.getByRole('region', { name: 'Workflow draft' });
  const inspector = draft.getByRole('complementary', { name: 'Workflow properties' });
  const source = draft.getByRole('button', { name: 'Connect from New', exact: true });
  await expect(source).toHaveCount(1);
  await draft.getByRole('group', { name: 'Workflow graph' }).scrollIntoViewIfNeeded();
  const start = (await source.boundingBox())!;
  const sourceCard = (await draft
    .getByRole('article', { name: 'New', exact: true })
    .boundingBox())!;
  expect(Math.abs(start.x + start.width / 2 - sourceCard.x - sourceCard.width)).toBeLessThan(1);
  expect(Math.abs(start.y + start.height / 2 - sourceCard.y - sourceCard.height / 2)).toBeLessThan(
    1,
  );

  const target = (await draft.getByRole('article', { name: 'Done', exact: true }).boundingBox())!;
  await page.mouse.move(start.x + start.width / 2, start.y + start.height / 2);
  await page.mouse.down();
  await page.mouse.move(target.x + target.width / 2, target.y + target.height / 2, { steps: 6 });
  const preview = draft.getByRole('img', { name: 'Drawing connection' });
  await expect(preview).toBeAttached();
  await expect
    .poll(() => preview.evaluate((element) => (element as SVGPathElement).getTotalLength()))
    .toBeGreaterThan(0);
  expect(validated).toHaveLength(0);
  await page.mouse.up();
  const name = inspector.getByRole('textbox', { name: 'Transition name' });
  await expect(name).toBeFocused();
  await expect(draft.getByRole('button', { name: 'Transition 1: New → Done' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await name.fill('Complete');
  const lineStart = await draft
    .getByRole('button', { name: 'Complete: New → Done' })
    .locator('path')
    .first()
    .evaluate((element) => {
      const path = element as SVGPathElement;
      const point = path.getPointAtLength(0).matrixTransform(path.getScreenCTM()!);
      return { x: point.x, y: point.y };
    });
  const connectedCard = (await draft
    .getByRole('article', { name: 'New', exact: true })
    .boundingBox())!;
  expect(Math.abs(lineStart.x - connectedCard.x - connectedCard.width)).toBeLessThan(1);
  expect(Math.abs(lineStart.y - connectedCard.y - connectedCard.height / 2)).toBeLessThan(1);

  const nameField = (await name.locator('..').boundingBox())!;
  const fromField = (await inspector
    .getByRole('combobox', { name: 'From status' })
    .locator('..')
    .locator('..')
    .boundingBox())!;
  expect(fromField.y - nameField.y - nameField.height).toBeGreaterThanOrEqual(8);
  const manual = (await inspector
    .getByRole('checkbox', { name: 'Manual move' })
    .locator('..')
    .boundingBox())!;
  const pipeline = (await inspector
    .getByRole('checkbox', { name: 'Pipeline action' })
    .locator('..')
    .boundingBox())!;
  expect(pipeline.y - manual.y - manual.height).toBeGreaterThanOrEqual(8);

  await inspector.getByRole('checkbox', { name: 'Pipeline action' }).check();
  await inspector.getByRole('checkbox', { name: 'Accepted execution' }).check();
  await page.getByRole('button', { name: 'Validate draft' }).click();
  await expect.poll(() => validated.length).toBe(1);
  const pointerEdge = validated[0]!.transitions[0]!;
  expect(pointerEdge).toEqual({
    id: expect.stringMatching(/^[0-9a-f-]{36}$/u),
    name: 'Complete',
    from: '10000000-0000-4000-8000-000000000001',
    to: '10000000-0000-4000-8000-000000000002',
    origins: ['manual', 'pipeline'],
    evidence: ['accepted_execution'],
  });
  await inspector.getByRole('button', { name: 'Remove transition' }).click();
  await expect(draft.getByRole('button', { name: 'Select status New', exact: true })).toBeFocused();
  await source.focus();
  await page.keyboard.press('Enter');
  const connect = inspector.getByRole('button', { name: 'Connect statuses', exact: true });
  await connect.focus();
  await page.keyboard.press('Enter');
  await expect(name).toBeFocused();
  await name.fill('Complete');
  await inspector.getByRole('checkbox', { name: 'Pipeline action' }).check();
  await inspector.getByRole('checkbox', { name: 'Accepted execution' }).check();
  await page.getByRole('button', { name: 'Validate draft' }).click();
  await expect.poll(() => validated.length).toBe(2);
  expect(validated[1]!.statuses).toEqual(validated[0]!.statuses);
  expect(validated[1]!.transitions).toEqual([{ ...pointerEdge, id: expect.any(String) }]);
  expect(validated[1]!.transitions[0]!.id).not.toBe(pointerEdge.id);
});

test('cancels pointer connections on self, blank space, another canvas and Escape without mutating the draft', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  const validated: WorkflowDefinition[] = [];
  await page.route('**/api/v1/projects/*/issue-workflows/validate', async (route) => {
    const definition: WorkflowDefinition = route.request().postDataJSON().definition;
    validated.push(definition);
    await route.fulfill({ json: definition });
  });
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  const draft = page.getByRole('region', { name: 'Workflow draft' });
  const graph = draft.getByRole('group', { name: 'Workflow graph' });
  const source = draft.getByRole('button', { name: 'Connect from New', exact: true });
  const preview = draft.getByRole('img', { name: 'Drawing connection' });
  for (const mode of ['self', 'blank', 'other canvas', 'Escape']) {
    await graph.scrollIntoViewIfNeeded();
    const start = (await source.boundingBox())!;
    await page.mouse.move(start.x + start.width / 2, start.y + start.height / 2);
    await page.mouse.down();
    await page.mouse.move(start.x + start.width + 20, start.y + start.height + 10);
    await expect(preview).toBeVisible();
    if (mode === 'blank') {
      const rect = (await graph.boundingBox())!;
      await page.mouse.move(rect.x + 20, rect.y + 20);
    } else {
      const region =
        mode === 'other canvas'
          ? page.getByRole('region', { name: 'Default for new tasks' })
          : draft;
      const target = region.getByRole('article', {
        name: mode === 'self' ? 'New' : 'Done',
        exact: true,
      });
      await target.scrollIntoViewIfNeeded();
      const rect = (await target.boundingBox())!;
      await page.mouse.move(rect.x + rect.width / 2, rect.y + rect.height / 2);
    }
    if (mode === 'Escape') {
      await page.keyboard.press('Escape');
      await expect(draft.getByRole('button', { name: 'Add transition' })).toBeFocused();
    }
    await page.mouse.up();
    await expect(preview).toHaveCount(0);
    await expect(
      draft.getByRole('group', { name: 'Permitted transitions' }).getByRole('button'),
    ).toHaveCount(0);
  }
  expect(validated).toHaveLength(0);
  await page.getByRole('button', { name: 'Validate draft' }).click();
  await expect.poll(() => validated.length).toBe(1);
  expect(validated[0]).toEqual(workflow.definition);
});

test('lays out compact workflow cards left to right with branches in one column and unobstructed connections', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  const [first, second, third] = workflow.definition.statuses;
  workflow.definition.transitions = [
    {
      id: '30000000-0000-4000-8000-000000000001',
      name: 'Complete',
      from: first!.id,
      to: second!.id,
      origins: ['manual'],
      evidence: [],
    },
    {
      id: '30000000-0000-4000-8000-000000000002',
      name: 'Decline',
      from: first!.id,
      to: third!.id,
      origins: ['manual'],
      evidence: [],
    },
    {
      id: '30000000-0000-4000-8000-000000000003',
      name: 'Reopen',
      from: third!.id,
      to: first!.id,
      origins: ['manual'],
      evidence: [],
    },
  ];
  await page.route('**/api/v1/projects/*/issue-workflow-default', (route) =>
    route.fulfill({ json: { version: 7, workflow } }),
  );
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  const graph = page
    .getByRole('region', { name: 'Default for new tasks' })
    .getByRole('group', { name: 'Workflow graph' });
  await graph.scrollIntoViewIfNeeded();
  const inbox = (await graph.getByRole('article', { name: 'New', exact: true }).boundingBox())!;
  const done = (await graph.getByRole('article', { name: 'Done', exact: true }).boundingBox())!;
  const declined = (await graph
    .getByRole('article', { name: 'Declined', exact: true })
    .boundingBox())!;
  const surface = (await graph.locator('svg').boundingBox())!;
  expect(inbox.height).toBeLessThanOrEqual(112);
  expect(done.x).toBeGreaterThan(inbox.x + inbox.width);
  expect(declined.x).toBe(done.x);
  expect(done.y).toBe(inbox.y);
  expect(declined.y).toBeGreaterThan(done.y + done.height);
  for (const [name, from, to, other] of [
    ['Complete: New → Done', inbox, done, declined],
    ['Decline: New → Declined', inbox, declined, done],
    ['Reopen: Declined → New', declined, inbox, done],
  ] as const) {
    const samples = await graph
      .getByRole('button', { name })
      .locator('path')
      .first()
      .evaluate((element) => {
        const path = element as SVGPathElement;
        const transform = path.getScreenCTM()!;
        return Array.from({ length: 41 }, (_, index) => {
          const point = path
            .getPointAtLength((path.getTotalLength() * index) / 40)
            .matrixTransform(transform);
          return { x: point.x, y: point.y };
        });
      });
    expect(samples[0]).toEqual({ x: from.x + from.width, y: from.y + from.height / 2 });
    expect(samples.at(-1)).toEqual({ x: to.x, y: to.y + to.height / 2 });
    for (const point of samples) {
      expect(point.x).toBeGreaterThanOrEqual(surface.x);
      expect(point.x).toBeLessThanOrEqual(surface.x + surface.width);
      expect(point.y).toBeGreaterThanOrEqual(surface.y);
      expect(point.y).toBeLessThanOrEqual(surface.y + surface.height);
      expect(
        point.x > other.x &&
          point.x < other.x + other.width &&
          point.y > other.y &&
          point.y < other.y + other.height,
      ).toBe(false);
    }
    if (name.startsWith('Complete:')) {
      expect(samples.every((point) => Math.abs(point.y - inbox.y - inbox.height / 2) < 0.5)).toBe(
        true,
      );
    }
  }
});

test('moves a status with attached arrows without changing validation, publication or browser storage', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  workflow.definition.transitions = [
    {
      id: '30000000-0000-4000-8000-000000000001',
      name: 'Complete',
      from: workflow.definition.statuses[0]!.id,
      to: workflow.definition.statuses[1]!.id,
      origins: ['manual'],
      evidence: [],
    },
  ];
  const commands: { definition: WorkflowDefinition }[] = [];
  await page.route('**/api/v1/projects/*/issue-workflow-default', (route) =>
    route.fulfill({ json: { version: 7, workflow } }),
  );
  await page.route('**/api/v1/projects/*/issue-workflows/**', async (route) => {
    if (route.request().method() !== 'POST') return route.fulfill({ json: workflow });
    const body: { definition: WorkflowDefinition } = route.request().postDataJSON();
    commands.push(body);
    await route.fulfill({
      json: route.request().url().endsWith('/validate')
        ? body.definition
        : {
            disposition: 'applied',
            resource: { ...workflow, version: 4, definition: body.definition },
          },
    });
  });
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  await page.getByRole('button', { name: 'Validate draft' }).click();
  const review = page.getByRole('button', { name: 'Review publication' });
  await expect(review).toBeEnabled();
  const draft = page.getByRole('region', { name: 'Workflow draft' });
  const graph = draft.getByRole('group', { name: 'Workflow graph' });
  const node = draft.getByRole('button', { name: 'Select status Done', exact: true });
  await node.click();
  await graph.scrollIntoViewIfNeeded();
  const before = (await node.boundingBox())!;
  const canvasBefore = (await graph.boundingBox())!;
  const stored = await page.evaluate(() => ({
    local: { ...localStorage },
    session: { ...sessionStorage },
  }));
  await page.mouse.move(before.x + before.width / 2, before.y + before.height / 2);
  await page.mouse.down();
  await page.mouse.move(before.x + before.width / 2 + 80, before.y + before.height / 2 + 64, {
    steps: 6,
  });
  await page.mouse.up();
  await expect.poll(async () => (await node.boundingBox())!.x - before.x).toBeCloseTo(80, 0);
  expect((await node.boundingBox())!.y - before.y).toBeCloseTo(64, 0);
  const end = await draft
    .getByRole('button', { name: 'Complete: New → Done' })
    .locator('path')
    .first()
    .evaluate((element) => {
      const path = element as SVGPathElement;
      const point = path
        .getPointAtLength(path.getTotalLength())
        .matrixTransform(path.getScreenCTM()!);
      return { x: point.x, y: point.y };
    });
  const moved = (await draft.getByRole('article', { name: 'Done', exact: true }).boundingBox())!;
  expect(end.x).toBeCloseTo(moved.x, 0);
  expect(end.y).toBeCloseTo(moved.y + moved.height / 2, 0);
  await expect(review).toBeEnabled();
  expect(commands).toEqual([{ definition: workflow.definition }]);
  expect(
    await page.evaluate(() => ({ local: { ...localStorage }, session: { ...sessionStorage } })),
  ).toEqual(stored);
  await review.click();
  await page.getByRole('dialog').getByRole('button', { name: 'Publish version' }).click();
  await expect(page.getByRole('dialog')).toContainText('Published version 4.');
  expect(commands).toEqual([
    { definition: workflow.definition },
    { definition: workflow.definition },
  ]);
  await page.reload();
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  await graph.scrollIntoViewIfNeeded();
  const reset = (await node.boundingBox())!;
  const canvasAfter = (await graph.boundingBox())!;
  expect(reset.x - canvasAfter.x).toBeCloseTo(before.x - canvasBefore.x, 0);
  expect(reset.y - canvasAfter.y).toBeCloseTo(before.y - canvasBefore.y, 0);
});

test('cancels a node drag with Escape or pointer cancellation and keeps focus on the node', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await installOperatorFixture(page);
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  const draft = page.getByRole('region', { name: 'Workflow draft' });
  await draft.getByRole('group', { name: 'Workflow graph' }).scrollIntoViewIfNeeded();
  const node = draft.getByRole('button', { name: 'Select status Done', exact: true });
  for (const mode of ['Escape', 'pointercancel']) {
    const before = (await node.boundingBox())!;
    await node.evaluate((element) => {
      element.removeAttribute('data-observed-pointer');
      element.addEventListener(
        'pointerdown',
        (event) => {
          if (event instanceof PointerEvent)
            element.setAttribute('data-observed-pointer', String(event.pointerId));
        },
        { once: true },
      );
    });
    await page.mouse.move(before.x + before.width / 2, before.y + before.height / 2);
    await page.mouse.down();
    const pointerId = await node.getAttribute('data-observed-pointer');
    expect(pointerId).not.toBeNull();
    await page.mouse.move(before.x + before.width / 2 + 40, before.y + before.height / 2 + 40, {
      steps: 4,
    });
    await expect.poll(async () => (await node.boundingBox())!.x - before.x).toBeCloseTo(40, 0);
    if (mode === 'Escape') await page.keyboard.press('Escape');
    else
      await node.dispatchEvent('pointercancel', {
        pointerId: Number(pointerId),
        pointerType: 'mouse',
        isPrimary: true,
      });
    await page.mouse.up();
    await expect.poll(async () => (await node.boundingBox())!.x).toBeCloseTo(before.x, 0);
    expect((await node.boundingBox())!.y).toBeCloseTo(before.y, 0);
    await expect(node).toBeFocused();
  }
  await draft.getByRole('button', { name: 'Add transition', exact: true }).click();
  await draft.getByRole('button', { name: 'Connect statuses', exact: true }).click();
  const source = (await draft.getByRole('article', { name: 'New', exact: true }).boundingBox())!;
  const target = draft.getByRole('article', { name: 'Done', exact: true });
  await expect
    .poll(async () => (await target.boundingBox())!.x)
    .toBeGreaterThan(source.x + source.width);
  expect((await target.boundingBox())!.y).toBe(source.y);
});

test('zooms the canvas and keeps node dragging and connection previews aligned with the pointer', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await installOperatorFixture(page);
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  const draft = page.getByRole('region', { name: 'Workflow draft' });
  const graph = draft.getByRole('group', { name: 'Workflow graph' });
  const sourceNode = draft.getByRole('button', { name: 'Select status New', exact: true });
  await sourceNode.click();
  await graph.scrollIntoViewIfNeeded();
  const initial = (await sourceNode.boundingBox())!;
  await draft.getByRole('button', { name: 'Zoom in', exact: true }).click();
  await expect(draft.getByRole('status', { name: 'Canvas zoom' })).toHaveText('125%');
  await expect
    .poll(async () => (await sourceNode.boundingBox())!.width / initial.width)
    .toBeCloseTo(1.25);
  const enlarged = (await sourceNode.boundingBox())!;
  expect(enlarged.width / initial.width).toBeCloseTo(1.25);
  expect(enlarged.x + enlarged.width / 2).toBeCloseTo(initial.x + initial.width / 2, 0);
  const destination = draft.getByRole('button', { name: 'Select status Done', exact: true });
  const beforeMove = (await destination.boundingBox())!;
  await page.mouse.move(beforeMove.x + beforeMove.width / 2, beforeMove.y + beforeMove.height / 2);
  await page.mouse.down();
  await page.mouse.move(
    beforeMove.x + beforeMove.width / 2 + 80,
    beforeMove.y + beforeMove.height / 2 + 40,
    { steps: 5 },
  );
  await page.mouse.up();
  await expect
    .poll(async () => (await destination.boundingBox())!.x - beforeMove.x)
    .toBeCloseTo(80, 0);
  expect((await destination.boundingBox())!.y - beforeMove.y).toBeCloseTo(40, 0);
  const source = (await draft
    .getByRole('button', { name: 'Connect from New', exact: true })
    .boundingBox())!;
  const target = (await destination.boundingBox())!;
  await page.mouse.move(source.x + source.width / 2, source.y + source.height / 2);
  await page.mouse.down();
  const pointer = { x: target.x + target.width / 2, y: target.y + target.height / 2 };
  await page.mouse.move(pointer.x, pointer.y, { steps: 5 });
  const preview = draft.getByRole('img', { name: 'Drawing connection' });
  await expect
    .poll(async () =>
      preview.evaluate((element) => {
        const path = element as SVGPathElement;
        const end = path
          .getPointAtLength(path.getTotalLength())
          .matrixTransform(path.getScreenCTM()!);
        return { x: Math.round(end.x), y: Math.round(end.y) };
      }),
    )
    .toEqual({ x: Math.round(pointer.x), y: Math.round(pointer.y) });
  await page.mouse.up();
  await expect(draft.getByRole('textbox', { name: 'Transition name' })).toBeFocused();
  await expect(draft.getByRole('button', { name: 'Transition 1: New → Done' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await draft.getByRole('button', { name: 'Zoom out', exact: true }).click();
  await expect(draft.getByRole('status', { name: 'Canvas zoom' })).toHaveText('100%');
});

test('pans the canvas with pointer and keyboard and fits every node without requests or persistence', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await installOperatorFixture(page);
  const mutations: string[] = [];
  page.on('request', (request) => {
    if (request.method() === 'POST') mutations.push(request.url());
  });
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  const draft = page.getByRole('region', { name: 'Workflow draft' });
  const graph = draft.getByRole('group', { name: 'Workflow graph' });
  await graph.scrollIntoViewIfNeeded();
  const node = draft.getByRole('article', { name: 'New', exact: true });
  const before = (await node.boundingBox())!;
  const canvas = (await graph.boundingBox())!;
  const stored = await page.evaluate(() => ({
    local: { ...localStorage },
    session: { ...sessionStorage },
  }));
  await page.mouse.move(canvas.x + canvas.width - 24, canvas.y + canvas.height - 24);
  await page.mouse.down();
  await page.mouse.move(canvas.x + canvas.width + 76, canvas.y + canvas.height + 36, { steps: 5 });
  await page.mouse.up();
  await expect.poll(async () => (await node.boundingBox())!.x - before.x).toBeCloseTo(100, 0);
  expect((await node.boundingBox())!.y - before.y).toBeCloseTo(60, 0);
  await graph.focus();
  await expect(graph).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect.poll(async () => (await node.boundingBox())!.x - before.x).toBeCloseTo(60, 0);
  await expect(graph).toHaveCSS('outline-style', 'solid');
  for (let index = 0; index < 3; index++)
    await draft.getByRole('button', { name: 'Zoom in', exact: true }).click();
  const fit = draft.getByRole('button', { name: 'Fit graph', exact: true });
  await fit.focus();
  await page.keyboard.press('Enter');
  await expect(draft.getByRole('status', { name: 'Canvas zoom' })).toHaveText('100%');
  await expect(fit).toBeFocused();
  const visible = (await graph.boundingBox())!;
  for (const card of await graph.getByRole('article').all()) {
    const box = (await card.boundingBox())!;
    expect(box.x).toBeGreaterThan(visible.x);
    expect(box.x + box.width).toBeLessThan(visible.x + visible.width);
    expect(box.y).toBeGreaterThan(visible.y);
    expect(box.y + box.height).toBeLessThan(visible.y + visible.height);
  }
  expect(mutations).toEqual([]);
  expect(
    await page.evaluate(() => ({ local: { ...localStorage }, session: { ...sessionStorage } })),
  ).toEqual(stored);
});

for (const language of ['en', 'ru'] as const) {
  test(`keeps the ${language} inspector reachable with keyboard at phone width and reveals focused graph elements`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await installOperatorFixture(page);
    await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
    if (language === 'ru') {
      await page.getByLabel('Operator menu').click();
      await page.getByRole('combobox', { name: 'Language', exact: true }).click();
      await page.getByRole('option', { name: 'Russian', exact: true }).click();
      await page.keyboard.press('Escape');
    }
    const text =
      language === 'en'
        ? {
            edit: 'Edit default workflow',
            draft: 'Workflow draft',
            graph: 'Workflow graph',
            properties: 'Workflow properties',
            select: 'Select status New',
            selected: 'Select status Inbox',
            open: 'Open properties',
            back: 'Back to canvas',
            status: 'Status name',
            add: 'Add transition',
            from: 'From status',
            connect: 'Connect statuses',
            transition: 'Transition name',
            fit: 'Fit graph',
            zoom: 'Canvas zoom',
          }
        : {
            edit: 'Редактировать workflow по умолчанию',
            draft: 'Черновик workflow',
            graph: 'Граф workflow',
            properties: 'Свойства workflow',
            select: 'Выбрать статус New',
            selected: 'Выбрать статус Inbox',
            open: 'Открыть свойства',
            back: 'Вернуться к графу',
            status: 'Название статуса',
            add: 'Добавить переход',
            from: 'Из статуса',
            connect: 'Соединить статусы',
            transition: 'Название перехода',
            fit: 'Показать весь граф',
            zoom: 'Масштаб схемы',
          };
    await page.getByRole('button', { name: text.edit }).click();
    const draft = page.getByRole('region', { name: text.draft });
    const graph = draft.getByRole('group', { name: text.graph });
    const inspector = draft.getByRole('complementary', { name: text.properties });
    const node = graph.getByRole('button', { name: text.select, exact: true });
    await node.focus();
    await page.keyboard.press('Enter');
    const open = draft.getByRole('button', { name: text.open, exact: true });
    await expect(open).toHaveCount(1);
    await open.focus();
    await page.keyboard.press('Enter');
    await expect(inspector).toBeFocused();
    await expect(inspector).toHaveCSS('outline-style', 'solid');
    const status = inspector.getByRole('textbox', { name: text.status });
    await page.keyboard.press('Tab');
    await page.keyboard.press('Tab');
    await expect(status).toBeFocused();
    await expect(status).toBeInViewport();
    await page.keyboard.press('ControlOrMeta+A');
    await page.keyboard.type('Inbox');
    const back = inspector.getByRole('button', { name: text.back, exact: true });
    await back.focus();
    await page.keyboard.press('Enter');
    const selected = graph.getByRole('button', { name: text.selected, exact: true });
    await expect(selected).toBeFocused();
    await expect(selected).toHaveCSS('outline-style', 'solid');
    const visible = (await graph.boundingBox())!;
    await expect.poll(async () => (await selected.boundingBox())!.x).toBeGreaterThan(visible.x);
    await expect
      .poll(async () => {
        const focused = (await selected.boundingBox())!;
        return focused.x + focused.width;
      })
      .toBeLessThan(visible.x + visible.width);
    await draft.getByRole('button', { name: text.add, exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect(inspector.getByRole('combobox', { name: text.from })).toBeFocused();
    await expect(inspector.getByRole('combobox', { name: text.from })).toBeInViewport();
    await inspector.getByRole('button', { name: text.connect, exact: true }).focus();
    await page.keyboard.press('Enter');
    const edgeName = inspector.getByRole('textbox', { name: text.transition });
    await expect(edgeName).toBeFocused();
    await expect(edgeName).toBeInViewport();
    await page.keyboard.type('Complete');
    await back.focus();
    await page.keyboard.press('Enter');
    await expect(graph.getByRole('button', { name: 'Complete: Inbox → Done' })).toBeFocused();
    await draft.getByRole('button', { name: text.fit, exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect(draft.getByRole('status', { name: text.zoom })).toHaveText(/^\d+%$/u);
    expect(
      await page.evaluate(() =>
        Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth),
      ),
    ).toBe(0);
  });
}

test('fits the supported 64-status graph within a small phone viewport', async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 844 });
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  const statuses = Array.from({ length: 64 }, (_, index) => ({
    id: `10000000-0000-4000-8000-${String(index + 1).padStart(12, '0')}`,
    name: `Step ${index + 1}`,
  }));
  workflow.definition = {
    ...workflow.definition,
    statuses,
    initial_status_id: statuses[0]!.id,
    terminal_status_ids: [statuses[63]!.id],
    transitions: statuses.slice(1).map((status, index) => ({
      id: `30000000-0000-4000-8000-${String(index + 1).padStart(12, '0')}`,
      name: `Advance ${index + 1}`,
      from: statuses[index]!.id,
      to: status.id,
      origins: ['manual'],
      evidence: [],
    })),
  };
  await page.route('**/api/v1/projects/*/issue-workflow-default', (route) =>
    route.fulfill({ json: { version: 7, workflow } }),
  );
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  const section = page.getByRole('region', { name: 'Default for new tasks' });
  await section.getByRole('button', { name: 'Fit graph', exact: true }).focus();
  await page.keyboard.press('Enter');
  const graph = section.getByRole('group', { name: 'Workflow graph' });
  const visible = (await graph.boundingBox())!;
  await expect
    .poll(() =>
      graph
        .getByRole('article')
        .evaluateAll((elements) =>
          elements.map((element) => {
            const box = element.getBoundingClientRect();
            return { left: box.left, right: box.right, top: box.top, bottom: box.bottom };
          }),
        )
        .then((boxes) =>
          boxes.every(
            (box) =>
              box.left > visible.x &&
              box.right < visible.x + visible.width &&
              box.top > visible.y &&
              box.bottom < visible.y + visible.height,
          ),
        ),
    )
    .toBe(true);
});

test('keeps the pointer grab point stable on a clipped phone node', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await installOperatorFixture(page);
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  const draft = page.getByRole('region', { name: 'Workflow draft' });
  await draft.getByRole('group', { name: 'Workflow graph' }).scrollIntoViewIfNeeded();
  const node = draft.getByRole('button', { name: 'Select status New', exact: true });
  const before = (await node.boundingBox())!;
  const grab = { x: before.x + before.width / 2, y: before.y + before.height / 2 };
  await page.mouse.move(grab.x, grab.y);
  await page.mouse.down();
  await expect(draft.getByRole('status', { name: 'Canvas zoom' })).toHaveText('100%');
  await page.mouse.move(grab.x + 40, grab.y + 20, { steps: 5 });
  await page.mouse.up();
  await expect
    .poll(async () => {
      const box = (await node.boundingBox())!;
      return box.x + box.width / 2 - grab.x;
    })
    .toBeCloseTo(40, 0);
  expect(
    (await node.boundingBox())!.y + (await node.boundingBox())!.height / 2 - grab.y,
  ).toBeCloseTo(20, 0);
});

for (const gesture of ['node', 'connection', 'pan'] as const) {
  test(`ignores foreign pointer events during ${gesture} gestures`, async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 1080 });
    await installOperatorFixture(page);
    await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
    await page.getByRole('button', { name: 'Edit default workflow' }).click();
    const draft = page.getByRole('region', { name: 'Workflow draft' });
    const graph = draft.getByRole('group', { name: 'Workflow graph' });
    await graph.scrollIntoViewIfNeeded();
    const node = graph.getByRole('button', { name: 'Select status New', exact: true });
    const before = (await node.boundingBox())!;
    const area = (await graph.boundingBox())!;
    const control =
      gesture === 'connection'
        ? (await graph
            .getByRole('button', { name: 'Connect from New', exact: true })
            .boundingBox())!
        : before;
    const grab =
      gesture === 'pan'
        ? { x: area.x + area.width - 200, y: area.y + 24 }
        : { x: control.x + control.width / 2, y: control.y + control.height / 2 };
    await page.mouse.move(grab.x, grab.y);
    await page.mouse.down();
    for (const [index, type] of ['pointerup', 'pointercancel', 'lostpointercapture'].entries()) {
      await graph.dispatchEvent(type, {
        pointerId: 999,
        pointerType: 'touch',
        isPrimary: false,
        bubbles: true,
      });
      const delta = (index + 1) * 40;
      await page.mouse.move(grab.x + delta, grab.y + delta / 2, { steps: 4 });
      if (gesture === 'connection') {
        const preview = graph.getByRole('img', { name: 'Drawing connection' });
        await expect(preview).toBeAttached();
        await expect
          .poll(() =>
            preview.evaluate((element) => {
              const path = element as SVGPathElement;
              return path
                .getPointAtLength(path.getTotalLength())
                .matrixTransform(path.getScreenCTM()!).x;
            }),
          )
          .toBeCloseTo(grab.x + delta, 0);
      } else {
        await expect
          .poll(async () => (await node.boundingBox())!.x - before.x)
          .toBeCloseTo(delta, 0);
      }
    }
    if (gesture !== 'pan') await page.keyboard.press('Escape');
    await page.mouse.up();
    if (gesture === 'node')
      await expect.poll(async () => (await node.boundingBox())!.x).toBeCloseTo(before.x, 0);
    if (gesture === 'connection')
      await expect(graph.getByRole('img', { name: 'Drawing connection' })).toHaveCount(0);
  });
}

test('keeps long transition labels visible between cards with an accessible full name', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  const name = 'Request authorization approval';
  workflow.definition.transitions = [
    {
      id: '30000000-0000-4000-8000-000000000001',
      name,
      from: workflow.definition.statuses[0]!.id,
      to: workflow.definition.statuses[1]!.id,
      origins: ['manual'],
      evidence: [],
    },
  ];
  await page.route('**/api/v1/projects/*/issue-workflow-default', (route) =>
    route.fulfill({ json: { version: 7, workflow } }),
  );
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  const graph = page
    .getByRole('region', { name: 'Default for new tasks' })
    .getByRole('group', { name: 'Workflow graph' });
  await graph.scrollIntoViewIfNeeded();
  const from = (await graph.getByRole('article', { name: 'New', exact: true }).boundingBox())!;
  const to = (await graph.getByRole('article', { name: 'Done', exact: true }).boundingBox())!;
  const arrow = graph.getByRole('button', { name: `${name}: New → Done`, exact: true });
  const label = arrow.locator('text, foreignObject');
  const box = (await label.boundingBox())!;
  expect(box.x).toBeGreaterThan(from.x + from.width);
  expect(box.x + box.width).toBeLessThan(to.x);
  const hit = await page.evaluate(
    ({ x, y }) =>
      document.elementFromPoint(x, y)?.closest('[role="button"]')?.getAttribute('aria-label'),
    { x: box.x + 2, y: box.y + box.height / 2 },
  );
  expect(hit).toBe(`${name}: New → Done`);
  await expect(arrow.locator('div')).toHaveCSS('text-overflow', 'ellipsis');
  await expect(arrow.locator('title')).toHaveText(name);
});

test('fits a single status to the phone viewport instead of fitting unused canvas space', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await installOperatorFixture(page);
  const workflow = workflowFixture(FIXTURE_IDS.project);
  workflow.definition.statuses = workflow.definition.statuses.slice(0, 1);
  workflow.definition.terminal_status_ids = [];
  await page.route('**/api/v1/projects/*/issue-workflow-default', (route) =>
    route.fulfill({ json: { version: 7, workflow } }),
  );
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  const section = page.getByRole('region', { name: 'Default for new tasks' });
  const graph = section.getByRole('group', { name: 'Workflow graph' });
  const node = graph.getByRole('article', { name: 'New', exact: true });
  const originalWidth = (await node.boundingBox())!.width;
  await section.getByRole('button', { name: 'Fit graph', exact: true }).focus();
  await page.keyboard.press('Enter');
  const visible = (await graph.boundingBox())!;
  await expect.poll(async () => (await node.boundingBox())!.width).toBeLessThan(originalWidth);
  await expect
    .poll(async () => (await node.boundingBox())!.width)
    .toBeGreaterThan(visible.width * 0.8);
  const card = (await node.boundingBox())!;
  expect(card.x).toBeGreaterThan(visible.x);
  expect(card.x + card.width).toBeLessThan(visible.x + visible.width);
});

test('uses the selected element as the same zoom anchor for keyboard and toolbar controls', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 1080 });
  await installOperatorFixture(page);
  await page.goto(`/projects/${FIXTURE_IDS.project}/issue-workflows`);
  await page.getByRole('button', { name: 'Edit default workflow' }).click();
  const draft = page.getByRole('region', { name: 'Workflow draft' });
  const graph = draft.getByRole('group', { name: 'Workflow graph' });
  const node = graph.getByRole('button', { name: 'Select status New', exact: true });
  await node.focus();
  await page.keyboard.press('Enter');
  await graph.scrollIntoViewIfNeeded();
  const before = (await node.boundingBox())!;
  const center = before.x + before.width / 2;
  await graph.focus();
  await page.keyboard.press('+');
  await expect(draft.getByRole('status', { name: 'Canvas zoom' })).toHaveText('125%');
  await expect.poll(async () => (await node.boundingBox())!.width / before.width).toBeCloseTo(1.25);
  const keyboardBox = (await node.boundingBox())!;
  expect(keyboardBox.x + keyboardBox.width / 2).toBeCloseTo(center, 0);
  await page.keyboard.press('-');
  await expect(draft.getByRole('status', { name: 'Canvas zoom' })).toHaveText('100%');
  await draft.getByRole('button', { name: 'Zoom in', exact: true }).click();
  await expect.poll(async () => (await node.boundingBox())!.width / before.width).toBeCloseTo(1.25);
  const toolbarBox = (await node.boundingBox())!;
  expect(toolbarBox.x + toolbarBox.width / 2).toBeCloseTo(center, 0);
});
