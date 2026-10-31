// @vitest-environment jsdom
import { cleanup, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { manualFlowEditorFixture } from '../../../tests/fixtures/manualFlowEditor';
import { factoryRoleFixture } from '../../../tests/fixtures/factoryRoles';
afterEach(cleanup);

it('keeps command tasks installation-owned instead of offering unsupported agent instructions', async () => {
  const { draft } = manualFlowEditorFixture();
  await userEvent.click(screen.getByRole('combobox', { name: 'Step type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Command' }));
  expect(draft().root.nodes[0]!.kind).toBe('build_command');
  expect(screen.queryByRole('group', { name: 'Step task' })).toBeNull();
  expect(screen.queryByRole('button', { name: 'Add task instructions' })).toBeNull();
});

it('clears the reasoning assignment when changing to an operator decision without changing sibling inputs', async () => {
  const { draft } = manualFlowEditorFixture((value) => {
    value.root.nodes[0]!.task = ['Assess Project fit'];
    value.root.nodes[1]!.task = ['Classify the accepted assessment'];
  });
  const before = structuredClone(draft());
  expect(await screen.findByRole('group', { name: 'Step task' })).toBeTruthy();
  await userEvent.click(screen.getByRole('combobox', { name: 'Step type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Operator decision' }));
  expect(draft().root.nodes[0]!.kind).toBe('human_gate');
  expect(draft().root.nodes[0]!.task ?? null).toBeNull();
  expect(screen.queryByRole('group', { name: 'Step task' })).toBeNull();
  expect(draft().root.nodes[0]!.context_binding).toEqual(before.root.nodes[0]!.context_binding);
  expect(draft().root.nodes[1]).toEqual(before.root.nodes[1]);
});

it('authors independent step tasks for the same reusable profile without replacing documents or executor', async () => {
  const { profile } = factoryRoleFixture();
  const { draft } = manualFlowEditorFixture((value) => {
    for (const node of value.root.nodes)
      node.profile_selection = {
        role: profile.role,
        candidates: {
          chosen: {
            profile,
            build: {
              build_configuration: profile.definition.build_configuration,
              plugin: profile.definition.toolchain.plugin,
              tool: profile.definition.toolchain.executable,
              model_or_tool: profile.definition.toolchain.model,
              task_digest: 'b'.repeat(64),
              result_output: 'result.json',
              result_output_kind: 'report',
              result_outcome: node.outcomes[0]!.key,
            },
          },
        },
        policy: { mode: 'manual', candidate: 'chosen' },
        failure: { kind: 'manual_selection' },
        selection_budget: {
          attempts: 0,
          elapsed_millis: 0,
          tokens: 0,
          cost_micro_units: 0,
          output_bytes: 0,
        },
      };
  });
  const before = structuredClone(draft());
  const task = within(await screen.findByRole('group', { name: 'Step task' }));
  expect(screen.getByRole('group', { name: 'Expected result' })).toBeTruthy();
  await userEvent.click(task.getByRole('button', { name: 'Add task instructions' }));
  await userEvent.type(
    task.getByRole('textbox', { name: 'Task instructions 1' }),
    'Assess Project fit and duplicate evidence using only attached inputs.',
  );
  expect(draft().root.nodes[0]!.task).toEqual([
    'Assess Project fit and duplicate evidence using only attached inputs.',
  ]);
  await userEvent.click(screen.getByRole('button', { name: 'Edit step: classify' }));
  const sibling = within(screen.getByRole('group', { name: 'Step task' }));
  await userEvent.click(sibling.getByRole('button', { name: 'Add task instructions' }));
  await userEvent.type(
    sibling.getByRole('textbox', { name: 'Task instructions 1' }),
    'Classify using the connected component and priority documents.',
  );
  expect(draft().root.nodes[1]!.task).toEqual([
    'Classify using the connected component and priority documents.',
  ]);
  for (let i = 0; i < 2; i++) {
    expect(draft().root.nodes[i]!.profile_selection).toEqual(
      before.root.nodes[i]!.profile_selection,
    );
    expect(draft().root.nodes[i]!.context_binding).toEqual(before.root.nodes[i]!.context_binding);
    expect(draft().root.nodes[i]!.outcomes).toEqual(before.root.nodes[i]!.outcomes);
  }
  expect(profile.definition.instructions).toEqual(['Use selected skills.']);
  await userEvent.click(screen.getByRole('button', { name: 'Edit step: assess' }));
  expect(screen.getByRole('textbox', { name: 'Task instructions 1' })).toHaveProperty(
    'value',
    'Assess Project fit and duplicate evidence using only attached inputs.',
  );
});
