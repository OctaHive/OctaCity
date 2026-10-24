// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { ManagementApiError } from '../../api/client';
import { FactoryDefinitions } from './FactoryDefinitions';
import { factoryApi } from './api';
import { factoryRoleFixture } from '../../../tests/fixtures/factoryRoles';
import type { components } from '../../../.generated/api/schema';

afterEach(cleanup);
function setup() {
  const { role, profile, capability, choice } = factoryRoleFixture();
  const page = <T,>(items: T[]) => ({ items, total: items.length, next_cursor: null });
  const roles = {
    listRoles: vi.fn(async () =>
      page([
        {
          reference: role.reference,
          ...role.definition,
          icon: role.definition.icon ?? null,
          concurrency_limit: role.definition.concurrency_limit ?? null,
          template: null,
        },
      ]),
    ),
    getRole: vi.fn(async () => role),
    deleteRole: vi.fn(async () => ({
      disposition: 'applied' as const,
      role: role.reference,
      deleted_at: 100,
    })),
    currentRole: vi.fn(async () => role),
    currentProfile: vi.fn(async () => profile),
    listProfiles: vi.fn(async () =>
      page([
        {
          reference: profile.reference,
          role: profile.role,
          name: profile.definition.name,
          toolchain: profile.definition.toolchain,
          template: null,
        },
      ]),
    ),
    getProfile: vi.fn(async () => profile),
    listCapabilities: vi.fn(async () => page([choice])),
    getCapability: vi.fn(async () => capability),
    refreshCapability: vi.fn(async () => capability),
    listTemplates: vi.fn(async (): Promise<components['schemas']['FactoryRoleTemplatePage']> =>
      page([]),
    ),
    listSkills: vi.fn(async (): Promise<components['schemas']['ProjectSkillCatalogPage']> =>
      page([]),
    ),
    validateRole: vi.fn(
      async (_project: string, body: components['schemas']['ValidateFactoryRoleRequest']) => ({
        reference: { ...role.reference, id: body.id },
        definition: body.definition,
      }),
    ),
    validateProfile: vi.fn(),
    publish: vi.fn(async () => ({
      disposition: 'applied' as const,
      role,
      profiles: [],
      published_at: 100,
    })),
    copyTemplate: vi.fn(),
    getTemplate: vi.fn(),
    importSkill: vi.fn(),
    getSkill: vi.fn(),
  };
  const api = {
    ...factoryApi,
    listChoices: vi.fn(async () => page([])),
    listConfigurations: vi.fn(async () => page([])),
    listFlows: vi.fn(async () => page([])),
  };
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <FactoryDefinitions api={api} rolesApi={roles} projectId={role.reference.project_id} />
      ),
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { roles, role, profile, router };
}

it('discovers role-backed AI agents separately from worker enrollment and reads pinned instructions and profiles', async () => {
  const { roles, role } = setup();
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  expect(await screen.findByText('Project implementation')).toBeTruthy();
  expect(await screen.findByText('Follow Project rules.')).toBeTruthy();
  await userEvent.click(await screen.findByRole('button', { name: 'default' }));
  expect(await screen.findByText('Use selected skills.')).toBeTruthy();
  expect(screen.getByText('balanced')).toBeTruthy();
  expect(roles.getRole).toHaveBeenCalledWith(
    role.reference.project_id,
    role.reference.id,
    1,
    expect.any(AbortSignal),
  );
  expect(screen.queryByRole('button', { name: 'Enroll agent' })).toBeNull();
});

