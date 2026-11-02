import { translate, type MessageKey } from '../../src/app/presentation/messages';
import { expect, test, type Page } from '@playwright/test';
import type { components } from '../../.generated/api/schema';
import { factoryRoleFixture } from '../fixtures/factoryRoles';
import { configurationFixture } from '../fixtures/factory';
import { installOperatorFixture } from './operatorFixture';

async function openGraph(page: Page, options: { language?: 'en' | 'ru'; executor?: boolean } = {}) {
  await installOperatorFixture(page);
  const configuration = configurationFixture().configuration;
  const flow = structuredClone(configuration.flow);
  const root = flow.closure.definitions.find((item) => item.reference.id === flow.closure.root.id)!;
  const schema = flow.data_schemas[0]!.reference;
  root.entry = 'first';
  root.input_schema = null;
  root.nodes = ['first', 'second', 'third'].map((key) => ({
    key,
    kind: 'human_gate',
    required: true,
    subflow: null,
    input_schema: null,
    budget: root.execution.budget,
    permissions: root.execution.permissions,
    outcomes: [{ key: 'observed', kind: 'success', schema }],
  }));
  if (options.executor) {
    const { profile, role, choice, capability } = factoryRoleFixture();
    const node = root.nodes[0]!;
    node.kind = 'reasoning';
    node.code = { kind: 'none' };
    node.profile_selection = {
      role: profile.role,
      candidates: {
        selected: {
          profile,
          build: {
            build_configuration: profile.definition.build_configuration,
            plugin: profile.definition.toolchain.plugin,
            tool: profile.definition.toolchain.executable,
            model_or_tool: profile.definition.toolchain.model,
            task_digest: 'd'.repeat(64),
            result_output: 'result.json',
            result_output_kind: 'report',
            result_outcome: 'observed',
          },
        },
      },
      policy: { mode: 'manual', candidate: 'selected' },
      failure: { kind: 'manual_selection' },
      selection_budget: {
        attempts: 0,
        elapsed_millis: 0,
        tokens: 0,
        cost_micro_units: 0,
        output_bytes: 0,
      },
    };
    const project = profile.reference.project_id;
    await page.route(
      `**/api/v1/projects/${project}/factory-role-profiles/${profile.reference.id}/versions/1`,
      (route) => route.fulfill({ json: profile }),
    );
    await page.route(
      `**/api/v1/projects/${project}/factory-roles/${role.reference.id}/versions/1`,
      (route) => route.fulfill({ json: role }),
    );
    await page.route(`**/api/v1/projects/${project}/factory-profile-capabilities`, (route) =>
      route.fulfill({ json: { items: [choice], total: 1, next_cursor: null } }),
    );
    await page.route(
      `**/api/v1/projects/${project}/factory-profile-capabilities/${choice.alias}`,
      (route) => route.fulfill({ json: capability }),
    );
  }
  root.context_projections = [];
  root.data_projections = [];
  root.terminals = [{ key: 'finished', schema }];
  root.transitions = [
    {
      predecessor: 'first',
      outcome: 'observed',
      target: { kind: 'node', key: 'second' },
      max_repeats: 0,
      exhausted_terminal: null,
    },
    {
      predecessor: 'second',
      outcome: 'observed',
      target: { kind: 'node', key: 'third' },
      max_repeats: 0,
      exhausted_terminal: null,
    },
    {
      predecessor: 'third',
      outcome: 'observed',
      target: { kind: 'terminal', key: 'finished' },
      max_repeats: 0,
      exhausted_terminal: null,
    },
  ];
  flow.closure.definitions = [root];
  const owner = configuration.reference.project_id;
  await page.route(`**/api/v1/projects/${owner}/flow-definitions*`, (route) =>
    route.fulfill({
      json: {
        items: [{ flow, project_id: owner, published_at_unix_ms: 1 }],
        total: 1,
        next_cursor: null,
      },
    }),
  );
  await page.route(`**/api/v1/flow-definitions/${flow.closure.root.id}/versions/*`, (route) =>
    route.fulfill({ json: { flow, project_id: owner, published_at_unix_ms: 1 } }),
  );
  await page.route(`**/api/v1/projects/${owner}/factory-authoring-capabilities`, (route) =>
    route.fulfill({
      json: {
        commands: [],
        human_gates: [],
        deterministic_gate: {
          policy: { identity: 'factory.declarative-gate', version: 'v1', digest: 'a'.repeat(64) },
          parameter_contract: 'declarative_gate_v1',
          max_parameter_bytes: 8192,
          resource_modes: ['none'],
        },
        build_resource_modes: ['repository'],
        source_free_incompatibility: 'executor_support_required',
      },
    }),
  );
  const validations: components['schemas']['CreateFactoryFlowDefinitionRequest'][] = [];
  const mutations: string[] = [];
  page.on('request', (request) => {
    if (request.method() !== 'GET' && !request.url().endsWith('/flow-definitions/validate'))
      mutations.push(request.url());
  });
  await page.route('**/api/v1/flow-definitions/validate', async (route) => {
    const body = route
      .request()
      .postDataJSON() as components['schemas']['CreateFactoryFlowDefinitionRequest'];
    validations.push(body);
    await route.fulfill({
      json: {
        closure: {
          root: flow.closure.root,
          definitions: [{ ...body.definition.root, reference: flow.closure.root }],
        },
        data_schemas: body.definition.data_schemas,
        limits: body.definition.limits,
      },
    });
  });
  await page.goto(`/factory?project_id=${owner}`);
  if (options.language === 'ru') {
    await page.getByLabel('Operator menu').click();
    await page.getByRole('combobox', { name: 'Language', exact: true }).click();
    await page.getByRole('option', { name: 'Russian', exact: true }).click();
    await page.keyboard.press('Escape');
  }
  await page
    .getByRole('button', {
      name: 'first · v1',
    })
    .click();
  await page
    .getByRole('button', {
      name: options.language === 'ru' ? 'Редактировать черновик Flow' : 'Edit Flow draft',
    })
    .click();
  await expect(
    page.getByRole('region', { name: options.language === 'ru' ? 'Граф шагов' : 'Step graph' }),
  ).toBeVisible();
  return { validations, mutations };
}

