import { render } from '@testing-library/react';
import { createMemoryRouter } from 'react-router-dom';
import { vi } from 'vitest';
import { App } from '../../src/App';
import { createConsoleQueryClient } from '../../src/app/query';
import { createConsoleRoutes, type ConsoleRouteDependencies } from '../../src/app/router';
import { fakeProjectsApi, project } from '../../src/features/projects/ProjectTestSupport';
import { issueFixture } from './issues';
import { readFileSync } from 'node:fs';
export const workflow = {
  id: issueFixture().workflow_id,
  project_id: 'delivery',
  version: 3,
  published_at_unix_ms: 1700000000000,
  definition: JSON.parse(
    readFileSync(
      '../server/api/octacity-server-api-rest/fixtures/v1/issue-workflow-definition.json',
      'utf8',
    ),
  ),
};

export const issue = issueFixture();
export function fakeIssuesApi() {
  return {
    editPlanning: vi
      .fn()
      .mockResolvedValue({ disposition: 'applied', resource: { ...issue, version: 2 } }),
    propertyDefinitions: vi.fn(),
    children: vi.fn().mockResolvedValue({ items: [], next_cursor: null }),
    planningChoices: vi.fn().mockResolvedValue({
      definitions: {
        project_id: 'delivery',
        version: 0,
        definition: { labels: [], properties: [] },
        published_at_unix_ms: 1,
      },
      assignees: { available: false, items: [] },
    }),
    list: vi.fn().mockResolvedValue({ items: [], next_cursor: null }),
    create: vi.fn().mockResolvedValue({ disposition: 'applied', resource: issue }),
    get: vi.fn().mockResolvedValue(issue),
    transitions: vi.fn().mockResolvedValue({
      issue_version: 1,
      workflow_id: issue.workflow_id,
      workflow_version: 3,
      status_id: issue.status_id,
      field_policy: { source_owned: [], status_mapping_workflow: null, push: null },
      items: [],
    }),
    revisions: vi.fn().mockResolvedValue({
      items: [{ version: 1, applicability_version: 1, updated_at_unix_ms: 1700000000000 }],
      next_cursor: null,
    }),
    revision: vi.fn().mockResolvedValue({
      ...issue,
      content: { ...issue.content, description: 'Original requirements' },
    }),
    adopt: vi
      .fn()
      .mockResolvedValue({ disposition: 'applied', resource: { ...issue, version: 2 } }),
    move: vi.fn().mockResolvedValue({
      disposition: 'applied',
      resource: { ...issue, version: 2 },
      transition: {},
    }),
    edit: vi.fn().mockResolvedValue({ disposition: 'applied', resource: { ...issue, version: 5 } }),
    history: vi.fn().mockResolvedValue({ items: [], next_cursor: null }),
  };
}
export function setup(
  path = '/projects/delivery/issues',
  configure?: (api: ReturnType<typeof fakeIssuesApi>) => void,
  overrides: Partial<ConsoleRouteDependencies> = {},
) {
  const projects = fakeProjectsApi();
  projects.getProject.mockResolvedValue({
    ancestors: [],
    project: project('delivery', 'Delivery'),
  });
  const api = fakeIssuesApi();
  configure?.(api);
  const workflows = {
    create: vi.fn(),
    selectDefault: vi.fn(),
    getDefault: vi.fn(),
    validate: vi.fn(),
    publish: vi.fn(),
    get: vi.fn().mockResolvedValue(workflow),
    list: vi.fn().mockResolvedValue({
      items: [
        {
          id: workflow.id,
          project_id: 'delivery',
          version: 4,
          name: 'Delivery policy',
          published_at_unix_ms: 1700000000000,
        },
      ],
      next_cursor: null,
    }),
  };
  const router = createMemoryRouter(
    createConsoleRoutes({
      projectsApi: projects,
      issuesApi: api,
      issueWorkflowsApi: workflows,
      readinessProbe: async () => 'ready',
      ...overrides,
    }),
    { initialEntries: [path] },
  );
  const queryClient = createConsoleQueryClient();
  render(<App router={router} queryClient={queryClient} />);
  return { api, router, queryClient, workflows };
}
