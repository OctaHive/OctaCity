// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient, queryKeys } from '../../app/query';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { factoryRoleFixture } from '../../../tests/fixtures/factoryRoles';
import type { FactoryWorkSources } from './FactoryWorkForm';
import { project as projectFixture } from '../projects/ProjectTestSupport';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { factorySourcesApi, type FactorySourcesApi } from './sourceApi';
import { factoryApi, type FactoryApi } from './api';
import { factoryRolesApi } from './roleApi';
import { FactoryTriageSetup } from './FactoryTriageSetup';
import { PresentationProvider } from '../../app/presentation/PresentationProvider';
import { DEFAULT_PREFERENCES, type Language } from '../../app/presentation/preferences';
import type { components } from '../../../.generated/api/schema';

afterEach(cleanup);
const page = <T,>(items: T[]) => ({ items, total: items.length, next_cursor: null });
function fixture({
  language = 'en',
  diagnostics = [{ path: '/profiles/eligibility', reason: 'required' }],
}: {
  language?: Language;
  diagnostics?: components['schemas']['FactoryPresetResolution']['diagnostics'];
} = {}) {
  const { role, profile, capability, choice } = factoryRoleFixture();
  const preset = { identity: 'triage', version: 'v1', digest: 'a'.repeat(64) };
  const mcp = { identity: 'project-reader', version: 'v1', digest: 'b'.repeat(64) };
  profile.definition.execution!.mcp_servers = { reader: { configuration: mcp } };
  capability.mcp_configurations = {
    reader: {
      reference: mcp,
      server: {
        transport: 'http',
        url: 'https://mcp.example/tools',
        bearer_token_env_var: 'READ_TOKEN',
      },
      authorization: {
        credential_profile: 'project-read',
        secret: { provider: 'agent-private', reference: 'project/issues-read' },
        environment: 'READ_TOKEN',
        purpose: 'mcp_read',
        resource: { identity: 'issues-repository', version: 'v1', digest: 'c'.repeat(64) },
      },
    },
  };
  const roles = {
    ...factoryRolesApi,
    listRoles: vi.fn(async () =>
      page([
        {
          ...role.definition,
          reference: role.reference,
          icon: null,
          concurrency_limit: null,
          template: null,
        },
      ]),
    ),
    listProfiles: vi.fn(async () =>
      page([
        {
          reference: profile.reference,
          role: role.reference,
          name: 'Reviewed reader',
          toolchain: profile.definition.toolchain,
          template: null,
        },
      ]),
    ),
    getProfile: vi.fn(async () => profile),
    listCapabilities: vi.fn(async () => page([choice])),
    getCapability: vi.fn(async () => capability),
  };
  const api = {
    ...factoryApi,
    listFlows: vi.fn(async () => page([])),
    listChoices: vi.fn<FactoryApi['listChoices']>(async () =>
      page(
        (['admission_policy', 'permission_ceiling'] as const).map((kind) => ({
          kind,
          alias: kind === 'admission_policy' ? 'reviewed-admission' : 'reviewed-permissions',
          identity: kind,
          reference: {
            kind: 'immutable' as const,
            reference: { identity: kind, version: 'v1', digest: 'f'.repeat(64) },
          },
          ...(kind === 'permission_ceiling'
            ? {
                metadata: {
                  compatible_choices: [],
                  enforcement: [],
                  node_kinds: [],
                  signal_purposes: [],
                  permission_declarations: profile.definition.permissions,
                },
              }
            : {}),
        })),
      ),
    ),
    listPresets: vi.fn<FactoryApi['listPresets']>(async () =>
      page([
        {
          reference: preset,
          profile_aliases: ['eligibility', 'classification'],
          parameters: [],
          required_node_kinds: ['reasoning'],
        },
      ]),
    ),
    resolvePreset: vi.fn<FactoryApi['resolvePreset']>(async () => ({
      preset,
      completion: {
        kind: 'draft',
        project_id: role.reference.project_id,
        preset,
        content: 'Reviewed process criteria',
      },
      diagnostics,
      flow: null,
      configuration: null,
      input: null,
      dependencies: [],
    })),
    readiness: vi.fn<FactoryApi['readiness']>(async () => ({
      configuration: configurationFixture().configuration.reference,
      flow: configurationFixture().configuration.flow.closure.root,
      repository: {
        project_id: role.reference.project_id,
        id: 'code-repo',
        version: 3,
        definition_digest: 'e'.repeat(64),
      },
      execution: 'unknown',
      native_write: 'unknown',
      observed_at: Date.now(),
      expires_at: Date.now() + 60_000,
      checks: [
        {
          aspect: 'agent',
          state: 'blocked',
          reason: 'capacity_absent',
          remediation: 'enroll_compatible_agent',
          required: true,
          observed_at: Date.now(),
          expires_at: Date.now() + 60_000,
        },
        {
          aspect: 'mcp_capture',
          state: 'unknown',
          reason: 'capture_unqualified',
          remediation: 'review_mcp_connection',
          required: true,
          observed_at: Date.now(),
          expires_at: Date.now() + 60_000,
        },
        {
          aspect: 'model_authentication',
          state: 'unknown',
          reason: 'credential_unverified',
          remediation: 'review_model_credentials',
          required: true,
          observed_at: Date.now(),
          expires_at: Date.now() + 60_000,
        },
      ],
    })),
    validateFlow: vi.fn<FactoryApi['validateFlow']>(
      async () => configurationFixture().configuration.flow,
    ),
    validateConfiguration: vi.fn<FactoryApi['validateConfiguration']>(async () => ({
      configuration: configurationFixture().configuration,
      diagnostics: {
        errors: [],
        required_node_capabilities: [],
        required_agent_capabilities: [],
        required_backend_capabilities: [],
        effective_permissions: [],
        unavailable_references: [],
        warnings: [],
      },
    })),
    createConfiguration: vi.fn<FactoryApi['createConfiguration']>(async () => ({
      disposition: 'applied',
      resource: configurationFixture(),
    })),
    createFlow: vi.fn<FactoryApi['createFlow']>(async () => ({
      disposition: 'applied',
      resource: {
        project_id: role.reference.project_id,
        flow: configurationFixture().configuration.flow,
        published_at_unix_ms: 1,
      },
    })),
  };
  const projectId = role.reference.project_id;
  const document = (id: string, name: string) => ({
    project_id: projectId,
    name,
    artifact: { artifact_id: id, content_digest: 'd'.repeat(64), encoded_size: 20 },
    published_at: 1,
    expires_at: Date.now() + 60_000,
  });
  const documents = {
    read: vi.fn(async () => {
      throw new Error('document content must be requested explicitly');
    }),
    list: vi.fn(async () => ({
      items: [
        document('goals', 'Reviewed Project goals'),
        document('rules', 'Reviewed Project rules'),
        document('architecture', 'Reviewed architecture'),
      ],
      next_cursor: null,
    })),
    publish: vi.fn(),
  };
  const workSources: FactoryWorkSources = {
    getProject: vi.fn(async () => ({
      project: projectFixture(projectId, 'Delivery'),
      ancestors: [],
    })),
    listRepositories: vi.fn(async () => ({
      items: [
        {
          id: 'code-repo',
          project_id: projectId,
          name: 'Independent code',
          version: 3,
          published_at_unix_ms: 1,
        },
      ],
      next_cursor: null,
    })),
    getRepository: vi.fn(async () => ({
      id: 'code-repo',
      project_id: projectId,
      name: 'Independent code',
      version: 3,
      published_at_unix_ms: 1,
      definition_digest: 'e'.repeat(64),
      definition: {
        repository_locator: 'https://code.example/repo.git',
        vcs_integration_id: 'vcs',
        selection: {
          allowed_references: ['main'],
          default_reference: 'main',
          allow_exact_revision: false,
        },
      },
    })),
    listBuilds: vi.fn<FactoryWorkSources['listBuilds']>(async () => ({
      items: [],
      next_cursor: null,
    })),
    listBuildArtifacts: vi.fn(async () => ({
      items: [],
    })),
  };
  const connection = integrationFixture('IntegrationConnection');
  const installed = integrationFixture('WorkSourceInputChoices');
  const read = installed.items.find((item) => item.purpose === 'task_read')!;
  const sources = {
    ...factorySourcesApi,
    connections: vi.fn(async () => ({ items: [connection], next_cursor: null })),
    choices: vi.fn<FactorySourcesApi['choices']>(async () => ({
      ...installed,
      items: [
        {
          ...read,
          preparation: {
            kind: 'process_completion',
            value: {
              kind: 'draft',
              project_id: projectId,
              preset,
              content: 'Installed frozen criteria',
            },
          },
        },
      ],
    })),
    validate: vi.fn(),
    publish: vi.fn(),
    preview: vi.fn(),
    list: vi.fn(async () => ({ items: [], next_cursor: null })),
  };
  const queryClient = createConsoleQueryClient();
  render(
    <PresentationProvider
      storage={{
        getItem: () => JSON.stringify({ ...DEFAULT_PREFERENCES, language }),
        setItem: vi.fn(),
      }}
    >
      <QueryClientProvider client={queryClient}>
        <RouterProvider
          router={createMemoryRouter([
            {
              path: '*',
              element: (
                <FactoryTriageSetup
                  api={api}
                  roles={roles}
                  workSources={workSources}
                  documents={documents}
                  sources={sources}
                  projectId={role.reference.project_id}
                  onClose={vi.fn()}
                />
              ),
            },
          ])}
        />
      </QueryClientProvider>
    </PresentationProvider>,
  );
  return {
    api,
    roles,
    profile,
    preset,
    workSources,
    documents,
    queryClient,
    choice,
    sources,
    read,
    connection,
  };
}

