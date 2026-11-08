// @vitest-environment jsdom
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { factoryProfileConfigurationFixture } from '../../../tests/fixtures/factoryRoles';
import { emptyFactoryPermissions } from './emptyFlowDraft';
import { AccessRequirements } from './AccessRequirements';
afterEach(cleanup);

it.each(['prepare-tests', 'analyze-ci'])(
  'reviews only the %s mode request of a shared profile',
  async (mode) => {
    const { configuration } = factoryProfileConfigurationFixture();
    const { reference: _reference, ...root } = configuration.flow.closure.definitions[0]!;
    void _reference;
    const node = root.nodes.find((node) => node.profile_selection)!;
    const candidate = Object.values(node.profile_selection!.candidates)[0]!;
    const required = { ...emptyFactoryPermissions(), secret_profiles: [mode] };
    candidate.profile.definition.permissions = {
      ...emptyFactoryPermissions(),
      secret_profiles: ['prepare-tests', 'analyze-ci', 'unselected-capability'],
    };
    Object.assign(candidate.profile.definition, { execution_binding: 'step' });
    Object.assign(candidate.build, { execution_permissions: required });
    node.permissions = emptyFactoryPermissions();
    const onChange = vi.fn();
    render(
      <AccessRequirements
        value={{
          root: { ...root, nodes: [node] },
          limits: configuration.flow.limits,
          data_schemas: configuration.flow.data_schemas,
        }}
        onChange={onChange}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Review required access' }));
    const dialog = within(screen.getByRole('dialog'));
    expect(dialog.queryAllByText(/unselected-capability/)).toEqual([]);
    expect(onChange).not.toHaveBeenCalled();
    await userEvent.click(dialog.getByRole('button', { name: 'Apply to draft' }));
    expect(onChange.mock.lastCall?.[0].root.nodes[0].permissions).toEqual(required);
    expect(candidate.profile.definition.permissions.secret_profiles).toHaveLength(3);
  },
);

it('requires manual reconciliation for overlapping mount scopes instead of widening a read-only parent', async () => {
  const { configuration } = factoryProfileConfigurationFixture();
  const { reference: _reference, ...root } = configuration.flow.closure.definitions[0]!;
  void _reference;
  const node = root.nodes.find((node) => node.profile_selection)!;
  const profile = Object.values(node.profile_selection!.candidates)[0]!.profile;
  profile.definition.permissions = emptyFactoryPermissions();
  profile.definition.permissions.mounts = [{ root: '/workspace/source', mode: 'read_write' }];
  node.permissions = emptyFactoryPermissions();
  node.permissions.mounts = [{ root: '/workspace', mode: 'read_only' }];
  const onChange = vi.fn();
  render(
    <AccessRequirements
      value={{
        root,
        limits: configuration.flow.limits,
        data_schemas: configuration.flow.data_schemas,
      }}
      onChange={onChange}
    />,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review required access' }));
  const dialog = within(screen.getByRole('dialog'));
  expect(dialog.getByRole('button', { name: 'Apply to draft' })).toHaveProperty('disabled', true);
  expect(dialog.getByRole('alert').textContent).toContain('overlap');
  expect(onChange).not.toHaveBeenCalled();
  expect(node.permissions.mounts).toEqual([{ root: '/workspace', mode: 'read_only' }]);
});

it('reviews only the selected publication target permissions before applying them', async () => {
  const { configuration } = factoryProfileConfigurationFixture();
  const { reference: _reference, ...root } = configuration.flow.closure.definitions[0]!;
  void _reference;
  const plugin = { identity: 'publisher', version: 'v1', digest: 'a'.repeat(64) };
  const adapter = { ...plugin, identity: 'forge-adapter' };
  const target = { ...plugin, identity: 'review-target' };
  const node = {
    ...root.nodes[0]!,
    profile_selection: null,
    action: { plugin, action: 'publish_candidate', parameters: { target } },
    permissions: emptyFactoryPermissions(),
  };
  const permissions = {
    ...emptyFactoryPermissions(),
    plugins: [adapter, plugin],
    network_hosts: ['forge.example'],
    secret_profiles: ['delivery-writer'],
  };
  const choice = {
    plugin,
    target,
    outcome: node.outcomes[0]!,
    receipt_schema: configuration.flow.data_schemas[0]!,
    review_url: true,
    permissions,
  };
  const publications = [
    choice,
    {
      ...choice,
      target: { ...target, identity: 'different-target' },
      permissions: { ...permissions, secret_profiles: ['different-writer'] },
    },
  ];
  const onChange = vi.fn();
  render(
    <AccessRequirements
      value={{
        root: { ...root, nodes: [node] },
        limits: configuration.flow.limits,
        data_schemas: configuration.flow.data_schemas,
      }}
      publications={publications}
      onChange={onChange}
    />,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Review required access' }));
  expect(onChange).not.toHaveBeenCalled();
  await userEvent.click(
    within(screen.getByRole('dialog')).getByRole('button', { name: 'Apply to draft' }),
  );
  expect(onChange.mock.lastCall?.[0].root.nodes[0].permissions).toEqual(permissions);
  expect(onChange.mock.lastCall?.[0].limits.permissions.secret_profiles).toContain(
    'delivery-writer',
  );
  expect(onChange.mock.lastCall?.[0].limits.permissions.secret_profiles).not.toContain(
    'different-writer',
  );
});