for (const language of ['en', 'ru'] as const) {
  for (const width of [390, 1280]) {
    test(`${language}: keeps both step actions inside their card at ${width}px with long names`, async ({
      page,
    }, info) => {
      await page.setViewportSize({ width, height: 900 });
      const { mutations } = await openGraph(page, { language });
      const t = (key: MessageKey) => translate(language, key);
      await page
        .getByRole('textbox', { name: t('factory.nodeName'), exact: true })
        .fill('project_check');
      const graph = page.getByRole('region', { name: t('factory.stepGraph'), exact: true });
      await graph.getByRole('button', { name: t('workflow.fitGraph'), exact: true }).click();
      await graph
        .getByRole('button', {
          name: translate(language, 'factory.editGraphStep', { name: 'second' }),
          exact: true,
        })
        .click();
      await page
        .getByRole('textbox', { name: t('factory.nodeName'), exact: true })
        .fill('systems_analysis_with_a_long_explicit_step_name');
      for (const card of await graph.getByRole('article').all()) {
        const owner = (await card.boundingBox())!;
        const buttons = await card.getByRole('button').all();
        expect(buttons).toHaveLength(2);
        for (const button of buttons) {
          const box = (await button.boundingBox())!;
          expect(box.x).toBeGreaterThan(owner.x);
          expect(box.y).toBeGreaterThan(owner.y);
          expect(box.x + box.width).toBeLessThan(owner.x + owner.width);
          expect(box.y + box.height).toBeLessThan(owner.y + owner.height);
        }
      }
      const scale = (await graph
        .getByLabel(t('workflow.zoomLevel'), { exact: true })
        .boundingBox())!;
      expect(scale.height).toBeGreaterThanOrEqual(32);
      if (width === 1280) {
        const zoom = (await graph
          .getByRole('button', { name: t('workflow.zoomOut'), exact: true })
          .boundingBox())!;
        expect(Math.abs(scale.y + scale.height / 2 - zoom.y - zoom.height / 2)).toBeLessThanOrEqual(
          1,
        );
      }
      await graph.getByRole('button', { name: t('workflow.fitGraph'), exact: true }).click();
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth - innerWidth),
      ).toBeLessThanOrEqual(1);
      expect(mutations).toEqual([]);
      await graph.screenshot({ path: info.outputPath('contained-step-actions.png') });
    });
  }
}

