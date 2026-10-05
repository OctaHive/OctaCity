import { QueryClient } from '@tanstack/react-query';

export const queryKeys = {
  agent: (agentId: string) => ['agents', agentId] as const,
  agentPool: (poolId: string) => ['agent-pools', poolId] as const,
  agentPoolAgents: (poolId: string) => ['agent-pools', poolId, 'agents'] as const,
  agentPools: ['agent-pools'] as const,
  audit: ['audit'] as const,
  auditFacts: (filters: object) => ['audit', 'facts', filters] as const,
  attempt: (attemptId: string) => ['attempts', attemptId] as const,
  buildArtifacts: (buildId: string) => ['builds', buildId, 'artifacts'] as const,
  buildCacheSessions: (buildId: string) => ['builds', buildId, 'cache-sessions'] as const,
  buildLogs: (
    projectId: string,
    buildId: string,
    attemptId: string | null,
    jobId: string | null,
    mode: string,
    query: string,
    stream: string | null,
  ) =>
    [
      'projects',
      projectId,
      'build-logs',
      { attemptId, buildId, jobId, mode, query, stream },
    ] as const,
  build: (buildId: string) => ['builds', buildId] as const,
  buildResultRetention: (buildId: string) => ['builds', buildId, 'retention'] as const,
  buildConfiguration: (configurationId: string, version: number) =>
    ['build-configurations', configurationId, version] as const,
  disabledExplorerDetail: (kind: 'build' | 'build-configuration' | 'project') =>
    ['explorer', 'disabled-detail', kind] as const,
  pipeline: (pipelineId: string, version: number) => ['pipelines', pipelineId, version] as const,
  project: (projectId: string) => ['projects', 'detail', projectId] as const,
  projectBuildConfigurations: (projectId: string) =>
    ['projects', projectId, 'build-configurations'] as const,
  projectBuildPages: (projectId: string) => ['projects', projectId, 'builds'] as const,
  projectBuilds: (projectId: string, configurationId: string | null, state: string | null) =>
    ['projects', projectId, 'builds', { configurationId, state }] as const,
  projectChildren: (parentId: string | null) => ['projects', 'children', parentId] as const,
  projectPipelines: (projectId: string) => ['projects', projectId, 'pipelines'] as const,
  projectRepositories: (projectId: string) => ['projects', projectId, 'repositories'] as const,
  projectTriggers: (projectId: string) => ['projects', projectId, 'triggers'] as const,
  disabledDefinitionDetail: (collectionKey: readonly unknown[]) =>
    [...collectionKey, 'detail', null] as const,
  readiness: ['system', 'readiness'] as const,
  operatorAttention: (
    buildIds: readonly string[],
    agentIds: readonly string[],
    poolIds: readonly string[],
  ) => ['operator-attention', [...buildIds], [...agentIds], [...poolIds]] as const,
  repository: (repositoryId: string, version: number) =>
    ['repositories', repositoryId, version] as const,
  resourceSearchRoot: ['resource-search'] as const,
  resourceSearch: (query: string, kinds: readonly string[]) =>
    ['resource-search', query, [...kinds]] as const,
  trigger: (kind: string, triggerId: string, version: number) =>
    ['triggers', kind, triggerId, version] as const,
};

/** Shared invalidation targets for cross-cutting server projections. */
export const queryInvalidations = {
  audit: { queryKey: queryKeys.audit },
} as const;

export const READINESS_REFRESH_MILLISECONDS = 10_000;
export const OPERATOR_ATTENTION_REFRESH_MILLISECONDS = 15_000;
export const RESOURCE_SEARCH_DEBOUNCE_MILLISECONDS = 250;

/** Creates an in-memory query client with conservative automatic retry behavior. */
export function createConsoleQueryClient() {
  return new QueryClient({
    defaultOptions: {
      mutations: { retry: false },
      queries: {
        refetchOnWindowFocus: false,
        retry: false,
        staleTime: 0,
      },
    },
  });
}
