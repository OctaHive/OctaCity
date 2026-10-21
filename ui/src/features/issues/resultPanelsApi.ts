import type { components } from '../../../.generated/api/schema';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { managementApi, requireManagementResponse } from '../../api/client';
export type IssueArtifact = components['schemas']['IssueArtifactProjection'];
export type IssueChecksPage = components['schemas']['IssueChecksPage'];
/** Exact retained reads only; this client owns no provider or execution commands. */
export function createIssueResultPanelsApi(client = managementApi) {
  return {
    async externalChecks(
      project_id: string,
      issue_id: string,
      review_id: string,
      after: string | null,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews/{review_id}/checks',
            {
              params: {
                path: { project_id, issue_id, review_id },
                query: { ...(after === null ? {} : { after }) },
              },
              cache: 'no-store',
              signal: signal ?? null,
            },
          )
        ).data,
      );
    },
    async content(
      project_id: string,
      issue_id: string,
      work_id: string,
      artifact_id: string,
      mode: 'preview' | 'download',
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/issues/{issue_id}/results/artifacts/{artifact_id}/content',
            {
              params: { path: { project_id, issue_id, artifact_id }, query: { work_id, mode } },
              parseAs: 'blob',
              cache: 'no-store',
              signal: signal ?? null,
            },
          )
        ).data,
      );
    },
    async artifacts(
      project_id: string,
      issue_id: string,
      work_id: string,
      after: string | null,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/results/artifacts', {
            params: {
              path: { project_id, issue_id },
              query: {
                work_id,
                limit: bounds.defaultPageSize,
                ...(after === null ? {} : { after }),
              },
            },
            cache: 'no-store',
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async checks(
      project_id: string,
      issue_id: string,
      work_id: string,
      record: string,
      after: string | null,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/results/checks', {
            params: {
              path: { project_id, issue_id },
              query: {
                work_id,
                record,
                limit: bounds.defaultPageSize,
                ...(after === null ? {} : { after }),
              },
            },
            cache: 'no-store',
            signal: signal ?? null,
          })
        ).data,
      );
    },
  };
}
export type IssueResultPanelsApi = ReturnType<typeof createIssueResultPanelsApi>;
export const issueResultPanelsApi = createIssueResultPanelsApi();
