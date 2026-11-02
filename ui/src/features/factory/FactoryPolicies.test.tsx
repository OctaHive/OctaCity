// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { ManagementApiError } from '../../api/client';
import { createConsoleQueryClient } from '../../app/query';
import { emptyFactoryPermissions } from './emptyFlowDraft';
import { FactoryPolicies } from './FactoryPolicies';

type S = components['schemas'];
const project = '11111111-1111-4111-8111-111111111111';
function launch(): Extract<S['FactoryProjectPolicyRecord'], { kind: 'launch' }> {
  return {
    kind: 'launch',
    policy: {
      reference: {
        project_id: project,
        id: '22222222-2222-4222-8222-222222222222',
        version: 1,
        digest: 'a'.repeat(64),
      },
      definition: { name: 'Standard launch', allow_manual: true, allow_automatic_source: false },
    },
  };
}
function access(): Extract<S['FactoryProjectPolicyRecord'], { kind: 'access' }> {
  return {
    kind: 'access',
    policy: {
      reference: {
        project_id: project,
        id: '33333333-3333-4333-8333-333333333333',
        version: 1,
        digest: 'b'.repeat(64),
      },
      definition: { name: 'Standard access', permissions: emptyFactoryPermissions() },
    },
  };
}
function setup(records: S['FactoryProjectPolicyRecord'][] = [launch(), access()]) {
  const api = {
    list: vi.fn(async () => ({
      items: records.map(({ kind, policy }) => ({
        name: policy.definition.name,
        reference: { kind, reference: policy.reference },
      })),
      next_cursor: null,
    })),
    get: vi.fn(async (_project: string, ref: S['FactoryProjectPolicyRef']) => ({
      policy: records.find((record) => record.kind === ref.kind)!,
    })),
    defaults: vi.fn(async () => ({
      defaults: {
        project_id: project,
        version: 1,
        launch_policy: launch().policy.reference,
        access_profile: access().policy.reference,
      },
    })),
    resolveLegacy: vi.fn(async () => ({ mapping: null })),
    validate: vi.fn(),
    publish: vi.fn(),
    selectDefaults: vi.fn(),
    choices: vi.fn(async (): Promise<S['FactoryConfigurationChoicePage']> => ({
      items: [],
      next_cursor: null,
      total: 0,
    })),
  };
  const client = createConsoleQueryClient();
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <QueryClientProvider client={client}>
          <FactoryPolicies project={project} api={api} />
        </QueryClientProvider>
      ),
    },
  ]);
  render(<RouterProvider router={router} />);
  return { api, records, client };
}
afterEach(cleanup);

it('shows separate named catalogs and exact defaults, then views rules without writing or starting execution', async () => {
  const { api } = setup();
  const catalog = within(await screen.findByRole('region', { name: 'Launch policies' }));
  await userEvent.click(await catalog.findByRole('button', { name: 'View Standard launch · v1' }));
  const view = within(await screen.findByRole('region', { name: 'Policy details' }));
  expect(view.getByText('Manual admission allowed')).toBeTruthy();
  expect(view.getByText('Automatic Source admission denied')).toBeTruthy();
  expect(screen.getByRole('region', { name: 'Access profiles' }).textContent).toContain(
    'Standard access',
  );
  expect(catalog.getByText('Project default · v1')).toBeTruthy();
  expect(view.queryByRole('textbox')).toBeNull();
  expect(api.publish).not.toHaveBeenCalled();
  expect(api.selectDefaults).not.toHaveBeenCalled();
  expect(api.validate).not.toHaveBeenCalled();
});

it('creates a deny-both launch policy only after explicit review and confirmation', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'New launch policy' }));
  const editor = within(screen.getByRole('region', { name: 'Policy editor' }));
  await userEvent.type(editor.getByRole('textbox', { name: 'Policy name' }), 'Paused intake');
  const saved: S['FactoryProjectPolicyRecord'] = {
    ...launch(),
    policy: {
      ...launch().policy,
      definition: { name: 'Paused intake', allow_manual: false, allow_automatic_source: false },
    },
  };
  api.validate.mockResolvedValue({ policy: saved });
  api.publish.mockResolvedValue({ policy: saved, disposition: 'applied', published_at: 10 });
  await userEvent.click(editor.getByRole('button', { name: 'Review policy' }));
  expect(api.publish).not.toHaveBeenCalled();
  expect(api.selectDefaults).not.toHaveBeenCalled();
  const dialog = within(await screen.findByRole('dialog'));
  expect(dialog.getByText('Manual admission denied')).toBeTruthy();
  expect(dialog.getByText('Automatic Source admission denied')).toBeTruthy();
  await userEvent.click(dialog.getByRole('button', { name: 'Publish policy' }));
  expect(
    await dialog.findByText(
      'Policy published. Existing configurations and Runs keep their versions.',
    ),
  ).toBeTruthy();
  expect(api.publish).toHaveBeenCalledWith(
    project,
    expect.objectContaining({
      kind: 'launch',
      expected_current: null,
      definition: { name: 'Paused intake', allow_manual: false, allow_automatic_source: false },
    }),
    expect.objectContaining({ 'Idempotency-Key': expect.any(String) }),
  );
});