it.each(['en', 'ru'] as const)(
  'explains resolution blockers and their setup handoffs in %s',
  async (language) => {
    const { profile } = fixture({
      language,
      diagnostics: [
        { path: '/profiles/eligibility/build', reason: 'unavailable' },
        { path: '/context/classification/rules', reason: 'required' },
        { path: '/effects', reason: 'unavailable' },
      ],
    });
    const text =
      language === 'en'
        ? {
            review: 'Review triage prerequisites',
            diagnostics: 'Setup dependencies',
            project: 'Project analyst',
            analyst: 'Systems analyst',
            build: 'Exact installed Build binding',
            rules: 'Project rules',
            effects: 'Accepted outcome effects',
            required: 'Review and select this dependency.',
            unavailable: 'The selected dependency is unavailable or incompatible.',
            runtime: 'Configure Runtime',
            integrations: 'Manage integrations',
          }
        : {
            review: 'Проверить условия триажа',
            diagnostics: 'Зависимости настройки',
            project: 'Аналитик проекта',
            analyst: 'Системный аналитик',
            build: 'Точная установленная привязка Build',
            rules: 'Правила проекта',
            effects: 'Эффекты принятых результатов',
            required: 'Проверьте и выберите эту зависимость.',
            unavailable: 'Выбранная зависимость недоступна или несовместима.',
            runtime: 'Настроить Runtime',
            integrations: 'Управлять интеграциями',
          };
    await userEvent.click(await screen.findByRole('button', { name: text.review }));
    const diagnostics = within(await screen.findByRole('list', { name: text.diagnostics }));
    const [build, rules, effects] = diagnostics.getAllByRole('listitem');
    expect(build!.textContent).toContain(text.project);
    expect(build!.textContent).toContain(text.build);
    expect(build!.textContent).toContain(text.unavailable);
    expect(within(build!).getByRole('link', { name: text.runtime }).getAttribute('href')).toBe(
      `/ai-settings?project_id=${profile.reference.project_id}&view=runtimes`,
    );
    expect(rules!.textContent).toContain(text.analyst);
    expect(rules!.textContent).toContain(text.rules);
    expect(rules!.textContent).toContain(text.required);
    expect(effects!.textContent).toContain(text.effects);
    expect(effects!.textContent).toContain(text.unavailable);
    expect(
      within(effects!).getByRole('link', { name: text.integrations }).getAttribute('href'),
    ).toBe('/integrations');
    expect(screen.queryByText('/profiles/eligibility/build')).toBeNull();
  },
);

