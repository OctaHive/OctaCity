import { emptyFactoryPermissions } from '../../src/features/factory/emptyFlowDraft';
import { expect, test, type Locator } from '@playwright/test';
import { configurationFixture } from '../fixtures/factory';
import type { components } from '../../.generated/api/schema';
import { issueFixture } from '../fixtures/issues';
import { executionFixture } from '../fixtures/issueExecutions';
import { integrationFixture } from '../fixtures/integrations';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { factoryRoleFixture } from '../fixtures/factoryRoles';
import { installOperatorFixture } from './operatorFixture';

test('offers a reviewed example without changing existing triage or enabling intake', async ({
  page,
}) => {
  await installOperatorFixture(page);
  const { configuration } = configurationFixture();
  const project = configuration.reference.project_id;
  const scope = `**/api/v1/projects/${project}`;
  const empty = { items: [], total: 0, next_cursor: null };
  const triage = structuredClone(configuration.flow);
  triage.closure.definitions[0]!.terminals = [];
  const preset = { identity: 'ticket-delivery', version: 'v1', digest: '3'.repeat(64) };
  const writes: string[] = [];
  page.on('request', (request) => {
    if (request.method() !== 'GET') writes.push(new URL(request.url()).pathname);
  });
  await page.route(`${scope}/flow-definitions*`, (route) =>
    route.fulfill({
      json: {
        ...empty,
        total: 1,
        items: [{ flow: triage, project_id: project, published_at_unix_ms: 1 }],
      },
    }),
  );
  for (const resource of ['factory-configurations', 'factory-configuration-choices', 'builds']) {
    await page.route(`${scope}/${resource}*`, (route) => route.fulfill({ json: empty }));
  }
  await page.route(`${scope}/factory-presets*`, (route) =>
    route.fulfill({
      json: {
        ...empty,
        total: 1,
        items: [
          {
            reference: preset,
            profile_aliases: [],
            required_node_kinds: ['build_command'],
            parameters: [
              { path: '/commands/reproduction', kind: 'command', required: true },
              { path: '/accepted_predecessor', kind: 'accepted_predecessor', required: true },
            ],
            editor: {
              name: 'Ticket delivery',
              budget: triage.limits.budget,
              max_active_nodes: 1,
              steps: [
                { alias: 'reproduction', kind: 'build_command', outcome: 'observed', result: null },
              ],
            },
          },
        ],
      },
    }),
  );
  await page.route(`${scope}/factory-authoring-capabilities`, (route) =>
    route.fulfill({
      json: {
        commands: [],
        human_gates: [],
        build_resource_modes: [],
        source_free_incompatibility: null,
        deterministic_gate: {
          policy: preset,
          parameter_contract: 'declarative_gate_v1',
          max_parameter_bytes: 64,
          resource_modes: [],
        },
      },
    }),
  );
  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: 'Add pipeline', exact: true }).click();
  await page.getByText('Start from an example', { exact: true }).click();
  await page.getByRole('button', { name: 'Ticket delivery · v1', exact: true }).click();
  await expect(page.getByRole('group', { name: 'Reproduction', exact: true })).toContainText(
    /install/i,
  );
  await expect(page.getByText(/Publish a Flow with a typed result/)).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Open example draft', exact: true }),
  ).toBeDisabled();
  expect(writes).toEqual([]);
});