it('preserves a replacement draft on conflict and requires explicit review against the refreshed head', async () => {
  const { api, records } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'View Standard launch · v1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit policy' }));
  const name = screen.getByRole('textbox', { name: 'Policy name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'My reviewed rules');
  const saved = launch();
  saved.policy.definition.name = 'My reviewed rules';
  saved.policy.reference.version = 2;
  api.validate.mockResolvedValue({ policy: saved });
  api.publish.mockRejectedValue(
    new ManagementApiError({
      code: 'conflict',
      message: 'Policy changed',
      status: 409,
      requestId: 'policy-conflict',
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review policy' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Publish policy' }),
  );
  const dialog = within(await screen.findByRole('dialog'));
  expect(await dialog.findByText('Policy changed')).toBeTruthy();
  expect(dialog.queryByRole('button', { name: 'Retry same request' })).toBeNull();
  const other = launch();
  other.policy.reference.version = 2;
  other.policy.reference.digest = 'c'.repeat(64);
  other.policy.definition.allow_automatic_source = true;
  records[0] = other;
  saved.policy.reference.version = 3;
  await userEvent.click(dialog.getByRole('button', { name: 'Refresh current data' }));
  expect(screen.getByRole('textbox', { name: 'Policy name' })).toHaveProperty(
    'value',
    'My reviewed rules',
  );
  expect(api.publish).toHaveBeenCalledTimes(1);
  expect(api.validate.mock.calls[0]![1].expected_current).toEqual(launch().policy.reference);
  await userEvent.click(screen.getByRole('button', { name: 'Review policy' }));
  expect(api.validate.mock.lastCall![1].expected_current).toEqual(other.policy.reference);
  const comparison = within(await screen.findByRole('dialog'));
  expect(
    within(comparison.getByRole('region', { name: 'Current saved policy' })).getByText(
      'Automatic Source admission allowed',
    ),
  ).toBeTruthy();
  expect(
    within(comparison.getByRole('region', { name: 'Proposed policy' })).getByText(
      'Automatic Source admission denied',
    ),
  ).toBeTruthy();
  expect(api.publish).toHaveBeenCalledTimes(1);
});

it('refreshes a stale version rejected during validation without changing the entered name', async () => {
  const { api, records } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'View Standard launch · v1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit policy' }));
  const name = screen.getByRole('textbox', { name: 'Policy name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'Keep my changes');
  api.validate.mockRejectedValueOnce(
    new ManagementApiError({
      code: 'conflict',
      message: 'Policy changed before review',
      status: 409,
      requestId: 'validation-conflict',
      retryAfterMilliseconds: null,
    }),
  );
  const other = launch();
  other.policy.reference.version = 2;
  other.policy.reference.digest = 'c'.repeat(64);
  records[0] = other;
  await userEvent.click(screen.getByRole('button', { name: 'Review policy' }));
  await screen.findByText('Policy changed before review');
  await userEvent.click(screen.getByRole('button', { name: 'Refresh current data' }));
  expect(await screen.findByText(/Editing.*v2/)).toBeTruthy();
  expect(name).toHaveProperty('value', 'Keep my changes');
  expect(api.validate).toHaveBeenCalledTimes(1);
  expect(api.publish).not.toHaveBeenCalled();
  api.validate.mockResolvedValue({ policy: other });
  await userEvent.click(screen.getByRole('button', { name: 'Review policy' }));
  await screen.findByRole('dialog');
  expect(api.validate.mock.lastCall![1].expected_current).toEqual(other.policy.reference);
});

