import {
  managementApi,
  requireManagementResponse,
  type IdempotencyRequestHeaders,
} from '../../api/client';
import type { components } from '../../../.generated/api/schema';

/** AI-agent content uses Project Role/Profile contracts, independently of worker enrollment. */
export function createFactoryRolesApi(client = managementApi) {
  return {
    async currentRole(project: string, role: string) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-roles/{role_id}', {
            params: { path: { project_id: project, role_id: role } },
          })
        ).data,
      );
    },
    async currentProfile(project: string, profile: string) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-role-profiles/{profile_id}', {
            params: { path: { project_id: project, profile_id: profile } },
          })
        ).data,
      );
    },
    async validateProfile(
      project: string,
      body: components['schemas']['ValidateFactoryRoleProfileRequest'],
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/factory-role-profiles/validate', {
            params: { path: { project_id: project } },
            body,
          })
        ).data,
      );
    },
    async getCapability(project: string, alias: string, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-profile-capabilities/{alias}', {
            params: { path: { project_id: project, alias } },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async getTemplate(
      project: string,
      ref: components['schemas']['FactoryImmutableReferenceResource'],
      signal?: AbortSignal,
    ) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/factory-role-templates/{identity}/versions/{version}',
            {
              params: {
                path: { project_id: project, identity: ref.identity, version: ref.version },
                query: { digest: ref.digest },
              },
              signal: signal ?? null,
            },
          )
        ).data,
      );
    },
    async copyTemplate(
      project: string,
      body: components['schemas']['CopyFactoryRoleTemplateRequest'],
      headers: IdempotencyRequestHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/factory-role-template-copies', {
            params: { path: { project_id: project }, header: headers },
            body,
          })
        ).data,
      );
    },
    async importSkill(
      project: string,
      body: components['schemas']['ImportProjectSkillRequest'],
      headers: IdempotencyRequestHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/factory-skills', {
            params: { path: { project_id: project }, header: headers },
            body,
          })
        ).data,
      );
    },
    async listCapabilities(project: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-profile-capabilities', {
            params: {
              path: { project_id: project },
              query: { limit: 25, ...(after ? { after } : {}) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async listTemplates(project: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-role-templates', {
            params: {
              path: { project_id: project },
              query: { limit: 25, ...(after ? { after } : {}) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async listSkills(project: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-skills', {
            params: {
              path: { project_id: project },
              query: { limit: 25, ...(after ? { after } : {}) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async validateRole(project: string, body: components['schemas']['ValidateFactoryRoleRequest']) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/factory-roles/validate', {
            params: { path: { project_id: project } },
            body,
          })
        ).data,
      );
    },
    async publish(
      project: string,
      body: components['schemas']['PublishFactoryRoleProfilesRequest'],
      headers: IdempotencyRequestHeaders,
    ) {
      return requireManagementResponse(
        (
          await client.POST('/api/v1/projects/{project_id}/factory-role-publications', {
            params: { path: { project_id: project }, header: headers },
            body,
          })
        ).data,
      );
    },
    async listRoles(project: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-roles', {
            params: {
              path: { project_id: project },
              query: { limit: 25, ...(after ? { after } : {}) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async getRole(project: string, role: string, version: number, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/factory-roles/{role_id}/versions/{version}',
            {
              params: { path: { project_id: project, role_id: role, version } },
              signal: signal ?? null,
            },
          )
        ).data,
      );
    },
    async listProfiles(project: string, role: string, after: string | null, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET('/api/v1/projects/{project_id}/factory-roles/{role_id}/profiles', {
            params: {
              path: { project_id: project, role_id: role },
              query: { limit: 25, ...(after ? { after } : {}) },
            },
            signal: signal ?? null,
          })
        ).data,
      );
    },
    async getProfile(project: string, profile: string, version: number, signal?: AbortSignal) {
      return requireManagementResponse(
        (
          await client.GET(
            '/api/v1/projects/{project_id}/factory-role-profiles/{profile_id}/versions/{version}',
            {
              params: { path: { project_id: project, profile_id: profile, version } },
              signal: signal ?? null,
            },
          )
        ).data,
      );
    },
  };
}
export const factoryRolesApi = createFactoryRolesApi();
export type FactoryRolesApi = ReturnType<typeof createFactoryRolesApi>;