it.each(['success', 'failure'] as const)(
  'retains the last selected code Repository when an earlier request completes with %s',
  async (completion) => {
    const { api, profile, workSources } = fixture();
    const projectId = profile.reference.project_id;
    const original = await workSources.getRepository('code-repo', 3);
    const latest = { ...original, id: 'latest-repo', name: 'Last selected code' };
    vi.mocked(workSources.listRepositories).mockResolvedValue(page([original, latest]));
    let finish!: (value: typeof original) => void;
    let fail!: (error: Error) => void;
    const earlier = new Promise<typeof original>((resolve, reject) => {
      finish = resolve;
      fail = reject;
    });
    vi.mocked(workSources.getRepository).mockImplementation((id) =>
      id === 'code-repo' ? earlier : Promise.resolve(latest),
    );
    await userEvent.click(
      await screen.findByRole('button', { name: 'Review triage prerequisites' }),
    );
    const repository = await screen.findByRole('combobox', { name: 'Code Repository' });
    await userEvent.click(repository);
    await userEvent.click(await screen.findByRole('option', { name: 'Independent code · v3' }));
    await userEvent.click(repository);
    await userEvent.click(await screen.findByRole('option', { name: 'Last selected code · v3' }));
    expect(repository.textContent).toContain('Last selected code');
    await act(async () => {
      if (completion === 'success') finish(original);
      else fail(new Error('An earlier Repository request failed'));
    });
    expect(repository.textContent).toContain('Last selected code');
    expect(screen.queryByRole('alert')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
    expect(api.resolvePreset).toHaveBeenLastCalledWith(
      projectId,
      expect.objectContaining({
        repository: expect.objectContaining({ id: 'latest-repo', version: 3 }),
      }),
    );
  },
);

it('selects independent exact stage profiles and reviews MCP read scope separately from AI and Reporter authority', async () => {
  const { api, profile, preset } = fixture();
  await userEvent.click(await screen.findByRole('button', { name: 'Review triage prerequisites' }));
  const project = within(await screen.findByRole('region', { name: 'Project analyst' }));
  const analyst = within(screen.getByRole('region', { name: 'Systems analyst' }));
  await userEvent.click(await project.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await project.findByRole('button', { name: 'Reviewed reader · v1' }));
  expect(await project.findByText('agent-private · project/issues-read')).toBeTruthy();
  expect(project.getByText('issues-repository · v1')).toBeTruthy();
  expect(project.getByText('Read-only MCP access')).toBeTruthy();
  expect(analyst.queryByText('agent-private · project/issues-read')).toBeNull();
  expect(
    screen.getByText(
      /AI-provider authentication and server Source\/Reporter access are independent/,
    ),
  ).toBeTruthy();
  expect(
    screen.getAllByRole('link', { name: 'Configure Runtime' })[0]?.getAttribute('href'),
  ).toContain('view=runtimes');
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  expect(api.resolvePreset).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      preset,
      profiles: { eligibility: { role: profile.role, profile: profile.reference } },
    }),
  );
  expect(api.createConfiguration).not.toHaveBeenCalled();
  expect(screen.queryByRole('textbox', { name: /json|digest|token|credential/i })).toBeNull();
});

