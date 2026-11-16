// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { QueryClientProvider } from '@tanstack/react-query';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { factoryIssueFieldsFixture } from '../../../tests/fixtures/factoryIssueFields';
import {
  factoryRoleFixture,
  factoryProfileConfigurationFixture,
} from '../../../tests/fixtures/factoryRoles';
import { createConsoleQueryClient, queryKeys } from '../../app/query';
import { FactoryExample } from './FactoryExample';
import { emptyFactoryPermissions } from './emptyFlowDraft';
import { factoryApi, type FactoryApi } from './api';
import { factoryRolesApi } from './roleApi';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
type S = components['schemas'];
it.each([
  'stable',
  'catalog refresh',
  'default catalog refresh',
  'default writer refresh',
] as const)(
  'retains the selected-field template contract on %s without a manual schema or apply step',
  async (scenario) => {
    const user = userEvent.setup();
    const data = factoryIssueFieldsFixture();
    const { role, profile, choice } = factoryRoleFixture();
    const base = configurationFixture().configuration.flow;
    const { reference: _reference, ...root } = base.closure.definitions[0]!;
    void _reference;
    const build =
      factoryProfileConfigurationFixture().configuration.flow.closure.definitions[0]!.nodes.find(
        (n) => n.profile_selection,
      )!.profile_selection!.candidates.default!.build;
    Object.assign(profile.definition, {
      execution_binding: 'step',
      permissions: { ...emptyFactoryPermissions(), secret_profiles: ['unselected-mode'] },
    });
    Object.assign(build, {
      execution_permissions: { ...emptyFactoryPermissions(), secret_profiles: ['selected-mode'] },
    });
    vi.spyOn(factoryRolesApi, 'listRoles').mockResolvedValue({
      items: [
        {
          ...role.definition,
          reference: role.reference,
          icon: role.definition.icon ?? null,
          concurrency_limit: role.definition.concurrency_limit ?? null,
          template: null,
        },
      ],
      total: 1,
      next_cursor: null,
    });
    vi.spyOn(factoryRolesApi, 'listProfiles').mockResolvedValue({
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
    });
    vi.spyOn(factoryRolesApi, 'listCapabilities').mockResolvedValue({
      items: [choice],
      total: 1,
      next_cursor: null,
    });
    vi.spyOn(factoryRolesApi, 'getProfile').mockResolvedValue(profile);
    vi.spyOn(factoryRolesApi, 'resolveExecutionBinding').mockResolvedValue({
      build,
      incompatibility: null,
    });
    const preset: S['FactoryPresetSummary'] = {
      reference: { identity: 'unrelated.analysis', version: 'v1', digest: 'a'.repeat(64) },
      profile_aliases: ['assess'],
      parameters: [
        { path: '/profiles/assess', kind: 'role_profile', required: true },
        { path: '/issue_fields/assess', kind: 'issue_fields', required: false },
      ],
      required_node_kinds: ['reasoning'],
      editor: {
        name: 'Analysis example',
        budget: base.limits.budget,
        max_active_nodes: 1,
        steps: [
          {
            alias: 'assess',
            kind: 'reasoning',
            outcome: 'analyzed',
            result: base.data_schemas[0]!,
          },
        ],
        issue_fields: { assess: [{ field: { kind: 'size' }, required: true }] },
      },
    };
    const opened = { root, limits: base.limits, data_schemas: base.data_schemas };
    let finish!: (result: S['FactoryPresetResolution']) => void;
    const response: S['FactoryPresetResolution'] = {
      preset: preset.reference,
      completion: {
        kind: 'draft',
        project_id: profile.reference.project_id,
        preset: preset.reference,
        content: 'Reviewed process',
      },
      diagnostics: [],
      flow: opened,
      configuration: null,
      input: null,
      dependencies: [],
    };
    const api = {
      ...factoryApi,
      listPresets: vi.fn<FactoryApi['listPresets']>(async () => ({
        items: [preset],
        total: 1,
        next_cursor: null,
      })),
      getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () => data),
      resolvePreset: vi.fn<FactoryApi['resolvePreset']>(async () =>
        scenario === 'stable'
          ? response
          : new Promise((resolve) => {
              finish = resolve;
            }),
      ),
      normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(),
    };
    const onOpen = vi.fn();
    const client = createConsoleQueryClient();
    render(
      <QueryClientProvider client={client}>
        <FactoryExample
          api={api}
          projectId={profile.reference.project_id}
          flowId="new-flow"
          flows={[]}
          onOpen={onOpen}
        />
      </QueryClientProvider>,
    );
    await user.click(await screen.findByRole('button', { name: 'Analysis example · v1' }));
    expect(await screen.findByRole('checkbox', { name: 'Issue size' })).toHaveProperty(
      'checked',
      true,
    );
    const untouched = scenario.startsWith('default');
    if (!untouched) {
      await user.click(screen.getByRole('checkbox', { name: 'Project field: Impact' }));
      await user.click(screen.getByRole('checkbox', { name: 'Required: Project field: Impact' }));
    }
    await user.click(screen.getByText('Assess', { exact: true, selector: 'summary' }));
    await user.click(await screen.findByRole('combobox', { name: 'Role' }));
    await user.click(
      await screen.findByRole('option', {
        name: `${role.definition.name} · v${role.reference.version}`,
      }),
    );
    await user.click(await screen.findByRole('combobox', { name: 'Profile' }));
    await user.click(
      await screen.findByRole('option', {
        name: `${profile.definition.name} · v${profile.reference.version}`,
      }),
    );
    await user.click(screen.getByRole('button', { name: 'Use this profile' }));
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Open example draft' })).toHaveProperty(
        'disabled',
        false,
      ),
    );
    await user.click(screen.getByRole('button', { name: 'Open example draft' }));
    await waitFor(() => expect(api.resolvePreset).toHaveBeenCalledTimes(1));
    if (scenario === 'stable') await waitFor(() => expect(onOpen).toHaveBeenCalledWith(opened));
    else {
      client.setQueryData(queryKeys.factoryAuthoringCapabilities(profile.reference.project_id), {
        ...data,
        ...(scenario === 'default writer refresh'
          ? {
              issue_classification: {
                ...data.issue_classification,
                plugin: { ...data.issue_classification.plugin, digest: '0'.repeat(64) },
                policy: 'new-field-policy',
              },
            }
          : {
              issue_fields: {
                ...data.issue_fields,
                definition_version: 8,
                fields: data.issue_fields.fields.map((field) =>
                  field.source.kind === 'size'
                    ? {
                        ...field,
                        shape: { type: 'enum', values: ['tiny', 'huge'] },
                        choice_labels: { tiny: 'Tiny', huge: 'Huge' },
                      }
                    : field,
                ),
              },
            }),
      });
      await screen.findByText(/Your selection is retained/);
      finish(response);
      await screen.findByText(/example inputs or Project field contract changed/);
      expect(onOpen).not.toHaveBeenCalled();
      expect(screen.getByRole('checkbox', { name: 'Issue size' })).toHaveProperty('checked', true);
      expect(screen.getByRole('checkbox', { name: 'Project field: Impact' })).toHaveProperty(
        'checked',
        !untouched,
      );
      expect(screen.getByRole('button', { name: 'Open example draft' })).toHaveProperty(
        'disabled',
        true,
      );
      const form = screen.getByRole('group', { name: 'Fill Issue fields' });
      expect(form.textContent).toContain('version 7');
      await user.click(screen.getByRole('button', { name: 'Use current field definitions' }));
      expect(screen.getByRole('button', { name: 'Open example draft' })).toHaveProperty(
        'disabled',
        false,
      );
      await user.click(screen.getByRole('button', { name: 'Open example draft' }));
      await waitFor(() => expect(api.resolvePreset).toHaveBeenCalledTimes(2));
      expect(api.resolvePreset.mock.calls[1]![1].issue_fields).toMatchObject({
        assess: {
          definition_version: scenario === 'default writer refresh' ? 7 : 8,
          fields: [
            { field: { kind: 'size' }, required: true },
            ...(!untouched ? [{ field: { kind: 'property', id: 'impact' }, required: false }] : []),
          ],
        },
      });
      finish(response);
      await waitFor(() => expect(onOpen).toHaveBeenCalledWith(opened));
    }
    expect(api.resolvePreset.mock.calls[0]![1]).toMatchObject({
      issue_fields: {
        assess: {
          project_id: profile.reference.project_id,
          definition_version: 7,
          fields: [
            { field: { kind: 'size' }, required: true },
            ...(!untouched ? [{ field: { kind: 'property', id: 'impact' }, required: false }] : []),
          ],
        },
      },
      execution: {
        permissions: { plugins: expect.arrayContaining([data.issue_classification.plugin]) },
      },
    });
    expect(api.resolvePreset.mock.calls[0]![1].execution?.permissions.secret_profiles).toEqual([
      'selected-mode',
    ]);
    expect(api.normalizeSchema).not.toHaveBeenCalled();
    expect(profile.definition.permissions.plugins).not.toContainEqual(
      data.issue_classification.plugin,
    );
  },
);