it('opens the retained skill tree and selected file as inert text without losing the agent draft', async () => {
  const { roles } = setup();
  const skill = { identity: 'review', version: 'v1', digest: 'a'.repeat(64) };
  roles.listSkills.mockResolvedValue({ items: [skill], total: 1, next_cursor: null });
  const bundle = {
    reference: skill,
    files: {
      'SKILL.md': {
        artifact_id: '10000000-0000-4000-8000-000000000001',
        content_digest: 'b'.repeat(64),
        encoded_size: 24,
      },
    },
  };
  roles.getSkill.mockResolvedValueOnce({ bundle, file: null }).mockResolvedValueOnce({
    bundle,
    file: { path: 'SKILL.md', size_bytes: 24, text: '# Review\n<script>bad()</script>' },
  });
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Create AI agent' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Agent name' }), 'Unsaved reviewer');
  await userEvent.click(await screen.findByRole('button', { name: 'View skill review · v1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'SKILL.md' }));
  expect(
    await screen.findByText('# Review\n<script>bad()</script>', {
      exact: true,
      normalizer: (value) => value,
    }),
  ).toHaveProperty('tagName', 'PRE');
  expect(screen.getByRole('textbox', { name: 'Agent name' })).toHaveProperty(
    'value',
    'Unsaved reviewer',
  );
  expect(roles.getSkill).toHaveBeenLastCalledWith(
    expect.any(String),
    skill,
    'SKILL.md',
    expect.any(AbortSignal),
  );
  expect(roles.publish).not.toHaveBeenCalled();
});

it('selects shared skills in the AI agent draft without needing an installed runtime', async () => {
  const { roles } = setup();
  const skill = { identity: 'review', version: 'v1', digest: 'a'.repeat(64) };
  roles.listSkills.mockResolvedValue({ items: [skill], total: 1, next_cursor: null });
  roles.listCapabilities.mockResolvedValue({ items: [], total: 0, next_cursor: null });
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Create AI agent' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Agent name' }), 'Shared reviewer');
  await userEvent.click(
    await screen.findByRole('checkbox', { name: `review · v1 · ${skill.digest}` }),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review AI agent' }));
  await screen.findByRole('dialog');
  expect(roles.validateRole).toHaveBeenCalledWith(
    expect.any(String),
    expect.objectContaining({
      definition: expect.objectContaining({ name: 'Shared reviewer', skills: { review: skill } }),
    }),
  );
  expect(roles.listCapabilities).not.toHaveBeenCalled();
  expect(roles.publish).not.toHaveBeenCalled();
});

it('creates an arbitrary role with ordered instructions and concurrency only after server validation and explicit review', async () => {
  const { roles, role, router } = setup();
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Create AI agent' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Agent name' }), 'Custom reviewer');
  await userEvent.type(screen.getByRole('textbox', { name: 'Description' }), 'private draft');
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Shared instruction 1' }),
    'Inspect the change.',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Add shared instruction' }));
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Shared instruction 2' }),
    'Keep Project rules.',
  );
  await userEvent.type(screen.getByRole('spinbutton', { name: 'Concurrency limit' }), '2');
  await userEvent.click(screen.getByRole('button', { name: 'Review AI agent' }));
  const dialog = await screen.findByRole('dialog');
  expect(roles.publish).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publish' }));
  expect(await within(dialog).findByText('Immutable version published.')).toBeTruthy();
  expect(roles.publish).toHaveBeenCalledWith(
    role.reference.project_id,
    {
      role: {
        id: expect.stringMatching(/^[a-f0-9-]{36}$/),
        expected_current_version: null,
        definition: {
          name: 'Custom reviewer',
          icon: null,
          description: 'private draft',
          instructions: ['Inspect the change.', 'Keep Project rules.'],
          concurrency_limit: 2,
        },
      },
      profiles: [],
    },
    expect.objectContaining({ 'Idempotency-Key': expect.any(String) }),
  );
  expect(router.state.location.search).toBe('');
  expect(JSON.stringify(localStorage) + JSON.stringify(sessionStorage)).not.toContain(
    'private draft',
  );
});