test('moves a step with pointer and keyboard while preserving the exact validated Flow and browser storage', async ({
  page,
}) => {
  const { validations, mutations } = await openGraph(page);
  await page.getByRole('button', { name: 'Validate draft', exact: true }).click();
  await expect.poll(() => validations.length).toBe(1);
  const graph = page.getByRole('region', { name: 'Step graph' });
  const step = graph.getByRole('button', { name: 'Edit step: first', exact: true });
  await step.scrollIntoViewIfNeeded();
  const before = (await step.boundingBox())!;
  await page.mouse.move(before.x + before.width / 2, before.y + before.height / 2);
  await page.mouse.down();
  await page.mouse.move(before.x + before.width / 2 + 48, before.y + before.height / 2 + 24, {
    steps: 8,
  });
  await page.mouse.up();
  await expect.poll(async () => (await step.boundingBox())!.x).toBeCloseTo(before.x + 48, 0);
  await step.focus();
  const keyboardStart = (await step.boundingBox())!;
  await step.press('Alt+ArrowRight');
  await expect.poll(async () => (await step.boundingBox())!.x).toBeCloseTo(keyboardStart.x + 16, 0);

  await page.getByRole('button', { name: 'Validate draft', exact: true }).click();
  await expect.poll(() => validations.length).toBe(2);
  expect(validations[1]).toEqual(validations[0]);
  expect(mutations).toEqual([]);
  const storage = await page.evaluate(
    () => `${JSON.stringify(localStorage)}${JSON.stringify(sessionStorage)}`,
  );
  expect(storage).not.toContain('first');
});

test('edits routes with the keyboard and fits every step on a narrow canvas without execution effects', async ({
  page,
}) => {
  const { validations, mutations } = await openGraph(page);
  const graph = page.getByRole('region', { name: 'Step graph' });
  await graph.getByRole('button', { name: 'Edit route: first → second (observed)' }).focus();
  await page.keyboard.press('Enter');
  await graph
    .getByRole('group', { name: 'Selected route' })
    .getByRole('button', { name: 'Remove transition' })
    .focus();
  await page.keyboard.press('Enter');
  await graph.getByRole('button', { name: 'Connect from first' }).focus();
  await page.keyboard.press('Enter');
  const route = graph.getByRole('group', { name: 'New route' });
  await route.getByRole('combobox', { name: 'Target' }).focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('option').first()).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('option', { name: 'Step · second' })).toBeFocused();
  await page.keyboard.press('Enter');
  await route.getByRole('button', { name: 'Add route' }).focus();
  await page.keyboard.press('Enter');
  await page.getByRole('button', { name: 'Validate draft', exact: true }).click();
  await expect.poll(() => validations.length).toBe(1);
  expect(validations[0]!.definition.root.transitions).toHaveLength(3);
  expect(validations[0]!.definition.root.transitions).toContainEqual({
    predecessor: 'first',
    outcome: 'observed',
    target: { kind: 'node', key: 'second' },
    max_repeats: 0,
    exhausted_terminal: null,
  });
  expect(validations[0]!.definition.root.data_projections).toEqual([]);
  expect(validations[0]!.definition.root.context_projections).toEqual([]);
  await page.setViewportSize({ width: 390, height: 844 });
  // The global reduced-motion rule sets transition-duration for every element.
  // Amplify that duration to detect accidental interpolation of the camera deterministically.
  await page.addStyleTag({
    content: '[aria-label="Flow graph"] * { transition-duration: 1s !important; }',
  });
  await graph.getByRole('button', { name: 'Fit graph' }).click();
  const canvas = graph.getByRole('region', { name: 'Flow graph' });
  const visible = (await canvas.boundingBox())!;
  for (const card of await graph.getByRole('article').all()) {
    const box = (await card.boundingBox())!;
    expect(box.x).toBeGreaterThan(visible.x);
    expect(box.x + box.width).toBeLessThan(visible.x + visible.width);
    expect(box.y).toBeGreaterThan(visible.y);
    expect(box.y + box.height).toBeLessThan(visible.y + visible.height);
  }
  expect(mutations).toEqual([]);
  await graph.screenshot({ path: '/tmp/octacity-section4-canvas-narrow.png' });
});

