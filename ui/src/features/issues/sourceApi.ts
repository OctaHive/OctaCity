import type { components } from '../../../.generated/api/schema';
import {
  managementApi,
  requireManagementResponse,
  ManagementApiError,
  versionedMutationHeaders,
  type IdempotencyRequestHeaders,
} from '../../api/client';
import type { VersionedMutationHeaders } from '../../shared/confirmedIntent';
import { sourceRequest } from '../../shared/sourceErrors';

export type IssueSourceImport = components['schemas']['ProjectIssueSourceImportConfiguration'];
export type IssueSourceImportIntent =
  components['schemas']['ProjectIssueSourceImportSubmission']['intent'];

export type IssueSynchronization = components['schemas']['IssueSynchronization'];
export type IssueSourceProjection = components['schemas']['IssueSourceProjection'];
export type IssueSourceProjectionPage = components['schemas']['IssueSourceProjectionPage'];
export type IssueSourceChoice = components['schemas']['IssueSourceChoice'];
export type IssueSourceMappingDefinition = components['schemas']['IssueSourceMappingDefinition'];
export type IssueSourceAttachment = components['schemas']['AttachIssueSourceRequest'];
export type IssueSourceMappingRequest = components['schemas']['PublishIssueSourceMappingRequest'];

/** A finite association conflict retains only a server-authorized canonical link. */
export class IssueSourceAssociationConflict extends ManagementApiError {
  constructor(readonly canonical: components['schemas']['IssueSourceLink'] | null) {
    super({
      code: 'conflict',
      message: 'Source association conflict',
      requestId: null,
      retryAfterMilliseconds: null,
      status: 200,
    });
  }
}

/** Optional task synchronization transport; retained reads never dispatch a provider. */
export function createIssueSourcesApi(client = managementApi) {
  return {
    async connections(after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await sourceRequest(
            client.GET('/api/v1/integrations', {
              params: { query: { limit: 25, ...(after ? { after } : {}) } },
              signal: signal ?? null,
            }),
          )
        ).data,
      );
    },
    async imports(project_id: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await sourceRequest(
            client.GET('/api/v1/projects/{project_id}/issue-source-imports', {
              params: { path: { project_id }, query: { limit: 25, ...(after ? { after } : {}) } },
              signal: signal ?? null,
            }),
          )
        ).data,
      );
    },
    async importChoices(
      project_id: string,
      connection_id: string,
      connection_version: number,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await sourceRequest(
            client.GET('/api/v1/projects/{project_id}/issue-source-choices', {
              params: { path: { project_id }, query: { connection_id, connection_version } },
              signal: signal ?? null,
            }),
          )
        ).data,
      );
    },
    async previewImport(
      project_id: string,
      body: components['schemas']['ProjectIssueSourcePreviewSubmission'],
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await sourceRequest(
            client.POST('/api/v1/projects/{project_id}/issue-sources/preview', {
              params: { path: { project_id } },
              body,
              signal: signal ?? null,
            }),
          )
        ).data,
      );
    },
    async publishImport(intent: IssueSourceImportIntent, headers: IdempotencyRequestHeaders) {
      const precondition =
        intent.expected_current_version === null
          ? { ...headers, 'If-None-Match': '*' as const }
          : versionedMutationHeaders(headers['Idempotency-Key'], intent.expected_current_version);
      return requireManagementResponse(
        (
          await sourceRequest(
            client.POST('/api/v1/projects/{project_id}/issue-source-imports/{import_id}/versions', {
              params: {
                path: { project_id: intent.project_id, import_id: intent.id },
                header: precondition,
              },
              body: { intent },
            }),
          )
        ).data,
      );
    },
    async projections(
      project_id: string,
      issue_id: string,
      after: string | null,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/issues/{issue_id}/synchronization/projections',
            {
              params: {
                path: { project_id, issue_id },
                query: { ...(after ? { after } : {}), limit: 20 },
              },
              signal: signal ?? null,
            },
          )
        ).data,
      );
    },
    async resolve(
      project_id: string,
      issue_id: string,
      body: components['schemas']['ResolveIssueSourceConflictRequest'],
      headers: VersionedMutationHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST(
            '/api/v1/projects/{project_id}/issues/{issue_id}/synchronization/resolve',
            {
              params: { path: { project_id, issue_id }, header: headers },
              body,
            },
          )
        ).data,
      );
    },
    async choices(project_id: string, issue_id: string, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/source-choices', {
            params: { path: { project_id, issue_id } },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async candidates(
      project_id: string,
      issue_id: string,
      body: components['schemas']['PreviewIssueSourceRequest'],
    ) {
      return requireManagementResponse(
        (
          await client.POST(
            '/api/v1/projects/{project_id}/issues/{issue_id}/source-link/candidates',
            { params: { path: { project_id, issue_id } }, body },
          )
        ).data,
      );
    },
    async attach(
      project_id: string,
      issue_id: string,
      body: IssueSourceAttachment,
      headers: VersionedMutationHeaders,
    ) {
      const result = requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/issues/{issue_id}/source-link', {
            params: { path: { project_id, issue_id }, header: headers },
            body,
          })
        ).data,
      );
      if (result.kind === 'conflict') throw new IssueSourceAssociationConflict(result.canonical);
      return result;
    },
    async publishMapping(
      project_id: string,
      issue_id: string,
      body: IssueSourceMappingRequest,
      headers: VersionedMutationHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST(
            '/api/v1/projects/{project_id}/issues/{issue_id}/source-link/mappings',
            { params: { path: { project_id, issue_id }, header: headers }, body },
          )
        ).data,
      );
    },
    async mappingVersion(
      project_id: string,
      issue_id: string,
      version: number,
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/issues/{issue_id}/source-link/mappings/{version}',
            { params: { path: { project_id, issue_id, version } }, signal: signal ?? null },
          )
        ).data,
      );
    },
    async refresh(
      project_id: string,
      issue_id: string,
      body: components['schemas']['RefreshIssueSourceRequest'],
      headers: IdempotencyRequestHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST(
            '/api/v1/projects/{project_id}/issues/{issue_id}/synchronization/refresh',
            { params: { path: { project_id, issue_id }, header: headers }, body },
          )
        ).data,
      );
    },
    async setPull(
      project_id: string,
      issue_id: string,
      body: components['schemas']['SetIssueSourcePullRequest'],
      headers: IdempotencyRequestHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST(
            '/api/v1/projects/{project_id}/issues/{issue_id}/synchronization/pull',
            { params: { path: { project_id, issue_id }, header: headers }, body },
          )
        ).data,
      );
    },
    async review(project_id: string, issue_id: string, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/issues/{issue_id}/synchronization', {
            params: { path: { project_id, issue_id } },
            signal: signal ?? null,
          })
        ).data,
      );
    },
  };
}
export type IssueSourcesApi = ReturnType<typeof createIssueSourcesApi>;
export const issueSourcesApi = createIssueSourcesApi();
