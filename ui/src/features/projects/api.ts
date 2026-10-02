import type { components } from '../../../.generated/api/schema';
import { ManagementApiError, managementApi } from '../../api/client';

export const PROJECT_PAGE_SIZE = 25;

export type ProjectDetails = components['schemas']['ProjectDetails'];
export type ProjectPage = components['schemas']['ProjectPage'];
export type ProjectResource = components['schemas']['ProjectResource'];

export interface ProjectsApi {
  getProject(projectId: string, signal?: AbortSignal): Promise<ProjectDetails>;
  listProjects(
    parentId: string | null,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<ProjectPage>;
}

/** Existing REST reads used by Project hierarchy screens. */
export const projectsApi: ProjectsApi = {
  async getProject(projectId, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}', {
      params: { path: { project_id: projectId } },
      signal: signal ?? null,
    });
    return requireResponse(data);
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
    return requireResponse(data);
  },
};

function requireResponse<T>(data: T | undefined): T {
  if (data === undefined) {
    throw new ManagementApiError({
      code: 'invalid_response',
      message: 'The management API returned an invalid success response.',
      requestId: null,
      retryAfterMilliseconds: null,
      status: null,
    });
  }
  return data;
}
