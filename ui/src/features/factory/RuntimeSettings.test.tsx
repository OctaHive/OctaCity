// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { factoryRoleFixture } from '../../../tests/fixtures/factoryRoles';
import { createConsoleQueryClient } from '../../app/query';
import { project } from '../projects/ProjectTestSupport';
import { AiSettings } from './AiSettings';
import { factoryRolesApi } from './roleApi';
import { factoryRuntimeApi } from './runtimeApi';
import { integrationsApi } from '../integrations/api';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { ManagementApiError } from '../../api/client';

afterEach(cleanup);
type Schema = components['schemas'];
function setup(
  transports: Schema['FactoryRuntimeSettingsSupport']['mcp_transports'] = ['http', 'stdio'],
) {
  const { profile } = factoryRoleFixture();
  const projectId = profile.role.project_id;
  const ceiling = structuredClone(profile.definition.permissions);
  ceiling.network_hosts = ['mcp.example'];
  ceiling.resources.cpu_millis = 650;
  ceiling.executables = [profile.definition.toolchain.executable];
  ceiling.commands = [
    {
      executable: profile.definition.toolchain.executable,
      arguments: [{ kind: 'exact', value: 'value with spaces' }],
    },
  ];
  const choice: Schema['FactoryRuntimeChoice'] = {
    alias: 'installed-runtime',
    toolchain: profile.definition.toolchain,
    permissions: ceiling,
    enforcement: [
      'plugin_identity',
      'executable_identity',
      'tool_identity',
      'command_arguments',
      'descendant_processes',
      'filesystem_paths',
      'mount_modes',
      'network_hosts',
      'secret_profiles',
      'workload_identity_profiles',
      'cpu',
      'memory',
      'disk',
      'process_count',
      'elapsed_time',
      'outputs',
    ],
    environment_names: ['LANGUAGE', 'ACCESS_TOKEN'],
    secret_environment_names: ['ACCESS_TOKEN'],
    support: {
      mcp_transports: transports,
      mcp_executables: { '/runtime/reader': profile.definition.toolchain.executable },
    },
  };
  const api = {
    ...factoryRuntimeApi,
    choices: vi.fn(async () => ({ items: [choice], next_cursor: null, total: 1 })),
    current: vi.fn(async (): Promise<Schema['FactoryRuntimeSettingsRead']> => ({ settings: null })),
    validate: vi.fn(
      async (_project: string, _alias: string, body: Schema['FactoryRuntimeSettingsRequest']) => ({
        settings: {
          project_id: projectId,
          runtime: choice.alias,
          version: (body.expected_version ?? 0) + 1,
          definition: structuredClone(body.definition),
          digest: 'b'.repeat(64),
        },
      }),
    ),
    publish: vi.fn(
      async (_project: string, _alias: string, body: Schema['FactoryRuntimeSettingsRequest']) => ({
        disposition: 'applied' as const,
        published_at: 100,
        settings: {
          project_id: projectId,
          runtime: choice.alias,
          version: (body.expected_version ?? 0) + 1,
          definition: structuredClone(body.definition),
          digest: 'b'.repeat(64),
        },
      }),
    ),
  };
  const projects = {
    listProjects: vi.fn(async () => ({
      items: [project(projectId, 'Delivery')],
      next_cursor: null,
    })),
    getProject: vi.fn(async () => ({ project: project(projectId, 'Delivery'), ancestors: [] })),
  };
  const roles = {
    ...factoryRolesApi,
    listRoles: vi.fn(async () => ({ items: [], next_cursor: null, total: 0 })),
    listTemplates: vi.fn(async () => ({ items: [], next_cursor: null, total: 0 })),
  };
  const connection = integrationFixture('IntegrationConnection');
  const access = {
    ...integrationFixture('IntegrationProjectAccess'),
    project_id: projectId,
    connection_id: connection.id,
  };
  const resources: Schema['IntegrationProjectResources'] = {
    resources: connection.definition.resources.map((resource) => ({
      ...resource,
      capabilities: resource.capabilities.map(({ capability, configuration, purpose }) => ({
        capability,
        configuration,
        purpose,
      })),
    })),
  };
  const integrations = {
    ...integrationsApi,
    publish: vi.fn(),
    listConnections: vi.fn(async () => ({ items: [connection], next_cursor: null, total: 1 })),
    getProjectAccess: vi.fn(async () => ({ access })),
    listProjectResources: vi.fn(async () => resources),
  };
  const router = createMemoryRouter(
    [
      {
        path: '*',
        element: (
          <AiSettings projects={projects} roles={roles} runtime={api} integrations={integrations} />
        ),
      },
    ],
    { initialEntries: [`/ai-settings?project_id=${projectId}&view=runtimes`] },
  );
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { api, choice, projectId, router, integrations, connection, access, resources };
}

