// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { ConfigurationEditor } from './ConfigurationEditor';
afterEach(cleanup);

it('keeps exact Project pins visible and unchanged when editing unrelated configuration settings', async () => {
  const baseline = configurationFixture().configuration;
  const pin = {
    project_id: baseline.reference.project_id,
    id: 'd6e5c639-258e-4eb3-9662-051d371462bb',
    version: 2,
    digest: 'c'.repeat(64),
  };
  const value = {
    flow: baseline.flow,
    hard_budget: baseline.hard_budget,
    wip_limits: baseline.wip_limits,
    enabled: true,
    admission_policy: { project_policy: pin },
    permission_ceiling: { project_policy: { ...pin, id: 'a7a3180f-b7a0-400b-a721-0a4cedf253e2' } },
  };
  const onChange = vi.fn();
  render(<ConfigurationEditor value={value} choices={[]} flows={[]} onChange={onChange} />);
  expect(screen.getByRole('combobox', { name: 'Admission policy' }).textContent).toContain(
    'Version 2',
  );
  expect(screen.getByRole('combobox', { name: 'Permission ceiling' }).textContent).toContain(
    'Version 2',
  );
  await userEvent.click(screen.getByRole('checkbox', { name: 'Enabled' }));
  expect(onChange).toHaveBeenCalledWith({ ...value, enabled: false });
});