it('edits a profile independently using installed Runtime/Model/Thinking settings and authoritative validation', async () => {
  const { roles, role, profile } = setup();
  roles.validateProfile.mockResolvedValue({ profile, incompatibility: null });
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit profile' }));
  const name = screen.getByRole('textbox', { name: 'Profile name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'careful');
  await userEvent.click(screen.getByRole('combobox', { name: /Runtime.*Model/ }));
  await userEvent.click(await screen.findByRole('option', { name: /test.runtime.*test.model/ }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Model parameters · deliberation' }));
  await userEvent.click(await screen.findByRole('option', { name: 'string: deep' }));
  roles.validateProfile.mockImplementation(async (_project, request) => ({
    profile: { ...profile, definition: request.definition },
    incompatibility: null,
  }));
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  const dialog = await screen.findByRole('dialog');
  expect(roles.publish).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publish' }));
  await within(dialog).findByText('Immutable version published.');
  expect(roles.publish).toHaveBeenCalledWith(
    role.reference.project_id,
    {
      profiles: [
        {
          id: profile.reference.id,
          role: role.reference,
          expected_current_version: 1,
          definition: {
            ...profile.definition,
            name: 'careful',
            model_parameters: { deliberation: 'deep' },
          },
        },
      ],
    },
    expect.anything(),
  );
  await userEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
  expect(screen.queryByRole('region', { name: 'Profile editor' })).toBeNull();
});

async function openProfile() {
  const state = setup();
  state.roles.validateProfile.mockImplementation(async (_project, request) => ({
    profile: { ...state.profile, definition: request.definition },
    incompatibility: null,
  }));
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit profile' }));
  return state;
}
it('explains a missing root SKILL.md before import and lets the operator correct the folder without losing the agent draft', async () => {
  const { roles } = setup();
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Create AI agent' }));
  const name = screen.getByRole('textbox', { name: 'Agent name' });
  await userEvent.type(name, 'my agent');
  const input = screen.getByLabelText('Import skill folder');
  const wrong = new File(['# Instructions'], 'skill1.md', { type: 'text/markdown' });
  Object.defineProperty(wrong, 'webkitRelativePath', { value: 'skills/skill1.md' });
  await userEvent.upload(input, wrong);
  expect(await screen.findByRole('alert')).toHaveProperty(
    'textContent',
    'Missing SKILL.md at folder root (case-sensitive). Rename the file and retry.',
  );
  expect(screen.queryByRole('button', { name: 'Review skill import' })).toBeNull();
  expect(roles.importSkill).not.toHaveBeenCalled();
  expect(name).toHaveProperty('value', 'my agent');

  const corrected = new File(['# Instructions'], 'SKILL.md', { type: 'text/markdown' });
  Object.defineProperty(corrected, 'webkitRelativePath', { value: 'skills/SKILL.md' });
  await userEvent.upload(input, corrected);
  await screen.findByText('SKILL.md · 14 bytes');
  expect(screen.queryByRole('alert')).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Review skill import' }));
  expect(await screen.findByRole('dialog')).toBeTruthy();
  expect(name).toHaveProperty('value', 'my agent');
});
it('imports a reviewed shared skill folder inline, replays a lost response and keeps the agent draft on cancel and failure', async () => {
  const { roles, role } = setup();
  const existing = { identity: 'existing', version: 'v2', digest: 'b'.repeat(64) };
  roles.listSkills.mockResolvedValue({ items: [existing], total: 1, next_cursor: null });
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Create AI agent' }));
  await userEvent.click(await screen.findByRole('checkbox', { name: /existing.*v2/ }));
  const name = screen.getByRole('textbox', { name: 'Agent name' });
  await userEvent.type(name, 'private local draft');
  const file = new File(['# Skill\nInstructions'], 'SKILL.md', { type: 'text/markdown' });
  Object.defineProperty(file, 'webkitRelativePath', { value: 'review/SKILL.md' });
  await userEvent.upload(screen.getByLabelText('Import skill folder'), file);
  await screen.findByText('SKILL.md · 20 bytes');
  await userEvent.click(screen.getByRole('button', { name: 'Review skill import' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Abandon' }),
  );
  expect(name).toHaveProperty('value', 'private local draft');
  expect(roles.importSkill).not.toHaveBeenCalled();
  expect(screen.queryByRole('checkbox', { name: /review.*v1/ })).toBeNull();
  const bundle = { identity: 'review', version: 'v1', digest: 'a'.repeat(64) };
  roles.importSkill
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'transport_failure',
        message: 'Unavailable',
        requestId: null,
        retryAfterMilliseconds: null,
        status: null,
      }),
    )
    .mockResolvedValue({ bundle, published_at: 100 });
  await userEvent.click(screen.getByRole('button', { name: 'Review skill import' }));
  const dialog = await screen.findByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm import' }));
  const retry = await within(dialog).findByRole('button', { name: 'Retry same command' });
  expect(screen.queryByRole('checkbox', { name: /review.*v1/ })).toBeNull();
  await userEvent.click(retry);
  await within(dialog).findByText('Skill folder imported and added to the AI-agent draft.');
  expect(roles.importSkill.mock.calls[0]).toEqual(roles.importSkill.mock.calls[1]);
  expect(roles.importSkill.mock.calls[0]?.[1]).toEqual({
    identity: 'review',
    files: [
      {
        path: 'SKILL.md',
        sha256: expect.stringMatching(/^[a-f0-9]{64}$/),
        bytes: Array.from(new TextEncoder().encode('# Skill\nInstructions')),
      },
    ],
  });
  await userEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
  expect(name).toHaveProperty('value', 'private local draft');
  expect(await screen.findByRole('checkbox', { name: /review.*v1/ })).toHaveProperty(
    'checked',
    true,
  );
  expect(screen.getByRole('checkbox', { name: /existing.*v2/ })).toHaveProperty('checked', true);
  await userEvent.click(screen.getByRole('button', { name: 'Review AI agent' }));
  expect(roles.validateRole).toHaveBeenLastCalledWith(
    role.reference.project_id,
    expect.objectContaining({
      definition: expect.objectContaining({ skills: { existing, review: bundle } }),
    }),
  );
});
it('blocks incompatible profile publication and reconciles a stale head without replacing the draft or resubmitting', async () => {
  const { roles, profile } = await openProfile();
  roles.validateProfile.mockResolvedValueOnce({ profile: null, incompatibility: 'dependencies' });
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  await screen.findByText('Profile is incompatible: dependencies');
  expect(screen.queryByRole('dialog')).toBeNull();
  roles.publish.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'precondition_failed',
      message: 'Stale',
      status: 412,
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.type(screen.getByRole('textbox', { name: 'Profile name' }), ' draft');
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  roles.currentProfile.mockResolvedValueOnce({
    ...profile,
    reference: { ...profile.reference, version: 2 },
  });
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(screen.getByRole('textbox', { name: 'Profile name' })).toHaveProperty(
    'value',
    'default draft',
  );
  expect(roles.publish).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  await screen.findByText('Immutable version published.');
  expect(roles.publish).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      profiles: [expect.objectContaining({ expected_current_version: 2 })],
    }),
    expect.anything(),
  );
});

