// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { PermissionEditor } from './ResourceEditors';
afterEach(cleanup);
it('narrows visible declared network, mount and output permissions without entering scripts or credential material', async () => {
  const ceiling = configurationFixture().configuration.flow.limits.permissions;
  ceiling.network_hosts = ['api.allowed.example'];
  ceiling.mounts = [{ root: '/workspace', mode: 'read_only' }];
  ceiling.outputs.kinds = ['report'];
  let observed = structuredClone(ceiling);
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <PermissionEditor
        value={value}
        ceiling={ceiling}
        choices={[]}
        label="Requested node permissions"
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(
    screen.getByRole('checkbox', { name: 'Network host · api.allowed.example' }),
  );
  await userEvent.click(screen.getByRole('checkbox', { name: 'Mount · /workspace · read_only' }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Output kind · report' }));
  expect(observed.network_hosts).toEqual([]);
  expect(observed.mounts).toEqual([]);
  expect(observed.outputs.kinds).toEqual([]);
  expect(ceiling.network_hosts).toEqual(['api.allowed.example']);
  expect(screen.queryByRole('textbox', { name: 'Script' })).toBeNull();
  await userEvent.click(
    screen.getByRole('checkbox', { name: 'Network host · api.allowed.example' }),
  );
  expect(observed.network_hosts).toEqual(ceiling.network_hosts);
});
