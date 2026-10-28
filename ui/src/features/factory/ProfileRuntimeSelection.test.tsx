// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { ManagementApiError } from '../../api/client';
import { aiSettingsPath } from '../../app/routes';
import { factoryRoleFixture } from '../../../tests/fixtures/factoryRoles';
import { project as projectFixture } from '../projects/ProjectTestSupport';
import { AiSettings } from './AiSettings';
import { factoryRolesApi } from './roleApi';
import { factoryRuntimeApi } from './runtimeApi';

afterEach(cleanup);
const page = <T,>(items: T[]) => ({ items, total: items.length, next_cursor: null });
function setup() {
  const { role, profile, capability, choice } = factoryRoleFixture();
  const catalog = { available: true, manual_model: true, models: [] };
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
    getRole: vi.fn(async () => role),
    listTemplates: vi.fn(async () => page([])),
    listSkills: vi.fn(async () => page([])),
    listProfiles: vi.fn(async () =>
      page([
        {
          reference: profile.reference,
          role: role.reference,
          name: 'default',
          toolchain: profile.definition.toolchain,
          template: null,
        },
      ]),
    ),
    getProfile: vi.fn(async () => profile),
    listCapabilities: vi.fn(async () => page([choice])),
    getCapability: vi.fn(async () => ({ ...capability, model_catalog: catalog })),
    validateProfile: vi.fn(
      async (_project: string, request: Parameters<typeof factoryRolesApi.validateProfile>[1]) => ({
        profile: { ...profile, definition: request.definition },
        incompatibility: null,
      }),
    ),
    publish: vi.fn(),
  };
  const runtime = { ...factoryRuntimeApi, choices: vi.fn(async () => page([])) };
  const router = createMemoryRouter(
    [
      {
        path: '/ai-settings',
        element: (
          <AiSettings
            projects={{
              listProjects: vi.fn(async () => page([])),
              getProject: vi.fn(async () => ({
                project: projectFixture(role.reference.project_id, 'Delivery'),
                ancestors: [],
              })),
            }}
            roles={roles}
            runtime={runtime}
            integrations={{
              listConnections: vi.fn(async () => page([])),
              getProjectAccess: vi.fn(),
              listProjectResources: vi.fn(),
            }}
          />
        ),
      },
    ],
    { initialEntries: [aiSettingsPath(role.reference.project_id)] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { roles, runtime, profile, capability, catalog, router };
}
async function edit() {
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit profile' }));
  return screen.findByRole('region', { name: 'Profile editor' });
}

it('returns from Runtime setup with its profile draft intact and adopts refreshed bindings only by selection', async () => {
  const { roles, capability, catalog, profile, router } = setup();
  const editor = within(await edit());
  await userEvent.clear(editor.getByRole('textbox', { name: 'Profile name' }));
  await userEvent.type(editor.getByRole('textbox', { name: 'Profile name' }), 'My pro');
  await userEvent.clear(editor.getByRole('textbox', { name: 'Model identifier' }));
  await userEvent.type(editor.getByRole('textbox', { name: 'Model identifier' }), 'Vendor/future');
  await userEvent.clear(editor.getByRole('textbox', { name: 'Additional instruction 1' }));
  await userEvent.type(
    editor.getByRole('textbox', { name: 'Additional instruction 1' }),
    'Unsaved instruction',
  );
  await userEvent.click(editor.getByRole('link', { name: 'Configure Runtime' }));
  expect(router.state.location.search).toContain('view=runtimes');
  const mcp = {
    configuration: { identity: 'reviewer', version: 'v2', digest: 'a'.repeat(64) },
    integration: null,
  };
  roles.getCapability.mockResolvedValue({
    ...capability,
    model_catalog: catalog,
    mcp_configurations: {
      reviewer: {
        reference: mcp.configuration,
        server: {
          transport: 'http',
          url: 'https://mcp.example/tools',
          bearer_token_env_var: 'ISSUE_TOKEN',
        },
        authorization: {
          credential_profile: 'mcp-read',
          secret: { provider: 'agent-private', reference: 'project/issues-read' },
          environment: 'ISSUE_TOKEN',
          purpose: 'mcp_read',
          resource: { identity: 'issues-repository', version: 'v1', digest: 'c'.repeat(64) },
        },
      },
    },
    definition: {
      ...capability.definition,
      permissions: { ...capability.definition.permissions, network_hosts: ['mcp.example'] },
      execution: { ...capability.definition.execution, mcp_servers: { reviewer: mcp } },
    },
  });
  const handoff = screen.getByRole('link', { name: 'Choose model and reasoning' });
  expect(handoff.getAttribute('href')).toBe(aiSettingsPath(profile.role.project_id, 'agents'));
  expect(screen.getByText(/Choose an AI agent and edit or create its profile/)).toBeTruthy();
  await userEvent.click(handoff);
  const reviewer = await editor.findByRole('checkbox', { name: 'reviewer' });
  expect(editor.getByText('agent-private · project/issues-read')).toBeTruthy();
  expect(editor.getByText('issues-repository · v1')).toBeTruthy();
  expect(reviewer).toHaveProperty('checked', false);
  const host = editor.getByRole('checkbox', { name: 'Network host · mcp.example' });
  expect(host).toHaveProperty('checked', false);
  expect(editor.getByRole('textbox', { name: 'Profile name' })).toHaveProperty('value', 'My pro');
  expect(editor.getByRole('textbox', { name: 'Model identifier' })).toHaveProperty(
    'value',
    'Vendor/future',
  );
  expect(editor.getByRole('textbox', { name: 'Additional instruction 1' })).toHaveProperty(
    'value',
    'Unsaved instruction',
  );
  expect(roles.publish).not.toHaveBeenCalled();
  await userEvent.click(editor.getByRole('button', { name: 'Review profile' }));
  expect(roles.validateProfile).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      definition: expect.objectContaining({
        execution: profile.definition.execution,
        permissions: profile.definition.permissions,
      }),
    }),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Abandon' }));
  await userEvent.click(reviewer);
  await userEvent.click(host);
  await userEvent.click(editor.getByRole('button', { name: 'Review profile' }));
  await waitFor(() =>
    expect(roles.validateProfile).toHaveBeenLastCalledWith(
      profile.role.project_id,
      expect.objectContaining({
        definition: expect.objectContaining({
          permissions: { ...profile.definition.permissions, network_hosts: ['mcp.example'] },
          name: 'My pro',
          model_identifier: 'Vendor/future',
          instructions: ['Unsaved instruction'],
          execution: expect.objectContaining({ mcp_servers: { reviewer: mcp } }),
        }),
      }),
    ),
  );
  expect(roles.publish).not.toHaveBeenCalled();
});