it('copies a starter template into independent default/pro profiles using installed settings', async () => {
  const { roles, role, profile } = setup();
  const { capability } = factoryRoleFixture();
  roles.getCapability.mockResolvedValue({
    ...capability,
    definition: {
      ...capability.definition,
      skills: { installed: { identity: 'installed', version: 'v1', digest: 'd'.repeat(64) } },
    },
  });
  const reference = { identity: 'coder', version: 'v1', digest: 'c'.repeat(64) };
  roles.listTemplates.mockResolvedValue({
    items: [{ reference, name: 'Coder', profiles: ['default', 'pro'] }],
    total: 1,
    next_cursor: null,
  });
  roles.getTemplate.mockResolvedValue({
    identity: 'coder',
    version: 'v1',
    role: role.definition,
    profiles: {
      default: { name: 'default', instructions: ['Default only'] },
      pro: { name: 'pro', instructions: ['Pro only'] },
    },
  });
  roles.copyTemplate.mockResolvedValue({
    role,
    profiles: [profile],
    disposition: 'applied',
    published_at: 100,
  });
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Copy Coder' }));
  const editor = await screen.findByRole('region', { name: 'Copy AI-agent template' });
  for (const alias of ['default', 'pro']) {
    await userEvent.click(
      within(editor).getByRole('combobox', { name: `Runtime / Model · ${alias}` }),
    );
    await userEvent.click(await screen.findByRole('option', { name: /test.runtime.*test.model/ }));
  }
  expect(
    within(editor).getByRole('combobox', { name: 'Runtime / Model · pro' }).textContent,
  ).toContain('test.runtime');
  await userEvent.type(
    within(editor).getByRole('textbox', { name: 'Additional instruction · pro 1' }),
    ' extra',
  );
  await userEvent.click(within(editor).getByRole('button', { name: 'Review AI agent' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  await screen.findByText('Immutable version published.');
  expect(roles.copyTemplate).toHaveBeenCalledWith(
    role.reference.project_id,
    expect.objectContaining({
      template: reference,
      role_id: expect.any(String),
      profiles: {
        default: {
          id: expect.any(String),
          definition: expect.objectContaining({ instructions: ['Default only'] }),
        },
        pro: {
          id: expect.any(String),
          definition: expect.objectContaining({ instructions: ['Pro only extra'] }),
        },
      },
    }),
    expect.anything(),
  );
  const request = roles.copyTemplate.mock.calls[0]?.[1];
  expect(request.profiles.default.id).not.toBe(request.profiles.pro.id);
  expect(request.profiles.default.definition.skills).toEqual({});
  expect(request.profiles.pro.definition.skills).toEqual({});
});

it('explains missing runtime configuration for default/pro instead of claiming no resources', async () => {
  const { roles, role } = setup();
  const reference = { identity: 'coder', version: 'v1', digest: 'c'.repeat(64) };
  roles.listTemplates.mockResolvedValue({
    items: [{ reference, name: 'Coder', profiles: ['default', 'pro'] }],
    total: 1,
    next_cursor: null,
  });
  roles.getTemplate.mockResolvedValue({
    identity: 'coder',
    version: 'v1',
    role: role.definition,
    profiles: {
      default: { name: 'default', instructions: [] },
      pro: { name: 'pro', instructions: [] },
    },
  });
  roles.listCapabilities.mockResolvedValue({ items: [], total: 0, next_cursor: null });
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Copy Coder' }));
  expect(
    await screen.findByText(
      'No Factory runtime/model is configured for this Project. Configure one on the server before creating execution profiles.',
    ),
  ).toBeTruthy();
  for (const alias of ['default', 'pro']) {
    expect(screen.getByRole('combobox', { name: `Runtime / Model · ${alias}` })).toHaveProperty(
      'disabled',
      true,
    );
  }
  expect(roles.copyTemplate).not.toHaveBeenCalled();
});

it('keeps profile instructions pinned until the current role version is explicitly adopted', async () => {
  const { roles, role, profile } = setup();
  const newer = {
    ...role,
    reference: { ...role.reference, version: 2, digest: 'b'.repeat(64) },
    definition: { ...role.definition, instructions: ['New shared rules'] },
  };
  roles.listRoles.mockResolvedValue({
    items: [
      {
        reference: newer.reference,
        ...newer.definition,
        icon: 'bot',
        concurrency_limit: 2,
        template: null,
      },
    ],
    total: 1,
    next_cursor: null,
  });
  roles.getRole.mockResolvedValueOnce(newer).mockResolvedValueOnce(role);
  roles.validateProfile.mockImplementation(async (_project, request) => ({
    profile: { ...profile, role: request.role, definition: request.definition },
    incompatibility: null,
  }));
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default' }));
  const detail = screen.getByRole('heading', { name: 'default · v1' }).parentElement!;
  expect(await within(detail).findByText('Follow Project rules.')).toBeTruthy();
  expect(within(detail).queryByText('New shared rules')).toBeNull();
  await userEvent.click(within(detail).getByRole('button', { name: 'Edit profile' }));
  await userEvent.click(
    screen.getByRole('button', { name: 'Adopt agent v2 (instructions and skills)' }),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  expect(roles.validateProfile).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({ role: newer.reference }),
  );
});
it('reconciles a stale role publication while preserving its own edited instructions', async () => {
  const { roles, role } = setup();
  roles.currentRole.mockResolvedValue({ ...role, reference: { ...role.reference, version: 2 } });
  roles.publish.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'precondition_failed',
      message: 'Stale',
      status: 412,
      requestId: null,
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit AI agent' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Shared instruction 1' }), ' local');
  await userEvent.click(screen.getByRole('button', { name: 'Review AI agent' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  expect(screen.getByRole('textbox', { name: 'Shared instruction 1' })).toHaveProperty(
    'value',
    'Follow Project rules. local',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review AI agent' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  await screen.findByText('Immutable version published.');
  expect(roles.publish).toHaveBeenLastCalledWith(
    role.reference.project_id,
    expect.objectContaining({ role: expect.objectContaining({ expected_current_version: 2 }) }),
    expect.anything(),
  );
});

it('keeps installed CLI vectors visible and immutable rather than offering unsupported argument edits', async () => {
  await openProfile();
  expect(await screen.findByText('--structured')).toBeTruthy();
  expect(screen.queryByRole('checkbox', { name: '--structured' })).toBeNull();
});

it('matches installed Runtime choices by complete identities instead of borrowing another alias with equal content digests', async () => {
  const { roles, role, profile } = setup();
  const { choice } = factoryRoleFixture();
  roles.listCapabilities.mockResolvedValue({
    items: [
      {
        ...choice,
        alias: 'different-runtime',
        toolchain: {
          ...choice.toolchain,
          model: { ...choice.toolchain.model, identity: 'different-model' },
        },
      },
      choice,
    ],
    total: 2,
    next_cursor: null,
  });
  roles.validateProfile.mockImplementation(async (_project, request) => ({
    profile: { ...profile, definition: request.definition },
    incompatibility: null,
  }));
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit profile' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  expect(roles.validateProfile).toHaveBeenCalledWith(
    role.reference.project_id,
    expect.objectContaining({ capability: 'installed-runtime' }),
  );
});

it('keeps authored settings when choosing a Runtime and resets them only through an explicit installed-settings action', async () => {
  await openProfile();
  const language = await screen.findByRole('textbox', { name: 'LANGUAGE' });
  await userEvent.clear(language);
  await userEvent.type(language, 'fr');
  await userEvent.click(screen.getByRole('combobox', { name: 'Runtime / Model' }));
  await userEvent.click(await screen.findByRole('option', { name: /test.runtime.*test.model/ }));
  expect(language).toHaveProperty('value', 'fr');
  await userEvent.click(screen.getByRole('button', { name: 'Use installed settings' }));
  expect(language).toHaveProperty('value', 'en');
  expect(screen.getByRole('textbox', { name: 'Additional instruction 1' })).toHaveProperty(
    'value',
    'Use selected skills.',
  );
});

it('keeps retained legacy profile skills readable without offering a separate profile skill editor', async () => {
  const { roles, profile } = setup();
  const bundle = { identity: 'review.bundle', version: 'v1', digest: 'f'.repeat(64) };
  roles.getProfile.mockResolvedValue({
    ...profile,
    definition: { ...profile.definition, skills: { reviewer: bundle } },
  });
  roles.validateProfile.mockImplementation(async (_project, request) => ({
    profile: { ...profile, definition: request.definition },
    incompatibility: null,
  }));
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit profile' }));
  expect(screen.queryByLabelText('Import skill folder')).toBeNull();
  expect(screen.queryByRole('checkbox', { name: /reviewer.*review.bundle.*v1/ })).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  expect(roles.validateProfile).toHaveBeenCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      definition: expect.objectContaining({ skills: { reviewer: bundle } }),
    }),
  );
});

