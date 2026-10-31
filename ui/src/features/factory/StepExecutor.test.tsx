// @vitest-environment jsdom
import { useState } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { factoryRoleFixture } from '../../../tests/fixtures/factoryRoles';
import { queryKeys, createConsoleQueryClient } from '../../app/query';
import { FlowGraphEditor } from './FlowGraphEditor';
import { emptyFlowDraft } from './emptyFlowDraft';
import { factoryRolesApi } from './roleApi';

afterEach(cleanup);

it.each(['profile', 'binding'] as const)(
  'ignores a late %s response after its step is removed instead of changing the sibling',
  async (pending) => {
    const { api, profile, build, draft } = setup({ selected: true });
    const sibling = structuredClone(draft().root.nodes[1]!);
    const executor = within(await screen.findByRole('group', { name: 'Executor' }));
    await executor.findByText('test.model');
    let complete!: () => void;
    if (pending === 'profile')
      api.getProfile.mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            complete = () => resolve(profile);
          }),
      );
    else
      api.resolveExecutionBinding.mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            complete = () => resolve({ build, incompatibility: null });
          }),
      );
    await userEvent.click(await executor.findByRole('combobox', { name: 'Role' }));
    await userEvent.click(await screen.findByRole('option', { name: 'Project · v1' }));
    await userEvent.click(await executor.findByRole('combobox', { name: 'Profile' }));
    await userEvent.click(await screen.findByRole('option', { name: 'default · v1' }));
    await executor.findByRole('combobox', { name: 'Execution runtime' });
    await userEvent.click(executor.getByRole('button', { name: 'Use this profile' }));
    expect(complete).toBeTypeOf('function');
    await userEvent.click(screen.getByRole('button', { name: 'Remove step' }));
    expect(draft().root.nodes).toEqual([sibling]);
    await act(async () => {
      complete();
    });
    expect(draft().root.nodes).toEqual([sibling]);
  },
);

