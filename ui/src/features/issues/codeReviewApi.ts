import type { components } from '../../../.generated/api/schema';
import type { VersionedMutationHeaders } from '../../shared/confirmedIntent';
import {
  managementApi,
  requireManagementResponse,
  type IdempotencyRequestHeaders,
} from '../../api/client';

const REVIEW_PAGE_LIMIT = 20;
export type IssueCodeReview = components['schemas']['IssueCodeReviewProjection'];
export type ReviewAttachment = components['schemas']['AttachIssueCodeReviewRequest'];

/** Retained reads share the generated management transport; none dispatches a provider. */
export function createIssueCodeReviewsApi(client = managementApi) {
  return {
    async refresh(
      project_id: string,
      issue_id: string,
      review_id: string,
      headers: IdempotencyRequestHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST(
            '/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews/{review_id}/observations',
            {
              params: { path: { project_id, issue_id, review_id }, header: headers },
              body: {},
            },
          )
        ).data,
      );
    },
    async observations(
      project_id: string,
      issue_id: string,
      review_id: string,
      after: string | null,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews/{review_id}/observations',
            {
              params: {
                path: { project_id, issue_id, review_id },
                query: { limit: REVIEW_PAGE_LIMIT, ...(after ? { after } : {}) },
              },
              signal: signal ?? null,
            },
          )
        ).data,
      );
    },
    async attach(
      project_id: string,
      issue_id: string,
      body: ReviewAttachment,
      headers: VersionedMutationHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews', {
            params: { path: { project_id, issue_id }, header: headers },
            body,
          })
        ).data,
      );
    },
    async list(project_id: string, issue_id: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/code-reviews', {
            params: {
              path: { project_id, issue_id },
              query: { limit: REVIEW_PAGE_LIMIT, ...(after ? { after } : {}) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
  };
}
export type IssueCodeReviewsApi = ReturnType<typeof createIssueCodeReviewsApi>;
export const issueCodeReviewsApi = createIssueCodeReviewsApi();

/** Existing connection/resource reads supplied by the composition root, with no Factory dependency. */
export interface IssueReviewConnections {
  getProjectAccess(
    connectionId: string,
    projectId: string,
    signal?: AbortSignal,
  ): Promise<components['schemas']['IntegrationProjectAccessState']>;
  getConnectionVersion(
    connectionId: string,
    version: number,
    signal?: AbortSignal,
  ): Promise<components['schemas']['IntegrationConnection']>;
  listConnections(
    after: string | null,
    signal?: AbortSignal,
  ): Promise<components['schemas']['IntegrationConnectionPage']>;
  listProjectResources(
    connectionId: string,
    projectId: string,
    signal?: AbortSignal,
  ): Promise<components['schemas']['IntegrationProjectResources']>;
}
