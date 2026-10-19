import { configurationFixture, runFixture, diagnosticFixture } from '../fixtures/factory';
import { issueFixture } from '../fixtures/issues';
import { workflowFixture } from '../fixtures/issueWorkflows';
import { integrationFixture } from '../fixtures/integrations';
import type { Page, Route } from '@playwright/test';

export const FIXTURE_IDS = {
  factoryRun: '44444444-4444-4444-8444-444444444444',
  agent: 'agent-accessible',
  attempt: 'attempt-accessible',
  build: 'build-accessible',
  pool: 'pool-accessible',
  project: 'project-accessible',
} as const;

/** Installs one deterministic, secret-free management fixture for cross-route browser checks. */
export async function installOperatorFixture(page: Page) {
  let apiReadsFail = false;
  await page.route('**/health/ready', async (route) => fulfill(route, { status: 'ready' }));
  await page.route('**/api/v1/**', async (route) => {
    if (apiReadsFail) {
      await failUnavailable(route);
      return;
    }
    const url = new URL(route.request().url());
    if (url.pathname === `/api/v1/projects/${FIXTURE_IDS.project}/issue-workflow-default`) {
      await fulfill(route, { version: 7, workflow: workflowFixture(FIXTURE_IDS.project) });
      return;
    }
    if (url.pathname === `/api/v1/projects/${FIXTURE_IDS.project}/issue-workflows`) {
      const workflow = workflowFixture(FIXTURE_IDS.project);
      await fulfill(route, {
        items: [{ ...workflow, name: workflow.definition.name }],
        next_cursor: null,
      });
      return;
    }
    const installedWorkflow = workflowFixture(FIXTURE_IDS.project);
    if (
      url.pathname ===
        `/api/v1/projects/${FIXTURE_IDS.project}/issue-workflows/${installedWorkflow.id}` ||
      url.pathname ===
        `/api/v1/projects/${FIXTURE_IDS.project}/issue-workflows/${installedWorkflow.id}/versions/${installedWorkflow.version}`
    ) {
      await fulfill(route, installedWorkflow);
      return;
    }
    if (url.pathname === `/api/v1/projects/${FIXTURE_IDS.project}/issue-property-choices`) {
      await fulfill(route, {
        definitions: {
          project_id: FIXTURE_IDS.project,
          version: 0,
          published_at_unix_ms: 1,
          definition: { labels: [], properties: [] },
        },
        assignees: { available: false, items: [] },
      });
      return;
    }
    const issue = issueFixture(FIXTURE_IDS.project);
    const issueBase = `/api/v1/projects/${FIXTURE_IDS.project}/issues`;
    if (url.pathname === issueBase) {
      await fulfill(route, {
        items: [{ ...issue, title: issue.content.title }],
        next_cursor: null,
      });
      return;
    }
    if (url.pathname === `${issueBase}/${issue.id}`) {
      await fulfill(route, issue);
      return;
    }
    if (url.pathname === `${issueBase}/${issue.id}/transitions`) {
      await fulfill(route, {
        issue_version: issue.version,
        workflow_id: issue.workflow_id,
        workflow_version: issue.workflow_version,
        status_id: issue.status_id,
        field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
        items: [],
      });
      return;
    }
    if (
      url.pathname === `${issueBase}/${issue.id}/children` ||
      url.pathname === `${issueBase}/${issue.id}/status-history` ||
      url.pathname === `${issueBase}/${issue.id}/revisions`
    ) {
      await fulfill(route, { items: [], next_cursor: null });
      return;
    }
    const connection = integrationFixture('IntegrationConnection');
    if (url.pathname === '/api/v1/integrations') {
      await fulfill(route, { items: [connection], next_cursor: null });
      return;
    }
    if (url.pathname === '/api/v1/integration-installations') {
      const definition = connection.definition;
      await fulfill(route, {
        items: [
          {
            installation: definition.installation,
            provider: definition.provider,
            adapter: definition.adapter,
            endpoints: [definition.endpoint],
            authentication: [definition.authentication],
            resources: definition.resources,
          },
        ],
        next_cursor: null,
      });
      return;
    }
    if (url.pathname === `/api/v1/integrations/${connection.id}`) {
      await fulfill(route, connection);
      return;
    }
    if (
      url.pathname === `/api/v1/integrations/${connection.id}/state` &&
      route.request().method() === 'GET'
    ) {
      await fulfill(route, integrationFixture('IntegrationAdministrationState'));
      return;
    }
    if (url.pathname === `/api/v1/integrations/${connection.id}/checks`) {
      await fulfill(route, integrationFixture('IntegrationConnectionCheckResult'));
      return;
    }
    const configuration = configurationFixture();
    const run = runFixture();
    const owner = configuration.configuration.reference.project_id;
    if (url.pathname === '/api/v1/factory-runs') {
      await fulfill(route, { items: [run], total: 1, next_cursor: null });
      return;
    }
    if (url.pathname === `/api/v1/factory-runs/${FIXTURE_IDS.factoryRun}`) {
      await fulfill(route, run);
      return;
    }
    if (url.pathname.startsWith(`/api/v1/factory-runs/${FIXTURE_IDS.factoryRun}/`)) {
      const collection = url.pathname.split('/').at(-1)!;
      if (route.request().method() === 'POST') {
        await fulfill(route, {
          run_id: run.id,
          version: run.version + 1,
          state: run.state,
          cancellation_requested: true,
          disposition: 'applied',
        });
      } else await fulfill(route, { items: diagnosticFixture(collection), next_cursor: null });
      return;
    }
    const repositoryId = '66666666-6666-4666-8666-666666666666';
    if (url.pathname === `/api/v1/projects/${owner}/repositories`) {
      await fulfill(
        route,
        pageOf([
          {
            id: repositoryId,
            name: 'Factory source',
            project_id: owner,
            version: 1,
            published_at_unix_ms: 1,
          },
        ]),
      );
      return;
    }
    if (url.pathname === `/api/v1/repositories/${repositoryId}/versions/1`) {
      await fulfill(route, {
        id: repositoryId,
        name: 'Factory source',
        project_id: owner,
        version: 1,
        published_at_unix_ms: 1,
        definition: {
          repository_locator: 'fixture',
          vcs_integration_id: 'visible-vcs',
          selection: {
            default_reference: 'main',
            allowed_references: ['main'],
            allow_exact_revision: false,
          },
        },
      });
      return;
    }
    if (url.pathname === `/api/v1/projects/${owner}/builds`) {
      await fulfill(
        route,
        pageOf([
          {
            ...buildResource(),
            id: '77777777-7777-4777-8777-777777777777',
            project_id: owner,
            cause: null,
            current_attempt_id: FIXTURE_IDS.attempt,
            current_attempt_number: 1,
            current_attempt_state: 'succeeded',
            terminal_at_unix_ms: 2,
          },
        ]),
      );
      return;
    }
    if (url.pathname === '/api/v1/builds/77777777-7777-4777-8777-777777777777/artifacts') {
      await fulfill(route, {
        items: ['Task input', 'Acceptance input', 'Specification input'].map((name, index) => ({
          id: `88888888-8888-4888-8888-${String(index + 1).padStart(12, '0')}`,
          name,
          build_id: '77777777-7777-4777-8777-777777777777',
          attempt_id: FIXTURE_IDS.attempt,
          job_id: 'fixture-job',
          media_type: 'text/plain',
          output_type: { kind: 'artifact' },
          published_at_unix_ms: 1,
          sha256: 'a'.repeat(64),
          size_bytes: 20,
        })),
      });
      return;
    }
    if (url.pathname === `/api/v1/projects/${owner}/factory-work`) {
      await fulfill(route, {
        disposition: 'applied',
        factory_run_id: run.id,
        work_envelope_id: run.work_id,
        base_revision: run.diagnostics.base_revision,
        state: 'admitted',
        admitted_at_unix_ms: 1,
      });
      return;
    }
    if (url.pathname === `/api/v1/projects/${owner}/factory-phase-ready`) {
      await fulfill(route, { items: [], next_cursor: null });
      return;
    }
    if (url.pathname === `/api/v1/projects/${owner}`) {
      await fulfill(route, {
        ancestors: [],
        project: { ...projectResource(), id: owner, name: 'Factory Project' },
      });
      return;
    }
    if (url.pathname === `/api/v1/projects/${owner}/factory-configurations`) {
      await fulfill(route, {
        items: [
          {
            ...configuration.configuration.reference,
            enabled: true,
            created_at_unix_ms: 1,
            published_at_unix_ms: 1,
          },
        ],
        total: 1,
        next_cursor: null,
      });
      return;
    }
    if (url.pathname === `/api/v1/projects/${owner}/factory-runs`) {
      await fulfill(route, { items: [run], total: 1, next_cursor: null });
      return;
    }
    if (
      url.pathname.startsWith(
        `/api/v1/factory-configurations/${configuration.configuration.reference.id}/versions/`,
      )
    ) {
      await fulfill(route, configuration);
      return;
    }
    if (url.pathname === `/api/v1/projects/${owner}/flow-definitions`) {
      await fulfill(route, {
        items: [
          { flow: configuration.configuration.flow, project_id: owner, published_at_unix_ms: 1 },
        ],
        total: 1,
        next_cursor: null,
      });
      return;
    }
    if (
      url.pathname.startsWith(
        `/api/v1/flow-definitions/${configuration.configuration.flow.closure.root.id}/versions/`,
      )
    ) {
      await fulfill(route, {
        flow: configuration.configuration.flow,
        project_id: owner,
        published_at_unix_ms: 1,
      });
      return;
    }
    if (
      url.pathname === `/api/v1/projects/${owner}/factory-configuration-choices` ||
      url.pathname === `/api/v1/projects/${FIXTURE_IDS.project}/factory-configurations`
    ) {
      await fulfill(route, { items: [], total: 0, next_cursor: null });
      return;
    }
    const projectPath = `/api/v1/projects/${FIXTURE_IDS.project}`;
    const buildPath = `/api/v1/builds/${FIXTURE_IDS.build}`;

    if (url.pathname === '/api/v1/projects') {
      await fulfill(route, pageOf(url.searchParams.has('parent_id') ? [] : [projectSummary()]));
      return;
    }
    if (url.pathname === projectPath) {
      await fulfill(route, { ancestors: [], project: projectResource() });
      return;
    }
    if (
      url.pathname === `${projectPath}/pipelines` ||
      url.pathname === `${projectPath}/repositories` ||
      url.pathname === `${projectPath}/build-configurations` ||
      url.pathname === `${projectPath}/trigger-definitions` ||
      url.pathname === `${projectPath}/builds`
    ) {
      await fulfill(route, pageOf([]));
      return;
    }
    if (url.pathname === buildPath) {
      await fulfill(route, buildResource());
      return;
    }
    if (url.pathname === `/api/v1/attempts/${FIXTURE_IDS.attempt}`) {
      await fulfill(route, { attempt: attemptResource(), edges: [], jobs: [] });
      return;
    }
    if (
      url.pathname === `${buildPath}/artifacts` ||
      url.pathname === `${buildPath}/cache-sessions`
    ) {
      await fulfill(route, { items: [] });
      return;
    }
    if (url.pathname === `${buildPath}/retention`) {
      await fulfill(route, retentionResource());
      return;
    }
    if (url.pathname === '/api/v1/agent-pools') {
      await fulfill(route, pageOf([poolResource()]));
      return;
    }
    if (url.pathname === `/api/v1/agent-pools/${FIXTURE_IDS.pool}`) {
      await fulfill(route, poolResource());
      return;
    }
    if (url.pathname === '/api/v1/agents') {
      await fulfill(route, pageOf([agentResource()]));
      return;
    }
    if (url.pathname === `/api/v1/agents/${FIXTURE_IDS.agent}`) {
      await fulfill(route, agentResource());
      return;
    }
    if (url.pathname === '/api/v1/audit-facts') {
      await fulfill(route, pageOf([auditFact()]));
      return;
    }
    await route.fulfill({
      body: JSON.stringify({
        code: 'not_found',
        message: 'The fixture resource was not found.',
        request_id: 'fixture-request',
      }),
      contentType: 'application/json',
      headers: { 'x-request-id': 'fixture-request' },
      status: 404,
    });
  });
  return {
    failApiReads() {
      apiReadsFail = true;
    },
  };
}