it('selects the exact viewed default only after confirmation and preserves the other default', async () => {
  const replacement = launch();
  replacement.policy.reference.version = 2;
  replacement.policy.reference.digest = 'c'.repeat(64);
  const { api } = setup([replacement, access()]);
  await userEvent.click(await screen.findByRole('button', { name: 'View Standard launch · v2' }));
  const target = within(await screen.findByRole('region', { name: 'Policy details' }));
  await userEvent.click(target.getByRole('button', { name: 'Use as Project default' }));
  expect(api.selectDefaults).not.toHaveBeenCalled();
  const defaults = {
    project_id: project,
    version: 2,
    launch_policy: replacement.policy.reference,
    access_profile: access().policy.reference,
  };
  api.selectDefaults.mockResolvedValue({ defaults, disposition: 'applied', published_at: 1 });
  const dialog = within(await screen.findByRole('dialog'));
  await userEvent.click(dialog.getByRole('button', { name: 'Confirm default' }));
  expect(
    await dialog.findByText('Project default saved. Existing pipeline selections are unchanged.'),
  ).toBeTruthy();
  expect(api.selectDefaults).toHaveBeenCalledWith(
    project,
    {
      expected_version: 1,
      launch_policy: replacement.policy.reference,
      access_profile: access().policy.reference,
    },
    expect.objectContaining({ 'Idempotency-Key': expect.any(String) }),
  );
  expect(api.publish).not.toHaveBeenCalled();
});

it('narrows access with typed installed controls and preserves unrelated permission categories', async () => {
  const { api, records } = setup();
  const baseline = access();
  baseline.policy.definition.permissions.network_hosts = [
    'api.allowed.example',
    'mcp.allowed.example',
  ];
  baseline.policy.definition.permissions.secret_profiles = ['model-login'];
  baseline.policy.definition.permissions.mounts = [{ root: '/workspace', mode: 'read_only' }];
  baseline.policy.definition.permissions.resources.cpu_millis = 500;
  records[1] = baseline;
  api.choices.mockResolvedValue({
    items: [
      {
        alias: 'Reviewed installation',
        identity: 'installed-ceiling',
        kind: 'permission_ceiling',
        reference: {
          kind: 'immutable',
          reference: { identity: 'installed-ceiling', version: 'v1', digest: 'd'.repeat(64) },
        },
        metadata: {
          compatible_choices: [],
          enforcement: [
            'network_hosts',
            'secret_profiles',
            'filesystem_paths',
            'mount_modes',
            'cpu',
          ],
          node_kinds: [],
          signal_purposes: [],
          permission_declarations: baseline.policy.definition.permissions,
        },
      },
    ],
    next_cursor: null,
    total: 1,
  });
  await userEvent.click(await screen.findByRole('button', { name: 'View Standard access · v1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit policy' }));
  await userEvent.click(await screen.findByText('Supported access settings'));
  await userEvent.click(
    await screen.findByRole('checkbox', { name: 'Network host · mcp.allowed.example' }),
  );
  const narrowed = structuredClone(baseline);
  narrowed.policy.reference.version = 2;
  narrowed.policy.definition.permissions.network_hosts = ['api.allowed.example'];
  api.validate.mockResolvedValue({ policy: narrowed });
  await userEvent.click(screen.getByRole('button', { name: 'Review policy' }));
  await screen.findByRole('dialog');
  expect(api.validate).toHaveBeenCalledWith(
    project,
    expect.objectContaining({
      kind: 'access',
      expected_current: baseline.policy.reference,
      definition: {
        name: 'Standard access',
        permissions: {
          ...baseline.policy.definition.permissions,
          network_hosts: ['api.allowed.example'],
        },
      },
    }),
  );
  expect(screen.getByRole('region', { name: 'Policy editor' }).textContent).toContain(
    'Installation',
  );
  expect(api.publish).not.toHaveBeenCalled();
});

