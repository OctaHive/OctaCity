import { expect, test } from '@playwright/test';
import { configurationFixture } from '../fixtures/factory';
import type { components } from '../../.generated/api/schema';
import { issueFixture } from '../fixtures/issues';
import { executionFixture } from '../fixtures/issueExecutions';
import { installOperatorFixture } from './operatorFixture';

test('publishes the exact Flow and Project policies from a clean Project and starts an Issue manually', async ({
  page,
}) => {
  await installOperatorFixture(page);
  type S = components['schemas'];
  const baseline = configurationFixture();
  const project = baseline.configuration.reference.project_id;
  const scope = `**/api/v1/projects/${project}`;
  const plugin = { identity: 'candidate.publisher', version: 'v1', digest: 'a'.repeat(64) };
  const target = { identity: 'project-review', version: 'v2', digest: 'b'.repeat(64) };
  const launch = { project_id: project, id: 'launch', version: 2, digest: 'd'.repeat(64) };
  const access = { project_id: project, id: 'access', version: 3, digest: 'e'.repeat(64) };
  const empty = { items: [], next_cursor: null, total: 0 };
  let publishedFlow: S['FactoryFlowDefinitionDetails'] | null = null;
  const configurations: S['FactoryConfigurationSummaryResource'][] = [];
  const writes: string[] = [];
  let validatedFlow: S['CreateFactoryFlowDefinitionRequest'] | undefined;
  let validatedConfiguration: S['CreateFactoryConfigurationRequest'] | undefined;
  page.on('request', (request) => {
    if (request.method() !== 'GET' && !request.url().endsWith('/validate'))
      writes.push(new URL(request.url()).pathname);
  });
  await page.route(`${scope}/flow-definitions*`, (route) =>
    route.fulfill({ json: publishedFlow ? { ...empty, items: [publishedFlow], total: 1 } : empty }),
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
                  ...baseline.configuration.flow.limits.permissions,
                  plugins: [plugin],
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
      },
    };
  }
  await page.route('**/api/v1/flow-definitions/validate', (route) => {
    validatedFlow = route.request().postDataJSON();
    return route.fulfill({ json: flowResource(validatedFlow!).flow });
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
        commands: [],
        human_gates: [],
        build_resource_modes: ['none', 'repository'],
        source_free_incompatibility: null,
        candidate_publications: [
          {
            plugin,
            target,
            outcome: {
              key: 'published',
              kind: 'success',
              schema: { identity: 'publication-receipt', version: 'v1', digest: 'c'.repeat(64) },
            },
            receipt_schema: {
              reference: { identity: 'publication-receipt', version: 'v1', digest: 'c'.repeat(64) },
              shape: { type: 'object', fields: {} },
            },
            review_url: true,
          },
        ],
      },
    }),
  );

  await page.goto(`/factory?project_id=${project}`);
  await page.getByRole('button', { name: 'Add pipeline', exact: true }).click();
  await page.getByRole('button', { name: 'Add step', exact: true }).click();
  await page.getByRole('textbox', { name: 'Step name', exact: true }).fill('publish_review');
  await page.getByRole('combobox', { name: 'Step type', exact: true }).click();
  await page.getByRole('option', { name: 'Server action', exact: true }).click();

  await page.getByRole('button', { name: 'Publish candidate', exact: true }).click();
  await expect(page.getByRole('combobox', { name: 'Publication target' })).toBeVisible();
  await page.getByRole('combobox', { name: 'Publication target' }).click();
  await expect(
    page.getByRole('option', { name: 'project-review · v2', exact: true }),
  ).toBeVisible();
  await page.getByRole('option', { name: 'project-review · v2', exact: true }).click();
  await page.getByRole('button', { name: 'Use publication target', exact: true }).click();
  await expect(page.getByRole('group', { name: 'Expected result', exact: true })).toContainText(
    'published',
  );

  await page.getByRole('button', { name: 'Review required access' }).click();
  await page.getByRole('button', { name: 'Apply to draft' }).click();
  await page.getByRole('button', { name: 'Flow settings', exact: true }).click();
  await page.getByRole('button', { name: 'Add terminal', exact: true }).click();
  await page
    .getByRole('textbox', { name: 'Terminal name', exact: true })
    .fill('delivered_for_review');
  await page.getByRole('button', { name: 'Connect from publish_review', exact: true }).click();
  await page.getByRole('combobox', { name: 'Target', exact: true }).click();
  await page.getByRole('option', { name: 'Terminal · delivered_for_review', exact: true }).click();
  await page.getByRole('button', { name: 'Add route', exact: true }).click();

  async function publish() {
    await page.getByRole('button', { name: 'Validate draft', exact: true }).click();
    await page.getByRole('button', { name: 'Review publication', exact: true }).click();
    await page.getByRole('button', { name: 'Confirm publish', exact: true }).click();
    await expect(page.getByText('Immutable version published.')).toBeVisible();
    await page.getByRole('button', { name: 'Close', exact: true }).click();
    await page.getByRole('button', { name: 'Discard draft', exact: true }).click();
  }
  await publish();
  expect(validatedFlow?.definition.root.nodes[0]?.action).toMatchObject({
    plugin,
    action: 'publish_candidate',
    parameters: { target, require_review_url: true },
  });
  expect(validatedFlow?.definition.root.transitions).toMatchObject([
    {
      predecessor: 'publish_review',
      outcome: 'published',
      target: { kind: 'terminal', key: 'delivered_for_review' },
    },
  ]);
  await page.getByRole('button', { name: 'Create configuration draft', exact: true }).click();
  await page.getByRole('checkbox', { name: 'Enabled', exact: true }).check();
  await publish();
  expect(validatedConfiguration?.definition).toMatchObject({
    enabled: true,
    flow: { closure: { root: publishedFlow!.flow.closure.root } },
    admission_policy: { project_policy: launch },
    permission_ceiling: { project_policy: access },
  });

  const issue = issueFixture(project);
  issue.content.acceptance = 'Show the result in the Issue card.';
  const issuePath = `/projects/${project}/issues/${issue.id}`;
  await page.route(`**/api/v1${issuePath}`, (route) => route.fulfill({ json: issue }));
  const repository = {
    id: 'repo',
    project_id: project,
    name: 'Repository',
    version: 2,
    published_at_unix_ms: 1,
  };
  await page.route(`${scope}/repositories*`, (route) =>
    route.fulfill({ json: { ...empty, items: [repository], total: 1 } }),
  );
  await page.route('**/api/v1/repositories/repo/versions/2', (route) =>
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
  await page.route(`**/api/v1${issuePath}/executions`, (route) => {
    expect(route.request().method()).toBe('POST');
    expect(route.request().postDataJSON().target).toMatchObject({
      configuration_id: configurations[0]!.id,
      configuration_version: configurations[0]!.version,
      repository_id: repository.id,
      repository_version: repository.version,
    });
    return route.fulfill({
      json: {
        disposition: 'applied',
        resource: executionFixture('admitted', 'delivery-request', project),
      },
    });
  });
  await page.goto(issuePath);
  await page.getByRole('button', { name: 'Start pipeline', exact: true }).click();
  await page.getByRole('button', { name: 'Review pipeline launch', exact: true }).click();
  await page.getByRole('button', { name: 'Confirm pipeline launch', exact: true }).click();
  await expect(page.getByRole('dialog').getByRole('link', { name: 'Open Run' })).toBeVisible();
  expect(writes).toEqual([
    '/api/v1/flow-definitions',
    '/api/v1/factory-configurations',
    `/api/v1${issuePath}/executions`,
  ]);
});
