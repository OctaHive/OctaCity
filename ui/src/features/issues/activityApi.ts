import type { components, operations } from '../../../.generated/api/schema';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { managementApi, requireManagementResponse } from '../../api/client';
import type { ConfirmedMutationAttempt } from '../../shared/confirmedIntent';
export type IssueTimelinePage = components['schemas']['IssueTimelinePage'];
export type IssueTimelineEntry = components['schemas']['IssueTimelineReference'];
export type IssueComment = components['schemas']['IssueCommentMetadata'];
export type IssueCommentRequest = components['schemas']['AppendIssueCommentRequest'];
export type IssueResultPage = components['schemas']['IssueResultPage'];
export type IssueResultCollection =
  operations['listIssueResults']['parameters']['query']['collection'];
/** Existing authorized Run summary port supplied by application composition. */
export interface IssueRunReader {
  getRun(
    runId: string,
    signal?: AbortSignal,
  ): Promise<components['schemas']['FactoryRunSummaryResource']>;
}
/** Bounded metadata and independently authorized discussion bodies; never a provider/control client. */
export function createIssueActivityApi(client = managementApi) {
  return {
    async timeline(
      project_id: string,
      issue_id: string,
      after: string | null,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/timeline', {
            params: {
              path: { project_id, issue_id },
              query: { limit: bounds.defaultPageSize, ...(after === null ? {} : { after }) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async results(
      project_id: string,
      issue_id: string,
      work_id: string,
      collection: IssueResultCollection,
      after: string | null,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/results', {
            params: {
              path: { project_id, issue_id },
              query: {
                work_id,
                collection,
                limit: bounds.defaultPageSize,
                ...(after === null ? {} : { after }),
              },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async append(
      project_id: string,
      issue_id: string,
      body: IssueCommentRequest,
      headers: ConfirmedMutationAttempt<IssueCommentRequest>['headers'],
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/issues/{issue_id}/comments', {
            params: { path: { project_id, issue_id }, header: headers },
            body,
          })
        ).data,
      );
    },
    async comment(project_id: string, issue_id: string, comment_id: string, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/issues/{issue_id}/comments/{comment_id}',
            {
              params: { path: { project_id, issue_id, comment_id } },
              cache: 'no-store',
              signal: signal ?? null,
            },
          )
        ).data,
      );
    },
  };
}
export const issueActivityApi = createIssueActivityApi();
export type IssueActivityApi = ReturnType<typeof createIssueActivityApi>;