it('keeps a late profile response from assigning an executor after the role was cleared', async () => {
  const { api, profile, draft } = setup();
  let complete!: (value: typeof profile) => void;
  api.getProfile.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        complete = resolve;
      }),
  );
  const executor = within(await screen.findByRole('group', { name: 'Executor' }));
  await userEvent.click(await executor.findByRole('combobox', { name: 'Role' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Project · v1' }));
  await userEvent.click(await executor.findByRole('combobox', { name: 'Profile' }));
  await userEvent.click(await screen.findByRole('option', { name: 'default · v1' }));
  await executor.findByRole('combobox', { name: 'Execution runtime' });
  await userEvent.click(executor.getByRole('button', { name: 'Use this profile' }));
  expect(api.getProfile).toHaveBeenCalledTimes(1);
  await userEvent.click(executor.getByRole('combobox', { name: 'Role' }));
  await userEvent.click(screen.getByRole('option', { name: 'Choose a role' }));
  await act(async () => {
    complete(profile);
  });
  expect(draft().root.nodes[0]!.profile_selection).toBeUndefined();
  expect(executor.getByText('No executor selected')).toBeTruthy();
  expect(api.resolveExecutionBinding).not.toHaveBeenCalled();
});

it('presents a first reasoning executor without an empty copy-from-Flow selector', async () => {
  setup();
  expect(await screen.findByRole('group', { name: 'Executor' })).toBeTruthy();
  expect(screen.queryByRole('combobox', { name: 'Execution profile' })).toBeNull();
});

function setup(
  options: {
    selected?: boolean;
    legacy?: boolean;
    roleVersion?: number;
    multipleOutcomes?: boolean;
    mcp?: boolean;
    unavailable?: boolean;
    profileResponse?: Promise<components['schemas']['FactoryRoleProfile']>;
    capabilityResponse?: Promise<components['schemas']['FactoryRoleProfileCapabilityDetails']>;
  } = {},
) {
  const { role, profile, choice, capability } = factoryRoleFixture();
  if (options.mcp) {
    const reference = { identity: 'issues-reader', version: 'v1', digest: '8'.repeat(64) };
    profile.definition.execution!.mcp_servers = { issues: { configuration: reference } };
    capability.mcp_configurations = {
      issues: {
        reference,
        server: {
          transport: 'http',
          url: 'https://example.test/mcp',
          enabled_tools: ['search', 'get'],
        },
        authorization: {
          purpose: 'mcp_read',
          environment: 'ISSUE_TOKEN',
          credential_profile: 'private-issues',
          resource: { identity: 'project-resource', version: 'v1', digest: '5'.repeat(64) },
          secret: { provider: 'agent-private', reference: 'ISSUE_TOKEN' },
        },
      },
    };
  }
  role.definition.name = 'Project';
  const schema = {
    reference: { identity: 'assessment', version: 'v1', digest: 'a'.repeat(64) },
    shape: { type: 'boolean' as const },
  };
  const build: components['schemas']['FactoryFlowBuildBinding'] = {
    build_configuration: profile.definition.build_configuration,
    tool: profile.definition.toolchain.executable,
    plugin: profile.definition.toolchain.plugin,
    model_or_tool: profile.definition.toolchain.model,
    task_digest: 'd'.repeat(64),
    result_output: 'result.json',
    result_output_kind: 'report',
    result_outcome: 'completed',
  };
  const api = {
    ...factoryRolesApi,
    listRoles: vi.fn(async () => ({
      items: [
        {
          reference: { ...role.reference, version: options.roleVersion ?? role.reference.version },
          name: role.definition.name,
          icon: null,
          concurrency_limit: null,
          template: null,
        },
      ],
      total: 1,
      next_cursor: null,
    })),
    listProfiles: vi.fn(async () => ({
      items: [
        {
          reference: profile.reference,
          role: profile.role,
          name: profile.definition.name,
          toolchain: profile.definition.toolchain,
          template: null,
        },
      ],
      total: 1,
      next_cursor: null,
    })),
    listCapabilities: vi.fn(async () => ({ items: [choice], total: 1, next_cursor: null })),
    getRole: vi.fn(async () => role),
    getCapability: vi.fn(async () => options.capabilityResponse ?? capability),
    getProfile: vi.fn(async () => {
      if (options.unavailable) throw new Error('private provider details');
      return options.profileResponse ?? profile;
    }),
    resolveExecutionBinding: vi.fn(async () => ({ build, incompatibility: null })),
  };
  let draft = emptyFlowDraft();
  draft.data_schemas = [schema];
  draft.root.entry = 'Project';
  draft.root.nodes = [
    {
      key: 'Project',
      kind: 'reasoning',
      required: true,
      input_schema: schema.reference,
      subflow: null,
      outcomes: [{ key: 'completed', kind: 'success', schema: schema.reference }],
      budget: draft.root.execution.budget,
      permissions: draft.root.execution.permissions,
    },
  ];
  if (options.legacy) draft.root.nodes[0]!.build = build;
  if (options.multipleOutcomes)
    draft.root.nodes[0]!.outcomes.push({
      key: 'reviewed',
      kind: 'success',
      schema: schema.reference,
    });
  if (options.selected) {
    draft.root.nodes[0]!.profile_selection = {
      role: profile.role,
      candidates: { selected: { profile, build } },
      policy: { mode: 'manual', candidate: 'selected' },
      failure: { kind: 'manual_selection' },
      selection_budget: {
        attempts: 0,
        elapsed_millis: 0,
        tokens: 0,
        cost_micro_units: 0,
        output_bytes: 0,
      },
    };
    draft.root.nodes[0]!.context_binding = {
      entries: {
        guidance: {
          source_kind: 'specification',
          source: {
            value: {
              mapping: 'constant',
              value: {
                kind: 'artifact',
                value: {
                  artifact_id: 'guidance-v1',
                  content_digest: 'b'.repeat(64),
                  encoded_size: 10,
                },
              },
            },
          },
        },
      },
    };
    draft.root.nodes.push({ ...structuredClone(draft.root.nodes[0]!), key: 'sibling' });
  }
  function Harness() {
    const [value, setValue] = useState(draft);
    return (
      <FlowGraphEditor
        value={value}
        roles={{ api, projectId: role.reference.project_id }}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          draft = next;
          setValue(next);
        }}
      />
    );
  }
  const client = createConsoleQueryClient();
  render(
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>,
  );
  return { api, role, profile, build, capability, client, draft: () => draft };
}

