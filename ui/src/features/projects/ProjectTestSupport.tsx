import { render } from '@testing-library/react';
import { createMemoryRouter } from 'react-router-dom';
import { vi } from 'vitest';

import { App } from '../../App';
import { ManagementApiError } from '../../api/client';
import { createConsoleQueryClient } from '../../app/query';
import { createConsoleRoutes } from '../../app/router';
import type {
  BuildConfigurationResource,
  BuildConfigurationSummary,
  BuildState,
  BuildSummary,
  PipelineResource,
  PipelineSummary,
  ProjectPage,
  ProjectSummary,
  ProjectsApi,
  RepositoryResource,
  RepositorySummary,
  TriggerDefinitionSummary,
} from './api';

export const CONFIGURATION_A = '11111111-1111-4111-8111-111111111111';
export const BUILD_A = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaa1';
export const BUILD_B = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbb2';
export const BUILD_C = 'cccccccc-cccc-4ccc-8ccc-ccccccccccc3';

export function renderProjects(path: string, api: ProjectsApi) {
  const queryClient = createConsoleQueryClient();
  const router = createMemoryRouter(
    createConsoleRoutes({ projectsApi: api, readinessProbe: async () => 'ready' }),
    {
      initialEntries: [path],
    },
  );
  return { ...render(<App queryClient={queryClient} router={router} />), queryClient, router };
}

export function fakeProjectsApi() {
  return {
    getBuildConfiguration: vi.fn<ProjectsApi['getBuildConfiguration']>(),
    getPipeline: vi.fn<ProjectsApi['getPipeline']>(),
    getProject: vi.fn<ProjectsApi['getProject']>(),
    getRepository: vi
      .fn<ProjectsApi['getRepository']>()
      .mockResolvedValue(repositoryDetails('repository-a', 'Application Source')),
    getTrigger: vi.fn<ProjectsApi['getTrigger']>(),
    listBuildConfigurations: vi
      .fn<ProjectsApi['listBuildConfigurations']>()
      .mockResolvedValue(definitionPage([], null)),
    listBuilds: vi.fn<ProjectsApi['listBuilds']>().mockResolvedValue(definitionPage([], null)),
    listPipelines: vi
      .fn<ProjectsApi['listPipelines']>()
      .mockResolvedValue(definitionPage([], null)),
    listProjects: vi.fn<ProjectsApi['listProjects']>().mockResolvedValue(page([], null)),
    listRepositories: vi
      .fn<ProjectsApi['listRepositories']>()
      .mockResolvedValue(definitionPage([], null)),
    listTriggers: vi.fn<ProjectsApi['listTriggers']>().mockResolvedValue(definitionPage([], null)),
    triggerBuild: vi.fn<ProjectsApi['triggerBuild']>(),
  };
}

export function page(items: ProjectSummary[], nextCursor: string | null): ProjectPage {
  return { items, next_cursor: nextCursor };
}

export function project(
  id: string,
  name: string,
  parentId: string | null = null,
  hasChildren = false,
): ProjectSummary {
  return {
    created_at_unix_ms: 1_700_000_000_000,
    has_children: hasChildren,
    id,
    name,
    parent_id: parentId,
    updated_at_unix_ms: 1_700_000_000_000,
    version: 1,
  };
}

export function definition(id: string, name: string): PipelineSummary & RepositorySummary {
  return {
    id,
    name,
    project_id: 'delivery',
    published_at_unix_ms: 1_700_000_000_000,
    version: 3,
  };
}

export function trigger(
  id: string,
  kind: TriggerDefinitionSummary['kind'],
): TriggerDefinitionSummary {
  return {
    configuration_id: 'configuration-a',
    configuration_version: 3,
    enabled: true,
    id,
    kind,
    project_id: 'delivery',
    published_at_unix_ms: 1_700_000_000_000,
    version: 3,
  };
}

export function configuration(id: string, name: string): BuildConfigurationSummary {
  return { ...definition(id, name), enabled: true };
}

export function configurationDetails(
  id: string,
  name: string,
  parameters: BuildConfigurationResource['definition']['parameters'] = {
    deny_unknown: true,
    parameters: {},
  },
): BuildConfigurationResource {
  return {
    definition: {
      agent_requirements: {
        capabilities: [],
        labels: {},
        minimum_cpu_millis: 0,
        minimum_disk_bytes: 0,
        minimum_memory_bytes: 0,
      },
      allowed_pools: [],
      artifacts: {
        artifact_bytes: 0,
        artifact_count: 0,
        report_bytes: 0,
        report_count: 0,
        single_output_bytes: 0,
      },
      cache: { namespace: null, read: false, write: false },
      enabled: true,
      job_concurrency_limit: 1,
      parameters,
      pipeline_id: 'pipeline-a',
      pipeline_version: 3,
      repository_id: 'repository-a',
      repository_version: 3,
      retry: { max_attempts: 1, retry_on: [] },
      runtime: {
        architecture: 'arm64',
        class: 'virtualization',
        cpu_millis: 1_000,
        immutable_image: null,
        memory_bytes: 1_073_741_824,
        network: { mode: 'disabled' },
        operating_system: 'linux',
        timeout_seconds: 600,
        workload_identity_profile: null,
        writable_disk_bytes: 1_073_741_824,
      },
      triggers: ['manual'],
    },
    id,
    name,
    project_id: 'delivery',
    published_at_unix_ms: 1_700_000_000_000,
    version: 3,
  };
}

export function definitionPage<T>(items: T[], nextCursor: string | null) {
  return { items, next_cursor: nextCursor };
}

export function pipelineDetails(id: string, name: string): PipelineResource {
  return {
    dag: { edges: [], nodes: [] },
    id,
    name,
    project_id: 'delivery',
    published_at_unix_ms: 1_700_000_000_000,
    version: 3,
  };
}

export function repositoryDetails(
  id: string,
  name: string,
  selection: RepositoryResource['definition']['selection'] = {
    allow_exact_revision: true,
    allowed_references: ['refs/heads/main'],
    default_reference: 'refs/heads/main',
  },
): RepositoryResource {
  return {
    definition: {
      repository_locator: 'https://example.test/source.git',
      selection,
      vcs_integration_id: 'integration-a',
    },
    id,
    name,
    project_id: 'delivery',
    published_at_unix_ms: 1_700_000_000_000,
    version: 3,
  };
}

export function build(
  id: string,
  configurationId: string,
  state: BuildState,
  createdAt: number,
): BuildSummary {
  return {
    cause: { kind: 'manual' },
    configuration_id: configurationId,
    configuration_version: 3,
    created_at_unix_ms: createdAt,
    current_attempt_id: 'dddddddd-dddd-4ddd-8ddd-dddddddddddd',
    current_attempt_number: 1,
    current_attempt_state: state === 'queued' ? 'created' : state,
    id,
    project_id: 'delivery',
    state,
    terminal_at_unix_ms: state === 'running' || state === 'queued' ? null : createdAt + 1_000,
  };
}

export function managementError(
  code: 'not_found' | 'unavailable',
  requestId: string,
): ManagementApiError {
  return new ManagementApiError({
    code,
    message: 'Safe management failure.',
    requestId,
    retryAfterMilliseconds: null,
    status: code === 'not_found' ? 404 : 503,
  });
}