it('edits distinct installed permission categories without offering tool references as plugins or executables', async () => {
  const { roles, profile } = setup();
  const { capability } = factoryRoleFixture();
  const plugin = { identity: 'runtime-plugin', version: 'v1', digest: 'a'.repeat(64) };
  const executable = { identity: 'runtime-executable', version: 'v1', digest: 'b'.repeat(64) };
  const tool = { identity: 'review-tool', version: 'v1', digest: 'c'.repeat(64) };
  const permissions = {
    ...profile.definition.permissions,
    plugins: [plugin],
    executables: [executable],
    tools: [tool],
    secret_profiles: ['model-credential'],
    workload_identity_profiles: ['workload-identity'],
  };
  roles.getCapability.mockResolvedValue({
    ...capability,
    definition: { ...capability.definition, permissions },
  });
  roles.getProfile.mockResolvedValue({
    ...profile,
    definition: { ...profile.definition, permissions },
  });
  roles.validateProfile.mockImplementation(async (_project, request) => ({
    profile: { ...profile, definition: request.definition },
    incompatibility: null,
  }));
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit profile' }));
  const pluginInput = await screen.findByRole('checkbox', { name: 'Plugin · runtime-plugin · v1' });
  expect(pluginInput).toHaveProperty('checked', true);
  expect(screen.queryByRole('checkbox', { name: 'Plugin · review-tool · v1' })).toBeNull();
  expect(screen.queryByRole('checkbox', { name: 'Executable · review-tool · v1' })).toBeNull();
  const executableInput = screen.getByRole('checkbox', {
    name: 'Executable · runtime-executable · v1',
  });
  const toolInput = screen.getByRole('checkbox', { name: 'Tool · review-tool · v1' });
  const secretInput = screen.getByRole('checkbox', { name: 'model-credential' });
  const identityInput = screen.getByRole('checkbox', { name: 'workload-identity' });
  for (const input of [pluginInput, executableInput, toolInput, secretInput, identityInput]) {
    expect(input).toHaveProperty('checked', true);
    await userEvent.click(input);
  }
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  expect(roles.validateProfile).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      definition: expect.objectContaining({
        permissions: expect.objectContaining({
          plugins: [],
          executables: [],
          tools: [],
          secret_profiles: [],
          workload_identity_profiles: [],
        }),
      }),
    }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Abandon' }));
  for (const input of [pluginInput, executableInput, toolInput, secretInput, identityInput])
    await userEvent.click(input);
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  expect(roles.validateProfile).toHaveBeenLastCalledWith(
    profile.role.project_id,
    expect.objectContaining({
      definition: expect.objectContaining({ permissions }),
    }),
  );
});

