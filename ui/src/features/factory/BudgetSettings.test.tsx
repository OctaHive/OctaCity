// @vitest-environment jsdom
import { useState } from 'react';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { BudgetSettings } from './BudgetSettings';
import { focusFactoryField } from './fieldFocus';

afterEach(cleanup);

it('treats values entered before a profile is available as a manual override', async () => {
  let observed = {
    max_attempts: 1,
    max_elapsed_millis: 1,
    max_tokens: 1,
    max_cost_micro_units: 1,
    max_output_bytes: 1,
  };
  function Harness() {
    const [value, setValue] = useState(observed);
    const [ready, setReady] = useState(false);
    return (
      <>
        <BudgetSettings
          automaticByDefault
          label="Step budget"
          value={value}
          suggested={ready ? { ...value, max_tokens: 200000 } : null}
          source="Profile"
          onChange={(next) => {
            observed = next;
            setValue(next);
          }}
        />
        <button onClick={() => setReady(true)}>Choose profile</button>
      </>
    );
  }
  render(<Harness />);
  const input = screen.getByRole('spinbutton', { name: 'Step budget · Maximum tokens' });
  await userEvent.clear(input);
  await userEvent.type(input, '175000');
  await userEvent.click(screen.getByRole('button', { name: 'Choose profile' }));
  expect(observed.max_tokens).toBe(175000);
});

it('keeps manual limits until calculation is chosen and preserves an override across changed suggestions', async () => {
  let observed = {
    max_attempts: 1,
    max_elapsed_millis: 1,
    max_tokens: 1,
    max_cost_micro_units: 1,
    max_output_bytes: 1,
  };
  function Harness() {
    const [value, setValue] = useState(observed);
    const [tokens, setTokens] = useState(150000);
    return (
      <>
        <BudgetSettings
          label="Step budget"
          fieldPath="/root/nodes/0/budget"
          value={value}
          suggested={{
            max_attempts: 2,
            max_elapsed_millis: 300000,
            max_tokens: tokens,
            max_cost_micro_units: 1000000,
            max_output_bytes: 67108864,
          }}
          source="Selected profiles and selection budget"
          onChange={(next) => {
            observed = next;
            setValue(next);
          }}
        />
        <button onClick={() => setTokens(200000)}>Change profile</button>
      </>
    );
  }
  render(<Harness />);
  expect(observed.max_tokens).toBe(1);
  await userEvent.click(screen.getByRole('combobox', { name: 'Step budget · Calculation' }));
  await userEvent.click(screen.getByRole('option', { name: 'Calculate automatically' }));
  expect(observed).toEqual({
    max_attempts: 2,
    max_elapsed_millis: 300000,
    max_tokens: 150000,
    max_cost_micro_units: 1000000,
    max_output_bytes: 67108864,
  });
  expect(screen.getByText('Selected profiles and selection budget')).toBeTruthy();
  expect(
    screen.getByRole('spinbutton', { name: 'Step budget · Maximum tokens' }).closest('details'),
  ).toHaveProperty('open', false);
  focusFactoryField('/root/nodes/0/budget/max_tokens');
  expect(document.activeElement).toBe(
    screen.getByRole('combobox', { name: 'Step budget · Calculation' }),
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Step budget · Calculation' }));
  await userEvent.click(screen.getByRole('option', { name: 'Set manually' }));
  const input = screen.getByRole('spinbutton', { name: 'Step budget · Maximum tokens' });
  await userEvent.clear(input);
  await userEvent.type(input, '175000');
  await userEvent.click(screen.getByRole('button', { name: 'Change profile' }));
  expect(observed.max_tokens).toBe(175000);
  await userEvent.click(screen.getByRole('combobox', { name: 'Step budget · Calculation' }));
  await userEvent.click(screen.getByRole('option', { name: 'Calculate automatically' }));
  expect(observed.max_tokens).toBe(200000);
});