it('explains when installation support prevents configuring MCP', async () => {
  setup([]);
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  expect(await screen.findByRole('button', { name: 'Add MCP server' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(
    screen.getByText(
      'This Runtime installation has no qualified MCP transports. An administrator must configure installation support first.',
    ),
  ).toBeTruthy();
  expect(screen.queryByRole('textbox', { name: 'MCP name' })).toBeNull();
});

it('gives installation steps for metadata-only Runtime without hiding profile authoring', async () => {
  const { choice, api } = setup([]);
  choice.permissions.plugins = [];
  choice.permissions.executables = [];
  choice.permissions.commands = [];
  choice.permissions.network_hosts = [];
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  await userEvent.click(screen.getByText('Installation prerequisites', { selector: 'summary' }));
  expect(screen.getByText(/--factory-profiles/)).toBeTruthy();
  expect(screen.getByText(/--factory-agent-profile/)).toBeTruthy();
  expect(screen.getByText(/No plugin execution permission is installed/)).toBeTruthy();
  expect(screen.getByText(/No network hosts are approved/)).toBeTruthy();
  expect(screen.getByText(/native MCP call\/result capture/)).toBeTruthy();
  expect(screen.getByRole('link', { name: 'Choose model and reasoning' })).toBeTruthy();
  expect(api.validate).not.toHaveBeenCalled();
  expect(api.publish).not.toHaveBeenCalled();
});

it('explicitly selects installed MCP read authority and publishes references instead of token values', async () => {
  const { api, choice } = setup();
  const authorization: Schema['FactoryMcpAuthorization'] = {
    credential_profile: 'mcp-issues-read',
    secret: { provider: 'agent-private', reference: 'project/issues-read' },
    environment: 'ACCESS_TOKEN',
    purpose: 'mcp_read',
    resource: { identity: 'issues-repository', version: 'v1', digest: 'a'.repeat(64) },
  };
  const server: Schema['FactoryMcpServer'] = {
    transport: 'http',
    url: 'https://mcp.example/tools',
    bearer_token_env_var: 'ACCESS_TOKEN',
    enabled_tools: ['read'],
  };
  choice.support.authenticated_mcp = {
    reader: {
      reference: { identity: 'reader', version: 'v1', digest: 'b'.repeat(64) },
      server,
      authorization,
    },
  };
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add MCP server' }));
  const select = screen.getByRole('combobox', { name: 'Installed MCP connection' });
  await userEvent.click(select);
  await userEvent.click(screen.getByRole('option', { name: 'reader' }));
  expect(screen.getByRole('textbox', { name: 'MCP name' })).toHaveProperty('value', 'reader');
  expect(screen.getByText('agent-private · project/issues-read')).toBeTruthy();
  expect(screen.getByText('mcp-issues-read')).toBeTruthy();
  expect(screen.getByText('issues-repository · v1')).toBeTruthy();
  expect(screen.getByText('Read-only MCP access')).toBeTruthy();
  expect(screen.getByText(/AI login, MCP read access and server Reporter writes/)).toBeTruthy();
  expect(screen.queryByRole('textbox', { name: /token value/i })).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Review Runtime settings' }));
  await screen.findByRole('dialog', { name: 'Review Runtime settings' });
  expect(api.validate.mock.calls[0]?.[2].definition.mcp_servers.reader).toEqual({
    server,
    authorization,
    integration: null,
  });
  expect(api.publish).not.toHaveBeenCalled();
});

it.each(['clear requirement', 'replace requirement', 'remove binding'])(
  'blocks installed MCP review when its required integration changes: %s',
  async (change) => {
    const { api, choice, projectId, connection, access, resources } = setup();
    const resource = resources.resources[0]!;
    const integration: Schema['FactoryRoleIntegrationBinding'] = {
      project_id: projectId,
      connection_id: connection.id,
      connection_version: access.connection_version,
      access_version: access.version,
      resource: resource.reference,
      capability: resource.capabilities[0]!.capability,
    };
    const server: Schema['FactoryMcpServer'] = {
      transport: 'http',
      url: 'https://mcp.example/read',
      bearer_token_env_var: 'ACCESS_TOKEN',
    };
    const authorization: Schema['FactoryMcpAuthorization'] = {
      credential_profile: 'mcp-read',
      secret: { provider: 'agent-private', reference: 'project/read' },
      environment: 'ACCESS_TOKEN',
      purpose: 'mcp_read',
      resource: resource.reference,
    };
    choice.support.authenticated_mcp = {
      reader: {
        reference: { identity: 'reader', version: 'v1', digest: 'b'.repeat(64) },
        server,
        authorization,
        integration,
      },
    };
    api.current.mockResolvedValueOnce({
      settings: {
        project_id: projectId,
        runtime: choice.alias,
        version: 1,
        digest: 'c'.repeat(64),
        definition: {
          toolchain: choice.toolchain,
          permissions: choice.permissions,
          integrations: {
            tracker: integration,
            other: { ...integration, access_version: integration.access_version + 1 },
          },
          mcp_servers: { reader: { server, authorization, integration: 'tracker' } },
        },
      },
    });
    await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
    const review = await screen.findByRole('button', { name: 'Review Runtime settings' });
    expect(review).toHaveProperty('disabled', false);
    if (change === 'remove binding') {
      await userEvent.click(screen.getByRole('button', { name: 'Remove integration tracker' }));
    } else {
      await userEvent.click(screen.getByRole('combobox', { name: 'Required integration' }));
      await userEvent.click(
        screen.getByRole('option', {
          name: change === 'clear requirement' ? 'None' : 'other',
        }),
      );
    }
    expect(review).toHaveProperty('disabled', true);
    expect(screen.getByRole('alert').textContent).toContain(
      'Restore the exact required Project integration binding before review',
    );
    expect(api.validate).not.toHaveBeenCalled();
    expect(api.publish).not.toHaveBeenCalled();
    if (change === 'remove binding') {
      await userEvent.type(screen.getByRole('textbox', { name: 'Integration alias' }), 'tracker');
      await userEvent.click(screen.getByRole('combobox', { name: 'Integration connection' }));
      await userEvent.click(
        await screen.findByRole('option', { name: connection.definition.name }),
      );
      await userEvent.click(
        await screen.findByRole('combobox', { name: 'Authorized resource and capability' }),
      );
      await userEvent.click(
        await screen.findByRole('option', { name: `${resource.locator} · Task reading` }),
      );
      await userEvent.click(screen.getByRole('button', { name: 'Add integration binding' }));
    } else {
      await userEvent.click(screen.getByRole('combobox', { name: 'Required integration' }));
      await userEvent.click(screen.getByRole('option', { name: 'tracker' }));
    }
    expect(review).toHaveProperty('disabled', false);
    expect(screen.queryByRole('alert')).toBeNull();
  },
);

it('blocks an unqualified MCP bearer mapping without borrowing model credentials', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add MCP server' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'MCP name' }), 'reader');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'HTTPS endpoint' }),
    'https://mcp.example/tools',
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Bearer environment name' }));
  await userEvent.click(screen.getByRole('option', { name: 'ACCESS_TOKEN' }));
  expect(screen.getByRole('alert').textContent).toContain(
    'No matching installed MCP authentication',
  );
  const review = screen.getByRole('button', { name: 'Review Runtime settings' });
  expect(review).toHaveProperty('disabled', true);
  await userEvent.click(review);
  expect(api.validate).not.toHaveBeenCalled();
  expect(api.publish).not.toHaveBeenCalled();
});

it('reviews and explicitly saves a supported named HTTP MCP through the shared REST owner', async () => {
  const { api, choice, projectId, router } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add MCP server' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'MCP name' }), 'reader');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'HTTPS endpoint' }),
    'https://mcp.example/tools',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review Runtime settings' }));
  const review = await screen.findByRole('dialog', { name: 'Review Runtime settings' });
  expect(
    within(review).getByText(/Configuration validation only; no MCP connection was tested/),
  ).toBeTruthy();
  expect(api.publish).not.toHaveBeenCalled();
  expect(api.validate).toHaveBeenCalledWith(projectId, choice.alias, {
    expected_version: null,
    definition: {
      toolchain: choice.toolchain,
      permissions: choice.permissions,
      integrations: {},
      mcp_servers: {
        reader: {
          server: {
            transport: 'http',
            url: 'https://mcp.example/tools',
          },
          integration: null,
        },
      },
    },
  });
  await userEvent.click(within(review).getByRole('button', { name: 'Publish reviewed settings' }));
  expect(
    await within(review).findByText(
      'Runtime settings published. No MCP connection was tested or execution started.',
    ),
  ).toBeTruthy();
  expect(api.publish).toHaveBeenCalledWith(
    projectId,
    choice.alias,
    expect.objectContaining({ expected_version: null }),
    expect.objectContaining({ 'Idempotency-Key': expect.any(String) }),
  );
  expect(router.state.location.search).not.toContain('mcp.example');
});

it('uses installed numeric ceilings and prevents selecting permissions the backend cannot enforce', async () => {
  const { api, choice } = setup();
  choice.enforcement = choice.enforcement.filter((category) => category !== 'network_hosts');
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  const cpu = await screen.findByRole('spinbutton', { name: 'Resource · cpu_millis' });
  expect(cpu.getAttribute('max')).toBe('650');
  await userEvent.clear(cpu);
  await userEvent.type(cpu, '320');
  const host = screen.getByRole('checkbox', { name: 'Network host · mcp.example' });
  await userEvent.click(host);
  expect(host).toHaveProperty('disabled', true);
  expect(
    screen.getByText(
      'This permission is not enforced by the backend. Existing selections can only be removed.',
    ),
  ).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Review Runtime settings' }));
  await screen.findByRole('dialog', { name: 'Review Runtime settings' });
  expect(api.validate.mock.calls[0]?.[2].definition.permissions.resources.cpu_millis).toBe(320);
  expect(api.validate.mock.calls[0]?.[2].definition.permissions.network_hosts).toEqual([]);
});

it('selects an existing Project-authorized integration resource and pins its exact binding for MCP', async () => {
  const { api, integrations, connection, access, resources, projectId } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  await userEvent.type(
    await screen.findByRole('textbox', { name: 'Integration alias' }),
    'tracker',
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Integration connection' }));
  await userEvent.click(await screen.findByRole('option', { name: connection.definition.name }));
  await userEvent.click(
    await screen.findByRole('combobox', { name: 'Authorized resource and capability' }),
  );
  const resource = resources.resources[0]!;
  const capability = resource.capabilities[0]!;
  await userEvent.click(
    await screen.findByRole('option', { name: `${resource.locator} · Task reading` }),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Add integration binding' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add MCP server' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'MCP name' }), 'reader');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'HTTPS endpoint' }),
    'https://mcp.example/tools',
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Required integration' }));
  await userEvent.click(screen.getByRole('option', { name: 'tracker' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review Runtime settings' }));
  await screen.findByRole('dialog', { name: 'Review Runtime settings' });
  expect(api.validate.mock.calls[0]?.[2].definition.integrations).toEqual({
    tracker: {
      project_id: projectId,
      connection_id: connection.id,
      connection_version: access.connection_version,
      access_version: access.version,
      resource: resource.reference,
      capability: capability.capability,
    },
  });
  expect(api.validate.mock.calls[0]?.[2].definition.mcp_servers.reader?.integration).toBe(
    'tracker',
  );
  expect(integrations.listProjectResources).toHaveBeenCalledWith(
    connection.id,
    projectId,
    expect.any(AbortSignal),
  );
  expect(integrations.publish).not.toHaveBeenCalled();
});

it('authors installed stdio with exact argv, mapped environment names and an explicit tool selection', async () => {
  const { api } = setup(['stdio']);
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add MCP server' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'MCP name' }), 'reader');
  await userEvent.click(screen.getByRole('button', { name: 'Add argument' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Argument 1' }), 'value with spaces');
  await userEvent.click(screen.getByRole('checkbox', { name: 'LANGUAGE' }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Select specific MCP tools' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add tool name' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Tool name 1' }), 'read');
  await userEvent.click(screen.getByRole('button', { name: 'Review Runtime settings' }));
  await screen.findByRole('dialog', { name: 'Review Runtime settings' });
  expect(api.validate.mock.calls[0]?.[2].definition.mcp_servers.reader?.server).toEqual({
    transport: 'stdio',
    command: '/runtime/reader',
    args: ['value with spaces'],
    env_vars: ['LANGUAGE'],
    enabled_tools: ['read'],
  });
  expect(screen.queryByRole('textbox', { name: 'ACCESS_TOKEN' })).toBeNull();
  expect(api.publish).not.toHaveBeenCalled();
});

it('keeps the Runtime draft after a version conflict and requires explicit refresh and a new review', async () => {
  const { api, choice, projectId } = setup();
  const saved = {
    project_id: projectId,
    runtime: choice.alias,
    version: 7,
    definition: {
      toolchain: choice.toolchain,
      permissions: choice.permissions,
      integrations: {},
      mcp_servers: {},
    },
    digest: 'b'.repeat(64),
  };
  api.current
    .mockResolvedValueOnce({ settings: saved })
    .mockResolvedValue({ settings: { ...saved, version: 8 } });
  api.publish.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'precondition_failed',
      message: 'Runtime settings changed.',
      requestId: null,
      status: 412,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add MCP server' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'MCP name' }), 'draft-reader');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'HTTPS endpoint' }),
    'https://mcp.example/unsaved',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review Runtime settings' }));
  let dialog = await screen.findByRole('dialog', { name: 'Review Runtime settings' });
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish reviewed settings' }));
  expect(await within(dialog).findByText('Runtime settings changed.')).toBeTruthy();
  expect(api.publish).toHaveBeenCalledTimes(1);
  await userEvent.click(within(dialog).getByRole('button', { name: 'Refresh current data' }));
  expect((await screen.findByRole('textbox', { name: 'MCP name' })).getAttribute('value')).toBe(
    'draft-reader',
  );
  expect(screen.getByRole('textbox', { name: 'HTTPS endpoint' }).getAttribute('value')).toBe(
    'https://mcp.example/unsaved',
  );
  expect(api.publish).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole('button', { name: 'Review Runtime settings' }));
  dialog = await screen.findByRole('dialog', { name: 'Review Runtime settings' });
  expect(api.validate.mock.calls[1]?.[2].expected_version).toBe(8);
  await userEvent.click(within(dialog).getByRole('button', { name: 'Publish reviewed settings' }));
  expect(
    await within(dialog).findByText(
      'Runtime settings published. No MCP connection was tested or execution started.',
    ),
  ).toBeTruthy();
  expect(api.publish.mock.calls[1]?.[2].expected_version).toBe(8);
});

it('requires explicit adoption when the Runtime installation pins change', async () => {
  const { api, choice, projectId } = setup();
  const oldToolchain = structuredClone(choice.toolchain);
  oldToolchain.executable.version = 'previous-installation';
  api.current.mockResolvedValue({
    settings: {
      project_id: projectId,
      runtime: choice.alias,
      version: 7,
      digest: 'b'.repeat(64),
      definition: {
        toolchain: oldToolchain,
        permissions: choice.permissions,
        integrations: {},
        mcp_servers: {},
      },
    },
  });
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  expect(await screen.findByRole('button', { name: 'Review Runtime settings' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(api.validate).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Adopt current Runtime installation' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review Runtime settings' }));
  await screen.findByRole('dialog', { name: 'Review Runtime settings' });
  expect(api.validate.mock.calls[0]?.[2]).toEqual({
    expected_version: 7,
    definition: {
      toolchain: choice.toolchain,
      permissions: choice.permissions,
      integrations: {},
      mcp_servers: {},
    },
  });
});

it('does not let validation retry collapse duplicate named MCP rows', async () => {
  const { api } = setup();
  api.validate.mockRejectedValueOnce(new Error('validation unavailable'));
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Add MCP server' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'MCP name' }), 'reader');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'HTTPS endpoint' }),
    'https://mcp.example/one',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review Runtime settings' }));
  const retry = await screen.findByRole('button', { name: 'Retry' });
  await userEvent.click(screen.getByRole('button', { name: 'Add MCP server' }));
  await userEvent.type(screen.getAllByRole('textbox', { name: 'MCP name' })[1]!, 'reader');
  await userEvent.type(
    screen.getAllByRole('textbox', { name: 'HTTPS endpoint' })[1]!,
    'https://mcp.example/two',
  );
  expect(screen.getByRole('button', { name: 'Review Runtime settings' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(retry);
  expect(api.validate).toHaveBeenCalledTimes(1);
  expect(screen.getAllByRole('textbox', { name: 'MCP name' })).toHaveLength(2);
});

it('explains unsupported resource limits and removes their zero-only control after clearing', async () => {
  const { choice } = setup();
  choice.enforcement = choice.enforcement.filter((category) => category !== 'cpu');
  await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
  const cpu = await screen.findByRole('spinbutton', { name: 'Resource · cpu_millis' });
  expect(
    screen.getByText(
      'This permission is not enforced by the backend. Existing selections can only be removed.',
    ),
  ).toBeTruthy();
  await userEvent.clear(cpu);
  expect(screen.queryByRole('spinbutton', { name: 'Resource · cpu_millis' })).toBeNull();
});

it.each(['forbidden', 'transport_failure'] as const)(
  'does not present an integration %s as an empty grant list',
  async (code) => {
    const { integrations, connection } = setup();
    const reader =
      code === 'forbidden' ? integrations.getProjectAccess : integrations.listProjectResources;
    reader.mockRejectedValue(
      new ManagementApiError({
        code,
        message: 'Resource read failed',
        requestId: null,
        status: code === 'forbidden' ? 403 : null,
        retryAfterMilliseconds: null,
      }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'installed-runtime' }));
    await userEvent.click(await screen.findByRole('combobox', { name: 'Integration connection' }));
    await userEvent.click(await screen.findByRole('option', { name: connection.definition.name }));
    await screen.findByRole('button', { name: 'Retry' });
    expect(
      screen.queryByText(
        'No currently usable resources are granted to this Project. Configure access through integration administration.',
      ),
    ).toBeNull();
    expect(screen.getByRole('button', { name: 'Add integration binding' })).toHaveProperty(
      'disabled',
      true,
    );
  },
);
