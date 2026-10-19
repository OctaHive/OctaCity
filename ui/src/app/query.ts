import { QueryClient } from '@tanstack/react-query';

export const queryKeys = {
  issues: (project: string) => ['issues', project] as const,
  issueList: (project: string, after: string | null) => ['issues', project, 'list', after] as const,
  issue: (project: string, id: string) => ['issues', project, id, 'current'] as const,
  issueTransitions: (project: string, id: string) =>
    ['issues', project, id, 'transitions'] as const,
  issueHistoryRoot: (project: string, id: string) => ['issues', project, id, 'history'] as const,
  issueHistory: (project: string, id: string, after: number | null) =>
    ['issues', project, id, 'history', after] as const,
  issueRevisionsRoot: (project: string, id: string) =>
    ['issues', project, id, 'revisions'] as const,
  issueRevisions: (project: string, id: string, after: number | null) =>
    ['issues', project, id, 'revisions', after] as const,
  issueRevision: (project: string, id: string, version: number | null) =>
    ['issues', project, id, 'revision', version] as const,
  issueWorkflowPage: (project: string, after: string | null) =>
    ['issue-workflows', project, 'list', after] as const,
  issueWorkflow: (project: string, id: string | null, version?: number) =>
    ['issue-workflows', project, id, version ?? 'current'] as const,
  issueWorkflowDefault: (project: string) => ['issue-workflows', project, 'default'] as const,
  issueWorkflows: (project: string) => ['issue-workflows', project, 'list'] as const,
  factorySourceProgress: (project: string, kind: string, id: string, version: number) =>
    ['factory', 'source-progress', project, kind, id, version] as const,
  integrationProjectAccess: (id: string, project: string) =>
    ['integrations', 'access', id, project] as const,
  integrationProjectResources: (id: string, project: string) =>
    ['integrations', 'resources', id, project] as const,
  integrationDependencies: (id: string, project: string) =>
    ['integrations', 'dependencies', id, project] as const,
  integrationInputHistory: (project: string, id: string, version: number) =>
    ['integrations', 'history', project, id, version] as const,
  factorySourceChoices: (project: string, connection: string, version: number) =>
    ['factory', 'source-choices', project, connection, version] as const,
  factorySourceConnections: ['factory', 'source-connections'] as const,
  factorySourceInputs: (project: string) => ['factory', 'source-inputs', project] as const,
  integrations: ['integrations'] as const,
  integration: (id: string) => ['integrations', 'detail', id] as const,
  integrationState: (id: string) => ['integrations', 'state', id] as const,
  integrationList: ['integrations', 'list'] as const,
  integrationInstallations: ['integrations', 'installations'] as const,
  factoryDiagnosticRoot: (runId: string) => ['factory', 'diagnostics', runId] as const,
  factoryPoolRoot: (projectId: string) => ['factory', 'pool', projectId] as const,
  factoryDiagnostics: (runId: string, collection: string, initialCursor: string | null = null) =>
    [
      'factory',
      'diagnostics',
      runId,
      collection,
      ...(initialCursor === null ? [] : [initialCursor]),
    ] as const,
  factoryWorkSources: (projectId: string, kind: 'repositories' | 'builds') =>
    ['factory', 'work-sources', projectId, kind] as const,
  factoryCurrent: (kind: string, id: string) => ['factory', 'current', kind, id] as const,
  factoryFlowVersions: (id: string) => ['factory', 'flow', id] as const,
  factoryConfigurationVersions: (id: string) => ['factory', 'configuration', id] as const,
  factoryChoices: (projectId: string) => ['factory', 'choices', projectId] as const,
  factoryConfigurations: (projectId: string) => ['factory', 'configurations', projectId] as const,
  factoryFlows: (projectId: string) => ['factory', 'flows', projectId] as const,
  factoryFlow: (id: string, version: number) => ['factory', 'flow', id, version] as const,
  factoryRun: (id: string | null) => ['factory', 'run', id] as const,
  factoryPool: (projectId: string, digest: string) =>
    ['factory', 'pool', projectId, digest] as const,
  factoryConfiguration: (id: string | null, version: number | null) =>
    ['factory', 'configuration', id, version] as const,
  factoryRuns: (projectId: string | null, filters: object) =>
    ['factory', 'runs', projectId, filters] as const,
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
  disabledExplorerDetail: (
    kind: 'agent' | 'agent-pool' | 'build' | 'build-configuration' | 'project',
  ) => ['explorer', 'disabled-detail', kind] as const,
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