it('shows retained unavailable bindings and requires explicit replacement of a changed exact MCP pin', async () => {
  const { roles, profile, capability, catalog } = setup();
  const old = {
    configuration: { identity: 'reviewer', version: 'v1', digest: 'a'.repeat(64) },
    integration: null,
  };
  const current = {
    configuration: { identity: 'reviewer', version: 'v2', digest: 'b'.repeat(64) },
    integration: null,
  };
  roles.getProfile.mockResolvedValue({
    ...profile,
    definition: {
      ...profile.definition,
      execution: { ...profile.definition.execution, mcp_servers: { reviewer: old } },
    },
  });
  roles.getCapability.mockResolvedValue({
    ...capability,
    model_catalog: catalog,
    definition: {
      ...capability.definition,
      execution: { ...capability.definition.execution, mcp_servers: { reviewer: current } },
    },
  });
  const editor = within(await edit());
  expect(await editor.findByRole('checkbox', { name: 'reviewer' })).toHaveProperty(
    'checked',
    false,
  );
  expect(
    editor.getByText(
      'reviewer · Retained selection is no longer available. Remove it or explicitly select its replacement.',
    ),
  ).toBeTruthy();
  await userEvent.click(editor.getByRole('button', { name: 'Review profile' }));
  expect(roles.validateProfile).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      definition: expect.objectContaining({
        execution: expect.objectContaining({ mcp_servers: { reviewer: old } }),
      }),
    }),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Abandon' }));
  await userEvent.click(editor.getByRole('checkbox', { name: 'reviewer' }));
  expect(editor.queryByText(/reviewer · Retained selection/)).toBeNull();
  await userEvent.click(editor.getByRole('button', { name: 'Review profile' }));
  expect(roles.validateProfile).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      definition: expect.objectContaining({
        execution: expect.objectContaining({ mcp_servers: { reviewer: current } }),
      }),
    }),
  );
});

