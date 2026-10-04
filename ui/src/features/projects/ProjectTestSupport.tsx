import { render } from '@testing-library/react';
import { createMemoryRouter } from 'react-router-dom';
import { vi } from 'vitest';

import { App } from '../../App';
import { ManagementApiError } from '../../api/client';
import { createConsoleQueryClient } from '../../app/query';
import { createConsoleRoutes } from '../../app/router';
import type {
  BuildConfigurationSummary,
  BuildState,
  BuildSummary,
  PipelineResource,
  PipelineSummary,
  ProjectPage,
  ProjectSummary,
  ProjectsApi,
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
    createConsoleRoutes(async () => 'ready', api),
    {
      initialEntries: [path],
    },
  );
  return { ...render(<App queryClient={queryClient} router={router} />), router };
}

export function fakeProjectsApi() {
  return {
    getBuildConfiguration: vi.fn<ProjectsApi['getBuildConfiguration']>(),
    getPipeline: vi.fn<ProjectsApi['getPipeline']>(),
    getProject: vi.fn<ProjectsApi['getProject']>(),
    getRepository: vi.fn<ProjectsApi['getRepository']>(),
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
    configuration_version: 2,
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