function pageOf(items: unknown[]) {
  return { items, next_cursor: null };
}

function projectResource() {
  return {
    created_at_unix_ms: 1_700_000_000_000,
    id: FIXTURE_IDS.project,
    name: 'Accessible Project',
    parent_id: null,
    updated_at_unix_ms: 1_700_000_001_000,
    version: 1,
  };
}

function projectSummary() {
  return { ...projectResource(), has_children: false };
}

function attemptResource() {
  return {
    build_id: FIXTURE_IDS.build,
    created_at_unix_ms: 1_700_000_000_000,
    id: FIXTURE_IDS.attempt,
    number: 1,
    retry_of_attempt_id: null,
    state: 'succeeded',
    updated_at_unix_ms: 1_700_000_001_000,
    version: 1,
  };
}

function buildResource() {
  return {
    configuration_id: 'configuration-accessible',
    configuration_version: 1,
    created_at_unix_ms: 1_700_000_000_000,
    current_attempt: attemptResource(),
    effective_policy: {
      policy: {
        artifacts: {
          artifact_bytes: 0,
          artifact_count: 0,
          report_bytes: 0,
          report_count: 0,
          single_output_bytes: 0,
        },
        cache: { max_bytes: 0, namespaces: [], read: false, write: false },
        concurrency: { active_builds: 1, active_jobs: 1 },
        execution_targets: [],
        identity_profiles: [],
        pools: [],
        repositories: [],
        retention: {
          artifact_seconds: 0,
          build_seconds: 0,
          cache_seconds: 0,
          log_seconds: 0,
        },
        runtimes: [],
        secret_profiles: [],
      },
      sources: [],
    },
    id: FIXTURE_IDS.build,
    immutable_revision: '0123456789abcdef',
    parameters: {},
    pipeline_id: 'pipeline-accessible',
    pipeline_version: 1,
    priority: 0,
    project_id: FIXTURE_IDS.project,
    repository_id: 'repository-accessible',
    repository_version: 1,
    source: { kind: 'exact_revision', value: '0123456789abcdef' },
    state: 'succeeded',
    trigger: {
      build_id: FIXTURE_IDS.build,
      causality: {
        depth: 0,
        parent_occurrence_id: null,
        root_occurrence_id: 'occurrence-accessible',
      },
      cause: { kind: 'manual' },
      created_at: 1_700_000_000_000,
      deduplication_identity: 'manual-accessible',
      id: 'occurrence-accessible',
      kind: 'manual',
      source_time: 1_700_000_000_000,
      state: 'accepted',
      target: { configuration_id: 'configuration-accessible', configuration_version: 1 },
      trigger: { id: 'trigger-accessible', version: 1 },
      updated_at: 1_700_000_000_000,
    },
    updated_at_unix_ms: 1_700_000_001_000,
    version: 1,
  };
}

