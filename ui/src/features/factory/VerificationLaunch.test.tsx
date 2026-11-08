// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { factoryProfileConfigurationFixture } from '../../../tests/fixtures/factoryRoles';
import { emptyFlowDraft } from './emptyFlowDraft';
import { VerificationLaunch } from './VerificationLaunch';

afterEach(cleanup);

it('selects a scoped Tester without replacing the retained verifier or its inputs, and can return to direct Build', async () => {
  const draft = emptyFlowDraft();
  const sample =
    factoryProfileConfigurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
  const build = sample.profile_selection!.candidates.default!.build;
  draft.root.nodes = [
    { ...sample, key: 'tester', kind: 'reasoning', profile_selection: null, build },
    {
      ...sample,
      key: 'checks',
      kind: 'build_command',
      profile_selection: null,
      build: { ...build, model_or_tool: { ...build.model_or_tool, identity: 'verifier' } },
    },
  ];
  const calls = vi.fn();
  function Harness() {
    const [value, setValue] = useState(draft);
    return (
      <VerificationLaunch
        root={value.root}
        node={value.root.nodes[1]!}
        profiles={[build.model_or_tool]}
        runners={[value.root.nodes[1]!.build!.model_or_tool]}
        onChange={(root) => {
          calls(root);
          setValue({ ...value, root });
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('combobox', { name: 'Verification launch' }));
  await userEvent.click(screen.getByRole('option', { name: 'Tester · tester' }));
  const selected = calls.mock.lastCall![0];
  expect(selected.nodes[0].scoped_validation_runner).toBe('checks');
  expect(selected.nodes[1]).toEqual(draft.root.nodes[1]);
  expect(selected.execution.max_active_nodes).toBeGreaterThanOrEqual(2);
  await userEvent.click(screen.getByRole('combobox', { name: 'Verification launch' }));
  await userEvent.click(screen.getByRole('option', { name: 'CI/CD Build' }));
  expect(calls.mock.lastCall![0].nodes[0].scoped_validation_runner).toBeNull();
  expect(calls.mock.lastCall![0].nodes[1]).toEqual(draft.root.nodes[1]);
});
