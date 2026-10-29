import { QueryClient } from '@tanstack/react-query';

export const queryKeys = {
  projectInputDocuments: (projectId: string) => ['project-input-documents', projectId] as const,
  factoryRuntimeChoices: (project: string) => ['factory', 'runtime-choices', project] as const,
  factoryRuntime: (project: string, alias: string) =>
    ['factory', 'runtime', project, alias] as const,
  factorySkill: (
    project: string,
    reference: { identity: string; version: string; digest: string },
    path: string | null,
  ) =>
    [
      'factory',
      'skill',
      project,
      reference.identity,
      reference.version,
      reference.digest,
      path,
    ] as const,
  factoryRoleCatalog: (project: string, kind: string, owner = '') =>
    ['factory', 'roles', project, kind, owner] as const,
  factoryRole: (reference: { project_id: string; id: string; version: number }) =>
    ['factory', 'role', reference.project_id, reference.id, reference.version] as const,
  factoryRoleProfile: (reference: { project_id: string; id: string; version: number }) =>
    ['factory', 'profile', reference.project_id, reference.id, reference.version] as const,
  issueLegacyHistory: (project: string, issue: string, after: string | null) =>
    ['issues', project, issue, 'legacy-work', after] as const,
  issueLegacyWork: (project: string, issue: string, work: string | null) =>
    ['issues', project, issue, 'work', work] as const,
  factoryActionsRoot: ['factory-actions'] as const,
  factoryActions: (project: string | null, issue: string | null, after: string | null) =>
    ['factory-actions', project, issue, after] as const,
  // Without scope this is the invalidation prefix for every observation of the exact target.
  factoryAction: (
    run: string | null,
    attempt: string | null,
    scope?: { projectId: string | null; issueId: string | null },
  ) => ['factory-action', run, attempt, ...(scope ? [scope] : [])] as const,
  issueExternalChecks: (project: string, issue: string, review: string, after: string | null) =>
    ['issues', project, issue, 'external-checks', review, after] as const,
  issueArtifactContent: (
    project: string,
    issue: string,
    work: string,
    artifact: string,
    mode: string,
  ) => ['issues', project, issue, 'artifact-content', work, artifact, mode] as const,
  issueArtifacts: (project: string, issue: string, work: string, after: string | null) =>
    ['issues', project, issue, 'artifacts', work, after] as const,
  issueChecks: (
    project: string,
    issue: string,
    work: string,
    record: string | null,
    after: string | null,
  ) => ['issues', project, issue, 'checks', work, record, after] as const,
  issueReviewResources: (
    connection: string,
    project: string,
    version: number | undefined,
    accessVersion: number | undefined,
  ) => ['integrations', 'resources', connection, project, version, accessVersion] as const,
  integrationVersion: (id: string, version: number) =>
    ['integrations', id, 'version', version] as const,
  issueCodeReviewsRoot: (project: string, issue: string) =>
    ['issues', project, issue, 'code-reviews'] as const,
  issueCodeReviewHistory: (project: string, issue: string, review: string, after: string | null) =>
    ['issues', project, issue, 'code-reviews', 'history', review, after] as const,
  issueCodeReviews: (project: string, issue: string, after: string | null) =>
    ['issues', project, issue, 'code-reviews', after] as const,
  issueJournal: (
    project: string,
    issue: string,
    request: string,
    collection: string,
    after: string | null,
  ) => ['issues', project, issue, 'journal', request, collection, after] as const,
  issueJournalEvents: (
    project: string,
    issue: string,
    request: string,
    build: string,
    job: string,
    generation: number,
  ) => ['issues', project, issue, 'journal-events', request, build, job, generation] as const,
  issueJournalEvent: (
    project: string,
    issue: string,
    request: string,
    build: string,
    job: string,
    sequence: number,
  ) => ['issues', project, issue, 'journal-event', request, build, job, sequence] as const,
  issueResults: (
    project: string,
    id: string,
    work: string,
    collection: string,
    after: string | null,
  ) => ['issues', project, id, 'results', work, collection, after] as const,
  issueTimelineRoot: (project: string, id: string) => ['issues', project, id, 'timeline'] as const,
  issueTimeline: (project: string, id: string, after: string | null) =>
    ['issues', project, id, 'timeline', after] as const,
  issueComment: (project: string, id: string, comment: string) =>
    ['issues', project, id, 'comment', comment] as const,
  issueProjections: (project: string, id: string, after: string | null) =>
    ['issues', project, id, 'projections', after] as const,
  issueRoot: (project: string, id: string) => ['issues', project, id] as const,
  issueExecutionsRoot: (project: string, id: string) =>
    ['issues', project, id, 'executions'] as const,
  issueExecutions: (project: string, id: string, after: string | null) =>
    ['issues', project, id, 'executions', 'history', after] as const,
  issueExecution: (project: string, id: string, request: string) =>
    ['issues', project, id, 'executions', 'request', request] as const,
  issuePropertyDefinitions: (project: string, version: number) =>
    ['issues', project, 'property-definitions', version] as const,
  issueChildren: (project: string, id: string, after: string | null) =>
    ['issues', project, id, 'children', after] as const,
  issueChildrenRoot: (project: string, id: string) => ['issues', project, id, 'children'] as const,
  issuePropertyChoices: (project: string) => ['issues', project, 'property-choices'] as const,
  issues: (project: string) => ['issues', project] as const,
  issueImportConnections: ['issue-import', 'connections'] as const,
  issueImports: (project: string) => ['issue-import', project] as const,
  issueImportChoices: (project: string, connection: string, version: number) =>
    ['issue-import', project, 'choices', connection, version] as const,
  issueList: (project: string, after: string | null) => ['issues', project, 'list', after] as const,
  issue: (project: string, id: string) => ['issues', project, id, 'current'] as const,
  issueSynchronization: (project: string, id: string) => ['issues', project, id, 'source'] as const,
  issueSourceChoices: (project: string, id: string) =>
    ['issues', project, id, 'source', 'choices'] as const,
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
  factoryPresets: (projectId: string) => ['factory', 'presets', projectId] as const,
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