it('pins independent code and stage documents through named choices and keeps them after unavailable resolution', async () => {
  const { api, profile, workSources } = fixture();
  await userEvent.click(await screen.findByRole('button', { name: 'Review triage prerequisites' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Code Repository' }));
  await userEvent.click(screen.getByRole('option', { name: 'Independent code · v3' }));
  const project = within(screen.getByRole('region', { name: 'Project analyst' }));
  await userEvent.click(await project.findByRole('combobox', { name: 'Project goals' }));
  await userEvent.click(screen.getByRole('option', { name: 'Reviewed Project goals' }));
  await userEvent.click(project.getByRole('combobox', { name: 'Project rules' }));
  await userEvent.click(screen.getByRole('option', { name: 'Reviewed Project rules' }));
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  expect(api.resolvePreset).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      repository: {
        id: 'code-repo',
        project_id: profile.role.project_id,
        version: 3,
        definition_digest: 'e'.repeat(64),
      },
      context: {
        eligibility: {
          project_id: profile.role.project_id,
          goals: { artifact_id: 'goals', content_digest: 'd'.repeat(64), encoded_size: 20 },
          rules: { artifact_id: 'rules', content_digest: 'd'.repeat(64), encoded_size: 20 },
          documents: {},
        },
      },
    }),
  );
  expect(project.getByRole('combobox', { name: 'Project goals' }).textContent).toContain(
    'Reviewed Project goals',
  );
  expect(workSources.listBuilds).not.toHaveBeenCalled();
  expect(workSources.listBuildArtifacts).not.toHaveBeenCalled();
  expect(
    screen.queryByRole('textbox', { name: /artifact|repository.*id|json|digest/i }),
  ).toBeNull();
});

