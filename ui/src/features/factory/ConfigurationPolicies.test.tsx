// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { ManagementApiError } from '../../api/client';
import { createConsoleQueryClient } from '../../app/query';
import { FactoryDefinitions } from './FactoryDefinitions';
import { FactoryAuthoring } from './FactoryAuthoring';
import { factoryApi } from './api';
import { emptyFactoryPermissions } from './emptyFlowDraft';
import { factoryPolicyApi, type PolicyRecord } from './policyApi';

const baseline = configurationFixture();
const project = baseline.configuration.reference.project_id;
const launch: PolicyRecord = {
  kind: 'launch',
  policy: {
    reference: {
      project_id: project,
      id: 'd6e5c639-258e-4eb3-9662-051d371462bb',
      version: 2,
      digest: 'a'.repeat(64),
    },
    definition: { name: 'Standard launch', allow_manual: true, allow_automatic_source: false },
  },
};
const access: PolicyRecord = {
  kind: 'access',
  policy: {
    reference: {
      project_id: project,
      id: 'a7a3180f-b7a0-400b-a721-0a4cedf253e2',
      version: 1,
      digest: 'b'.repeat(64),
    },
    definition: {
      name: 'Standard access',
      permissions: { ...emptyFactoryPermissions(), network_hosts: ['api.example.test'] },
    },
  },
};
const custom: PolicyRecord = {
  kind: 'launch',
  policy: {
    reference: { ...launch.policy.reference, version: 3, digest: 'c'.repeat(64) },
    definition: { name: 'Source only', allow_manual: false, allow_automatic_source: true },
  },
};

