// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { factoryProfileConfigurationFixture } from '../../../tests/fixtures/factoryRoles';
import { emptyFlowDraft } from './emptyFlowDraft';
import { VerificationLaunch } from './VerificationLaunch';

afterEach(cleanup);

it('offers the analysis execution and excludes the author using the same model', async () => {
  const draft = emptyFlowDraft();
  const sample =
    factoryProfileConfigurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
  const build = sample.profile_selection!.candidates.default!.build;
  const analysis = { ...build, build_configuration: { ...build.build_configuration, version: 2 } };
  const checks = { ...build, model_or_tool: { ...build.model_or_tool, identity: 'verifier' } };
  draft.root.nodes = [
    { ...sample, key: 'test-author', profile_selection: null, build },
    { ...sample, key: 'test-analysis', profile_selection: null, build: analysis },
    { ...sample, key: 'checks', kind: 'build_command', profile_selection: null, build: checks },
  ];
  render(
    <VerificationLaunch
      root={draft.root}
      node={draft.root.nodes[2]!}
      profiles={[build.model_or_tool]}
      executions={[analysis]}
      runners={[checks.model_or_tool]}
      onChange={vi.fn()}
    />,
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Verification analysis' }));
  expect(screen.queryByRole('option', { name: 'Tester · test-author' })).toBeNull();
  expect(screen.getByRole('option', { name: 'Tester · test-analysis' })).toBeTruthy();
});

it('adds Tester analysis after mandatory verification without requiring parallel slots or replacing the verifier', async () => {
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
  expect(screen.getByText(/Factory always starts the protected verification Build/)).toBeTruthy();
  await userEvent.click(screen.getByRole('combobox', { name: 'Verification analysis' }));
  await userEvent.click(screen.getByRole('option', { name: 'Tester · tester' }));
  const selected = calls.mock.lastCall![0];
  expect(selected.nodes[0].scoped_validation_runner).toBe('checks');
  expect(selected.nodes[1]).toEqual(draft.root.nodes[1]);
  expect(selected.execution.max_active_nodes).toBe(draft.root.execution.max_active_nodes);
  await userEvent.click(screen.getByRole('combobox', { name: 'Verification analysis' }));
  await userEvent.click(screen.getByRole('option', { name: 'Without AI analysis' }));
  expect(calls.mock.lastCall![0].nodes[0].scoped_validation_runner).toBeNull();
  expect(calls.mock.lastCall![0].nodes[1]).toEqual(draft.root.nodes[1]);
});