it('assigns the first reasoning executor from installed choices only after explicit profile confirmation', async () => {
  const { api, profile, build, draft } = setup();
  const executor = within(await screen.findByRole('group', { name: 'Executor' }));
  expect(executor.getByText('No executor selected')).toBeTruthy();
  expect(draft().root.nodes[0]!.profile_selection).toBeUndefined();
  await userEvent.click(await executor.findByRole('combobox', { name: 'Role' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Project · v1' }));
  await userEvent.click(await executor.findByRole('combobox', { name: 'Profile' }));
  await userEvent.click(await screen.findByRole('option', { name: 'default · v1' }));
  expect(api.resolveExecutionBinding).not.toHaveBeenCalled();
  expect(draft().root.nodes[0]!.profile_selection).toBeUndefined();
  await executor.findByRole('combobox', { name: 'Execution runtime' });
  await userEvent.click(executor.getByRole('button', { name: 'Use this profile' }));
  expect(await executor.findByText('Selected profile: default · v1')).toBeTruthy();
  const selected = draft().root.nodes[0]!.profile_selection!;
  expect(Object.values(selected.candidates)[0]).toEqual({ profile, build });
  expect(selected.policy).toEqual({ mode: 'manual', candidate: profile.reference.id });
  expect(api.resolveExecutionBinding).toHaveBeenCalledWith(
    profile.reference.project_id,
    {
      profile: profile.reference,
      capability: 'installed-runtime',
      result_outcome: 'completed',
    },
    expect.any(AbortSignal),
  );
});

it('requires a new profile when the chosen role version changes while preserving documents and siblings', async () => {
  const { api, draft } = setup({ selected: true, roleVersion: 2 });
  const before = structuredClone(draft());
  const executor = within(await screen.findByRole('group', { name: 'Executor' }));
  await userEvent.click(await executor.findByRole('combobox', { name: 'Role' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Project · v2' }));
  expect(executor.getByText('No executor selected')).toBeTruthy();
  expect(draft().root.nodes[0]!.profile_selection).toBeNull();
  expect(draft().root.nodes[0]!.context_binding).toEqual(before.root.nodes[0]!.context_binding);
  expect(draft().root.nodes[1]).toEqual(before.root.nodes[1]);
  expect(api.resolveExecutionBinding).not.toHaveBeenCalled();
});

it('does not offer a profile belonging to another exact role version', async () => {
  const { api, draft } = setup({ roleVersion: 2 });
  const executor = within(await screen.findByRole('group', { name: 'Executor' }));
  await userEvent.click(await executor.findByRole('combobox', { name: 'Role' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Project · v2' }));
  expect(
    await executor.findByText(
      'No profiles for this role version. Choose another role or create a matching profile in AI agents.',
    ),
  ).toBeTruthy();
  expect(executor.getByRole('button', { name: 'Use this profile' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(draft().root.nodes[0]!.profile_selection).toBeUndefined();
  expect(api.getProfile).not.toHaveBeenCalled();
});

it('uses readable profile names in selection policies instead of technical candidate identifiers', async () => {
  setup();
  const executor = within(await screen.findByRole('group', { name: 'Executor' }));
  await userEvent.click(await executor.findByRole('combobox', { name: 'Role' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Project · v1' }));
  await userEvent.click(await executor.findByRole('combobox', { name: 'Profile' }));
  await userEvent.click(await screen.findByRole('option', { name: 'default · v1' }));
  await executor.findByRole('combobox', { name: 'Execution runtime' });
  await userEvent.click(executor.getByRole('button', { name: 'Use this profile' }));
  await executor.findByText('Selected profile: default · v1');
  await userEvent.click(executor.getByText('Profile selection policy'));
  await userEvent.click(executor.getByRole('combobox', { name: 'Baseline profile' }));
  expect(screen.getByRole('option', { name: 'default · v1' })).toBeTruthy();
  expect(executor.getByRole('button', { name: 'Remove candidate default' })).toBeTruthy();
});

it('requires an explicit result outcome when a step has several successful outcomes', async () => {
  const { api, draft } = setup({ multipleOutcomes: true });
  const executor = within(await screen.findByRole('group', { name: 'Executor' }));
  await userEvent.click(await executor.findByRole('combobox', { name: 'Role' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Project · v1' }));
  await userEvent.click(await executor.findByRole('combobox', { name: 'Profile' }));
  await userEvent.click(await screen.findByRole('option', { name: 'default · v1' }));
  await executor.findByRole('combobox', { name: 'Execution runtime' });
  expect(executor.getByRole('button', { name: 'Use this profile' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(draft().root.nodes[0]!.profile_selection).toBeUndefined();
  await userEvent.click(executor.getByRole('combobox', { name: 'Result outcome' }));
  await userEvent.click(screen.getByRole('option', { name: 'reviewed' }));
  await userEvent.click(executor.getByRole('button', { name: 'Use this profile' }));
  expect(api.resolveExecutionBinding).toHaveBeenCalledWith(
    expect.any(String),
    expect.objectContaining({ result_outcome: 'reviewed' }),
    expect.any(AbortSignal),
  );
});

it('does not repin an explicitly chosen profile when its catalogue row advances to another version', async () => {
  const { api, profile, role, client, draft } = setup();
  const executor = within(await screen.findByRole('group', { name: 'Executor' }));
  await userEvent.click(await executor.findByRole('combobox', { name: 'Role' }));
  await userEvent.click(screen.getByRole('option', { name: 'Project · v1' }));
  await userEvent.click(await executor.findByRole('combobox', { name: 'Profile' }));
  await userEvent.click(screen.getByRole('option', { name: 'default · v1' }));
  await executor.findByRole('combobox', { name: 'Execution runtime' });
  await act(async () => {
    client.setQueryData(
      queryKeys.factoryRoleCatalog(role.reference.project_id, 'profiles', role.reference.id),
      {
        pages: [
          {
            items: [
              {
                ...profile,
                id: profile.reference.id,
                reference: { ...profile.reference, version: 2 },
                name: 'default',
                toolchain: profile.definition.toolchain,
              },
            ],
            total: 1,
            next_cursor: null,
          },
        ],
        pageParams: [null],
      },
    );
  });
  await userEvent.click(executor.getByRole('button', { name: 'Use this profile' }));
  expect(api.getProfile).not.toHaveBeenCalled();
  expect(api.resolveExecutionBinding).not.toHaveBeenCalled();
  expect(draft().root.nodes[0]!.profile_selection).toBeUndefined();
  expect(executor.getByRole('alert').textContent).toContain('Select it again');
});

it('shows the exact authorized profile separately from tools and keeps the draft while viewing its instructions', async () => {
  const { api, profile, draft } = setup({ selected: true });
  const before = structuredClone(draft());
  const executor = within(await screen.findByRole('group', { name: 'Executor' }));
  expect(await executor.findByText('test.model')).toBeTruthy();
  const link = executor.getByRole('link', { name: 'View profile · default · v1' });
  await userEvent.click(link);
  const view = within(await screen.findByRole('region', { name: 'Profile · default · v1' }));
  expect(await view.findByText('Use selected skills.')).toBeTruthy();
  await userEvent.click(view.getByRole('button', { name: 'Back to step' }));
  expect(draft()).toEqual(before);
  expect(api.getProfile).toHaveBeenCalledWith(
    profile.reference.project_id,
    profile.reference.id,
    1,
    expect.any(AbortSignal),
  );
  expect(screen.getByRole('group', { name: 'Tools and access' })).toBeTruthy();
  expect(executor.queryByRole('group', { name: 'Tools and access' })).toBeNull();
  expect(api.resolveExecutionBinding).not.toHaveBeenCalled();
});

it('shows MCP read scope separately and keeps logical credentials inside Details without probing the connection', async () => {
  const { api, profile, draft } = setup({ selected: true, mcp: true });
  const before = structuredClone(draft());
  const tools = within(await screen.findByRole('group', { name: 'Tools and access' }));
  expect(await tools.findByText('Read-only MCP access', { selector: 'p' })).toBeTruthy();
  expect(tools.getByText('MCP tools: search, get')).toBeTruthy();
  expect(tools.getByText('agent-private · ISSUE_TOKEN').closest('details')).toHaveProperty(
    'open',
    false,
  );
  expect(
    screen.getByRole('group', { name: 'Executor' }).contains(tools.getByText('MCP · issues')),
  ).toBe(false);
  expect(api.getCapability).toHaveBeenCalledWith(
    profile.reference.project_id,
    'installed-runtime',
    expect.any(AbortSignal),
  );
  expect(api.resolveExecutionBinding).not.toHaveBeenCalled();
  expect(draft()).toEqual(before);
});

it('keeps an unavailable exact profile explicit without presenting retained draft content as authorized metadata', async () => {
  const { draft, api } = setup({ selected: true, unavailable: true });
  const before = structuredClone(draft());
  expect(
    await within(screen.getByRole('group', { name: 'Executor' })).findByText(
      'Exact profile metadata is unavailable. Your selection is retained; check Project access or retry.',
    ),
  ).toBeTruthy();
  expect(screen.queryByRole('link', { name: 'View profile · default · v1' })).toBeNull();
  expect(screen.queryByText('private provider details')).toBeNull();
  expect(api.resolveExecutionBinding).not.toHaveBeenCalled();
  expect(draft()).toEqual(before);
});

it('distinguishes loading exact profile and MCP metadata from an unavailable configuration', async () => {
  let profileReady!: (value: components['schemas']['FactoryRoleProfile']) => void;
  let mcpReady!: (value: components['schemas']['FactoryRoleProfileCapabilityDetails']) => void;
  const profileResponse = new Promise<components['schemas']['FactoryRoleProfile']>((resolve) => {
    profileReady = resolve;
  });
  const capabilityResponse = new Promise<
    components['schemas']['FactoryRoleProfileCapabilityDetails']
  >((resolve) => {
    mcpReady = resolve;
  });
  const { profile, capability } = setup({
    selected: true,
    mcp: true,
    profileResponse,
    capabilityResponse,
  });
  const tools = within(screen.getByRole('group', { name: 'Tools and access' }));
  expect(tools.queryByRole('alert')).toBeNull();
  await act(async () => {
    profileReady(profile);
  });
  expect(await tools.findByText('MCP · issues')).toBeTruthy();
  expect(tools.queryByRole('alert')).toBeNull();
  await act(async () => {
    mcpReady(capability);
  });
  expect(await tools.findByText('Read-only MCP access', { selector: 'p' })).toBeTruthy();
});

it('does not report a missing executor when an existing reasoning step retains its static execution binding', async () => {
  setup({ legacy: true });
  const executor = within(await screen.findByRole('group', { name: 'Executor' }));
  expect(executor.getByText('Existing executor is configured.')).toBeTruthy();
  expect(executor.queryByText('No executor selected')).toBeNull();
});
