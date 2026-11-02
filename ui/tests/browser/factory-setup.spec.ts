import { expect, test } from '@playwright/test';
import type { components } from '../../.generated/api/schema';
import { translate, type MessageKey } from '../../src/app/presentation/messages';
import { factoryRoleFixture } from '../fixtures/factoryRoles';
import { installOperatorFixture } from './operatorFixture';

test.use({ trace: 'off', screenshot: 'off', video: 'off' });
for (const [language, width] of [
  ['en', 1280],
  ['ru', 390],
] as const) {
  test(`${language}: authors an isolated AI step with computed limits and reviewed access without editing schemas`, async ({
    page,
  }, info) => {
    await page.setViewportSize({ width, height: 844 });
    await installOperatorFixture(page);
    const t = (key: MessageKey) => translate(language, key);
    const { role, profile, choice, capability } = factoryRoleFixture();
    const project = role.reference.project_id;
    profile.definition.context_requirements = ['task'];
    profile.definition.permissions.network_hosts = ['api.allowed.example'];
    profile.definition.permissions.secret_profiles = ['private-model'];
    profile.definition.budget = {
      max_attempts: 1,
      max_elapsed_millis: 300000,
      max_tokens: 150000,
      max_cost_micro_units: 1000000,
      max_output_bytes: 67108864,
    };
    const pageOf = (items: unknown[]) => ({ items, total: items.length, next_cursor: null });
    await page.route('**/api/v1/projects/*/factory-**', async (route) => {
      const path = new URL(route.request().url()).pathname;
      let body: unknown = pageOf([]);
      if (path.endsWith('/factory-roles'))
        body = pageOf([
          {
            reference: role.reference,
            name: role.definition.name,
            icon: null,
            concurrency_limit: null,
            template: null,
          },
        ]);
      else if (path.endsWith(`/factory-roles/${role.reference.id}/profiles`))
        body = pageOf([
          {
            reference: profile.reference,
            role: profile.role,
            name: profile.definition.name,
            toolchain: profile.definition.toolchain,
            template: null,
          },
        ]);
      else if (path.includes('/factory-role-profiles/') && path.includes('/versions/'))
        body = profile;
      else if (path.includes('/factory-roles/') && path.includes('/versions/')) body = role;
      else if (path.endsWith('/factory-profile-capabilities')) body = pageOf([choice]);
      else if (path.includes('/factory-profile-capabilities/')) body = capability;
      else if (path.endsWith('/factory-authoring-capabilities'))
        body = {
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
        };
      else if (path.endsWith('/factory-execution-bindings/resolve'))
        body = {
          incompatibility: null,
          build: {
            build_configuration: profile.definition.build_configuration,
            plugin: profile.definition.toolchain.plugin,
            tool: profile.definition.toolchain.executable,
            model_or_tool: profile.definition.toolchain.model,
            task_digest: 'd'.repeat(64),
            result_output: 'result',
            result_output_kind: 'report',
            result_outcome: route.request().postDataJSON().result_outcome,
          },
        };
      await route.fulfill({ json: body });
    });
    let schemaCount = 0;
    await page.route('**/api/v1/flow-data-schemas/validate', (route) => {
      const body = route.request().postDataJSON();
      return route.fulfill({
        json: {
          reference: {
            identity: body.identity,
            version: body.version,
            digest: (++schemaCount).toString(16).padStart(64, '0'),
          },
          shape: body.shape,
        },
      });
    });
    let submitted: components['schemas']['CreateFactoryFlowDefinitionRequest'] | undefined;
    await page.route('**/api/v1/flow-definitions/validate', (route) => {
      submitted = route.request().postDataJSON();
      if (!submitted) throw new Error('Missing validation body');
      return route.fulfill({
        json: {
          closure: {
            root: { id: submitted.id, version: 1, digest: 'f'.repeat(64) },
            definitions: [
              {
                ...submitted.definition.root,
                reference: { id: submitted.id, version: 1, digest: 'f'.repeat(64) },
              },
            ],
          },
          limits: submitted.definition.limits,
          data_schemas: submitted.definition.data_schemas,
        },
      });
    });
    await page.goto(`/factory?project_id=${project}`);
    if (language === 'ru') {
      await page.getByLabel('Operator menu').click();
      await page.getByRole('combobox', { name: 'Language', exact: true }).click();
      await page.getByRole('option', { name: 'Russian', exact: true }).click();
      await page.keyboard.press('Escape');
    }
    await page.getByRole('button', { name: t('factory.addPipeline'), exact: true }).click();
    await page.getByRole('button', { name: t('factory.addNode'), exact: true }).click();
    await page
      .getByRole('textbox', { name: t('factory.nodeName'), exact: true })
      .fill('project_check');
    await page.getByRole('combobox', { name: t('factory.nodeKind'), exact: true }).click();
    await page.getByRole('option', { name: t('factory.kind.reasoning'), exact: true }).click();
    await page.getByRole('button', { name: t('factory.addResult'), exact: true }).click();
    await page.getByRole('button', { name: t('factory.saveResult'), exact: true }).click();
    const executor = page.getByRole('group', { name: t('factory.executor'), exact: true });
    await executor.getByRole('combobox', { name: t('factory.executorRole') }).click();
    await page.getByRole('option', { name: `${role.definition.name} · v1`, exact: true }).click();
    await executor.getByRole('combobox', { name: t('factory.executorProfile') }).click();
    await page
      .getByRole('option', { name: `${profile.definition.name} · v1`, exact: true })
      .click();
    await executor.getByRole('button', { name: t('factory.useExecutorProfile') }).click();
    await expect(
      executor.getByText(
        translate(language, 'factory.selectedExecutorProfile', {
          name: profile.definition.name,
          version: 1,
        }),
      ),
    ).toBeVisible();
    const task = page.getByRole('checkbox', { name: t('factory.workTask'), exact: true });
    await expect(task).not.toBeChecked();
    await task.check();
    const review = page.getByRole('button', {
      name: t('factory.reviewRequiredAccess'),
      exact: true,
    });
    await review.click();
    const dialog = page.getByRole('dialog', { name: t('factory.requiredAccess') });
    await expect(
      dialog.getByRole('heading', { name: t('factory.requiredAccess'), exact: true }),
    ).toBeInViewport();
    await expect(dialog.getByText(/api.allowed.example/).first()).toBeVisible();
    await page.screenshot({ path: info.outputPath('access-preview.png') });
    await dialog.getByRole('button', { name: t('factory.cancelSetup'), exact: true }).click();
    await expect(review).toBeFocused();
    await review.click();
    await dialog.getByRole('button', { name: t('factory.applyToDraft'), exact: true }).click();
    await page.getByRole('button', { name: t('factory.flowSettings'), exact: true }).click();
    await page.getByRole('button', { name: t('factory.addTerminal'), exact: true }).click();
    await page
      .getByRole('textbox', { name: t('factory.terminalName'), exact: true })
      .fill('finished');
    await page.getByRole('combobox', { name: t('factory.terminalSchema'), exact: true }).click();
    await page.getByRole('option', { name: 'project_check › result', exact: true }).click();
    await page.getByRole('button', { name: t('factory.addTransition'), exact: true }).click();
    await page.getByRole('button', { name: t('factory.validateDraft'), exact: true }).click();
    await expect(page.getByText(t('factory.validationPassed'), { exact: true })).toBeVisible();
    expect(submitted?.definition.root.nodes[0]?.budget).toEqual({
      max_attempts: 1,
      max_elapsed_millis: 300000,
      max_tokens: 150000,
      max_cost_micro_units: 1000000,
      max_output_bytes: 67108864,
    });
    expect(submitted?.definition.limits.budget).toEqual(
      submitted?.definition.root.execution.budget,
    );
    const node = submitted?.definition.root.nodes[0];
    expect(submitted?.definition.root.input_schema).toEqual(node?.input_schema);
    expect(node?.context_binding).toEqual({ entries: {}, work_task: 'task' });
    expect(node?.input_binding).toEqual({
      mapping: { value: { mapping: 'object', fields: {} } },
      sources: {},
    });
    expect(
      submitted?.definition.data_schemas.find(
        (schema) => schema.reference.digest === node?.input_schema?.digest,
      )?.shape,
    ).toEqual({ type: 'object', fields: {} });
    expect(node?.permissions).toEqual(profile.definition.permissions);
    expect(submitted?.definition.root.context_projections).toEqual([]);
    expect(submitted?.definition.root.data_projections).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(
      width,
    );
  });
}