it('deletes an AI agent only after review and safely retries a lost response before clearing its card', async () => {
  const { roles, role } = setup();
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  const remove = screen.getByRole('button', { name: 'Delete AI agent' });
  await userEvent.click(remove);
  let dialog = await screen.findByRole('dialog');
  expect(within(dialog).getByText(/Run history is retained/)).toBeTruthy();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Abandon' }));
  expect(roles.deleteRole).not.toHaveBeenCalled();
  expect(remove).toHaveProperty('isConnected', true);
  roles.deleteRole.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'transport_failure',
      message: 'Unavailable',
      requestId: null,
      retryAfterMilliseconds: null,
      status: null,
    }),
  );
  await userEvent.click(remove);
  dialog = await screen.findByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Delete AI agent' }));
  const retry = await within(dialog).findByRole('button', { name: 'Retry same command' });
  roles.listRoles.mockResolvedValue({ items: [], total: 0, next_cursor: null });
  await userEvent.click(retry);
  await within(dialog).findByText('AI agent deleted.');
  expect(roles.deleteRole).toHaveBeenCalledTimes(2);
  expect(roles.deleteRole.mock.calls[0]).toEqual(roles.deleteRole.mock.calls[1]);
  expect(roles.deleteRole).toHaveBeenCalledWith(
    role.reference.project_id,
    role.reference.id,
    expect.objectContaining({ 'If-Match': '"1"', 'Idempotency-Key': expect.any(String) }),
  );
  await userEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
  expect(screen.queryByRole('heading', { name: /Code specialist/ })).toBeNull();
  expect(screen.queryByRole('button', { name: 'Code specialist' })).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Create AI agent' }));
});

