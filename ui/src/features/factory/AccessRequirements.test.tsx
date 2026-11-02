// @vitest-environment jsdom
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { factoryProfileConfigurationFixture } from '../../../tests/fixtures/factoryRoles';
import { emptyFactoryPermissions } from './emptyFlowDraft';
import { AccessRequirements } from './AccessRequirements';
afterEach(cleanup);

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
