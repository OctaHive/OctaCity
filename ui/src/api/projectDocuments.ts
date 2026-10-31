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

/** Confirms the exact server-owned content identity without deriving a new digest in the browser. */
export async function readExactProjectDocument(
  api: ProjectDocumentsApi,
  projectId: string,
  artifact: components['schemas']['FactoryArtifactReference'],
): Promise<string> {
  const result = await api.read(projectId, artifact.artifact_id);
  if (
    result.project_id !== projectId ||
    result.artifact.artifact_id !== artifact.artifact_id ||
    result.artifact.content_digest !== artifact.content_digest ||
    result.artifact.encoded_size !== artifact.encoded_size
  )
    throw new Error('Project document response is invalid');
  return result.content;
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
