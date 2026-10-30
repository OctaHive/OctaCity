// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { PermissionEditor } from './ResourceEditors';
afterEach(cleanup);

it('distinguishes command argument grants for the same executable before selection', async () => {
  const ceiling = structuredClone(configurationFixture().configuration.flow.limits.permissions);
  const executable = { identity: 'reviewed-cli', version: 'v1', digest: 'a'.repeat(64) };
  ceiling.executables = [executable];
  ceiling.commands = [
    { executable, arguments: [{ kind: 'exact', value: '--version' }] },
    { executable, arguments: [{ kind: 'exact', value: 'exec' }, { kind: 'any' }] },
    { executable, arguments: [{ kind: 'exact', value: '*' }] },
  ];
  let observed = structuredClone(ceiling);
  observed.commands = [];
  render(
    <PermissionEditor
      value={observed}
      ceiling={ceiling}
      choices={ceiling}
      label="Requested permissions"
      onChange={(next) => {
        observed = next;
      }}
    />,
  );
  expect(
    screen.getByRole('checkbox', { name: 'Command permission · reviewed-cli · v1 · "--version"' }),
  ).toBeTruthy();
  expect(
    screen.getByRole('checkbox', { name: 'Command permission · reviewed-cli · v1 · "*"' }),
  ).toBeTruthy();
  await userEvent.click(
    screen.getByRole('checkbox', { name: 'Command permission · reviewed-cli · v1 · "exec" *' }),
  );
  expect(observed.commands).toEqual([
    { executable, arguments: [{ kind: 'exact', value: 'exec' }, { kind: 'any' }] },
  ]);
});

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

it('explains an empty Runtime permission ceiling without showing empty groups or zero-only controls', () => {
  const ceiling = structuredClone(configurationFixture().configuration.flow.limits.permissions);
  for (const field of [
    'plugins',
    'executables',
    'tools',
    'commands',
    'mounts',
    'network_hosts',
    'secret_profiles',
    'workload_identity_profiles',
  ] as const)
    ceiling[field] = [];
  ceiling.max_descendants = 0;
  for (const key of Object.keys(ceiling.resources) as (keyof typeof ceiling.resources)[])
    ceiling.resources[key] = 0;
  ceiling.outputs = {
    kinds: [],
    max_artifact_count: 0,
    max_artifact_bytes: 0,
    max_report_count: 0,
    max_report_bytes: 0,
  };
  render(
    <PermissionEditor
      value={ceiling}
      ceiling={ceiling}
      choices={ceiling}
      label="Requested node permissions"
      onChange={() => {}}
    />,
  );
  expect(
    screen.getByText(
      'This Runtime has no configurable permissions. Configure its allowed capabilities before selecting them in a profile.',
    ),
  ).toBeTruthy();
  expect(screen.queryByRole('group', { name: 'Plugin' })).toBeNull();
  expect(screen.queryByRole('group', { name: 'Executable' })).toBeNull();
  expect(screen.queryByRole('group', { name: 'Secret profiles' })).toBeNull();
  expect(screen.queryByRole('group', { name: 'Declared network hosts' })).toBeNull();
  expect(screen.queryByRole('spinbutton')).toBeNull();
});
