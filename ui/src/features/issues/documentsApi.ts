import type { VersionedMutationHeaders } from '../../shared/confirmedIntent';
import type { components } from '../../../.generated/api/schema';
import {
  managementApi,
  requireManagementResponse,
  type IdempotencyRequestHeaders,
} from '../../api/client';
export type IssueMaterialReference = components['schemas']['IssueMaterialReference'];
export type IssueDocument = components['schemas']['IssueDocumentResource'];
export type IssueDocumentBody = components['schemas']['IssueDocumentContent'];
/** Issue-owned publications use their own routes, never the shared Project catalog. */
export function createIssueDocumentsApi(client = managementApi) {
  return {
    async editMaterials(
      project_id: string,
      issue_id: string,
      body: components['schemas']['EditIssueMaterialsRequest'],
      headers: VersionedMutationHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.PATCH('/api/v1/projects/{project_id}/issues/{issue_id}/materials', {
            params: { path: { project_id, issue_id }, header: headers },
            body,
          })
        ).data,
      );
    },
    async publish(
      project_id: string,
      issue_id: string,
      body: components['schemas']['PublishIssueDocumentRequest'],
      headers: IdempotencyRequestHeaders,
    ) {
      const result = requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/issues/{issue_id}/input-documents', {
            params: { path: { project_id, issue_id }, header: headers },
            body,
          })
        ).data,
      );
      if (result.project_id !== project_id || result.issue_id !== issue_id)
        throw new Error('Issue document response is invalid');
      return result;
    },
    async list(project_id: string, issue_id: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/input-documents', {
            params: {
              path: { project_id, issue_id },
              query: { ...(after === null ? {} : { after }) },
            },
            cache: 'no-store',
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async read(project_id: string, issue_id: string, artifact_id: string) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/issues/{issue_id}/input-documents/{artifact_id}',
            {
              params: { path: { project_id, issue_id, artifact_id } },
              cache: 'no-store',
            },
          )
        ).data,
      );
    },
  };
}
export type IssueDocumentsApi = ReturnType<typeof createIssueDocumentsApi>;
export const issueDocumentsApi = createIssueDocumentsApi();
/** A preview must match the exact owner and publication selected before the read. */
export async function readExactIssueDocument(
  api: Pick<IssueDocumentsApi, 'read'>,
  document: Pick<IssueDocument, 'project_id' | 'issue_id' | 'artifact'>,
): Promise<IssueDocumentBody> {
  const result = await api.read(
    document.project_id,
    document.issue_id,
    document.artifact.artifact_id,
  );
  if (
    result.document.project_id !== document.project_id ||
    result.document.issue_id !== document.issue_id ||
    result.document.artifact.artifact_id !== document.artifact.artifact_id ||
    result.document.artifact.content_digest !== document.artifact.content_digest ||
    result.document.artifact.encoded_size !== document.artifact.encoded_size
  )
    throw new Error('Issue document response is invalid');
  return result;
}
