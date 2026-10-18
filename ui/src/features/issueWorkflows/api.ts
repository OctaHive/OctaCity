import type { components } from '../../../.generated/api/schema';
import {
  managementApi,
  requireManagementResponse,
  type IdempotencyRequestHeaders,
} from '../../api/client';
export type WorkflowDefinition = components['schemas']['IssueWorkflowDefinition'];
export type WorkflowResource = components['schemas']['IssueWorkflowResource'];
export type WorkflowDefault = components['schemas']['IssueWorkflowDefault'];
type VersionHeaders = IdempotencyRequestHeaders & Readonly<{ 'If-Match': string }>;

/** Project business policy transport; no Factory or source-adapter dependency. */
export function createIssueWorkflowsApi(client = managementApi) {
  return {
    async list(project_id: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issue-workflows', {
            params: {
              path: { project_id },
              query: { limit: 50, ...(after === null ? {} : { after }) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async get(project_id: string, workflow_id: string, version?: number, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await (version === undefined
            ? client.GET('/api/v1/projects/{project_id}/issue-workflows/{workflow_id}', {
                params: { path: { project_id, workflow_id } },
                signal: signal ?? null,
              })
            : client.GET(
                '/api/v1/projects/{project_id}/issue-workflows/{workflow_id}/versions/{version}',
                { params: { path: { project_id, workflow_id, version } }, signal: signal ?? null },
              ))
        ).data,
      );
    },
    async create(
      project_id: string,
      request: components['schemas']['CreateIssueWorkflowRequest'],
      headers: IdempotencyRequestHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/issue-workflows', {
            params: { path: { project_id }, header: headers },
            body: request,
          })
        ).data,
      );
    },
    async selectDefault(
      project_id: string,
      request: components['schemas']['SelectDefaultIssueWorkflowRequest'],
      headers: VersionHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/issue-workflow-default', {
            params: { path: { project_id }, header: headers },
            body: request,
          })
        ).data,
      );
    },
    async getDefault(project_id: string, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issue-workflow-default', {
            params: { path: { project_id } },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async validate(project_id: string, definition: WorkflowDefinition) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/issue-workflows/validate', {
            params: { path: { project_id } },
            body: { definition },
          })
        ).data,
      );
    },
    async publish(
      project_id: string,
      workflow_id: string,
      definition: WorkflowDefinition,
      headers: VersionHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST(
            '/api/v1/projects/{project_id}/issue-workflows/{workflow_id}/versions',
            {
              params: { path: { project_id, workflow_id }, header: headers },
              body: { definition },
            },
          )
        ).data,
      );
    },
  };
}
export const issueWorkflowsApi = createIssueWorkflowsApi();
export type IssueWorkflowsApi = ReturnType<typeof createIssueWorkflowsApi>;