it('offers setup guidance instead of empty permission frames or editable deny-all limits', async () => {
  setup();
  const editor = within(await edit());
  expect(
    await editor.findByText(
      'This Runtime has no configurable permissions. Configure its allowed capabilities before selecting them in a profile.',
    ),
  ).toBeTruthy();
  expect(editor.queryByRole('group', { name: 'Requested node permissions' })).toBeNull();
  expect(editor.queryByRole('spinbutton', { name: /Requested node permissions/ })).toBeNull();
  expect(editor.getByRole('link', { name: 'Configure Runtime' })).toBeTruthy();
});

it('explains an unselected MCP integration dependency and retains revoked selections until explicit removal', async () => {
  const { roles, profile, capability, catalog } = setup();
  const ref = (identity: string) => ({ identity, version: 'v1', digest: 'a'.repeat(64) });
  const source = {
    connection_id: '71000000-0000-4000-8000-000000000001',
    connection_version: 3,
    project_id: profile.role.project_id,
    access_version: 2,
    resource: ref('repository'),
    capability: ref('repository-read'),
  };
  const reviewer = { configuration: ref('reviewer'), integration: 'source' };
  roles.getCapability.mockResolvedValue({
    ...capability,
    model_catalog: catalog,
    definition: {
      ...capability.definition,
      execution: {
        ...capability.definition.execution,
        mcp_servers: { reviewer },
        integrations: { source },
      },
    },
  });
  const editor = within(await edit());
  await userEvent.click(await editor.findByRole('checkbox', { name: /reviewer/ }));
  expect(
    editor.getByText(
      'reviewer · Select integration source separately. MCP does not grant integration access.',
    ),
  ).toBeTruthy();
  await userEvent.click(editor.getByRole('checkbox', { name: 'source' }));
  expect(editor.queryByText(/Select integration source separately/)).toBeNull();
  await userEvent.click(editor.getByRole('link', { name: 'Configure Runtime' }));
  roles.getCapability.mockResolvedValue({ ...capability, model_catalog: catalog });
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await editor.findByRole('button', { name: 'Remove retained source' });
  await userEvent.click(editor.getByRole('button', { name: 'Remove retained source' }));
  await userEvent.click(editor.getByRole('button', { name: 'Review profile' }));
  expect(roles.validateProfile).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      definition: expect.objectContaining({
        execution: expect.objectContaining({ mcp_servers: { reviewer }, integrations: {} }),
      }),
    }),
  );
  expect(editor.getByRole('button', { name: 'Remove retained reviewer' })).toBeTruthy();
});

it.each(['forbidden', 'capability_unavailable'] as const)(
  'keeps %s distinct from empty choices without resetting the profile',
  async (code) => {
    const { roles } = setup();
    const editor = within(await edit());
    await editor.findByText(
      'This Runtime has no configurable permissions. Configure its allowed capabilities before selecting them in a profile.',
    );
    await userEvent.click(editor.getByRole('link', { name: 'Configure Runtime' }));
    roles.getCapability.mockRejectedValue(
      new ManagementApiError({
        code,
        message: 'Runtime choices cannot be read',
        requestId: null,
        status: code === 'forbidden' ? 403 : 422,
        retryAfterMilliseconds: null,
      }),
    );
    await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
    await editor.findByRole('button', { name: 'Retry' });
    expect(editor.queryByText(/No MCP servers are configured/)).toBeNull();
    expect(editor.getByRole('textbox', { name: 'Profile name' })).toHaveProperty(
      'value',
      'default',
    );
    expect(editor.getByRole('button', { name: 'Review profile' })).toHaveProperty('disabled', true);
    expect(roles.validateProfile).not.toHaveBeenCalled();
  },
);