for (const language of ['en', 'ru'] as const)
  test(`keeps the ${language} step inspector and exact profile review usable at phone width`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    const { mutations } = await openGraph(page, { language, executor: true });
    const ru = language === 'ru';
    const step = page.getByRole('group', { name: ru ? 'Шаг' : 'Step', exact: true });
    await expect(step).toBeVisible();
    const remove = step.getByRole('button', {
      name: ru ? 'Удалить шаг' : 'Remove step',
      exact: true,
    });
    const color = await remove.evaluate((element) =>
      getComputedStyle(element).color.match(/\d+/g)!.slice(0, 3).map(Number),
    );
    expect(color[0]!).toBeGreaterThan(color[1]!);
    expect(color[0]!).toBeGreaterThan(color[2]!);
    const task = step.getByRole('group', { name: ru ? 'Задание шага' : 'Step task', exact: true });
    await task
      .getByRole('button', { name: ru ? 'Добавить: инструкции задания' : 'Add task instructions' })
      .click();
    const text = task.getByRole('textbox', {
      name: ru ? 'Инструкции задания 1' : 'Task instructions 1',
    });
    await text.fill('Private step assignment');
    expect((await text.boundingBox())!.width).toBeGreaterThanOrEqual(210);
    const executor = step.getByRole('group', {
      name: ru ? 'Исполнитель' : 'Executor',
      exact: true,
    });
    await expect(executor.getByText('test.model', { exact: true })).toBeVisible();
    const link = executor.getByRole('link', {
      name: ru ? 'Посмотреть профиль · default · v1' : 'View profile · default · v1',
    });
    await link.focus();
    await page.keyboard.press('Enter');
    const view = executor.getByRole('region', {
      name: ru ? 'Профиль · default · v1' : 'Profile · default · v1',
    });
    await expect(view.getByText('Use selected skills.')).toBeVisible();
    await view.getByRole('button', { name: ru ? 'Вернуться к шагу' : 'Back to step' }).focus();
    await page.keyboard.press('Enter');
    await expect(text).toHaveValue('Private step assignment');
    await expect(link).toBeFocused();
    await expect(
      step.getByRole('group', {
        name: ru ? 'Инструменты и доступы' : 'Tools and access',
        exact: true,
      }),
    ).toBeVisible();
    const advanced = step.getByText(
      ru ? 'Расширенные привязки входных данных и Context' : 'Advanced input and Context mappings',
      { exact: true },
    );
    await expect(advanced).toBeVisible();
    await expect(
      step.getByRole('button', {
        name: ru ? 'Добавить источник' : 'Add input source',
        exact: true,
      }),
    ).toBeHidden();
    await page
      .getByRole('button', { name: ru ? 'Настройки Flow' : 'Flow settings', exact: true })
      .click();
    for (const name of ru
      ? ['Запуск и очередь', 'Лимиты выполнения', 'Доступы pipeline']
      : ['Start and queue', 'Execution limits', 'Pipeline access'])
      await expect(page.getByRole('group', { name, exact: true })).toBeVisible();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth),
    ).toBeLessThanOrEqual(1);
    await task.scrollIntoViewIfNeeded();
    await page.screenshot({ path: `/tmp/octacity-section4-step-${language}.png` });
    await remove.scrollIntoViewIfNeeded();
    await page.screenshot({ path: `/tmp/octacity-step-actions-${language}.png` });
    expect(mutations).toEqual([]);
    expect(
      await page.evaluate(() => JSON.stringify(localStorage) + JSON.stringify(sessionStorage)),
    ).not.toContain('Private step assignment');
  });

