// @vitest-environment jsdom
import { useState } from 'react';
import { act, cleanup, render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { emptyFlowDraft } from './emptyFlowDraft';
import { FlowGraphEditor } from './FlowGraphEditor';

afterEach(cleanup);

it('starts new steps with explicit code absence and no inherited pipeline access', async () => {
  let observed = emptyFlowDraft();
  observed.root.execution.permissions.network_hosts = ['api.example.test'];
  observed.root.execution.permissions.mounts = [{ root: '/workspace', mode: 'read_only' }];
  const before = structuredClone(observed.root.execution.permissions);
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Add step' }));
  expect(observed.root.code_sources).toEqual({});
  expect(observed.root.nodes[0]!.code).toEqual({ kind: 'none' });
  expect(observed.root.nodes[0]!.permissions.network_hosts).toEqual([]);
  expect(observed.root.nodes[0]!.permissions.mounts).toEqual([]);
  expect(observed.root.nodes[0]!.permissions.secret_profiles).toEqual([]);
  expect(observed.root.execution.permissions).toEqual(before);
});

it('declares an exact authorized repository without attaching it, then explicitly connects only the selected step', async () => {
  const { QueryClientProvider } = await import('@tanstack/react-query');
  const { createConsoleQueryClient } = await import('../../app/query');
  const repository = {
    id: 'repo-a',
    project_id: 'delivery',
    name: 'Application code',
    version: 3,
    published_at_unix_ms: 1,
    definition_digest: 'a'.repeat(64),
    definition: {
      repository_locator: 'https://example.test/code',
      vcs_integration_id: 'vcs',
      selection: {
        default_reference: 'main',
        allowed_references: ['release'],
        allow_exact_revision: true,
      },
    },
  };
  const repositories = {
    listRepositories: vi.fn().mockResolvedValue({ items: [repository], next_cursor: null }),
    getRepository: vi.fn().mockResolvedValue(repository),
  };
  let observed = emptyFlowDraft();
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        codeResources={{ projectId: 'delivery', repositories }}
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Add step' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Step name' }), 'first');
  const inventory = within(await screen.findByRole('group', { name: 'Available code sources' }));
  await userEvent.click(await inventory.findByRole('combobox', { name: 'Project repository' }));
  await userEvent.click(screen.getByRole('option', { name: 'Application code · v3' }));
  await userEvent.click(await inventory.findByRole('button', { name: 'Add code source' }));
  expect(observed.root.code_sources?.code_1?.repository).toEqual({
    id: 'repo-a',
    project_id: 'delivery',
    version: 3,
    definition_digest: 'a'.repeat(64),
  });
  expect(observed.root.nodes[0]!.code).toEqual({ kind: 'none' });
  await userEvent.click(screen.getByRole('button', { name: 'Add step' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Step name' }), 'second');
  const code = within(screen.getByRole('group', { name: 'Step code' }));
  await userEvent.click(code.getByRole('combobox', { name: 'Code available to this step' }));
  await userEvent.click(screen.getByRole('option', { name: 'Application code · v3' }));
  expect(observed.root.nodes[1]!.code).toEqual({
    kind: 'source',
    source: 'code_1',
    revision: { kind: 'admitted_base' },
    access: 'read_only',
    scope: { kind: 'repository' },
  });
  expect(observed.root.nodes[0]!.code).toEqual({ kind: 'none' });
  await userEvent.click(code.getByRole('combobox', { name: 'Step code revision' }));
  await userEvent.click(screen.getByRole('option', { name: 'Exact revision' }));
  await userEvent.type(code.getByRole('textbox', { name: 'Exact revision' }), 'a'.repeat(40));
  await userEvent.click(code.getByRole('combobox', { name: 'Code scope' }));
  await userEvent.click(screen.getByRole('option', { name: 'Selected paths and lines' }));
  await userEvent.type(code.getByRole('textbox', { name: 'Relative path' }), 'src/main.rs');
  await userEvent.click(code.getByRole('checkbox', { name: 'Limit to line range (read only)' }));
  await userEvent.clear(code.getByRole('spinbutton', { name: 'Last line' }));
  await userEvent.type(code.getByRole('spinbutton', { name: 'Last line' }), '20');
  expect(observed.root.nodes[1]!.code).toMatchObject({
    revision: { kind: 'exact_revision', revision: 'a'.repeat(40) },
    scope: {
      kind: 'selected',
      entries: [{ kind: 'range', path: 'src/main.rs', lines: { first_line: 1, last_line: 20 } }],
    },
  });
  expect(observed.root.nodes[0]!.code).toEqual({ kind: 'none' });
  expect(
    inventory.getByRole('button', { name: 'Remove code source Application code' }),
  ).toHaveProperty('disabled', true);
  await userEvent.click(code.getByRole('combobox', { name: 'Code available to this step' }));
  await userEvent.click(screen.getByRole('option', { name: 'No code' }));
  await userEvent.click(
    inventory.getByRole('button', { name: 'Remove code source Application code' }),
  );
  expect(observed.root.code_sources).toEqual({});
  expect(repositories.getRepository.mock.calls[0]?.slice(0, 2)).toEqual(['repo-a', 3]);
});

it('identifies unqualified source-free executor support without claiming the saved no-code request is executable', async () => {
  const { QueryClientProvider } = await import('@tanstack/react-query');
  const { createConsoleQueryClient } = await import('../../app/query');
  const { factoryApi } = await import('./api');
  const value = emptyFlowDraft();
  value.root.nodes = [
    {
      key: 'assess',
      kind: 'reasoning',
      code: { kind: 'none' },
      required: true,
      subflow: null,
      input_schema: null,
      outcomes: [],
      budget: value.root.execution.budget,
      permissions: value.root.execution.permissions,
    },
  ];
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn(async () => ({
      commands: [],
      human_gates: [],
      deterministic_gate: {
        policy: { identity: 'gate', version: 'v1', digest: 'a'.repeat(64) },
        parameter_contract: 'declarative_gate_v1' as const,
        max_parameter_bytes: 1024,
        resource_modes: ['none' as const],
      },
      build_resource_modes: ['repository' as const],
      source_free_incompatibility: 'executor_support_required' as const,
    })),
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        authoring={{ api, projectId: 'delivery' }}
        codeResources={{
          projectId: 'delivery',
          repositories: {
            listRepositories: vi.fn(async () => ({ items: [], next_cursor: null })),
            getRepository: vi.fn(),
          },
        }}
        onChange={() => {}}
      />
    </QueryClientProvider>,
  );
  expect(
    await screen.findByText(
      'Source-free execution is not confirmed by the installed executor. Review runtime support before starting.',
    ),
  ).toBeTruthy();
});