it('uses installed declarations without an enforcement list and saves zero resource limits', async () => {
  const { api } = setup();
  const permissions = emptyFactoryPermissions();
  permissions.network_hosts = ['api.allowed.example'];
  permissions.resources.cpu_millis = 500;
  api.choices.mockResolvedValue({
    items: [
      {
        alias: 'Stand ceiling',
        identity: 'stand-ceiling',
        kind: 'permission_ceiling',
        reference: {
          kind: 'immutable',
          reference: { identity: 'stand-ceiling', version: 'v1', digest: 'd'.repeat(64) },
        },
        metadata: {
          compatible_choices: [],
          enforcement: [],
          node_kinds: [],
          signal_purposes: [],
          permission_declarations: permissions,
        },
      },
    ],
    next_cursor: null,
    total: 1,
  });
  await userEvent.click(await screen.findByRole('button', { name: 'New access profile' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Policy name' }), 'Network only');
  await userEvent.click(screen.getByText('Supported access settings'));
  const host = await screen.findByRole('checkbox', { name: 'Network host · api.allowed.example' });
  await userEvent.click(host);
  expect(host).toHaveProperty('checked', true);
  const requested = { ...emptyFactoryPermissions(), network_hosts: ['api.allowed.example'] };
  const saved = access();
  saved.policy.definition = { name: 'Network only', permissions: requested };
  api.validate.mockResolvedValue({ policy: saved });
  await userEvent.click(screen.getByRole('button', { name: 'Review policy' }));
  await screen.findByRole('dialog');
  expect(api.validate).toHaveBeenCalledWith(
    project,
    expect.objectContaining({ definition: saved.policy.definition }),
  );
});

it('shows authoritative access rejection at the permission field and keeps the draft unpublished', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'New access profile' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Policy name' }), 'Restricted access');
  expect(await screen.findByText(/No qualified installed access declarations/)).toBeTruthy();
  api.validate.mockRejectedValue(
    new ManagementApiError({
      code: 'invalid_request',
      message: 'Access unavailable',
      status: 400,
      requestId: 'access-denied',
      retryAfterMilliseconds: null,
      validationIssues: [
        {
          field: '/definition/permissions',
          code: 'permission_exceeds_ceiling',
          ceiling: 'installation',
        },
      ],
    }),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review policy' }));
  const issue = await screen.findByRole('button', { name: 'Requested access · Installation' });
  await userEvent.click(issue);
  expect(screen.getByRole('group', { name: 'Requested access' })).toBe(document.activeElement);
  expect(screen.getByRole('textbox', { name: 'Policy name' })).toHaveProperty(
    'value',
    'Restricted access',
  );
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(api.publish).not.toHaveBeenCalled();
});

it('retains edits after a forbidden publication and never retries it automatically', async () => {
  const { api } = setup();
  await userEvent.click(await screen.findByRole('button', { name: 'New launch policy' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Policy name' }), 'Read only operator');
  api.validate.mockResolvedValue({ policy: launch() });
  api.publish.mockRejectedValue(
    new ManagementApiError({
      code: 'forbidden',
      message: 'Policy update forbidden',
      status: 403,
      requestId: 'forbidden-update',
      retryAfterMilliseconds: null,
    }),
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review policy' }));
  const dialog = within(await screen.findByRole('dialog'));
  await userEvent.click(dialog.getByRole('button', { name: 'Publish policy' }));
  expect(await dialog.findByText('Policy update forbidden')).toBeTruthy();
  await userEvent.click(dialog.getByRole('button', { name: 'Abandon' }));
  expect(screen.getByRole('textbox', { name: 'Policy name' })).toHaveProperty(
    'value',
    'Read only operator',
  );
  expect(api.publish).toHaveBeenCalledTimes(1);
  expect(api.selectDefaults).not.toHaveBeenCalled();
});

it('reviews a retained numeric grant with multiple ceilings and names the server-verified envelope', async () => {
  const { api, records } = setup();
  const baseline = access();
  baseline.policy.definition.permissions.resources.cpu_millis = 500;
  records[1] = baseline;
  const makeCeiling = (alias: string, cpu: number) => ({
    alias,
    identity: alias,
    kind: 'permission_ceiling' as const,
    reference: {
      kind: 'immutable' as const,
      reference: { identity: alias, version: 'v1', digest: 'd'.repeat(64) },
    },
    metadata: {
      compatible_choices: [],
      enforcement: ['cpu'] as ['cpu'],
      node_kinds: [],
      signal_purposes: [],
      permission_declarations: {
        ...emptyFactoryPermissions(),
        resources: { ...emptyFactoryPermissions().resources, cpu_millis: cpu },
      },
    },
  });
  const small = makeCeiling('small', 100);
  const large = makeCeiling('large', 750);
  api.choices.mockResolvedValue({ items: [small, large], next_cursor: null, total: 2 });
  const proposed = structuredClone(baseline);
  proposed.policy.definition.name = 'Renamed access';
  proposed.policy.reference.version = 2;
  api.validate.mockResolvedValue({
    policy: proposed,
    installed_ceiling: {
      alias: large.alias,
      reference: large.reference.reference,
      permissions: large.metadata.permission_declarations,
    },
  });
  await userEvent.click(await screen.findByRole('button', { name: 'View Standard access · v1' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Edit policy' }));
  const name = screen.getByRole('textbox', { name: 'Policy name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'Renamed access');
  await userEvent.click(screen.getByRole('button', { name: 'Review policy' }));
  expect(api.validate).toHaveBeenCalledWith(
    project,
    expect.objectContaining({ definition: proposed.policy.definition }),
  );
  const preview = within(
    await screen.findByRole('region', { name: 'Validated installation ceiling' }),
  );
  expect(preview.getByText('large · v1')).toBeTruthy();
  await userEvent.click(preview.getByText('Permission details', { selector: 'summary' }));
  expect(preview.getByRole('spinbutton', { name: /cpu_millis/ })).toHaveProperty('value', '750');
  expect(api.publish).not.toHaveBeenCalled();
});
