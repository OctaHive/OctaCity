import type { Issue } from '../../src/features/issues/api';
import { vi } from 'vitest';
import { issue, setup } from './issueTestSupport';
import { executionFixture } from './issueExecutions';
import { configurationFixture, runFixture } from './factory';
import { factoryApi } from '../../src/features/factory/api';
import { fakeProjectsApi, project } from '../../src/features/projects/ProjectTestSupport';
import type { IssueExecutionsApi } from '../../src/features/issues/executionApi';
export function setupExecution(
  path = `/projects/delivery/issues/${issue.id}`,
  version = 1,
  prepared?: Issue,
) {
  const configuration = {
    ...configurationFixture().configuration.reference,
    project_id: 'delivery',
  };
  const executions = {
    review: vi.fn().mockImplementation(async (_p, _i, body, version) => ({
      version,
      target: body.target,
      snapshot: null,
      snapshot_digest: null,
    })),
    snapshot: vi.fn().mockResolvedValue({ availability: 'not_selected' }),
    start: vi
      .fn<IssueExecutionsApi['start']>()
      .mockResolvedValue({ disposition: 'applied', resource: executionFixture() }),
    get: vi.fn<IssueExecutionsApi['get']>().mockResolvedValue(executionFixture()),
    list: vi.fn<IssueExecutionsApi['list']>().mockResolvedValue({
      items: [],
      next_cursor: null,
      observation: 'request-watermark-v1',
      watermark: 0,
      issue_version: 1,
    }),
  };
  const run = runFixture();
  const legacy = {
    listLegacy: vi
      .fn()
      .mockResolvedValue({ items: [], next_cursor: null, observation: 'a'.repeat(64) }),
    getWork: vi.fn().mockResolvedValue({
      association: {
        project_id: 'delivery',
        issue_id: issue.id,
        request_id: null,
        requirements_version: null,
        work_id: 'legacy-work',
        run_id: 'retained-run',
      },
      run: {
        availability: 'available',
        resource: {
          ...run,
          id: 'retained-run',
          work_id: 'legacy-work',
          admitted_at: 1,
          updated_at: 2,
        },
      },
    }),
  };
  const projects = fakeProjectsApi();
  projects.getProject.mockResolvedValue({
    ancestors: [],
    project: project('delivery', 'Delivery'),
  });
  projects.listRepositories.mockResolvedValue({
    items: [
      {
        id: 'repo',
        project_id: 'delivery',
        name: 'Application source',
        version: 2,
        published_at_unix_ms: 1,
      },
    ],
    next_cursor: null,
  });
  projects.getRepository.mockResolvedValue({
    id: 'repo',
    project_id: 'delivery',
    name: 'Application source',
    version: 2,
    published_at_unix_ms: 1,
    definition: {
      repository_locator: 'hidden',
      vcs_integration_id: 'vcs',
      selection: {
        default_reference: 'main',
        allowed_references: ['main'],
        allow_exact_revision: true,
      },
    },
  });
  const getConfiguration = vi.fn().mockResolvedValue({
    ...configurationFixture(),
    configuration: { ...configurationFixture().configuration, reference: configuration },
  });
  const listConfigurations = vi.fn().mockResolvedValue({
    items: [{ ...configuration, enabled: true, created_at_unix_ms: 1, published_at_unix_ms: 1 }],
    next_cursor: null,
    total: 1,
  });
  const result = setup(
    path,
    (api) =>
      api.get.mockResolvedValue(
        prepared ?? {
          ...issue,
          version,
          content: { ...issue.content, acceptance: 'Check checkout works' },
        },
      ),
    {
      issueExecutionsApi: executions,
      issueLegacyWorkApi: legacy,
      projectsApi: projects,
      factoryApi: { ...factoryApi, listConfigurations, getConfiguration },
    },
  );
  return {
    ...result,
    executions,
    legacy,
    configuration,
    projects,
    listConfigurations,
    getConfiguration,
  };
}