function setup(existing?: typeof baseline, workbench = Boolean(existing)) {
  const records = [launch, access, custom];
  const defaults = {
    project_id: project,
    version: 1,
    launch_policy: launch.policy.reference,
    access_profile: access.policy.reference,
  };
  vi.spyOn(factoryPolicyApi, 'defaults').mockResolvedValue({ defaults });
  vi.spyOn(factoryPolicyApi, 'list').mockResolvedValue({
    items: [custom, access].map(({ kind, policy }) => ({
      name: policy.definition.name,
      reference: { kind, reference: policy.reference },
    })),
    next_cursor: null,
  });
  vi.spyOn(factoryPolicyApi, 'get').mockImplementation(async (_project, selected) => ({
    policy: records.find(
      ({ kind, policy }) =>
        kind === selected.kind && policy.reference.digest === selected.reference.digest,
    )!,
  }));
  const api = {
    ...factoryApi,
    listConfigurations: vi.fn(async () => ({
      items: existing
        ? [
            {
              ...existing.configuration.reference,
              enabled: existing.configuration.enabled,
              created_at_unix_ms: 1,
              published_at_unix_ms: 1,
            },
          ]
        : [],
      total: existing ? 1 : 0,
      next_cursor: null,
    })),
    getConfiguration: vi.fn(async () => existing ?? baseline),
    listFlows: vi.fn(async () => ({
      items: [{ flow: baseline.configuration.flow, project_id: project, published_at_unix_ms: 1 }],
      total: 1,
      next_cursor: null,
    })),
    listChoices: vi.fn(async () => ({
      items: [],
      total: 0,
      next_cursor: null,
      project_policy_defaults: defaults,
    })),
    validateConfiguration: vi.fn(async () => {
      throw new Error('Fixture validates request shape only');
    }),
    createConfiguration: vi.fn(),
  };
  const client = createConsoleQueryClient();
  render(
    <QueryClientProvider client={client}>
      <RouterProvider
        router={createMemoryRouter([
          {
            path: '*',
            element: workbench ? (
              <FactoryDefinitions api={api} projectId={project} />
            ) : (
              <FactoryAuthoring
                api={api}
                projectId={project}
                kind="configuration"
                onDiscard={() => {}}
              />
            ),
          },
        ])}
      />
    </QueryClientProvider>,
  );
  return { api, client, records };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it('selects exact defaults including a historical version and submits a deliberate override', async () => {
  const { api } = setup();
  const launchPicker = await screen.findByRole('combobox', { name: 'Launch policy' });
  expect(await within(launchPicker).findByText('Standard launch · v2')).toBeTruthy();
  expect(
    within(screen.getByRole('combobox', { name: 'Access profile' })).getByText(
      'Standard access · v1',
    ),
  ).toBeTruthy();
  expect(screen.getByRole('combobox', { name: 'Root Flow' })).toBeTruthy();
  expect(
    await screen.findByText('Plugins 0 · Executables 0 · Tools 0 · Network hosts 1'),
  ).toBeTruthy();
  expect(screen.getByText('Manual admission allowed')).toBeTruthy();
  await userEvent.click(launchPicker);
  await userEvent.click(await screen.findByRole('option', { name: 'Source only · v3' }));
  expect(await screen.findByText('Automatic Source admission allowed')).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  expect(api.validateConfiguration).toHaveBeenCalledWith(
    expect.objectContaining({
      definition: expect.objectContaining({
        admission_policy: { project_policy: custom.policy.reference },
        permission_ceiling: { project_policy: access.policy.reference },
      }),
    }),
  );
  await userEvent.click(launchPicker);
  await userEvent.click(await screen.findByRole('option', { name: 'Standard launch · v2' }));
  expect(launchPicker.textContent).toContain('Standard launch · v2');
  expect(api.createConfiguration).not.toHaveBeenCalled();
});

it('keeps the mounted draft through policy editing and adopts a publication only on request', async () => {
  const { api, records } = setup();
  const picker = await screen.findByRole('combobox', { name: 'Launch policy' });
  await userEvent.click(picker);
  await userEvent.click(await screen.findByRole('option', { name: 'Source only · v3' }));
  await userEvent.click(await screen.findByRole('checkbox', { name: 'Enabled' }));
  const runs = screen.getByRole('spinbutton', { name: 'Maximum active Runs' });
  await userEvent.clear(runs);
  await userEvent.type(runs, '4');
  const open = await screen.findByRole('button', { name: 'View / Edit launch policy' });
  await userEvent.click(open);
  const settings = within(await screen.findByRole('region', { name: 'Factory settings' }));
  const details = within(await settings.findByRole('region', { name: 'Policy details' }));
  expect(details.getByRole('heading', { name: 'Source only · v3' })).toBeTruthy();
  await userEvent.click(details.getByRole('button', { name: 'Edit policy' }));
  const edited = structuredClone(custom);
  edited.policy.definition.name = 'Revised policy';
  edited.policy.reference.version = 4;
  edited.policy.reference.digest = 'd'.repeat(64);
  const name = settings.getByRole('textbox', { name: 'Policy name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'Revised policy');
  vi.spyOn(factoryPolicyApi, 'validate').mockResolvedValue({ policy: edited });
  const publish = vi.spyOn(factoryPolicyApi, 'publish').mockResolvedValue({
    policy: edited,
    disposition: 'applied',
    published_at: 1,
  });
  records.push(edited);
  await userEvent.click(settings.getByRole('button', { name: 'Review policy' }));
  const dialog = within(await screen.findByRole('dialog'));
  await userEvent.click(dialog.getByRole('button', { name: 'Publish policy' }));
  await dialog.findByText(/Policy published/);
  await userEvent.click(dialog.getByRole('button', { name: 'Close' }));
  const defaults = {
    project_id: project,
    version: 2,
    launch_policy: edited.policy.reference,
    access_profile: access.policy.reference,
  };
  const selectDefaults = vi
    .spyOn(factoryPolicyApi, 'selectDefaults')
    .mockImplementation(async () => {
      vi.mocked(factoryPolicyApi.defaults).mockResolvedValue({ defaults });
      return { defaults, disposition: 'applied', published_at: 2 };
    });
  await userEvent.click(await settings.findByRole('button', { name: 'Use as Project default' }));
  const defaultReview = within(await screen.findByRole('dialog'));
  await userEvent.click(defaultReview.getByRole('button', { name: 'Confirm default' }));
  await defaultReview.findByText(/Project default saved/);
  await userEvent.click(defaultReview.getByRole('button', { name: 'Close' }));
  await userEvent.click(screen.getByRole('button', { name: 'Back to Configuration draft' }));
  expect(document.activeElement).toBe(open);
  expect(runs).toHaveProperty('value', '4');
  expect(screen.getByRole('checkbox', { name: 'Enabled' })).toHaveProperty('checked', true);
  expect(screen.getByRole('combobox', { name: 'Launch policy' }).textContent).toContain(
    'Source only · v3',
  );
  await userEvent.click(
    screen.getByRole('button', { name: 'Use Revised policy · v4 in this draft' }),
  );
  expect(screen.getByRole('combobox', { name: 'Launch policy' }).textContent).toContain(
    'Revised policy · v4',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  expect(api.validateConfiguration).toHaveBeenCalledWith(
    expect.objectContaining({
      definition: expect.objectContaining({
        enabled: true,
        wip_limits: expect.objectContaining({ max_active_runs: 4 }),
        admission_policy: { project_policy: edited.policy.reference },
        permission_ceiling: { project_policy: access.policy.reference },
      }),
    }),
  );
  expect(publish).toHaveBeenCalledTimes(1);
  expect(selectDefaults).toHaveBeenCalledTimes(1);
  expect(api.createConfiguration).not.toHaveBeenCalled();
});

it('shows mapped legacy rules and provenance and requires an explicit replacement for an unmapped access profile', async () => {
  const legacy = configurationFixture();
  const resolve = vi
    .spyOn(factoryPolicyApi, 'resolveLegacy')
    .mockImplementation(async (_project, original) => ({
      mapping:
        original.kind === 'launch'
          ? {
              project_id: project,
              legacy: original,
              target: { kind: 'launch', reference: custom.policy.reference },
            }
          : null,
    }));
  const { api } = setup(legacy);
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Factory Configuration · v1' }),
  );
  const published = within(
    await screen.findByRole('region', { name: 'Factory Configuration · v1' }),
  );
  expect(await published.findByText('Source only · v3')).toBeTruthy();
  expect(published.getByText('Manual admission denied')).toBeTruthy();
  const original = legacy.configuration.admission_policy;
  const provenance = published.getByText(`${original.identity} · ${original.version}`, {
    exact: true,
  });
  expect(provenance.closest('details')).toHaveProperty('open', false);
  await userEvent.click(published.getByRole('button', { name: 'Edit configuration draft' }));
  const draft = within(await published.findByRole('group', { name: 'Configuration draft' }));
  expect(draft.getByRole('combobox', { name: 'Launch policy' }).textContent).toContain(
    'Source only · v3',
  );
  const accessPicker = draft.getByRole('combobox', { name: 'Access profile' });
  expect(accessPicker.textContent).toContain('Choose a Project policy');
  expect(draft.getByText(/saved installation reference has no verified mapping/)).toBeTruthy();
  expect(
    within(published.getByRole('region', { name: 'Draft validation' })).queryByRole('alert'),
  ).toBeNull();
  const validate = published.getByRole('button', { name: 'Validate draft' });
  expect(validate).toHaveProperty('disabled', true);
  await userEvent.click(accessPicker);
  await userEvent.click(await screen.findByRole('option', { name: 'Standard access · v1' }));
  await userEvent.click(published.getByRole('button', { name: 'Validate draft' }));
  expect(api.validateConfiguration).toHaveBeenCalledWith(
    expect.objectContaining({
      definition: expect.objectContaining({
        admission_policy: { project_policy: custom.policy.reference },
        permission_ceiling: { project_policy: access.policy.reference },
      }),
    }),
  );
  expect(resolve).toHaveBeenCalledWith(
    project,
    { kind: 'launch', reference: original },
    expect.any(AbortSignal),
  );
});

it('lets an operator repair a legacy Configuration when the original binding cannot be read', async () => {
  vi.spyOn(factoryPolicyApi, 'resolveLegacy').mockRejectedValue(
    new ManagementApiError({
      code: 'forbidden',
      status: 403,
      message: 'Original reference is inaccessible',
      requestId: 'read-legacy-denied',
      retryAfterMilliseconds: null,
    }),
  );
  setup(configurationFixture());
  await userEvent.click(
    await screen.findByRole('button', { name: 'Inspect Factory Configuration · v1' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Edit configuration draft' }));
  const draft = within(await screen.findByRole('group', { name: 'Configuration draft' }));
  expect(draft.getByRole('combobox', { name: 'Launch policy' }).textContent).toContain(
    'Choose a Project policy',
  );
  expect(draft.getByRole('spinbutton', { name: 'Maximum active Runs' })).toBeTruthy();
  expect(draft.getAllByText(/Select a policy or create one/)).toHaveLength(2);
  expect(screen.getByRole('button', { name: 'Validate draft' })).toHaveProperty('disabled', true);
});

it('reads the current Project defaults when creating a draft from an already-open workbench', async () => {
  const { api } = setup(undefined, true);
  const create = await screen.findByRole('button', { name: 'Create configuration draft' });
  await waitFor(() => expect(create).toHaveProperty('disabled', false));
  const defaults = {
    project_id: project,
    version: 2,
    launch_policy: custom.policy.reference,
    access_profile: access.policy.reference,
  };
  vi.mocked(factoryPolicyApi.defaults).mockResolvedValue({ defaults });
  api.listChoices.mockResolvedValue({
    items: [],
    total: 0,
    next_cursor: null,
    project_policy_defaults: defaults,
  });
  await userEvent.click(create);
  const picker = await screen.findByRole('combobox', { name: 'Launch policy' });
  expect(await within(picker).findByText('Source only · v3')).toBeTruthy();
});
