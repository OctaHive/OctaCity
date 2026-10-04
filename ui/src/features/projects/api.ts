import type { components } from '../../../.generated/api/schema';
import { managementApi, requireManagementResponse } from '../../api/client';

export const PROJECT_PAGE_SIZE = 25;

export type ProjectDetails = components['schemas']['ProjectDetails'];
export type ProjectPage = components['schemas']['ProjectPage'];
export type ProjectResource = components['schemas']['ProjectResource'];
export type BuildState = components['schemas']['BuildState'];
export type BuildSummaryPage = components['schemas']['BuildSummaryPage'];
export type BuildSummary = components['schemas']['BuildSummaryResource'];
export type BuildConfigurationResource = components['schemas']['BuildConfigurationResource'];
export type BuildConfigurationSummaryPage = components['schemas']['BuildConfigurationSummaryPage'];
export type BuildConfigurationSummary = components['schemas']['BuildConfigurationSummaryResource'];
export type PipelineResource = components['schemas']['PipelineResource'];
export type PipelineSummaryPage = components['schemas']['PipelineSummaryPage'];
export type PipelineSummary = components['schemas']['PipelineSummaryResource'];
export type RepositoryResource = components['schemas']['RepositoryResource'];
export type RepositorySummaryPage = components['schemas']['RepositorySummaryPage'];
export type RepositorySummary = components['schemas']['RepositorySummaryResource'];
export type TriggerDefinitionSummaryPage = components['schemas']['TriggerDefinitionSummaryPage'];
export type TriggerDefinitionSummary = components['schemas']['TriggerDefinitionSummaryResource'];

export interface BuildFilters {
  configurationId: string | null;
  state: BuildState | null;
}

export type TriggerDefinitionDetails =
  | {
      kind: 'internal';
      resource: components['schemas']['InternalTriggerResource'];
    }
  | {
      kind: 'manual';
      resource: components['schemas']['ManualTriggerDefinitionResource'];
    }
  | {
      kind: 'scheduled';
      resource: components['schemas']['ScheduleResource'];
    };

/** Project hierarchy reads consumed by Project navigation. */
export interface ProjectHierarchyApi {
  getProject(projectId: string, signal?: AbortSignal): Promise<ProjectDetails>;
  listProjects(
    parentId: string | null,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<ProjectPage>;
}

/** Current-definition reads consumed by the Project workspace. */
export interface ProjectDefinitionsApi {
  getBuildConfiguration(
    configurationId: string,
    version: number,
    signal?: AbortSignal,
  ): Promise<BuildConfigurationResource>;
  getPipeline(pipelineId: string, version: number, signal?: AbortSignal): Promise<PipelineResource>;
  getRepository(
    repositoryId: string,
    version: number,
    signal?: AbortSignal,
  ): Promise<RepositoryResource>;
  getTrigger(
    summary: TriggerDefinitionSummary,
    signal?: AbortSignal,
  ): Promise<TriggerDefinitionDetails>;
  listBuildConfigurations(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<BuildConfigurationSummaryPage>;
  listPipelines(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<PipelineSummaryPage>;
  listRepositories(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<RepositorySummaryPage>;
  listTriggers(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<TriggerDefinitionSummaryPage>;
}

/** Recent-Build reads consumed by the Project workspace. */
export interface ProjectBuildsApi {
  listBuildConfigurations(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<BuildConfigurationSummaryPage>;
  listBuilds(
    projectId: string,
    filters: BuildFilters,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<BuildSummaryPage>;
}

/** Composed Project REST boundary supplied by the application route. */
export type ProjectsApi = ProjectHierarchyApi & ProjectDefinitionsApi & ProjectBuildsApi;

/** Typed REST reads used by Project hierarchy, definitions, and recent Builds. */
export const projectsApi: ProjectsApi = {
  async getBuildConfiguration(configurationId, version, signal) {
    const { data } = await managementApi.GET(
      '/api/v1/build-configurations/{configuration_id}/versions/{version}',
      {
        params: { path: { configuration_id: configurationId, version } },
        signal: signal ?? null,
      },
    );
    return requireManagementResponse(data);
  },
  async getPipeline(pipelineId, version, signal) {
    const { data } = await managementApi.GET('/api/v1/pipelines/{pipeline_id}/versions/{version}', {
      params: { path: { pipeline_id: pipelineId, version } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async getProject(projectId, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}', {
      params: { path: { project_id: projectId } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async getRepository(repositoryId, version, signal) {
    const { data } = await managementApi.GET(
      '/api/v1/repositories/{repository_id}/versions/{version}',
      {
        params: { path: { repository_id: repositoryId, version } },
        signal: signal ?? null,
      },
    );
    return requireManagementResponse(data);
  },
  async getTrigger(summary, signal) {
    const { id: triggerId, kind, version } = summary;
    if (kind === 'manual') {
      const { data } = await managementApi.GET(
        '/api/v1/trigger-definitions/manual/{trigger_id}/versions/{version}',
        {
          params: { path: { trigger_id: triggerId, version } },
          signal: signal ?? null,
        },
      );
      return { kind, resource: requireManagementResponse(data) };
    }
    if (kind === 'scheduled') {
      const { data } = await managementApi.GET(
        '/api/v1/schedules/{trigger_id}/versions/{version}',
        {
          params: { path: { trigger_id: triggerId, version } },
          signal: signal ?? null,
        },
      );
      return { kind, resource: requireManagementResponse(data) };
    }
    const { data } = await managementApi.GET(
      '/api/v1/trigger-definitions/internal/{trigger_id}/versions/{version}',
      {
        params: { path: { trigger_id: triggerId, version } },
        signal: signal ?? null,
      },
    );
    return { kind, resource: requireManagementResponse(data) };
  },
  async listBuildConfigurations(projectId, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}/build-configurations', {
      params: { path: { project_id: projectId }, query: pageQuery(cursor) },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async listBuilds(projectId, filters, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}/builds', {
      params: {
        path: { project_id: projectId },
        query: {
          ...pageQuery(cursor),
          ...(filters.configurationId === null
            ? {}
            : { configuration_id: filters.configurationId }),
          ...(filters.state === null ? {} : { state: filters.state }),
        },
      },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async listPipelines(projectId, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}/pipelines', {
      params: { path: { project_id: projectId }, query: pageQuery(cursor) },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async listProjects(parentId, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/projects', {
      params: {
        query: {
          ...(cursor === null ? {} : { after: cursor }),
          limit: PROJECT_PAGE_SIZE,
          ...(parentId === null ? {} : { parent_id: parentId }),
        },
      },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async listRepositories(projectId, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}/repositories', {
      params: { path: { project_id: projectId }, query: pageQuery(cursor) },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async listTriggers(projectId, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}/trigger-definitions', {
      params: { path: { project_id: projectId }, query: pageQuery(cursor) },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
};

function pageQuery(cursor: string | null) {
  return {
    ...(cursor === null ? {} : { after: cursor }),
    limit: PROJECT_PAGE_SIZE,
  };
}
