// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { ManagementApiError } from '../../api/client';
import { createConsoleQueryClient, queryKeys } from '../../app/query';
import { factoryApi, type FactoryApi } from './api';
import { FactoryWorkForm, type FactoryWorkSources } from './FactoryWorkForm';
import { App } from '../../App';
import { createConsoleRoutes } from '../../app/router';
import { fakeProjectsApi, project } from '../projects/ProjectTestSupport';
import { buildDiagnosticsApi } from '../builds/api';
import { factoryProfileConfigurationFixture } from '../../../tests/fixtures/factoryRoles';
import { configurationFixture } from '../../../tests/fixtures/factory';

afterEach(cleanup);
function renderWork(specificationCount = 1, composed = false, allowExactRevision = false) {
  const configuration = configurationFixture().configuration.reference;
  const artifact = (id: string, name: string) => ({
    id,
    name,
    build_id: 'input-build',
    attempt_id: 'attempt',
    job_id: 'job',
    media_type: 'text/plain',
    output_type: { kind: 'artifact' as const },
    published_at_unix_ms: 1,
    sha256: 'a'.repeat(64),
    size_bytes: 20,
  });
  const sources: FactoryWorkSources = {
    getProject: vi.fn(async () => ({
      ancestors: [],
      project: project(configuration.project_id, 'Application Project'),
    })),
    listRepositories: vi.fn(async () => ({
      items: [
        {
          id: 'repo',
          name: 'Application source',
          project_id: configuration.project_id,
          version: 2,
          published_at_unix_ms: 1,
        },
      ],
      next_cursor: null,
    })),
    getRepository: vi.fn(async () => ({
      id: 'repo',
      name: 'Application source',
      project_id: configuration.project_id,
      version: 2,
      published_at_unix_ms: 1,
      definition: {
        repository_locator: 'hidden',
        vcs_integration_id: 'vcs',
        selection: {
          allow_exact_revision: allowExactRevision,
          allowed_references: ['main'],
          default_reference: 'main',
        },
      },
    })),
    listBuilds: vi.fn(async () => ({
      items: [
        {
          id: 'input-build',
          project_id: configuration.project_id,
          configuration_id: 'ordinary',
          configuration_version: 1,
          cause: null,
          created_at_unix_ms: 1,
          current_attempt_id: 'attempt',
          current_attempt_number: 1,
          current_attempt_state: 'succeeded' as const,
          state: 'succeeded' as const,
          terminal_at_unix_ms: 2,
        },
      ],
      next_cursor: null,
    })),
    listBuildArtifacts: vi.fn(async () => ({
      items: [
        artifact('task', 'Requested task'),
        artifact('acceptance', 'Acceptance criteria'),
        ...Array.from({ length: specificationCount }, (_, index) =>
          artifact(`spec-${index}`, index === 0 ? 'Specification' : `Specification ${index + 1}`),
        ),
      ],
    })),
  };
  const admitWork = vi.fn<FactoryApi['admitWork']>(async () => ({
    disposition: 'applied',
    factory_run_id: 'returned-run',
    work_envelope_id: 'work',
    base_revision: 'exact-base',
    state: 'admitted',
    admitted_at_unix_ms: 1,
  }));
  const api = {
    ...factoryApi,
    listChoices: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listRuns: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listFlows: vi.fn(async () => ({ items: [], total: 0, next_cursor: null })),
    listConfigurations: vi.fn(async () => ({
      items: [{ ...configuration, enabled: true, published_at_unix_ms: 1, created_at_unix_ms: 1 }],
      total: 1,
      next_cursor: null,
    })),
    getConfiguration: vi.fn(async () => factoryProfileConfigurationFixture()),
    admitWork,
  };
  const router = createMemoryRouter(
    composed
      ? createConsoleRoutes({
          factoryApi: api,
          projectsApi: { ...fakeProjectsApi(), ...sources },
          buildApi: { ...buildDiagnosticsApi, listBuildArtifacts: sources.listBuildArtifacts },
          readinessProbe: async () => 'ready',
        })
      : [
          {
            path: '/factory',
            element: (
              <FactoryWorkForm
                api={api}
                projectId={configuration.project_id}
                sources={sources}
                onDiscard={() => {}}
              />
            ),
          },
          { path: '/factory/:id', element: <p>Admitted Run</p> },
        ],
    { initialEntries: [composed ? `/factory?project_id=${configuration.project_id}` : '/factory'] },
  );
  const queryClient = createConsoleQueryClient();
  render(
    <QueryClientProvider client={queryClient}>
      {composed ? (
        <App queryClient={queryClient} router={router} />
      ) : (
        <RouterProvider router={router} />
      )}
    </QueryClientProvider>,
  );
  return { admitWork, router, configuration, sources, queryClient };
}
it('confirms selected visible Work artifacts and opens the returned Run without persisting the draft', async () => {
  const stored = JSON.stringify(localStorage);
  const { admitWork, router, configuration, queryClient } = renderWork();
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries');
  expect(await screen.findByText('Application Project')).toBeTruthy();
  await userEvent.type(
    await screen.findByRole('textbox', { name: 'Work label' }),
    'private unsent label',
  );
  await userEvent.click(await screen.findByRole('combobox', { name: 'Task Artifact' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Requested task' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Acceptance Artifact' }));
  await userEvent.click(screen.getByRole('option', { name: 'Acceptance criteria' }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Specification' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review Work admission' }));
  expect(admitWork).not.toHaveBeenCalled();
  const dialog = await screen.findByRole('dialog');
  expect(dialog.textContent).toContain('Application source · v2');
  expect(dialog.textContent).toContain('Requested task');
  expect(dialog.textContent).toContain('Acceptance criteria');
  expect(dialog.textContent).toContain('Specification');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm Work admission' }));
  expect(await screen.findByText('Admitted Run')).toBeTruthy();
  expect(router.state.location.pathname).toBe('/factory/returned-run');
  expect(router.state.location.search).toBe('');
  expect(admitWork.mock.calls[0]?.[1]).toMatchObject({
    configuration_id: configuration.id,
    repository_id: 'repo',
    repository_version: 2,
    source: { kind: 'default_reference' },
    artifacts: {
      task: { artifact_id: 'task', content_digest: 'a'.repeat(64), encoded_size: 20 },
      acceptance: { artifact_id: 'acceptance' },
      specifications: [{ artifact_id: 'spec-0' }],
    },
  });
  expect(admitWork.mock.calls[0]?.[2]).toMatchObject({ 'Idempotency-Key': expect.any(String) });
  expect(JSON.stringify(localStorage)).toBe(stored);
  expect(invalidate.mock.calls.map(([target]) => target?.queryKey)).toContainEqual(
    queryKeys.factoryPoolRoot(configuration.project_id),
  );
});

