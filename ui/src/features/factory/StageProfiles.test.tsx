// @vitest-environment jsdom
import { useState } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { FlowGraphEditor } from './FlowGraphEditor';
import { factoryRolesApi } from './roleApi';
import {
  factoryRoleFixture,
  factoryProfileConfigurationFixture,
} from '../../../tests/fixtures/factoryRoles';
import type { components } from '../../../.generated/api/schema';

afterEach(cleanup);
function setup(
  staticStage = false,
  choices: components['schemas']['FactoryConfigurationChoiceResource'][] = [],
) {
  const config = factoryProfileConfigurationFixture().configuration.flow;
  const { reference: _reference, ...root } = config.closure.definitions[0]!;
  void _reference;
  const { role, profile } = factoryRoleFixture();
  if (staticStage) {
    root.nodes[0]!.build = root.nodes[0]!.profile_selection!.candidates.default!.build;
    root.nodes[0]!.profile_selection = null;
  }
  const pro = {
    ...structuredClone(profile),
    reference: {
      ...profile.reference,
      id: '62000000-0000-4000-8000-000000000002',
      digest: '8'.repeat(64),
    },
    definition: { ...profile.definition, name: 'pro' },
  };
  const services = {
    projectId: role.reference.project_id,
    api: {
      ...factoryRolesApi,
      listRoles: async () => ({
        items: [
          {
            reference: role.reference,
            name: role.definition.name,
            icon: null,
            concurrency_limit: null,
            template: null,
          },
        ],
        total: 1,
        next_cursor: null,
      }),
      listProfiles: async () => ({
        items: [profile, pro].map((item) => ({
          reference: item.reference,
          name: item.definition.name,
          role: item.role,
          toolchain: item.definition.toolchain,
          template: null,
        })),
        total: 2,
        next_cursor: null,
      }),
      getProfile: async (_project: string, id: string) => (id === pro.reference.id ? pro : profile),
    },
  };
  const changed = vi.fn();
  function Harness() {
    const [value, setValue] = useState<components['schemas']['FactoryFlowDefinitionDraft']>({
      root,
      limits: config.limits,
      data_schemas: config.data_schemas,
    });
    return (
      <FlowGraphEditor
        roles={services}
        value={value}
        onChange={(next) => {
          setValue(next);
          changed(next);
        }}
        choices={choices}
        subflows={[]}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  return { changed, root };
}
it('edits a retained manual candidate and explicit reserve without changing static execution', async () => {
  const { changed, root } = setup();
  await userEvent.click(screen.getByRole('combobox', { name: 'Profile selection mode' }));
  await userEvent.click(screen.getByRole('option', { name: 'Manual selection' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Selection failure policy' }));
  await userEvent.click(screen.getByRole('option', { name: 'Reserve profile' }));
  const selection = changed.mock.lastCall?.[0].root.nodes[0].profile_selection;
  expect(selection.failure).toEqual({ kind: 'reserve_profile', candidate: 'default' });
  expect(selection.candidates).toEqual(root.nodes[0]?.profile_selection?.candidates);
  await userEvent.click(screen.getByRole('button', { name: 'Remove profile selection' }));
  expect(changed.mock.lastCall?.[0].root.nodes[0].profile_selection).toBeNull();
  expect(changed.mock.lastCall?.[0].root.nodes[0].build).toEqual(
    root.nodes[0]?.profile_selection?.candidates.default?.build,
  );
});
it('authors ordered typed rules and pins an installed profile-purpose policy without evaluating criteria in the browser', async () => {
  const { changed } = setup();
  await userEvent.click(screen.getByRole('combobox', { name: 'Profile selection mode' }));
  await userEvent.click(screen.getByRole('option', { name: 'Ordered rules' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add selection rule' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Observation path' }), '/risk');
  expect(changed.mock.lastCall?.[0].root.nodes[0].profile_selection.policy).toEqual({
    mode: 'rules',
    otherwise: null,
    rules: [
      {
        candidate: 'default',
        when: { operation: 'exists', operand: { source: 'path', path: '/risk' } },
      },
    ],
  });
  await userEvent.click(screen.getByRole('combobox', { name: 'Profile selection mode' }));
  await userEvent.click(screen.getByRole('option', { name: 'Decision API' }));
  expect(
    await screen.findByText(
      'No qualified profile-selection policy is installed. Use manual selection or ordered rules.',
    ),
  ).toBeTruthy();
  expect(screen.queryByRole('combobox', { name: 'Installed selector policy' })).toBeNull();
});

it('converts a static reasoning binding to role selection through authorized exact profiles without retaining a competing static executor', async () => {
  const { changed, root } = setup(true);
  await userEvent.click(screen.getByRole('button', { name: 'Add profile candidate' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default · v1' }));
  const next = changed.mock.lastCall?.[0].root.nodes[0];
  expect(next.build).toBeNull();
  const { profile } = factoryRoleFixture();
  expect(next.profile_selection.role).toEqual(profile.role);
  expect(Object.values(next.profile_selection.candidates)).toEqual([
    { profile, build: root.nodes[0]!.build },
  ]);
});

it('shows profile-purpose provider and qualification pins from authorized installed policy choices', async () => {
  const config = factoryProfileConfigurationFixture().configuration.flow;
  const { reference: _reference, ...root } = config.closure.definitions[0]!;
  void _reference;
  const choices = selectorChoices();
  const selected = { identity: 'selector', version: 'v1', digest: 'd'.repeat(64) };
  const changed = vi.fn();
  function Harness() {
    const [value, setValue] = useState<components['schemas']['FactoryFlowDefinitionDraft']>({
      root,
      limits: config.limits,
      data_schemas: config.data_schemas,
    });
    return (
      <FlowGraphEditor
        value={value}
        onChange={(next) => {
          changed(next);
          setValue(next);
        }}
        choices={choices}
        subflows={[]}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Profile selection mode' }));
  await userEvent.click(screen.getByRole('option', { name: 'Decision API' }));
  expect(await screen.findByText('authorized adapter')).toBeTruthy();
  expect(screen.getByText('semantic criteria v1')).toBeTruthy();
  expect(screen.getByText('bounded_control')).toBeTruthy();
  expect(screen.getByText('800000')).toBeTruthy();
  expect(screen.getByText('100000')).toBeTruthy();
  expect(screen.getByText('calibration · v1')).toBeTruthy();
  expect(changed.mock.lastCall?.[0].root.nodes[0].profile_selection.policy.policy).toEqual(
    selected,
  );
});

function selectorChoices(): components['schemas']['FactoryConfigurationChoiceResource'][] {
  const selected = { identity: 'selector', version: 'v1', digest: 'd'.repeat(64) };
  return [
    {
      identity: 'selector-choice',
      alias: 'qualified selector',
      kind: 'decision_signal_policy',
      reference: { kind: 'immutable', reference: selected },
      metadata: {
        node_kinds: ['reasoning'],
        signal_purposes: ['profile_selection'],
        enforcement: [],
        compatible_choices: [
          { kind: 'decision_signal_provider', alias: 'authorized adapter' },
          { kind: 'decision_signal_question_set', alias: 'semantic criteria v1' },
        ],
        profile_selection: {
          provider: { ...selected, identity: 'provider' },
          adapter: { ...selected, identity: 'adapter' },
          model: { ...selected, identity: 'selector-model' },
          question_set: { ...selected, identity: 'criteria' },
          calibration: { ...selected, identity: 'calibration' },
          probability_semantics: { ...selected, identity: 'probability' },
          mode: 'bounded_control',
          baseline: 'default',
          candidates: ['default', 'pro'],
          minimum_probability: 800_000,
          minimum_margin: 100_000,
        },
      },
    },
  ];
}

it('maps newly selected exact profiles to the installed selector answers and preserves explicit reserve recovery', async () => {
  const { changed } = setup(true, selectorChoices());
  await userEvent.click(screen.getByRole('button', { name: 'Add profile candidate' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default · v1' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Selection failure policy' }));
  await userEvent.click(screen.getByRole('option', { name: 'Reserve profile' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Profile selection mode' }));
  await userEvent.click(screen.getByRole('option', { name: 'Decision API' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Selector answer · default' }));
  await userEvent.click(screen.getByRole('option', { name: 'default' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add profile candidate' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'default · v1' }));
  expect(
    Object.keys(changed.mock.lastCall?.[0].root.nodes[0].profile_selection.candidates),
  ).toEqual(['default']);
  await userEvent.click(screen.getByRole('button', { name: 'Add profile candidate' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Code specialist' }));
  await userEvent.click(await screen.findByRole('button', { name: 'pro · v1' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Selector answer · pro' }));
  expect(screen.queryByRole('option', { name: 'default' })).toBeNull();
  await userEvent.click(screen.getByRole('option', { name: 'pro' }));
  const selection = changed.mock.lastCall?.[0].root.nodes[0].profile_selection;
  expect(Object.keys(selection.candidates)).toEqual(['default', 'pro']);
  expect(selection.candidates.default.profile.reference.id).toBe(
    '62000000-0000-4000-8000-000000000001',
  );
  expect(selection.candidates.pro.profile.reference.id).toBe(
    '62000000-0000-4000-8000-000000000002',
  );
  expect(selection.failure).toEqual({ kind: 'reserve_profile', candidate: 'default' });
  await userEvent.click(screen.getByRole('combobox', { name: 'Profile selection mode' }));
  await userEvent.click(screen.getByRole('option', { name: 'Manual selection' }));
  expect(changed.mock.lastCall?.[0].root.nodes[0].profile_selection.policy).toEqual({
    mode: 'manual',
    candidate: null,
  });
});