it('attaches only an explicitly named supporting document to the selected stage', async () => {
  const { api, profile, documents, queryClient } = fixture();
  await userEvent.click(await screen.findByRole('button', { name: 'Review triage prerequisites' }));
  const project = within(screen.getByRole('region', { name: 'Project analyst' }));
  await userEvent.type(
    await project.findByRole('textbox', { name: 'Supporting document alias' }),
    'architecture',
  );
  await userEvent.click(project.getByRole('combobox', { name: 'Supporting document' }));
  await userEvent.click(screen.getByRole('option', { name: 'Reviewed architecture' }));
  await userEvent.click(project.getByRole('button', { name: 'Add supporting document' }));
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  expect(api.resolvePreset).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      context: {
        eligibility: {
          project_id: profile.role.project_id,
          goals: null,
          rules: null,
          documents: {
            architecture: {
              artifact_id: 'architecture',
              content_digest: 'd'.repeat(64),
              encoded_size: 20,
            },
          },
        },
      },
    }),
  );
  expect(
    within(screen.getByRole('region', { name: 'Systems analyst' })).queryByRole('button', {
      name: 'Remove document architecture',
    }),
  ).toBeNull();
  documents.list.mockResolvedValue({ items: [], next_cursor: null });
  await queryClient.invalidateQueries({
    queryKey: queryKeys.projectInputDocuments(profile.role.project_id),
  });
  await screen.findAllByText(/No retained Project documents/);
  await userEvent.click(project.getByRole('button', { name: 'Remove document architecture' }));
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  expect(api.resolvePreset).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      context: {
        eligibility: {
          project_id: profile.role.project_id,
          goals: null,
          rules: null,
          documents: {},
        },
      },
    }),
  );
});

it('publishes a first named Project document only after review and leaves each stage selection explicit', async () => {
  const { api, profile, documents, workSources } = fixture();
  documents.list.mockResolvedValue({ items: [], next_cursor: null });
  const published = {
    project_id: profile.role.project_id,
    name: 'First goals',
    artifact: { artifact_id: 'first-goals', content_digest: 'd'.repeat(64), encoded_size: 22 },
    published_at: 1,
    expires_at: Date.now() + 60_000,
  };
  documents.publish.mockImplementation(async () => {
    documents.list.mockResolvedValue({ items: [published], next_cursor: null });
    return published;
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Review triage prerequisites' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Document name' }), 'First goals');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Document text' }),
    'Exact reviewed goals\n',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review document publication' }));
  expect(documents.publish).not.toHaveBeenCalled();
  const dialog = within(screen.getByRole('dialog'));
  expect(dialog.getByText('First goals')).toBeTruthy();
  await userEvent.click(dialog.getByRole('button', { name: 'Publish document' }));
  expect(documents.publish).toHaveBeenCalledWith(
    profile.role.project_id,
    { name: 'First goals', content: 'Exact reviewed goals\n' },
    expect.objectContaining({ 'Idempotency-Key': expect.any(String) }),
  );
  await userEvent.click(dialog.getByRole('button', { name: 'Close' }));
  const project = within(screen.getByRole('region', { name: 'Project analyst' }));
  const analyst = within(screen.getByRole('region', { name: 'Systems analyst' }));
  expect(project.getByRole('combobox', { name: 'Project goals' }).textContent).toContain('None');
  expect(analyst.getByRole('combobox', { name: 'Project goals' }).textContent).toContain('None');
  await userEvent.click(project.getByRole('combobox', { name: 'Project goals' }));
  await userEvent.click(screen.getByRole('option', { name: 'First goals' }));
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  expect(api.resolvePreset).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      context: {
        eligibility: {
          project_id: profile.role.project_id,
          goals: published.artifact,
          rules: null,
          documents: {},
        },
      },
    }),
  );
  expect(workSources.listBuilds).not.toHaveBeenCalled();
  expect(api.createConfiguration).not.toHaveBeenCalled();
});