it('blocks a new source declaration when exact metadata refresh fails while retaining the selection', async () => {
  const { QueryClientProvider } = await import('@tanstack/react-query');
  const { createConsoleQueryClient, queryKeys } = await import('../../app/query');
  const { FlowCodeSources } = await import('./FlowCodeSources');
  const repository = {
    id: 'repo',
    project_id: 'delivery',
    name: 'Application',
    version: 2,
    published_at_unix_ms: 1,
    definition_digest: 'a'.repeat(64),
    definition: {
      repository_locator: 'https://example.test/code',
      vcs_integration_id: 'vcs',
      selection: { default_reference: 'main', allowed_references: [], allow_exact_revision: false },
    },
  };
  const repositories = {
    listRepositories: vi.fn(async () => ({ items: [repository], next_cursor: null })),
    getRepository: vi.fn(async () => repository),
  };
  const client = createConsoleQueryClient();
  const change = vi.fn();
  render(
    <QueryClientProvider client={client}>
      <FlowCodeSources
        root={emptyFlowDraft().root}
        services={{ projectId: 'delivery', repositories }}
        onChange={change}
      />
    </QueryClientProvider>,
  );
  await userEvent.click(await screen.findByRole('combobox', { name: 'Project repository' }));
  await userEvent.click(screen.getByRole('option', { name: 'Application · v2' }));
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Add code source' })).toHaveProperty(
      'disabled',
      false,
    ),
  );
  repositories.getRepository.mockRejectedValue(new Error('private provider failure'));
  await act(async () => {
    await client.refetchQueries({ queryKey: queryKeys.repository('repo', 2) });
  });
  await waitFor(() =>
    expect(screen.getByRole('button', { name: 'Add code source' })).toHaveProperty(
      'disabled',
      true,
    ),
  );
  expect(change).not.toHaveBeenCalled();
  expect(screen.getByRole('combobox', { name: 'Project repository' }).textContent).toContain(
    'Application · v2',
  );
  expect(screen.queryByText('private provider failure')).toBeNull();
});