test('submits the reviewed Flow, Project policies, and manual Issue start to the management API', async ({
  page,
}) => {
  test.slow(); // Author the full graph, publish both drafts, then launch from the Issue.
  await installOperatorFixture(page);
  type S = components['schemas'];
  const release = JSON.parse(
    readFileSync('../config/factory/presets/ticket-delivery/pull-request-v1.json', 'utf8'),
  ) as {
    nodes: S['FactoryFlowNodeDefinition'][];
    schemas: Record<
      string,
      { identity: string; version: string; shape: S['FactoryFlowValueSchema'] }
    >;
  };
  const releasePublisher = release.nodes.find((node) => node.key === 'publish_pull_request')!;
  const expiry = ['valid_until', 'review_valid_until'] as const;
  const baseline = configurationFixture();
  const project = baseline.configuration.reference.project_id;
  const scope = `**/api/v1/projects/${project}`;
  const plugin = { identity: 'candidate.publisher', version: 'v1', digest: 'a'.repeat(64) };
  const commandTool = { identity: 'test.runner', version: 'v1', digest: '9'.repeat(64) };
  const checkReport = {
    reference: { identity: 'command.report', version: 'v1', digest: '4'.repeat(64) },
    shape: { type: 'boolean' as const },
  };
  const command = {
    alias: 'Run protected checks',
    output_schema: checkReport.reference,
    output_definition: checkReport,
    evidence: [
      {
        kind: 'validation_facts',
        output_kind: 'report' as const,
        tool: commandTool,
        plugin: commandTool,
        schema: {
          identity: 'factory.base.protected_validation_facts',
          version: 'v1',
          digest: '5'.repeat(64),
        },
      },
    ],
    execution: {
      build_configuration: {
        project_id: project,
        id: '64000000-0000-4000-8000-000000000002',
        version: 1,
        definition_digest: '8'.repeat(64),
      },
      tool: commandTool,
      plugin: commandTool,
      model_or_tool: commandTool,
      task_digest: '7'.repeat(64),
      result_output: 'checks.json',
      result_output_kind: 'report' as const,
    },
  };
  const sourceChoices = integrationFixture('WorkSourceInputChoices', 1);
  const writer = sourceChoices.items.find((item) => item.purpose === 'task_update')!;
  const reporterSchema = {
    reference: { identity: 'reporter-receipt', version: 'v1', digest: '6'.repeat(64) },
    shape: { type: 'object' as const, fields: {} },
  };
  const reporterOutcome = {
    key: 'requested',
    kind: 'success' as const,
    schema: reporterSchema.reference,
  };
  writer.reporting!.action = {
    action: 'set_status',
    plugin: writer.binding.capability,
    outcome: reporterOutcome,
    output: reporterSchema,
    parameters: {
      contract: 'work_status_v1',
      max_attempts: 3,
      max_retry_delay_millis: 1000,
      max_status_bytes: 128,
      outbox_required: true,
      source_update_required: true,
    },
  };
  await page.route(`${scope}/factory-source-choices**`, (route) =>
    route.fulfill({ json: sourceChoices }),
  );
  const target = { identity: 'project-review', version: 'v2', digest: 'b'.repeat(64) };
  const receipt = release.schemas.pull_request_receipt!;
  const receiptSchema = {
    reference: { identity: receipt.identity, version: receipt.version, digest: 'c'.repeat(64) },
    shape: receipt.shape,
  };
  const predecessor = {
    flow: structuredClone(baseline.configuration.flow),
    project_id: project,
    published_at_unix_ms: 1,
  };
  predecessor.flow.closure.definitions[0]!.name = 'Triage';
  const launch = { project_id: project, id: 'launch', version: 2, digest: 'd'.repeat(64) };
  const access = { project_id: project, id: 'access', version: 3, digest: 'e'.repeat(64) };
  const empty = { items: [], next_cursor: null, total: 0 };
  const repository = {
    id: '64000000-0000-4000-8000-000000000001',
    project_id: project,
    name: 'Repository',
    version: 2,
    published_at_unix_ms: 1,
    definition_digest: '2'.repeat(64),
  };
  await page.route(`${scope}/repositories*`, (route) =>
    route.fulfill({ json: { ...empty, items: [repository], total: 1 } }),
  );
  await page.route(`**/api/v1/repositories/${repository.id}/versions/2`, (route) =>
    route.fulfill({
      json: {
        ...repository,
        definition: {
          vcs_integration_id: 'vcs',
          repository_locator: 'private',
          selection: {
            default_reference: 'main',
            allowed_references: ['main'],
            allow_exact_revision: false,
          },
        },
      },
    }),
  );
  const { role, profile, choice } = factoryRoleFixture();
  role.definition.name = 'Implementation';
  profile.definition.context_requirements = ['task'];
  const toolchain = profile.definition.toolchain;
  const executionPermissions = {
    ...baseline.configuration.flow.limits.permissions,
    plugins: [toolchain.plugin],
    executables: [toolchain.executable],
    tools: [toolchain.model],
  };
  profile.definition.permissions = executionPermissions;
  await page.route(
    (url) => url.pathname === `/api/v1/projects/${project}/factory-roles`,
    (route) =>
      route.fulfill({
        json: {
          ...empty,
          items: [{ reference: role.reference, name: role.definition.name }],
          total: 1,
        },
      }),
  );
  await page.route(`${scope}/factory-roles/${role.reference.id}/profiles**`, (route) =>
    route.fulfill({
      json: {
        ...empty,
        items: [
          {
            reference: profile.reference,
            role: profile.role,
            name: profile.definition.name,
            toolchain,
          },
        ],
        total: 1,
      },
    }),
  );
  await page.route(`${scope}/factory-role-profiles/${profile.reference.id}/versions/1`, (route) =>
    route.fulfill({ json: profile }),
  );
  await page.route(`${scope}/factory-profile-capabilities**`, (route) =>
    route.fulfill({ json: { ...empty, items: [choice], total: 1 } }),
  );
  const implementationBuild = {
    build_configuration: profile.definition.build_configuration,
    tool: toolchain.executable,
    plugin: toolchain.plugin,
    model_or_tool: toolchain.model,
    task_digest: '1'.repeat(64),
    result_output: 'implementation.json',
    result_output_kind: 'report',
    result_outcome: 'candidate',
  } satisfies S['FactoryFlowBuildBinding'];
  await page.route(`${scope}/factory-execution-bindings/resolve`, (route) => {
    expect(route.request().postDataJSON()).toEqual({
      profile: profile.reference,
      capability: choice.alias,
      result_outcome: 'candidate',
    });
    return route.fulfill({ json: { build: implementationBuild, incompatibility: null } });
  });
  let publishedFlow: S['FactoryFlowDefinitionDetails'] | null = null;
  const configurations: S['FactoryConfigurationSummaryResource'][] = [];
  const writes: string[] = [];
  const editorErrors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error' && message.text().includes('Encountered two children'))
      editorErrors.push(message.text());
  });
  let validatedFlow: S['CreateFactoryFlowDefinitionRequest'] | undefined;
  let validatedConfiguration: S['CreateFactoryConfigurationRequest'] | undefined;
  page.on('request', (request) => {
    const path = new URL(request.url()).pathname;
    if (request.method() !== 'GET' && !path.endsWith('/validate') && !path.endsWith('/resolve'))
      writes.push(path);
  });
  await page.route(`${scope}/flow-definitions*`, (route) =>
    route.fulfill({
      json: {
        ...empty,
        items: [predecessor, ...(publishedFlow ? [publishedFlow] : [])],
        total: publishedFlow ? 2 : 1,
      },
    }),
  );
  await page.route(`${scope}/factory-configurations*`, (route) =>
    route.fulfill({ json: { ...empty, items: configurations, total: configurations.length } }),
  );
  await page.route(`${scope}/builds*`, (route) => route.fulfill({ json: empty }));
  await page.route(`${scope}/factory-configuration-choices*`, (route) =>
    route.fulfill({
      json: {
        ...empty,
        project_policy_defaults: {
          project_id: project,
          version: 1,
          launch_policy: launch,
          access_profile: access,
        },
      },
    }),
  );
  await page.route(`${scope}/factory-policies*`, (route) => route.fulfill({ json: empty }));
  await page.route(`${scope}/factory-policies/defaults`, (route) =>
    route.fulfill({
      json: {
        defaults: {
          project_id: project,
          version: 1,
          launch_policy: launch,
          access_profile: access,
        },
      },
    }),
  );
  await page.route(
    `${scope}/factory-policies/launch/${launch.id}/versions/${launch.version}`,
    (route) =>
      route.fulfill({
        json: {
          policy: {
            kind: 'launch',
            policy: {
              reference: launch,
              definition: {
                name: 'Manual launch',
                allow_manual: true,
                allow_automatic_source: false,
              },
            },
          },
        },
      }),
  );
  await page.route(
    `${scope}/factory-policies/access/${access.id}/versions/${access.version}`,
    (route) =>
      route.fulfill({
        json: {
          policy: {
            kind: 'access',
            policy: {
              reference: access,
              definition: {
                name: 'Project access',
                permissions: {
                  ...executionPermissions,
                  plugins: [
                    ...executionPermissions.plugins,
                    plugin,
                    commandTool,
                    writer.binding.capability,
                  ],
                  executables: [...executionPermissions.executables, commandTool],
                },
              },
            },
          },
        },
      }),
  );
  function flowResource(request: S['CreateFactoryFlowDefinitionRequest']) {
    const reference = { id: request.id, version: 1, digest: 'f'.repeat(64) };
    return {
      project_id: project,
      published_at_unix_ms: 1,
      flow: {
        closure: { root: reference, definitions: [{ ...request.definition.root, reference }] },
        data_schemas: request.definition.data_schemas,
        limits: request.definition.limits,
        ...(request.definition.accepted_predecessor
          ? { accepted_predecessor: request.definition.accepted_predecessor }
          : {}),
      },
    };
  }
  await page.route('**/api/v1/flow-definitions/validate', (route) => {
    validatedFlow = route.request().postDataJSON();
    const publisher = validatedFlow!.definition.root.nodes.find(
      (node) => node.action?.action === 'publish_candidate',
    );
    expect(publisher?.action?.parameters).toMatchObject(
      Object.fromEntries(
        expiry.map((name) => [name, { value: { mapping: 'pointer', path: `/${name}` } }]),
      ),
    );
    expect(publisher?.code).toMatchObject({ kind: 'source', access: 'read_only' });
    expect(validatedFlow!.definition.accepted_predecessor).toEqual({
      definition: predecessor.flow.closure.root,
      terminal: 'succeeded',
      outcomes: ['succeeded'],
    });
    expect(
      validatedFlow!.definition.root.nodes.find(({ key }) => key === 'requirements_review')
        ?.input_binding,
    ).toMatchObject({
      sources: { triage: { source: 'incoming', path: '' } },
      mapping: { value: { mapping: 'pointer', path: '/triage' } },
    });
    // The same Context contract is exercised by the server's publication and execution tests.
    expect(publisher?.context_binding).toEqual(releasePublisher.context_binding);
    expect(publisher?.input_binding?.sources).toMatchObject({
      bundle: {
        source: 'record',
        node: 'implementation',
        view: 'metadata',
        path: '/build/outputs/change-set.bundle/artifact',
      },
      manifest: {
        source: 'record',
        node: 'implementation',
        view: 'metadata',
        path: '/build/outputs/change-set-manifest.json/artifact',
      },
      validation: {
        source: 'record',
        node: 'validate_candidate',
        view: 'input_payload',
        path: '/candidate_revision',
      },
      valid_until: {
        source: 'record',
        node: 'validate_candidate',
        outcome: 'checked',
        view: 'metadata',
        path: '/build/outputs/validation_facts/data/payload/value/fresh_until',
      },
      review_valid_until: {
        source: 'record',
        node: 'review_candidate',
        outcome: 'candidate',
        view: 'metadata',
        path: '/build/primary_fresh_until',
      },
    });
    return route.fulfill({ json: flowResource(validatedFlow!).flow });
  });
  await page.route('**/api/v1/flow-data-schemas/validate', (route) => {
    const { identity, version, shape }: S['ValidateFactoryFlowDataSchemaRequest'] = route
      .request()
      .postDataJSON();
    const digest = createHash('sha256')
      .update(JSON.stringify({ identity, version, shape }))
      .digest('hex');
    return route.fulfill({ json: { reference: { identity, version, digest }, shape } });
  });
  await page.route('**/api/v1/flow-definitions', (route) => {
    expect(route.request().postDataJSON()).toEqual(validatedFlow);
    publishedFlow = flowResource(validatedFlow!);
    return route.fulfill({ json: { disposition: 'applied', resource: publishedFlow } });
  });
  function configurationResource(request: S['CreateFactoryConfigurationRequest']) {
    return {
      ...baseline,
      configuration: {
        ...request.definition,
        reference: {
          ...baseline.configuration.reference,
          id: request.id,
        },
      },
    };
  }
  await page.route('**/api/v1/factory-configurations/validate', (route) => {
    validatedConfiguration = route.request().postDataJSON();
    return route.fulfill({
      json: {
        configuration: configurationResource(validatedConfiguration!).configuration,
        diagnostics: {
          errors: [],
          effective_permissions: [],
          required_node_capabilities: [],
          required_agent_capabilities: [],
          required_backend_capabilities: [],
          unavailable_references: [],
          warnings: [],
        },
      },
    });
  });
  await page.route('**/api/v1/factory-configurations', (route) => {
    expect(route.request().postDataJSON()).toEqual(validatedConfiguration);
    const resource = configurationResource(validatedConfiguration!);
    configurations.push({
      ...resource.configuration.reference,
      enabled: true,
      created_at_unix_ms: 1,
      published_at_unix_ms: 1,
    });
    return route.fulfill({ json: { disposition: 'applied', resource } });
  });
  await page.route(`**/api/v1/projects/${project}/factory-authoring-capabilities`, (route) =>
    route.fulfill({
      json: {
        commands: [command],
        human_gates: [
          {
            operation: 'escalation',
            resource_modes: ['none'],
            responses: [
              { disposition: 'resume', shape: { type: 'boolean' } },
              { disposition: 'abort', shape: { type: 'string', min_bytes: 1, max_bytes: 500 } },
            ],
          },
        ],
        build_resource_modes: ['none', 'repository'],
        source_free_incompatibility: null,
        candidate_publications: [
          {
            plugin,
            target,
            outcome: {
              key: 'published',
              kind: 'success',
              schema: receiptSchema.reference,
            },
            receipt_schema: receiptSchema,
            review_url: true,
            permissions: { ...emptyFactoryPermissions(), plugins: [plugin] },
          },
        ],
      },
    }),
  );

  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: 'Add pipeline', exact: true }).click();
  await page.getByRole('textbox', { name: 'Pipeline name', exact: true }).fill('Ticket delivery');
  async function choose(label: string, option: string, scope: Locator | typeof page = page) {
    await scope.getByRole('combobox', { name: label, exact: true }).click();
    await page.getByRole('option', { name: option, exact: true }).click();
  }
  await choose('Project repository', 'Repository · v2');
  await page.getByRole('button', { name: 'Add code source', exact: true }).click();
  await choose('Source access ceiling · Repository', 'Read and write');
  async function addStep(name: string, kind: string) {
    await page.getByRole('button', { name: 'Add step', exact: true }).click();
    await page.getByRole('textbox', { name: 'Step name', exact: true }).fill(name);
    await choose('Step type', kind);
  }
  async function addResult(name: string, kind = 'Success') {
    await page.getByRole('button', { name: 'Add result', exact: true }).click();
    await page.getByText('Technical details and reuse', { exact: true }).last().click();
    await page.getByRole('textbox', { name: 'Result key', exact: true }).fill(name);
    await choose('Outcome kind', kind);
    await page.getByRole('button', { name: 'Save result', exact: true }).click();
    await expect(page.getByRole('group', { name, exact: true })).toBeVisible();
  }
  await addStep('requirements_review', 'Operator decision');
  await addResult('approved');
  await addResult('needs_correction', 'Failure');
  await choose('Request purpose', 'Review and continuation');
  await choose('Continue → outcome', 'approved');
  await choose('Stop → outcome', 'needs_correction');
  await page.getByRole('button', { name: 'Use human decision', exact: true }).click();
  await expect(page.getByText('Human decision is configured')).toBeVisible();

  async function useResult(step: string, input: string, outcome?: string) {
    await choose('Result from step', step);
    if (outcome) await choose('Transition outcome', outcome);
    await choose('Result field', 'Entire typed value');
    await page.getByRole('textbox', { name: 'Input name', exact: true }).fill(input);
    await page.getByRole('button', { name: 'Use selected field', exact: true }).click();
    await expect(page.getByRole('group', { name: 'Accepted results' })).toContainText(
      `${input} ← ${step}`,
    );
  }
  await addStep('implementation', 'AI agent');
  await addResult('candidate');
  await choose('Role', 'Implementation · v1');
  await choose('Profile', 'default · v1');
  await page.getByRole('button', { name: 'Use this profile', exact: true }).click();
  await page.getByRole('checkbox', { name: 'Task from admitted Work', exact: true }).check();
  await choose('Code available to this step', 'Repository · v2');
  await choose('Step code access', 'Read and write');
  await expect(page.getByText('Candidate capture', { exact: true })).toBeVisible();
  await page.getByText('Candidate capture', { exact: true }).click();
  await page
    .getByRole('textbox', { name: 'Writable directories', exact: true })
    .fill('src\ndocs/specs');
  await page.getByRole('button', { name: 'Use this profile', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Use this profile', exact: true })).toBeEnabled();
  await expect(
    page.getByRole('textbox', { name: 'Writable directories', exact: true }),
  ).toHaveValue('docs/specs\nsrc');
  await useResult('requirements_review', 'approved', 'approved');

  await addStep('review_candidate', 'AI agent');
  await addResult('candidate');
  await page.getByText('Advanced data contracts', { exact: true }).click();
  const schema = page.getByRole('group', { name: 'Data schemas', exact: true });
  await schema.getByRole('textbox', { name: 'Schema name' }).fill('factory.base.code_review');
  await schema.getByRole('textbox', { name: 'Schema version' }).fill('v1');
  await choose('Schema type', 'Boolean', schema);
  await schema.getByRole('button', { name: 'Add schema', exact: true }).click();
  await expect(schema.getByRole('textbox', { name: 'Schema name' })).toHaveValue('');
  await page.getByText('Advanced data contracts', { exact: true }).click();
  const reviewResult = page.getByRole('group', { name: 'candidate', exact: true });
  await reviewResult.getByText('Reuse result format', { exact: true }).click();
  await reviewResult.getByRole('combobox', { name: 'Outcome schema', exact: true }).click();
  await page.getByRole('option', { name: 'factory.base.code_review · v1', exact: true }).click();
  await choose('Role', 'Implementation · v1');
  await choose('Profile', 'default · v1');
  await page.getByRole('button', { name: 'Use this profile', exact: true }).click();
  await page.getByRole('checkbox', { name: 'Task from admitted Work', exact: true }).check();
  await useResult('implementation', 'candidate_revision');

  await addStep('validate_candidate', 'Command');
  await addResult('checked');
  await choose('Installed command', 'Run protected checks · v1 · checks.json · test.runner v1');
  await page.getByRole('button', { name: 'Use command', exact: true }).click();
  await useResult('implementation', 'candidate_revision');
  await page.getByText('Advanced input and Context mappings', { exact: true }).click();
  await page.getByRole('checkbox', { name: 'Select a candidate revision' }).check();
  await page
    .getByRole('group', { name: 'Candidate revision', exact: true })
    .getByRole('textbox', { name: 'Selection path', exact: true })
    .fill('/candidate_revision');

  await addStep('report_review', 'Server action');
  await choose('Source connection', 'Tracker');
  await choose('Original task source', 'team/project · triage-labels');
  await choose('Reporter mapping', 'editorial-effects');
  await page.getByRole('button', { name: 'Use reviewed source effects' }).click();
  await expect(page.getByRole('group', { name: 'Expected result' })).toContainText('requested');
  expect(editorErrors).toEqual([]);

  await addStep('publish_review', 'Server action');

  await page.getByRole('button', { name: 'Publish candidate', exact: true }).click();
  await expect(page.getByRole('combobox', { name: 'Publication target' })).toBeVisible();
  await page.getByRole('combobox', { name: 'Publication target' }).click();
  await expect(
    page.getByRole('option', { name: 'project-review · v2', exact: true }),
  ).toBeVisible();
  await page.getByRole('option', { name: 'project-review · v2', exact: true }).click();
  await page.getByRole('button', { name: 'Use publication target', exact: true }).click();
  await choose('Code available to this step', 'Repository · v2');
  await choose('Step code revision', 'Accepted candidate from another step');
  await choose('Candidate-producing step', 'implementation');
  await choose('Accepted candidate outcome', 'candidate');
  await page.getByText('Publication inputs', { exact: true }).click();
  await page
    .getByRole('textbox', { name: 'Validation expiry input path', exact: true })
    .fill('/valid_until');
  await page
    .getByRole('textbox', { name: 'Review expiry input path', exact: true })
    .fill('/review_valid_until');
  await expect(page.getByRole('group', { name: 'Expected result', exact: true })).toContainText(
    'published',
  );

  await page.getByText('Advanced data contracts', { exact: true }).click();
  await schema.getByRole('textbox', { name: 'Schema name' }).fill('publication-input');
  await schema.getByRole('textbox', { name: 'Schema version' }).fill('v1');
  await choose('Schema type', 'Object', schema);
  async function addField(container: Locator, name: string, kind: string) {
    const fields = container.getByRole('textbox', { name: 'Field name', exact: true });
    const index = await fields.count();
    await container.getByRole('button', { name: 'Add field', exact: true }).first().click();
    const field = fields.nth(index);
    await field.fill(name);
    const row = field.locator('xpath=ancestor::fieldset[1]');
    await choose('Schema type', kind, row);
    return row;
  }
  await addField(schema, 'candidate_revision', 'String');
  for (const name of expiry) await addField(schema, name, 'Integer');
  for (const name of ['bundle', 'manifest']) {
    const field = await addField(schema, name, 'Object');
    await addField(field, 'artifact_id', 'String');
    await addField(field, 'content_digest', 'String');
    await addField(field, 'encoded_size', 'Integer');
  }
  await schema.getByRole('button', { name: 'Add schema', exact: true }).click();
  await expect(schema.getByRole('textbox', { name: 'Schema name' })).toHaveValue('');
  await page.getByText('Advanced data contracts', { exact: true }).click();

  const summary = page.getByText('Advanced input and Context mappings', { exact: true });
  const advanced = summary.locator('..');
  const inputSchema = advanced.getByRole('combobox', { name: 'Input schema', exact: true });
  if (!(await inputSchema.isVisible())) await summary.click();
  await inputSchema.click();
  await page.getByRole('option', { name: 'publication-input · v1', exact: true }).click();
  async function mapField(container: Locator, name: string) {
    await container.getByRole('button', { name: 'Add mapped field', exact: true }).first().click();
    const field = container.getByRole('textbox', { name: 'Mapped field name' }).last();
    await field.fill(name);
    return field.locator('xpath=ancestor::fieldset[1]');
  }
  for (const [name, node, view, path, field] of [
    [
      'validation',
      'validate_candidate',
      'Retained step input',
      '/candidate_revision',
      'candidate_revision',
    ],
    ['bundle', 'implementation', 'Metadata', '/build/outputs/change-set.bundle/artifact', 'bundle'],
    [
      'manifest',
      'implementation',
      'Metadata',
      '/build/outputs/change-set-manifest.json/artifact',
      'manifest',
    ],
    [
      'valid_until',
      'validate_candidate',
      'Metadata',
      '/build/outputs/validation_facts/data/payload/value/fresh_until',
      'valid_until',
    ],
    [
      'review_valid_until',
      'review_candidate',
      'Metadata',
      '/build/primary_fresh_until',
      'review_valid_until',
    ],
  ]) {
    const mapped = await mapField(advanced, field!);
    await mapped.getByRole('textbox', { name: 'Selection path' }).fill(`/${name}`);
    await advanced.getByRole('button', { name: 'Add input source', exact: true }).click();
    const input = advanced.getByRole('textbox', { name: 'Input source name' }).last();
    await input.fill(name!);
    const row = input.locator('xpath=ancestor::fieldset[1]');
    await choose('Input source', 'Node record', row);
    await choose('Source node', node!, row);
    await choose('Record view', view!, row);
    await row.getByRole('textbox', { name: 'Source field path' }).fill(path!);
  }
  await advanced.getByRole('checkbox', { name: 'Select a candidate revision' }).check();
  await advanced
    .getByRole('group', { name: 'Candidate revision' })
    .getByRole('textbox', { name: 'Selection path' })
    .fill('/candidate_revision');
  for (const name of ['bundle', 'manifest']) {
    await advanced.getByRole('button', { name: 'Add Context entry', exact: true }).click();
    const entry = advanced.getByRole('textbox', { name: 'Context entry name' }).last();
    await entry.fill(`candidate_${name}`);
    const row = entry.locator('xpath=ancestor::fieldset[1]');
    await choose('Context source kind', 'Evidence', row);
    await choose('Mapping kind', 'Object', row);
    const kind = await mapField(row, 'kind');
    await choose('Mapping kind', 'Typed constant', kind);
    await choose('Constant type', 'String', kind);
    await kind.getByRole('textbox', { name: 'Constant value' }).fill('artifact');
    const value = await mapField(row, 'value');
    await value.getByRole('textbox', { name: 'Selection path' }).fill(`/${name}`);
  }

  await page.getByRole('button', { name: 'Review required access' }).click();
  await page.getByRole('button', { name: 'Apply to draft' }).click();
  await page.getByRole('button', { name: 'Flow settings', exact: true }).click();
  await choose('Incoming pipeline completion', 'Triage · v1 · succeeded');
  await page.getByRole('checkbox', { name: 'succeeded', exact: true }).check();
  await page.getByRole('button', { name: 'Add terminal', exact: true }).click();
  await page
    .getByRole('textbox', { name: 'Terminal name', exact: true })
    .fill('delivered_for_review');
  const completions = page.getByRole('combobox', { name: 'Completion result', exact: true });
  await completions.first().click();
  await page.getByRole('option', { name: 'publish_review.published', exact: true }).click();
  await page.getByRole('button', { name: 'Add terminal', exact: true }).click();
  await page
    .getByRole('textbox', { name: 'Terminal name', exact: true })
    .last()
    .fill('needs_correction');
  await completions.last().click();
  await page
    .getByRole('option', { name: 'requirements_review.needs_correction', exact: true })
    .click();
  await page.getByRole('button', { name: 'Fit graph', exact: true }).click();
  async function connect(from: string, to: string, outcome?: string) {
    await page.getByRole('button', { name: `Connect from ${from}`, exact: true }).press('Enter');
    const route = page.getByRole('group', { name: 'New route', exact: true });
    if (outcome) {
      await route.getByRole('combobox', { name: 'Transition outcome', exact: true }).click();
      await page.getByRole('option', { name: outcome, exact: true }).click();
    }
    await route.getByRole('combobox', { name: 'Target', exact: true }).click();
    await page.getByRole('option', { name: to, exact: true }).click();
    await route.getByRole('button', { name: 'Add route', exact: true }).click();
  }
  await connect('requirements_review', 'Step · implementation', 'approved');
  await connect('requirements_review', 'Terminal · needs_correction', 'needs_correction');
  await connect('implementation', 'Step · review_candidate');
  await connect('review_candidate', 'Step · validate_candidate');
  await connect('validate_candidate', 'Step · report_review');
  await connect('report_review', 'Step · publish_review');
  await connect('publish_review', 'Terminal · delivered_for_review');

  await page
    .getByRole('button', { name: 'Edit step: requirements_review', exact: true })
    .press('Enter');
  const incomingSchema = advanced.getByRole('combobox', { name: 'Input schema', exact: true });
  if (!(await incomingSchema.isVisible())) await summary.click();
  await incomingSchema.click();
  const triageResult = predecessor.flow.closure.definitions[0]!.terminals.find(
    ({ key }) => key === 'succeeded',
  )!.schema;
  await page
    .getByRole('option', {
      name: `${triageResult.identity} · ${triageResult.version}`,
      exact: true,
    })
    .click();
  await choose('Mapping kind', 'Field selection', advanced);
  await advanced.getByRole('textbox', { name: 'Selection path', exact: true }).fill('/triage');
  await advanced.getByRole('button', { name: 'Add input source', exact: true }).click();
  await advanced.getByRole('textbox', { name: 'Input source name', exact: true }).fill('triage');
  await choose('Input source', 'Incoming projection', advanced);
  await choose('Source schema', 'Step input · requirements_review', advanced);

  async function publish() {
    await page.getByRole('button', { name: 'Validate draft', exact: true }).click();
    await page.getByRole('button', { name: 'Review publication', exact: true }).click();
    await page.getByRole('button', { name: 'Confirm publish', exact: true }).click();
    await expect(page.getByText('Immutable version published.')).toBeVisible();
    await page.getByRole('button', { name: 'Close', exact: true }).click();
    await page.getByRole('button', { name: 'Discard draft', exact: true }).click();
  }
  await publish();
  const root = validatedFlow!.definition.root;
  expect(root.nodes.find((node) => node.key === 'requirements_review')?.operator_requests).toEqual({
    operation: 'escalation',
    outcomes: { resume: 'approved', abort: 'needs_correction' },
  });
  expect(root.nodes.find((node) => node.key === 'implementation')).toMatchObject({
    code: {
      kind: 'source',
      source: 'code_1',
      access: 'read_write',
      revision: { kind: 'admitted_base' },
    },
    profile_selection: {
      candidates: {
        [profile.reference.id]: {
          profile,
          build: { ...implementationBuild, capture_write_paths: ['docs/specs', 'src'] },
        },
      },
    },
    context_binding: { work_task: 'task', entries: {} },
    input_binding: {
      sources: {
        accepted_0: {
          source: 'record',
          node: 'requirements_review',
          outcome: 'approved',
          view: 'payload',
          path: '',
        },
      },
    },
  });
  expect(root.nodes.find((node) => node.key === 'validate_candidate')).toMatchObject({
    build: { ...command.execution, result_outcome: 'checked' },
    input_binding: {
      sources: {
        accepted_0: {
          source: 'record',
          node: 'implementation',
          outcome: 'candidate',
          view: 'payload',
          path: '',
        },
      },
    },
    context_binding: {
      candidate_revision: { value: { mapping: 'pointer', path: '/candidate_revision' } },
    },
  });
  expect(root.nodes.find((node) => node.key === 'report_review')?.action).toMatchObject({
    plugin: writer.binding.capability,
    action: 'set_status',
    parameters: { source_update: { reporter: writer.binding } },
  });
  expect(root.nodes.find((node) => node.key === 'publish_review')?.action).toMatchObject({
    plugin,
    action: 'publish_candidate',
    parameters: { target, require_review_url: true },
  });
  expect(root.transitions).toHaveLength(7);
  expect(root.transitions).toContainEqual(
    expect.objectContaining({
      predecessor: 'publish_review',
      outcome: 'published',
      target: { kind: 'terminal', key: 'delivered_for_review' },
    }),
  );
  await page.getByRole('button', { name: 'Create configuration draft', exact: true }).click();
  await page.getByRole('checkbox', { name: 'Enabled', exact: true }).check();
  await choose('Root Flow', 'Ticket delivery · v1');
  await publish();
  expect(validatedConfiguration?.definition).toMatchObject({
    enabled: true,
    flow: { closure: { root: publishedFlow!.flow.closure.root } },
    admission_policy: { project_policy: launch },
    permission_ceiling: { project_policy: access },
  });

  const issue = issueFixture(project);
  const triage = executionFixture('admitted', 'triage-request', project);
  triage.intent.target.repository_id = repository.id;
  issue.content.acceptance = 'Show the result in the Issue card.';
  const issuePath = `/projects/${project}/issues/${issue.id}`;
  await page.route(`**/api/v1${issuePath}`, (route) => route.fulfill({ json: issue }));
  await page.route(
    (url) => url.pathname === `/api/v1${issuePath}/executions`,
    (route) => {
      if (route.request().method() === 'GET')
        return route.fulfill({ json: { ...empty, items: [triage], total: 1 } });
      expect(route.request().method()).toBe('POST');
      expect(route.request().postDataJSON().target).toMatchObject({
        configuration_id: configurations[0]!.id,
        configuration_version: configurations[0]!.version,
        repository_id: repository.id,
        repository_version: repository.version,
        predecessor_run: triage.association!.run_id,
      });
      return route.fulfill({
        json: {
          disposition: 'applied',
          resource: executionFixture('admitted', 'delivery-request', project),
        },
      });
    },
  );
  await page.goto(issuePath);
  await page.getByRole('button', { name: 'Start pipeline', exact: true }).click();
  await page
    .getByRole('checkbox', { name: 'Use a result from an earlier run', exact: true })
    .check();
  await page.getByRole('combobox', { name: 'Earlier Issue run', exact: true }).click();
  await page.getByRole('option', { name: /triage-request$/ }).click();
  await page.getByRole('button', { name: 'Review pipeline launch', exact: true }).click();
  await page.getByRole('button', { name: 'Confirm pipeline launch', exact: true }).click();
  await expect(page.getByRole('dialog').getByRole('link', { name: 'Open Run' })).toBeVisible();
  expect(writes).toEqual([
    '/api/v1/flow-definitions',
    '/api/v1/factory-configurations',
    `/api/v1${issuePath}/executions`,
  ]);
});