it('reviews installed policy choices and explicit budgets without enabling publication or intake', async () => {
  const { api } = fixture();
  await userEvent.click(await screen.findByRole('button', { name: 'Review triage prerequisites' }));
  await userEvent.click(await screen.findByRole('combobox', { name: 'Admission policy' }));
  await userEvent.click(screen.getByRole('option', { name: 'reviewed-admission · v1' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Permission ceiling' }));
  await userEvent.click(screen.getByRole('option', { name: 'reviewed-permissions · v1' }));
  const tokens = screen.getByRole('spinbutton', { name: 'Flow execution budget · Maximum tokens' });
  await userEvent.clear(tokens);
  await userEvent.type(tokens, '500');
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  expect(api.resolvePreset).toHaveBeenLastCalledWith(
    expect.any(String),
    expect.objectContaining({
      flow_id: expect.any(String),
      configuration_id: expect.any(String),
      execution: expect.objectContaining({ budget: expect.objectContaining({ max_tokens: 500 }) }),
      configuration: expect.objectContaining({
        admission_policy: 'reviewed-admission',
        permission_ceiling: 'reviewed-permissions',
      }),
    }),
  );
  expect(api.createConfiguration).not.toHaveBeenCalled();
});

it('publishes reviewed ordinary Flow and disabled Configuration separately and resumes Source setup after partial publication', async () => {
  const { api } = fixture();
  const { reference: _configurationReference, ...configuration } =
    configurationFixture().configuration;
  void _configurationReference;
  const { reference: _reference, ...root } = configuration.flow.closure.definitions[0]!;
  void _reference;
  const draft = {
    root,
    data_schemas: configuration.flow.data_schemas,
    limits: configuration.flow.limits,
  };
  await userEvent.click(await screen.findByRole('button', { name: 'Review triage prerequisites' }));
  const initial = api.resolvePreset.mock.results[0]!.value;
  api.resolvePreset.mockResolvedValue({
    ...(await initial),
    diagnostics: [],
    flow: draft,
    configuration: {
      ...configuration,
      admission_policy: 'reviewed-admission',
      permission_ceiling: 'reviewed-permissions',
      enabled: false,
    },
  });
  await userEvent.click(await screen.findByRole('combobox', { name: 'Code Repository' }));
  await userEvent.click(screen.getByRole('option', { name: 'Independent code · v3' }));
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  expect(await screen.findByRole('heading', { name: 'Flow graph editor' })).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Review publication' }));
  expect(api.createFlow).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Confirm publish' }));
  expect(await screen.findByText('Flow published. Continue with the Configuration.')).toBeTruthy();
  expect(api.createFlow).toHaveBeenCalledOnce();
  expect(api.createConfiguration).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(screen.getByRole('button', { name: 'Confirm publish' }));
  expect(await screen.findByRole('button', { name: 'Configure automatic intake' })).toBeTruthy();
  expect(api.createConfiguration.mock.calls[0]![0].definition.enabled).toBe(false);
  expect(screen.getByRole('link', { name: 'Task sources' }).getAttribute('href')).toContain(
    '/projects/',
  );
  expect(api.readiness).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Check execution readiness' }));
  expect(await screen.findByText('MCP evidence capture is not qualified.')).toBeTruthy();
  expect(screen.getByText('No compatible Agent capacity is available.')).toBeTruthy();
  expect(screen.getByText('Credential authentication has not been verified.')).toBeTruthy();
  expect(screen.getByText('Native writes remain unverified.')).toBeTruthy();
});

it('does not disclose MCP metadata from a different Build configuration sharing the same toolchain', async () => {
  const { roles, profile, choice } = fixture();
  roles.listCapabilities.mockResolvedValue(
    page([
      {
        ...choice,
        build_configuration: { ...profile.definition.build_configuration, id: 'another-build' },
      },
    ]),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Review triage prerequisites' }));
  const stage = within(await screen.findByRole('region', { name: 'Project analyst' }));
  await userEvent.click(await stage.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await stage.findByRole('button', { name: 'Reviewed reader · v1' }));
  expect(await stage.findByText(/Review the selected MCP endpoint/)).toBeTruthy();
  expect(stage.queryByText('agent-private · project/issues-read')).toBeNull();
  expect(roles.getCapability).not.toHaveBeenCalled();
});

it('composes the exact prepared Source before publication while keeping import, preview and intake independent', async () => {
  const { api, sources, read, connection } = fixture();
  const { reference: _ref, ...configuration } = configurationFixture().configuration;
  void _ref;
  const { reference: _flowRef, ...root } = configuration.flow.closure.definitions[0]!;
  void _flowRef;
  await userEvent.click(await screen.findByRole('button', { name: 'Review triage prerequisites' }));
  const initial = await api.resolvePreset.mock.results[0]!.value;
  api.resolvePreset.mockResolvedValue({
    ...initial,
    diagnostics: [],
    flow: {
      root,
      data_schemas: configuration.flow.data_schemas,
      limits: configuration.flow.limits,
    },
    configuration: {
      ...configuration,
      admission_policy: 'reviewed-admission',
      permission_ceiling: 'reviewed-permissions',
      enabled: false,
    },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Review Source selections' }));
  const editor = within(await screen.findByRole('region', { name: 'Triage Source selection' }));
  for (const [label, option] of [
    ['Source connection', connection.definition.name],
    ['Task container', read.locator],
    ['Selector and input mapping', `${read.binding.configuration.identity} · v1`],
    ['Code Repository', 'Independent code · v3'],
  ] as const) {
    await userEvent.click(await editor.findByRole('combobox', { name: label }));
    await userEvent.click(screen.getByRole('option', { name: option }));
  }
  expect(editor.getByText('Installed frozen criteria')).toBeTruthy();
  await userEvent.click(editor.getByRole('button', { name: 'Use reviewed Source selection' }));
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  expect(api.resolvePreset).toHaveBeenLastCalledWith(
    expect.any(String),
    expect.objectContaining({
      input: expect.objectContaining({
        source: read.binding,
        reporter: null,
        connection: expect.objectContaining({ id: connection.id, version: connection.version }),
      }),
    }),
  );
  expect(screen.getByRole('region', { name: 'Accepted outcome effects' })).toBeTruthy();
  expect(sources.preview).not.toHaveBeenCalled();
  expect(sources.publish).not.toHaveBeenCalled();
  expect(api.createFlow).not.toHaveBeenCalled();
  expect(api.createConfiguration).not.toHaveBeenCalled();
  for (let step = 0; step < 2; step++) {
    await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
    await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
    await userEvent.click(screen.getByRole('button', { name: 'Confirm publish' }));
  }
  await userEvent.click(await screen.findByRole('button', { name: 'Configure automatic intake' }));
  const intake = within(await screen.findByRole('region', { name: 'Automatic intake' }));
  expect(
    (await intake.findByRole('combobox', { name: 'Selector and input mapping' })).textContent,
  ).toContain(read.binding.configuration.identity);
  expect(intake.getByRole('checkbox', { name: 'Automatic intake enabled' })).toHaveProperty(
    'checked',
    false,
  );
  expect(sources.publish).not.toHaveBeenCalled();
});

it('preserves an edited Flow when an exact parent becomes unavailable and blocks publication until reviewed again', async () => {
  const { api } = fixture();
  const { reference: _ref, ...configuration } = configurationFixture().configuration;
  void _ref;
  const { reference: _flowRef, ...root } = configuration.flow.closure.definitions[0]!;
  void _flowRef;
  await userEvent.click(await screen.findByRole('button', { name: 'Review triage prerequisites' }));
  const initial = await api.resolvePreset.mock.results[0]!.value;
  api.resolvePreset.mockResolvedValue({
    ...initial,
    diagnostics: [],
    flow: {
      root,
      data_schemas: configuration.flow.data_schemas,
      limits: configuration.flow.limits,
    },
    configuration: {
      ...configuration,
      admission_policy: 'reviewed-admission',
      permission_ceiling: 'reviewed-permissions',
      enabled: false,
    },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  const tokens = await screen.findByRole('spinbutton', { name: 'Node budget · Maximum tokens' });
  await userEvent.clear(tokens);
  await userEvent.type(tokens, '777');
  api.resolvePreset.mockResolvedValue({
    ...initial,
    diagnostics: [{ path: '/profiles/eligibility/build', reason: 'unavailable' }],
  });
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  expect(
    await screen.findByRole('spinbutton', { name: 'Node budget · Maximum tokens' }),
  ).toHaveProperty('value', '777');
  expect(screen.getByRole('button', { name: 'Validate draft' })).toHaveProperty('disabled', true);
  expect(
    screen.getByText('The retained draft needs a new prerequisite review before publication.'),
  ).toBeTruthy();
  expect(api.createFlow).not.toHaveBeenCalled();
});

it('explains the required process-completion mapping when an installed Source uses authored acceptance', async () => {
  const { api, sources, read, connection } = fixture();
  const installed = integrationFixture('WorkSourceInputChoices');
  sources.choices.mockResolvedValue({
    ...installed,
    items: [{ ...read, preparation: { kind: 'body' } }],
  });
  const { reference: _ref, ...configuration } = configurationFixture().configuration;
  void _ref;
  const { reference: _flowRef, ...root } = configuration.flow.closure.definitions[0]!;
  void _flowRef;
  await userEvent.click(await screen.findByRole('button', { name: 'Review triage prerequisites' }));
  const initial = await api.resolvePreset.mock.results[0]!.value;
  api.resolvePreset.mockResolvedValue({
    ...initial,
    diagnostics: [],
    flow: {
      root,
      data_schemas: configuration.flow.data_schemas,
      limits: configuration.flow.limits,
    },
    configuration: {
      ...configuration,
      admission_policy: 'reviewed-admission',
      permission_ceiling: 'reviewed-permissions',
      enabled: false,
    },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Resolve reviewed triage' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Review Source selections' }));
  const editor = within(await screen.findByRole('region', { name: 'Triage Source selection' }));
  for (const [label, option] of [
    ['Source connection', connection.definition.name],
    ['Task container', read.locator],
    ['Selector and input mapping', `${read.binding.configuration.identity} · v1`],
    ['Code Repository', 'Independent code · v3'],
  ] as const) {
    await userEvent.click(await editor.findByRole('combobox', { name: label }));
    await userEvent.click(screen.getByRole('option', { name: option }));
  }
  await userEvent.click(editor.getByRole('button', { name: 'Use reviewed Source selection' }));
  expect(await editor.findByRole('alert')).toHaveProperty(
    'textContent',
    'Select an installed Source with explicit process-completion criteria. Task-authored acceptance cannot be used as an implicit triage fallback.',
  );
  expect(api.resolvePreset).toHaveBeenCalledTimes(2);
  expect(sources.publish).not.toHaveBeenCalled();
});