it('suspends a hidden profile confirmation while preserving its draft and reviewed intent', async () => {
  const { roles, profile, router } = setup();
  await act(() => router.navigate(aiSettingsPath(profile.role.project_id, 'runtimes')));
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  const editor = within(await edit());
  await userEvent.clear(editor.getByRole('textbox', { name: 'Profile name' }));
  await userEvent.type(editor.getByRole('textbox', { name: 'Profile name' }), 'Unsaved pro');
  await userEvent.click(editor.getByRole('button', { name: 'Review profile' }));
  await screen.findByRole('dialog', { name: 'Review profile' });
  await act(() => router.navigate(-1));
  expect(screen.queryByRole('dialog')).toBeNull();
  screen.getByRole('button', { name: 'AI agents' }).focus();
  for (const key of ['Tab', 'Escape']) {
    const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
    act(() => {
      window.dispatchEvent(event);
    });
    expect(event.defaultPrevented).toBe(false);
  }
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  expect(screen.getByRole('dialog', { name: 'Review profile' })).toBeTruthy();
  expect(roles.publish).not.toHaveBeenCalled();
  await userEvent.keyboard('{Escape}');
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(editor.getByRole('textbox', { name: 'Profile name' })).toHaveProperty(
    'value',
    'Unsaved pro',
  );
});

it('retains revoked permission selections for individual removal without resetting the authored profile', async () => {
  const { roles, profile, capability, catalog } = setup();
  const ref = (identity: string) => ({ identity, version: 'v1', digest: 'a'.repeat(64) });
  const executable = ref('retained-executable');
  const permissions: typeof profile.definition.permissions = {
    ...profile.definition.permissions,
    plugins: [ref('retained-plugin')],
    executables: [executable],
    tools: [ref('retained-tool')],
    secret_profiles: ['retained-secret'],
    workload_identity_profiles: ['retained-identity'],
    network_hosts: ['retained.example'],
    mounts: [{ root: 'workspace', mode: 'read_only' }],
    commands: [{ executable, arguments: [{ kind: 'exact', value: '--old' }] }],
    outputs: { ...profile.definition.permissions.outputs, kinds: ['report'] },
  };
  roles.getProfile.mockResolvedValue({
    ...profile,
    definition: { ...profile.definition, permissions },
  });
  roles.getCapability.mockResolvedValue({
    ...capability,
    model_catalog: catalog,
    definition: { ...capability.definition, permissions },
  });
  const editor = within(await edit());
  await editor.findByRole('checkbox', { name: 'Tool · retained-tool · v1' });
  await userEvent.clear(editor.getByRole('textbox', { name: 'Profile name' }));
  await userEvent.type(editor.getByRole('textbox', { name: 'Profile name' }), 'Unsaved pro');
  await userEvent.clear(editor.getByRole('textbox', { name: 'Model identifier' }));
  await userEvent.type(editor.getByRole('textbox', { name: 'Model identifier' }), 'Vendor/future');
  await userEvent.click(editor.getByRole('link', { name: 'Configure Runtime' }));
  const replacement = { identity: 'retained-tool', version: 'v2', digest: 'b'.repeat(64) };
  roles.getCapability.mockResolvedValue({
    ...capability,
    model_catalog: catalog,
    definition: {
      ...capability.definition,
      permissions: { ...profile.definition.permissions, tools: [replacement] },
    },
  });
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  const current = await editor.findByRole('checkbox', { name: 'Tool · retained-tool · v2' });
  expect(current).toHaveProperty('checked', false);
  expect(editor.getByRole('checkbox', { name: /Tool · retained-tool · v1/ })).toHaveProperty(
    'checked',
    true,
  );
  const selections = within(editor.getByRole('group', { name: 'Requested node permissions' }));
  expect(selections.getAllByRole('checkbox', { checked: true })).toHaveLength(9);
  expect(
    selections.getAllByText(/No longer available; remove this retained permission\./),
  ).toHaveLength(9);
  for (const selected of selections.getAllByRole('checkbox', { checked: true }))
    await userEvent.click(selected);
  expect(selections.queryAllByRole('checkbox', { checked: true })).toHaveLength(0);
  expect(selections.queryByRole('checkbox', { name: /retained-tool · v1/ })).toBeNull();
  await userEvent.click(current);
  await userEvent.click(editor.getByRole('button', { name: 'Review profile' }));
  expect(roles.validateProfile).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      definition: expect.objectContaining({
        name: 'Unsaved pro',
        model_identifier: 'Vendor/future',
        model_parameters: profile.definition.model_parameters,
        instructions: profile.definition.instructions,
        permissions: { ...profile.definition.permissions, tools: [replacement] },
      }),
    }),
  );
  expect(roles.publish).not.toHaveBeenCalled();
});
