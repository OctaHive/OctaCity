// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { FlowSettings } from './FlowSettings';

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
