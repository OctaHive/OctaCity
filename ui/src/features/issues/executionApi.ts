import type { components } from '../../../.generated/api/schema';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { managementApi, requireManagementResponse } from '../../api/client';
import type { VersionedMutationHeaders } from '../../shared/confirmedIntent';
export type IssueExecutionRequest = components['schemas']['RequestIssueExecutionRequest'];
export type IssueExecution = components['schemas']['IssueExecutionResource'];
export type IssueExecutionReceipt = components['schemas']['IssueExecutionMutationResponse'];
export type IssueWorkAssociation = components['schemas']['IssueWorkAssociation'];

/** Execution transport is separate from manual Issue save and uses the generated HTTP contract. */
export function createIssueExecutionsApi(client = managementApi) {
  return {
    async listLegacy(
      project_id: string,
      issue_id: string,
      after: string | null,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/legacy-work', {
            params: {
              path: { project_id, issue_id },
              query: { limit: bounds.defaultPageSize, ...(after ? { after } : {}) },
            },
            cache: 'no-store',
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async getWork(project_id: string, issue_id: string, work_id: string, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/work/{work_id}', {
            params: { path: { project_id, issue_id, work_id } },
            cache: 'no-store',
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async start(
      project_id: string,
      issue_id: string,
      body: IssueExecutionRequest,
      headers: VersionedMutationHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/issues/{issue_id}/executions', {
            params: { path: { project_id, issue_id }, header: headers },
            body,
          })
        ).data,
      );
    },
    async get(project_id: string, issue_id: string, request_id: string, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/issues/{issue_id}/executions/{request_id}',
            {
              params: { path: { project_id, issue_id, request_id } },
              signal: signal ?? null,
            },
          )
        ).data,
      );
    },
    async list(project_id: string, issue_id: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/executions', {
            params: {
              path: { project_id, issue_id },
              query: { limit: bounds.defaultPageSize, ...(after === null ? {} : { after }) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
  };
}
export const issueExecutionsApi = createIssueExecutionsApi();
export type IssueExecutionsApi = Pick<
  ReturnType<typeof createIssueExecutionsApi>,
  'start' | 'get' | 'list'
>;
export type IssueLegacyWorkApi = Pick<
  ReturnType<typeof createIssueExecutionsApi>,
  'listLegacy' | 'getWork'
>;