it('selects plugin model suggestions and preserves a manual model and draft through refresh', async () => {
  const { roles, profile } = setup();
  const { capability } = factoryRoleFixture();
  const choice = {
    identifier: 'runtime/suggested',
    label: 'Suggested model',
    parameters: capability.parameters,
  };
  const installed = {
    ...capability,
    model_catalog: { available: true, manual_model: true, models: [choice] },
  };
  roles.getProfile.mockResolvedValue({
    ...profile,
    definition: { ...profile.definition, model_identifier: 'runtime/suggested' },
  });
  roles.getCapability.mockResolvedValue(installed);
  roles.refreshCapability
    .mockRejectedValueOnce(new Error('catalog offline'))
    .mockResolvedValueOnce({
      ...installed,
      model_catalog: {
        ...installed.model_catalog,
        models: [{ ...choice, identifier: 'runtime/new', label: 'New model' }],
      },
    });
  roles.validateProfile.mockImplementation(async (_project, request) => ({
    profile: { ...profile, definition: request.definition },
    incompatibility: null,
  }));
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default' }));
  expect(
    await screen.findByText(`${profile.definition.toolchain.plugin.identity} · runtime/suggested`),
  ).toBeTruthy();
  await userEvent.click(await screen.findByRole('button', { name: 'Edit profile' }));
  const model = await screen.findByRole('textbox', { name: 'Model identifier' });
  await userEvent.click(screen.getByRole('combobox', { name: 'Suggested models' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Suggested model' }));
  expect(model).toHaveProperty('value', 'runtime/suggested');
  await userEvent.clear(model);
  await userEvent.type(model, 'Vendor/future:preview');
  await userEvent.clear(screen.getByRole('textbox', { name: 'Profile name' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Profile name' }), 'my profile');
  await userEvent.click(screen.getByRole('button', { name: 'Refresh models' }));
  await screen.findByRole('alert');
  expect(model).toHaveProperty('value', 'Vendor/future:preview');
  await userEvent.click(screen.getByRole('button', { name: 'Refresh models' }));
  await waitFor(() => expect(roles.refreshCapability).toHaveBeenCalledTimes(2));
  expect(model).toHaveProperty('value', 'Vendor/future:preview');
  expect(screen.getByRole('textbox', { name: 'Profile name' })).toHaveProperty(
    'value',
    'my profile',
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Suggested models' }));
  expect(await screen.findByRole('option', { name: 'New model' })).toBeTruthy();
  await userEvent.keyboard('{Escape}');
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  await screen.findByRole('dialog');
  expect(roles.validateProfile).toHaveBeenCalledWith(
    expect.anything(),
    expect.objectContaining({
      definition: expect.objectContaining({
        model_identifier: 'Vendor/future:preview',
        name: 'my profile',
        model_parameters: profile.definition.model_parameters,
        thinking_parameter: profile.definition.thinking_parameter,
      }),
    }),
  );
});

it('removes unsupported thinking when explicitly selecting a model without parameters', async () => {
  const { roles, profile } = setup();
  const { capability } = factoryRoleFixture();
  roles.getCapability.mockResolvedValue({
    ...capability,
    model_catalog: {
      available: true,
      manual_model: true,
      models: [
        {
          identifier: 'runtime/plain',
          label: 'Plain model',
          parameters: { ...capability.parameters, shape: { type: 'object', fields: {} } },
        },
      ],
    },
  });
  roles.validateProfile.mockImplementation(async (_project, request) => ({
    profile: { ...profile, definition: request.definition },
    incompatibility: null,
  }));
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit profile' }));
  await screen.findByRole('textbox', { name: 'Model identifier' });
  await userEvent.click(screen.getByRole('combobox', { name: 'Suggested models' }));
  await userEvent.click(await screen.findByRole('option', { name: 'Plain model' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  await screen.findByRole('dialog');
  expect(roles.validateProfile).toHaveBeenCalledWith(
    expect.anything(),
    expect.objectContaining({
      definition: expect.objectContaining({
        model_identifier: 'runtime/plain',
        model_parameters: {},
        thinking_parameter: null,
      }),
    }),
  );
});

it('keeps model-specific parameters while an identifier is being edited and restored', async () => {
  const { roles, profile } = setup();
  const { capability } = factoryRoleFixture();
  roles.getProfile.mockResolvedValue({
    ...profile,
    definition: {
      ...profile.definition,
      model_identifier: 'runtime/custom',
    },
  });
  roles.getCapability.mockResolvedValue({
    ...capability,
    parameters: { ...capability.parameters, shape: { type: 'object', fields: {} } },
    model_catalog: {
      available: true,
      manual_model: true,
      models: [
        {
          identifier: 'runtime/custom',
          label: 'Custom model',
          parameters: capability.parameters,
        },
      ],
    },
  });
  roles.validateProfile.mockImplementation(async (_project, request) => ({
    profile: { ...profile, definition: request.definition },
    incompatibility: null,
  }));
  await userEvent.click(screen.getByRole('button', { name: 'AI agents' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit profile' }));
  const model = await screen.findByRole('textbox', { name: 'Model identifier' });
  await userEvent.clear(model);
  await userEvent.type(model, 'runtime/custom');
  await userEvent.click(screen.getByRole('button', { name: 'Review profile' }));
  await screen.findByRole('dialog');
  expect(roles.validateProfile).toHaveBeenCalledWith(
    expect.anything(),
    expect.objectContaining({
      definition: expect.objectContaining({
        model_identifier: 'runtime/custom',
        model_parameters: profile.definition.model_parameters,
        thinking_parameter: 'deliberation',
      }),
    }),
  );
});
