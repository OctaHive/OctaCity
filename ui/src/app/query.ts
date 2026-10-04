import { QueryClient } from '@tanstack/react-query';

export const queryKeys = {
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
  pipeline: (pipelineId: string, version: number) => ['pipelines', pipelineId, version] as const,
  project: (projectId: string) => ['projects', 'detail', projectId] as const,
  projectBuildConfigurations: (projectId: string) =>
    ['projects', projectId, 'build-configurations'] as const,
  projectBuilds: (projectId: string, configurationId: string | null, state: string | null) =>
    ['projects', projectId, 'builds', { configurationId, state }] as const,
  projectChildren: (parentId: string | null) => ['projects', 'children', parentId] as const,
  projectPipelines: (projectId: string) => ['projects', projectId, 'pipelines'] as const,
  projectRepositories: (projectId: string) => ['projects', projectId, 'repositories'] as const,
  projectTriggers: (projectId: string) => ['projects', projectId, 'triggers'] as const,
  readiness: ['system', 'readiness'] as const,
  repository: (repositoryId: string, version: number) =>
    ['repositories', repositoryId, version] as const,
  trigger: (kind: string, triggerId: string, version: number) =>
    ['triggers', kind, triggerId, version] as const,
};

export const READINESS_REFRESH_MILLISECONDS = 10_000;

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
