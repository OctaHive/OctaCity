// @vitest-environment jsdom
import { useState } from 'react';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { ConfigurationEditor } from './ConfigurationEditor';
afterEach(cleanup);

it('shows the retained root Flow even when the visible page only lists a newer version', async () => {
  const { enabled, flow, hard_budget, wip_limits } = configurationFixture().configuration;
  const newer = structuredClone(flow);
  newer.closure.root.version = 2;
  newer.closure.definitions[0]!.reference.version = 2;
  const onChange = vi.fn();
  function Harness() {
    const [value, setValue] = useState({ enabled, flow, hard_budget, wip_limits });
    return (
      <ConfigurationEditor
        value={value}
        policies={{ launch: null, access: null }}
        flows={[{ project_id: 'project', flow: newer, published_at_unix_ms: 1 }]}
        onChange={(next) => {
          setValue(next);
          onChange(next);
        }}
      />
    );
  }
  render(<Harness />);
  const picker = screen.getByRole('combobox', { name: 'Root Flow' });
  expect(picker.textContent).toContain('implement · v1');
  await userEvent.click(picker);
  await userEvent.click(screen.getByRole('option', { name: 'implement · v2' }));
  expect(onChange).toHaveBeenCalledWith({ enabled, flow: newer, hard_budget, wip_limits });
  await userEvent.click(picker);
  await userEvent.click(screen.getByRole('option', { name: 'implement · v1' }));
  expect(onChange).toHaveBeenLastCalledWith({ enabled, flow, hard_budget, wip_limits });
});