function retentionResource() {
  return {
    build_id: FIXTURE_IDS.build,
    deadlines: {
      artifacts_at_unix_ms: 1_700_000_400_000,
      logs_at_unix_ms: 1_700_000_300_000,
      metadata_at_unix_ms: 1_700_000_200_000,
      reports_at_unix_ms: 1_700_000_500_000,
    },
    hold: null,
    visibility: { artifacts: true, logs: true, metadata: true, reports: true },
  };
}

function poolResource() {
  return {
    definition: {
      admission_policy: { mode: 'any' },
      concurrency_limit: 2,
      drain_state: 'accepting',
      enabled: true,
      fairness_policy: 'priority_fifo',
      static_capacity_limit: 2,
    },
    id: FIXTURE_IDS.pool,
    name: 'Accessible Pool',
    published_at_unix_ms: 1_700_000_000_000,
    version: 1,
  };
}

function agentResource() {
  return {
    capacity: {
      logical_cpu_count: 8,
      state_disk_total_bytes: 107_374_182_400,
      total_memory_bytes: 17_179_869_184,
      virtualization_available: true,
      work_disk_total_bytes: 107_374_182_400,
    },
    current_execution: null,
    id: FIXTURE_IDS.agent,
    inventory: {
      agent_version: '0.1.0',
      cache: null,
      coordinator_protocols: [1],
      execution_contract: { min: 1, max: 1 },
      executions: [],
      factory_executions: [],
      host_platform: { architecture: 'arm64', os: 'macos' },
      labels: {},
      octa: {
        build_commit: null,
        event_schemas: [1],
        features: [],
        octafile_versions: [1],
        plugin_protocols: [1],
        plugins: [],
        runner_protocols: [1],
        runner_sha256: '1'.repeat(64),
        version: '0.4.0',
      },
      runtimes: [],
      source_plugins: [],
    },
    last_seen_at_unix_ms: 1_700_000_000_000,
    name: 'Accessible Agent',
    pool_id: FIXTURE_IDS.pool,
    pool_version: 1,
    status: 'online',
    version: 1,
  };
}

function auditFact() {
  return {
    actor: { identity: FIXTURE_IDS.agent, kind: 'agent' },
    id: 'audit-accessible',
    idempotency_key: null,
    metadata: {},
    occurred_at_unix_ms: 1_700_000_000_000,
    operation: 'complete-job',
    outcome: 'accepted',
    request_identity: 'fixture-request',
    target_identity: FIXTURE_IDS.build,
    target_kind: 'build',
  };
}

async function fulfill(route: Route, body: unknown) {
  await route.fulfill({ body: JSON.stringify(body), contentType: 'application/json', status: 200 });
}

async function failUnavailable(route: Route) {
  await route.fulfill({
    body: JSON.stringify({
      code: 'unavailable',
      message: 'The deterministic fixture is unavailable.',
      request_id: 'fixture-refresh-failure',
    }),
    contentType: 'application/json',
    headers: { 'x-request-id': 'fixture-refresh-failure' },
    status: 503,
  });
}
