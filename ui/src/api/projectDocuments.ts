import type { components } from '../../.generated/api/schema';
import { managementApi, requireManagementResponse, type IdempotencyRequestHeaders } from './client';

/** Named private Project inputs share server publication and retention with other Project text. */
export interface ProjectDocumentsApi {
  read(
    projectId: string,
    artifactId: string,
    signal?: AbortSignal,
  ): Promise<components['schemas']['ProjectDocumentContent']>;
  list(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['ProjectDocumentPage']>;
  publish(
    projectId: string,
    body: components['schemas']['PublishProjectDocumentRequest'],
    headers: IdempotencyRequestHeaders,
  ): Promise<components['schemas']['ProjectDocumentResource']>;
}
export const projectDocumentsApi: ProjectDocumentsApi = {
  async read(project_id, artifact_id, signal) {
    const { data } = await managementApi.GET(
      '/api/v1/projects/{project_id}/input-documents/{artifact_id}',
      {
        params: { path: { project_id, artifact_id } },
        signal: signal ?? null,
      },
    );
    return requireManagementResponse(data);
  },
  async list(project_id, after, signal) {
    const { data } = await managementApi.GET('/api/v1/projects/{project_id}/input-documents', {
      params: { path: { project_id }, query: { ...(after === null ? {} : { after }) } },
      signal: signal ?? null,
    });
    return requireManagementResponse(data);
  },
  async publish(project_id, body, headers) {
    const { data } = await managementApi.POST('/api/v1/projects/{project_id}/input-documents', {
      params: { path: { project_id }, header: headers },
      body,
    });
    return requireManagementResponse(data);
  },
};
