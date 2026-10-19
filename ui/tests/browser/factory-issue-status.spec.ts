import { expect, test } from '@playwright/test';
import { configurationFixture } from '../fixtures/factory';
import { workflowFixture } from '../fixtures/issueWorkflows';
import { installOperatorFixture } from './operatorFixture';

test.use({ trace: 'off', screenshot: 'off', video: 'off' });
for (const language of ['en', 'ru'] as const) {
  test(`local Issue action uses existing Flow controls and keyboard review (${language})`, async ({
    page,
  }) => {
    await installOperatorFixture(page);
    if (language === 'ru') {
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
            explorerOpen: false,
            explorerWidth: 288,
            notificationLastOpenedAt: null,
          }),
        ),
      );
      await page.setViewportSize({ width: 390, height: 844 });
    }
    const configuration = configurationFixture();
    const owner = configuration.configuration.reference.project_id;
    const flow = configuration.configuration.flow;
    const root = flow.closure.definitions[0]!;
    const node = root.nodes.find((node) => node.action)!;
    root.entry = node.key;
    const workflow = workflowFixture(owner);
    workflow.definition.transitions = [
      {
        id: '30000000-0000-4000-8000-000000000001',
        name: 'Review delivery',
        from: workflow.definition.initial_status_id,
        to: workflow.definition.terminal_status_ids[0]!,
        origins: ['pipeline'],
        evidence: ['passed_required_checks'],
      },
    ];
    node.action = {
      ...node.action!,
      action: 'transition_issue',
      parameters: {
        workflow_id: workflow.id,
        workflow_version: 3,
        transition_id: workflow.definition.transitions[0]!.id,
        outcome: node.outcomes[0]!.key,
      },
    };
    await page.route('**/api/v1/**', async (route) => {
      const path = new URL(route.request().url()).pathname;
      if (path === `/api/v1/projects/${owner}/flow-definitions`)
        return route.fulfill({
          json: {
            items: [{ flow, project_id: owner, published_at_unix_ms: 1 }],
            total: 1,
            next_cursor: null,
          },
        });
      if (path.startsWith(`/api/v1/flow-definitions/${flow.closure.root.id}`))
        return route.fulfill({ json: { flow, project_id: owner, published_at_unix_ms: 1 } });
      if (path === `/api/v1/projects/${owner}/issue-workflows`)
        return route.fulfill({
          json: {
            items: [{ id: workflow.id, name: workflow.definition.name, version: 3 }],
            next_cursor: null,
          },
        });
      if (path.startsWith(`/api/v1/projects/${owner}/issue-workflows/${workflow.id}`))
        return route.fulfill({ json: workflow });
      return route.fallback();
    });
    const text =
      language === 'en'
        ? {
            inspect: 'Inspect Flow Definition · v1',
            edit: 'Edit Flow draft',
            heading: 'Local Issue status action',
            workflow: 'Issue workflow',
            transition: 'Issue transition',
            apply: 'Use reviewed Issue action',
          }
        : {
            inspect: 'Просмотреть поток · v1',
            edit: 'Редактировать черновик Flow',
            heading: 'Локальное действие со статусом задачи',
            workflow: 'Процесс задачи',
            transition: 'Переход задачи',
            apply: 'Использовать проверенное действие задачи',
          };
    await page.goto(`/factory?project_id=${owner}`);
    await page.getByRole('button', { name: text.inspect }).click();
    await page.getByRole('button', { name: text.edit }).click();
    await expect(page.getByRole('heading', { name: text.heading })).toBeVisible();
    const select = page.getByRole('combobox', { name: text.workflow, exact: true });
    await select.focus();
    await page.keyboard.press('ArrowDown');
    await page.getByRole('option', { name: 'Local workflow · v3' }).click();
    await expect(select).toBeFocused();
    await page.getByRole('combobox', { name: text.transition, exact: true }).click();
    await page.getByRole('option', { name: /Review delivery/ }).click();
    await expect(page.getByRole('button', { name: text.apply })).toBeEnabled();
    await page.getByRole('button', { name: text.apply }).click();
    await expect(
      page.getByText(new RegExp(workflow.definition.transitions[0]!.id)).first(),
    ).toBeVisible();
    expect(
      await page.evaluate(() => JSON.stringify({ local: localStorage, session: sessionStorage })),
    ).not.toContain('30000000-0000-4000-8000-000000000001');
    await expect(
      page.getByRole('combobox', { name: 'Source connection', exact: true }),
    ).toHaveCount(0);
  });
}
