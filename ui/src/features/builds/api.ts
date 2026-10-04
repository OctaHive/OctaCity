import type { components } from '../../../.generated/api/schema';
import { managementApi, requireManagementResponse } from '../../api/client';

export type AttemptResource = components['schemas']['AttemptResource'];
export type ArtifactDownload = components['schemas']['ArtifactDownload'];
export type ArtifactPage = components['schemas']['ArtifactPage'];
export type BuildLogSearchMode = components['schemas']['BuildLogSearchMode'];
export type BuildLogSearchPage = components['schemas']['BuildLogSearchPage'];
export type BuildLogStream = components['schemas']['BuildLogStream'];
export type BuildResource = components['schemas']['BuildResource'];
export type BuildResultRetentionResource = components['schemas']['BuildResultRetentionResource'];
export type CacheSessionPage = components['schemas']['CacheSessionPage'];
export type DagEdge = components['schemas']['DagEdgeResource'];
export type JobEventPage = components['schemas']['JobEventPage'];
export type JobEventResource = components['schemas']['JobEventResource'];
export type JobResource = components['schemas']['JobResource'];

const JOB_EVENT_PAGE_SIZE = 256;
const BUILD_LOG_SEARCH_PAGE_SIZE = 100;
const BUILD_DIAGNOSTIC_PAGE_SIZE = 100;

export interface JobEventRequest {
  afterSequence: number;
  waitMilliseconds: number;
}

export interface BuildLogSearchFilters {
  attemptId: string | null;
  buildId: string;
  jobId: string | null;
  mode: BuildLogSearchMode;
  query: string;
  stream: BuildLogStream | null;
}

export interface BuildLogSearchApi {
  searchBuildLogs(
    projectId: string,
    filters: BuildLogSearchFilters,
    cursor: string | null,
    signal: AbortSignal,
  ): Promise<BuildLogSearchPage>;
}

export interface BuildResultDiagnosticsApi {
  authorizeArtifactDownload(artifactId: string, signal?: AbortSignal): Promise<ArtifactDownload>;
  getBuildResultRetention(
    buildId: string,
    signal?: AbortSignal,
  ): Promise<BuildResultRetentionResource>;
  listBuildArtifacts(buildId: string, signal?: AbortSignal): Promise<ArtifactPage>;
  listBuildCacheSessions(buildId: string, signal?: AbortSignal): Promise<CacheSessionPage>;
}

/** Detail reads needed by the Build diagnostics route. */
export interface BuildDiagnosticsApi extends BuildLogSearchApi, BuildResultDiagnosticsApi {
  getAttempt(attemptId: string, signal?: AbortSignal): Promise<AttemptResource>;
  getBuild(buildId: string, signal?: AbortSignal): Promise<BuildResource>;
  getJob(jobId: string, signal?: AbortSignal): Promise<JobResource>;
  getJobEvents(jobId: string, request: JobEventRequest, signal: AbortSignal): Promise<JobEventPage>;
}

/** Typed REST boundary for Build and current-Attempt diagnostics. */
export const buildDiagnosticsApi: BuildDiagnosticsApi = {
  async authorizeArtifactDownload(artifactId, signal) {
    const { data } = await managementApi.POST('/api/v1/artifacts/{artifact_id}/download', {
      params: { path: { artifact_id: artifactId } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async getAttempt(attemptId, signal) {
    const { data } = await managementApi.GET('/api/v1/attempts/{attempt_id}', {
      params: { path: { attempt_id: attemptId } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async getBuild(buildId, signal) {
    const { data } = await managementApi.GET('/api/v1/builds/{build_id}', {
      params: { path: { build_id: buildId } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async getJob(jobId, signal) {
    const { data } = await managementApi.GET('/api/v1/jobs/{job_id}', {
      params: { path: { job_id: jobId } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async getJobEvents(jobId, request, signal) {
    const { data } = await managementApi.GET('/api/v1/jobs/{job_id}/events', {
      params: {
        path: { job_id: jobId },
        query: {
          after: request.afterSequence,
          limit: JOB_EVENT_PAGE_SIZE,
          wait_ms: request.waitMilliseconds,
        },
      },
      signal,
    });
    return requireManagementResponse(data);
  },
  async searchBuildLogs(projectId, filters, cursor, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}/build-logs/search', {
      params: {
        path: { project_id: projectId },
        query: {
          query: filters.query,
          mode: filters.mode,
          build_id: filters.buildId,
          ...(filters.attemptId === null ? {} : { attempt_id: filters.attemptId }),
          ...(filters.jobId === null ? {} : { job_id: filters.jobId }),
          ...(filters.stream === null ? {} : { stream: filters.stream }),
          ...(cursor === null ? {} : { after: cursor }),
          limit: BUILD_LOG_SEARCH_PAGE_SIZE,
        },
      },
      signal,
    });
    return requireManagementResponse(data);
  },
  async getBuildResultRetention(buildId, signal) {
    const { data } = await managementApi.GET('/api/v1/builds/{build_id}/retention', {
      params: { path: { build_id: buildId } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async listBuildArtifacts(buildId, signal) {
    const { data } = await managementApi.GET('/api/v1/builds/{build_id}/artifacts', {
      params: {
        path: { build_id: buildId },
        query: { limit: BUILD_DIAGNOSTIC_PAGE_SIZE },
      },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async listBuildCacheSessions(buildId, signal) {
    const { data } = await managementApi.GET('/api/v1/builds/{build_id}/cache-sessions', {
      params: {
        path: { build_id: buildId },
        query: { limit: BUILD_DIAGNOSTIC_PAGE_SIZE },
      },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
};
