import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import type { components } from '../../../.generated/api/schema';
import {
  managementApi,
  requireManagementResponse,
  type IdempotencyRequestHeaders,
} from '../../api/client';
import type { VersionedMutationHeaders } from '../../shared/confirmedIntent';
export type IssuePolicy = components['schemas']['IssueFieldPolicy'];
export type Issue = components['schemas']['IssueResource'];
export type IssueContent = components['schemas']['IssueContent'];

/** Project-owned task transport. Saving a task never invokes Factory or a provider. */
export function createIssuesApi(client = managementApi) {
  return {
    async list(project_id: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues', {
            params: {
              path: { project_id },
              query: { limit: bounds.defaultPageSize, ...(after === null ? {} : { after }) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async get(project_id: string, issue_id: string, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}', {
            params: { path: { project_id, issue_id } },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async revisions(
      project_id: string,
      issue_id: string,
      after: number | null,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/revisions', {
            params: {
              path: { project_id, issue_id },
              query: { limit: bounds.defaultPageSize, ...(after === null ? {} : { after }) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async revision(project_id: string, issue_id: string, version: number, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/revisions/{version}', {
            params: { path: { project_id, issue_id, version } },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async adopt(
      project_id: string,
      issue_id: string,
      body: components['schemas']['AdoptIssueWorkflowRequest'],
      headers: VersionedMutationHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/issues/{issue_id}/workflow-adoption', {
            params: { path: { project_id, issue_id }, header: headers },
            body,
          })
        ).data,
      );
    },
    async move(
      project_id: string,
      issue_id: string,
      body: components['schemas']['TransitionIssueRequest'],
      headers: VersionedMutationHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/issues/{issue_id}/transitions', {
            params: { path: { project_id, issue_id }, header: headers },
            body,
          })
        ).data,
      );
    },
    async edit(
      project_id: string,
      issue_id: string,
      body: IssueContent,
      headers: VersionedMutationHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.PATCH('/api/v1/projects/{project_id}/issues/{issue_id}', {
            params: { path: { project_id, issue_id }, header: headers },
            body,
          })
        ).data,
      );
    },
    async transitions(project_id: string, issue_id: string, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/transitions', {
            params: { path: { project_id, issue_id } },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async history(
      project_id: string,
      issue_id: string,
      after: number | null,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/status-history', {
            params: {
              path: { project_id, issue_id },
              query: { limit: bounds.defaultPageSize, ...(after === null ? {} : { after }) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async create(project_id: string, body: IssueContent, headers: IdempotencyRequestHeaders) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/issues', {
            params: { path: { project_id }, header: headers },
            body,
          })
        ).data,
      );
    },
  };
}
export const issuesApi = createIssuesApi();
export type IssuesApi = ReturnType<typeof createIssuesApi>;
