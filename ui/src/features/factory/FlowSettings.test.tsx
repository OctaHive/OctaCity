// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { FlowSettings } from './FlowSettings';
import { emptyFlowDraft } from './emptyFlowDraft';

afterEach(cleanup);

it('offers step results for completion while retaining every custom format under advanced settings', async () => {
  const flow = configurationFixture().configuration.flow;
  const { reference, ...root } = flow.closure.definitions[0]!;
  void reference;
  const result = flow.data_schemas[0]!;
  const input = {
    ...result,
    reference: { identity: 'input.generated', version: '1', digest: 'e'.repeat(64) },
  };
  root.nodes = [
    {
      ...root.nodes[0]!,
      key: 'systems_analysis',
      outcomes: [{ key: 'result', kind: 'success', schema: result.reference }],
    },
  ];
  root.terminals = [];
  let observed = { root, limits: flow.limits, data_schemas: [input, result] };
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowSettings
        value={value}
        choices={[]}
        permissionDeclarations={value.limits.permissions}
        budgetCalculation={{ run: false, admission: false }}
        onBudgetCalculationChange={() => {}}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Add terminal' }));
  expect(observed.root.terminals[0]?.schema).toEqual(result.reference);
  const basic = screen.getByRole('combobox', { name: 'Completion result' });
  await userEvent.click(basic);
  expect(screen.getByRole('option', { name: 'systems_analysis.result' })).toBeTruthy();
  expect(screen.queryByRole('option', { name: /input\.generated/ })).toBeNull();
  await userEvent.keyboard('{Escape}');
  const advanced = screen.getByText('Technical completion formats', { exact: true });
  expect(advanced.closest('details')).toHaveProperty('open', false);
  await userEvent.click(advanced);
  await userEvent.click(screen.getByRole('combobox', { name: 'Any completion data contract' }));
  await userEvent.click(screen.getByRole('option', { name: 'Additional format 1' }));
  expect(observed.root.terminals[0]?.schema).toEqual(input.reference);
  await userEvent.click(advanced);
  expect(basic.textContent).toContain('Custom format — see technical details');
  expect(observed.root.terminals[0]?.schema).toEqual(input.reference);
});

it('pins a selected published completion as incoming data for a new Flow', async () => {
  const configuration = configurationFixture().configuration;
  const flow = structuredClone(configuration.flow);
  const root = flow.closure.definitions[0]!;
  const schema = flow.data_schemas[0]!;
  root.name = 'Triage';
  root.nodes[0]!.outcomes = [
    { key: 'classified', kind: 'success', schema: schema.reference },
    { key: 'failed', kind: 'failure', schema: schema.reference },
  ];
  root.terminals = [{ key: 'reviewed', schema: schema.reference }];
  root.transitions = root.nodes[0]!.outcomes.map(({ key }) => ({
    predecessor: root.nodes[0]!.key,
    outcome: key,
    target: { kind: 'terminal', key: 'reviewed' },
    max_repeats: 0,
    exhausted_terminal: null,
  }));
  let observed = emptyFlowDraft();
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowSettings
        value={value}
        choices={[]}
        flows={[{ flow, project_id: configuration.reference.project_id, published_at_unix_ms: 1 }]}
        permissionDeclarations={value.limits.permissions}
        budgetCalculation={{ run: true, admission: true }}
        onBudgetCalculationChange={() => {}}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('combobox', { name: 'Incoming pipeline completion' }));
  await userEvent.click(screen.getByRole('option', { name: /Triage.*reviewed/u }));
  expect(screen.queryByRole('checkbox', { name: 'failed' })).toBeNull();
  await userEvent.click(screen.getByRole('checkbox', { name: 'classified' }));
  expect(observed.accepted_predecessor).toEqual({
    definition: flow.closure.root,
    terminal: 'reviewed',
    outcomes: ['classified'],
  });
  expect(observed.data_schemas).toEqual([schema]);
  expect(observed.root.nodes).toEqual([]);
});