for (const language of ['en', 'ru'] as const) {
  test(`authors and connects a step result in ${language} without technical schema setup`, async ({
    page,
  }, testInfo) => {
    const { validations } = await openGraph(page, { language });
    await page.setViewportSize({ width: 390, height: 844 });
    const t = (key: MessageKey) => translate(language, key);
    const advanced = page.getByText(t('factory.advancedContracts'), { exact: true }).first();
    const graph = page.getByRole('region', { name: t('factory.stepGraph'), exact: true });
    expect((await advanced.boundingBox())!.y).toBeGreaterThan((await graph.boundingBox())!.y);
    const normalized: components['schemas']['ValidateFactoryFlowDataSchemaRequest'][] = [];
    await page.route('**/api/v1/flow-data-schemas/validate', async (route) => {
      const body = route
        .request()
        .postDataJSON() as components['schemas']['ValidateFactoryFlowDataSchemaRequest'];
      normalized.push(body);
      await route.fulfill({
        json: {
          shape: body.shape,
          reference: {
            identity: body.identity,
            version: body.version,
            digest: String(normalized.length).repeat(64),
          },
        },
      });
    });
    const step = page.getByRole('group', { name: t('factory.node'), exact: true });
    await expect(
      page.getByRole('textbox', { name: t('factory.schemaName'), exact: true }),
    ).toHaveCount(0);
    await step.getByRole('button', { name: t('factory.addResult'), exact: true }).click();
    await step.getByText(t('factory.resultDetails'), { exact: true }).last().click();
    const name = step.getByRole('textbox', { name: t('factory.resultName'), exact: true });
    await name.fill('assessment');
    expect((await name.boundingBox())!.width).toBeGreaterThanOrEqual(200);
    await expect(
      page.getByRole('button', { name: t('factory.validateDraft'), exact: true }),
    ).toBeDisabled();
    await step.getByText(t('factory.resultStructure'), { exact: true }).click();
    await step.getByRole('combobox', { name: t('factory.schemaType'), exact: true }).click();
    await page.getByRole('option', { name: t('factory.object'), exact: true }).click();
    for (const field of ['summary', 'private_notes']) {
      await step.getByRole('button', { name: t('factory.addField'), exact: true }).click();
      await step
        .getByRole('textbox', { name: t('factory.fieldName'), exact: true })
        .last()
        .fill(field);
    }
    await step.getByRole('button', { name: t('factory.saveResult'), exact: true }).click();
    await expect(step.getByRole('group', { name: 'assessment', exact: true })).toContainText(
      'summary',
    );
    await step
      .getByRole('group', { name: t('factory.expectedResult'), exact: true })
      .screenshot({ path: testInfo.outputPath('step-results.png') });
    await page
      .getByRole('button', {
        name: translate(language, 'factory.editGraphStep', { name: 'second' }),
        exact: true,
      })
      .press('Enter');
    const inputs = step.getByRole('group', { name: t('factory.acceptedResults'), exact: true });
    const resultField = inputs.getByRole('combobox', {
      name: t('factory.resultField'),
      exact: true,
    });
    const inputField = inputs.getByRole('combobox', {
      name: t('factory.stepInputField'),
      exact: true,
    });
    await expect(inputs.getByRole('combobox')).toHaveCount(1);
    await inputs.getByRole('combobox', { name: t('factory.resultFromStep'), exact: true }).click();
    await page.getByRole('option', { name: 'first', exact: true }).click();
    await expect(resultField).toHaveCount(0);
    await expect(inputField).toHaveCount(0);
    await inputs
      .getByRole('combobox', { name: t('factory.transitionOutcome'), exact: true })
      .click();
    await page.getByRole('option', { name: 'assessment', exact: true }).click();
    await expect(resultField).toBeVisible();
    await expect(inputField).toHaveCount(0);
    await inputs.getByRole('combobox', { name: t('factory.resultField'), exact: true }).click();
    await page.getByRole('option', { name: 'summary', exact: true }).click();
    await expect(inputField).toBeVisible();
    await inputs.screenshot({ path: testInfo.outputPath('progressive-result-fields.png') });
    await inputs
      .getByRole('combobox', { name: t('factory.transitionOutcome'), exact: true })
      .click();
    await page.getByRole('option', { name: t('factory.chooseInput'), exact: true }).click();
    await expect(resultField).toHaveCount(0);
    await expect(inputField).toHaveCount(0);
    await expect(
      inputs.getByRole('textbox', { name: t('factory.resultInputName'), exact: true }),
    ).toHaveCount(0);
    await inputs
      .getByRole('combobox', { name: t('factory.transitionOutcome'), exact: true })
      .click();
    await page.getByRole('option', { name: 'assessment', exact: true }).click();
    await resultField.click();
    await page.getByRole('option', { name: 'summary', exact: true }).click();
    await expect(
      inputs.getByRole('textbox', { name: t('factory.resultInputName'), exact: true }),
    ).toHaveValue('summary');
    await inputs.getByRole('button', { name: t('factory.useAcceptedField'), exact: true }).click();
    await expect(inputs.getByRole('listitem')).toContainText('summary ← first');
    await inputs.screenshot({ path: testInfo.outputPath('result-connection.png') });
    // A data connection grants no control route. Declare the new outcome's
    // route independently, as required by the server graph contract.
    await page.getByRole('button', { name: t('factory.addTransition'), exact: true }).click();
    const transition = page.getByRole('group', {
      name: translate(language, 'factory.transitionNumber', { number: 4 }),
      exact: true,
    });
    await transition
      .getByRole('combobox', { name: t('factory.transitionOutcome'), exact: true })
      .click();
    await page.getByRole('option', { name: 'assessment', exact: true }).click();
    await transition.getByRole('combobox', { name: t('factory.target'), exact: true }).click();
    await page
      .getByRole('option', {
        name: translate(language, 'factory.nodeTarget', { name: 'second' }),
        exact: true,
      })
      .click();
    await page.getByRole('button', { name: t('factory.validateDraft'), exact: true }).click();
    await expect.poll(() => validations.length).toBe(1);
    const node = validations[0]!.definition.root.nodes.find((item) => item.key === 'second')!;
    expect(node.input_binding?.sources).toEqual({
      accepted_0: {
        source: 'record',
        node: 'first',
        outcome: 'assessment',
        view: 'payload',
        path: '/summary',
      },
    });
    expect(normalized[1]!.shape).toEqual({
      type: 'object',
      fields: { summary: { required: true, value: { type: 'boolean' } } },
    });
    expect(node.context_binding ?? null).toBeNull();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBe(true);
  });
}
