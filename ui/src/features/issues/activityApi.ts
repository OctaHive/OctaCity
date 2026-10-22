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
export type IssueJournalPage = components['schemas']['IssueExecutionJournalPage'];
export type IssueJournalEvents = components['schemas']['IssueExecutionJournalEvents'];
/** Reviewed request, or an exact original Artifact Work with no fabricated request. */
export type IssueJournalSelection = string | { work_id: string };
export function issueJournalKey(selection: IssueJournalSelection): string {
  return typeof selection === 'string' ? selection : `work:${selection.work_id}`;
}
export type IssueJournalTarget = Pick<
  operations['readIssueExecutionJournalEvents']['parameters']['query'],
  'build_execution' | 'job_id'
>;
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
    async journal(
      project_id: string,
      issue_id: string,
      request_id: string,
      collection: IssueResultCollection,
      after: string | null,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/issues/{issue_id}/executions/{request_id}/journal',
            {
              params: {
                path: { project_id, issue_id, request_id },
                query: {
                  collection,
                  limit: bounds.defaultPageSize,
                  ...(after === null ? {} : { after }),
                },
              },
              cache: 'no-store',
              signal: signal ?? null,
            },
          )
        ).data,
      );
    },
    async journalEvents(
      project_id: string,
      issue_id: string,
      selection: IssueJournalSelection,
      target: IssueJournalTarget,
      after: string | null,
      signal?: AbortSignal,
    ) {
      const query = {
        ...target,
        limit: bounds.defaultPageSize,
        ...(after === null ? {} : { after }),
      };
      const options = { cache: 'no-store', signal: signal ?? null } satisfies RequestInit;
      const response =
        typeof selection === 'string'
          ? await client.GET(
              '/api/v1/projects/{project_id}/issues/{issue_id}/executions/{request_id}/journal/events',
              {
                ...options,
                params: { path: { project_id, issue_id, request_id: selection }, query },
              },
            )
          : await client.GET(
              '/api/v1/projects/{project_id}/issues/{issue_id}/work/{work_id}/journal/events',
              {
                ...options,
                params: { path: { project_id, issue_id, work_id: selection.work_id }, query },
              },
            );
      return requireManagementResponse(response.data);
    },
    async journalEvent(
      project_id: string,
      issue_id: string,
      selection: IssueJournalSelection,
      target: IssueJournalTarget,
      sequence: number,
      signal?: AbortSignal,
    ) {
      const options = { cache: 'no-store', signal: signal ?? null } satisfies RequestInit;
      const response =
        typeof selection === 'string'
          ? await client.GET(
              '/api/v1/projects/{project_id}/issues/{issue_id}/executions/{request_id}/journal/events/{sequence}',
              {
                ...options,
                params: {
                  path: { project_id, issue_id, request_id: selection, sequence },
                  query: target,
                },
              },
            )
          : await client.GET(
              '/api/v1/projects/{project_id}/issues/{issue_id}/work/{work_id}/journal/events/{sequence}',
              {
                ...options,
                params: {
                  path: { project_id, issue_id, work_id: selection.work_id, sequence },
                  query: target,
                },
              },
            );
      return requireManagementResponse(response.data);
    },
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
