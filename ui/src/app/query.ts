import { QueryClient } from '@tanstack/react-query';

export const queryKeys = {
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