it('bounds specification selections before confirmation using the published Work contract', async () => {
  const { admitWork } = renderWork(65);
  await screen.findByRole('combobox', { name: 'Task Artifact' });
  await screen.findByRole('checkbox', { name: 'Specification 65' });
  for (const checkbox of screen.getAllByRole('checkbox').slice(0, 64))
    await userEvent.click(checkbox);
  expect(
    (screen.getByRole('checkbox', { name: 'Specification 65' }) as HTMLInputElement).disabled,
  ).toBe(true);
  await userEvent.click(screen.getByRole('checkbox', { name: 'Specification' }));
  expect(
    (screen.getByRole('checkbox', { name: 'Specification 65' }) as HTMLInputElement).disabled,
  ).toBe(false);
  expect(admitWork).not.toHaveBeenCalled();
});

it('uses the application-composed Project and Build reads for guided Factory admission', async () => {
  renderWork(1, true);
  await userEvent.click(await screen.findByRole('button', { name: /^Admit Work$/ }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Task Artifact' }));
  expect(await screen.findByRole('option', { name: 'Requested task' })).toBeTruthy();
});

it.each(['getProject', 'getRepository', 'listBuildArtifacts'] as const)(
  'marks retained %s input choices stale after a failed refresh',
  async (method) => {
    const { sources, queryClient, configuration } = renderWork();
    await screen.findByText('Application Project');
    await screen.findByRole('checkbox', { name: 'Specification' });
    vi.mocked(sources[method]).mockRejectedValue(
      new ManagementApiError({
        code: 'unavailable',
        message: 'safe input refresh failure',
        requestId: 'input-refresh',
        status: 503,
        retryAfterMilliseconds: null,
      }),
    );
    const queryKey =
      method === 'getProject'
        ? queryKeys.project(configuration.project_id)
        : method === 'getRepository'
          ? queryKeys.repository('repo', 2)
          : queryKeys.buildArtifacts('input-build');
    await queryClient.invalidateQueries({ queryKey });
    expect(await screen.findByText(/Refresh failed. Showing the last loaded/)).toBeTruthy();
    expect(screen.getByRole('textbox', { name: 'Work label' })).toBeTruthy();
  },
);

it.each(['revision-one', 'revision-two'])(
  'reviews the exact source %s and configuration before admitting Work',
  async (revision) => {
    const { configuration, admitWork } = renderWork(1, false, true);
    await screen.findByText('Application Project');
    await userEvent.type(screen.getByRole('textbox', { name: 'Work label' }), 'Reviewed Work');
    await userEvent.click(await screen.findByRole('combobox', { name: 'Task Artifact' }));
    await userEvent.click(screen.getByRole('option', { name: 'Requested task' }));
    await userEvent.click(screen.getByRole('combobox', { name: 'Acceptance Artifact' }));
    await userEvent.click(screen.getByRole('option', { name: 'Acceptance criteria' }));
    await userEvent.click(screen.getByRole('combobox', { name: 'Source' }));
    await userEvent.click(screen.getByRole('option', { name: 'Exact revision' }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Exact revision' }), revision);
    await userEvent.click(screen.getByRole('button', { name: 'Review Work admission' }));
    const dialog = screen.getByRole('dialog');
    expect(dialog.textContent).toContain(revision);
    expect(dialog.textContent).toContain(configuration.id);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm Work admission' }));
    expect(await screen.findByText('Admitted Run')).toBeTruthy();
    expect(admitWork.mock.calls[0]?.[1]).toMatchObject({
      configuration_id: configuration.id,
      source: { kind: 'exact_revision', value: revision },
    });
  },
);

it('admits an exact optional stage override from the pinned configuration without changing the stage defaults', async () => {
  const { admitWork } = renderWork();
  await userEvent.type(
    await screen.findByRole('textbox', { name: 'Work label' }),
    'Reviewed override',
  );
  await userEvent.click(await screen.findByRole('combobox', { name: 'Task Artifact' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Requested task' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Acceptance Artifact' }));
  await userEvent.click(screen.getByRole('option', { name: 'Acceptance criteria' }));
  await userEvent.click(screen.getByRole('button', { name: 'Override stage profiles' }));
  const configuration = factoryProfileConfigurationFixture().configuration;
  const root = configuration.flow.closure.definitions[0]!;
  const node = root.nodes[0]!;
  await userEvent.click(
    await screen.findByRole('combobox', { name: `Profile override · ${node.key}` }),
  );
  await userEvent.click(await screen.findByRole('option', { name: 'default · default v1' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review Work admission' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', {
      name: 'Confirm Work admission',
    }),
  );
  await screen.findByText('Admitted Run');
  expect(admitWork).toHaveBeenCalledWith(
    configuration.reference.project_id,
    expect.objectContaining({
      profile_overrides: [
        {
          stage: { root: configuration.flow.closure.root, calls: [], node: node.key },
          profile: node.profile_selection!.candidates.default!.profile.reference,
        },
      ],
    }),
    expect.anything(),
  );
});
