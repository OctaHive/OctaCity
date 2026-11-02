// @vitest-environment jsdom
import { cleanup, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { factoryPolicyApi } from '../factory/policyApi';
import { fakeProjectsApi, project, renderProjects } from './ProjectTestSupport';

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
it('opens Factory policy settings from Project without provisioning and preserves a collapsed draft', async () => {
  const api = fakeProjectsApi();
  api.getProject.mockResolvedValue({ ancestors: [], project: project('delivery', 'Delivery') });
  const list = vi
    .spyOn(factoryPolicyApi, 'list')
    .mockResolvedValue({ items: [], next_cursor: null });
  vi.spyOn(factoryPolicyApi, 'defaults').mockResolvedValue({ defaults: null });
  const publish = vi.spyOn(factoryPolicyApi, 'publish');
  renderProjects('/projects/delivery', api);
  const button = await screen.findByRole('button', { name: 'Factory settings' });
  expect(list).not.toHaveBeenCalled();
  await userEvent.click(button);
  await screen.findByRole('region', { name: 'Launch policies' });
  await userEvent.click(screen.getByRole('button', { name: 'New launch policy' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Policy name' }), 'Kept in memory');
  await userEvent.click(button);
  expect(screen.queryByRole('textbox', { name: 'Policy name' })).toBeNull();
  await userEvent.click(button);
  expect(screen.getByRole('textbox', { name: 'Policy name' })).toHaveProperty(
    'value',
    'Kept in memory',
  );
  expect(publish).not.toHaveBeenCalled();
  expect(api.triggerBuild).not.toHaveBeenCalled();
});
