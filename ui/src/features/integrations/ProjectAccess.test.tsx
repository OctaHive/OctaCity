// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { integrationsApi } from './api';
import { ProjectAccessPanel } from './ProjectAccess';

afterEach(cleanup);
const project = {
  id: '0199a6f4-d56c-7440-9aa2-6a320f862790',
  name: 'City',
  version: 1,
  parent_id: null,
  has_children: false,
  created_at_unix_ms: 1,
  updated_at_unix_ms: 1,
};
const projects = { listProjects: vi.fn(async () => ({ items: [project], next_cursor: null })) };
function setup(overrides: Partial<typeof integrationsApi> = {}) {
  const connection = integrationFixture('IntegrationConnection');
  const capability = connection.definition.resources[0]!.capabilities[0]!;
  connection.definition.resources[0]!.capabilities = [
    'task_read',
    'task_update',
    'repository_read',
    'delivery',
  ].map((purpose) => ({ ...capability, purpose: purpose as typeof capability.purpose }));
  const publishProjectAccess = vi.fn<typeof integrationsApi.publishProjectAccess>(
    async (intent) => ({
      disposition: 'applied',
      access: { ...intent, version: 1, published_at_unix_ms: 1 },
    }),
  );
  const api = {
    ...integrationsApi,
    getProjectAccess: vi.fn(async () => ({ access: null })),
    listProjectResources: vi.fn(async () => ({ resources: [] })),
    listDependencies: vi.fn(async () => ({ items: [], next_cursor: null })),
    publishProjectAccess,
    ...overrides,
  };
  const client = createConsoleQueryClient();
  render(
    <QueryClientProvider client={client}>
      <RouterProvider
        router={createMemoryRouter([
          {
            path: '*',
            element: <ProjectAccessPanel api={api} connection={connection} projects={projects} />,
          },
        ])}
      />
    </QueryClientProvider>,
  );
  return { api, connection, client };
}
async function selectProject() {
  await userEvent.click(await screen.findByRole('combobox', { name: 'OctaCity Project' }));
  await userEvent.click(await screen.findByRole('option', { name: 'City' }));
}
it('reviews a Project-only task-read grant without inheriting write, repository or Delivery authority', async () => {
  const { api, connection } = setup();
  await selectProject();
  expect(await screen.findByText('No resources are currently granted for use.')).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Edit Project access' }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Task reading' }));
  for (const purpose of ['Task updates', 'Repository reading', 'Delivery'])
    expect(screen.getByRole('checkbox', { name: purpose })).toHaveProperty('checked', false);
  await userEvent.click(screen.getByRole('button', { name: 'Review Project access' }));
  const dialog = await screen.findByRole('dialog', { name: 'Confirm Project access' });
  expect(dialog.textContent).toContain('City');
  expect(dialog.textContent).toContain('Task reading');
  expect(api.publishProjectAccess).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm change' }));
  expect(await within(dialog).findByText('Project access published.')).toBeTruthy();
  expect(vi.mocked(api.publishProjectAccess).mock.calls[0]![0]).toEqual({
    connection_id: connection.id,
    connection_version: 1,
    project_id: project.id,
    expected_current_version: null,
    resources: [
      { reference: connection.definition.resources[0]!.reference, purposes: ['task_read'] },
    ],
  });
});

it('reviews visible retained inputs and pending reporting before revoking all purposes without undoing Work', async () => {
  const access = integrationFixture('IntegrationProjectAccess');
  const diagnostic = integrationFixture('WorkSourceDiagnostics');
  const getInputDiagnostics = vi.fn(async () => diagnostic);
  const { api } = setup({
    getProjectAccess: vi.fn(async () => ({ access })),
    listDependencies: vi.fn(async () => ({ items: [diagnostic.configuration], next_cursor: null })),
    getInputDiagnostics,
  });
  await selectProject();
  expect(await screen.findByText(/Retained input versions/)).toBeTruthy();
  await userEvent.click(
    await screen.findByRole('button', { name: 'Show Run and reporting history' }),
  );
  expect(await screen.findByText('Pending')).toBeTruthy();
  expect(screen.getByRole('link', { name: 'Open Run' }).getAttribute('href')).toContain(
    diagnostic.admissions[0]!.association.run_id,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Edit Project access' }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Task reading' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review Project access' }));
  const dialog = await screen.findByRole('dialog', { name: 'Confirm Project access' });
  expect(dialog.textContent).toContain('Revoke all resource purposes');
  expect(dialog.textContent).toContain(
    'Accepted Work, Run results and already sent remote requests remain retained',
  );
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm change' }));
  expect(await within(dialog).findByText('Project access published.')).toBeTruthy();
  expect(vi.mocked(api.publishProjectAccess).mock.calls[0]![0].resources).toEqual([]);
  expect(vi.mocked(api.publishProjectAccess).mock.calls[0]![0].expected_current_version).toBe(
    access.version,
  );
  expect(getInputDiagnostics).toHaveBeenCalledTimes(1);
});

it('preserves a stale access draft and requires a successful baseline refresh before a new confirmed version', async () => {
  const { ManagementApiError } = await import('../../api/client');
  const access = integrationFixture('IntegrationProjectAccess');
  const getProjectAccess = vi
    .fn()
    .mockResolvedValueOnce({ access })
    .mockRejectedValueOnce(new Error('private details'))
    .mockResolvedValue({ access: { ...access, version: 9 } });
  const publishProjectAccess = vi
    .fn<typeof integrationsApi.publishProjectAccess>()
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'precondition_failed',
        message: 'stale',
        status: 412,
        requestId: null,
        retryAfterMilliseconds: null,
      }),
    )
    .mockImplementation(async (intent) => ({
      disposition: 'applied',
      access: { ...intent, version: 10, published_at_unix_ms: 1 },
    }));
  setup({ getProjectAccess, publishProjectAccess });
  await selectProject();
  await userEvent.click(await screen.findByRole('button', { name: 'Edit Project access' }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Task updates' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review Project access' }));
  let dialog = await screen.findByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm change' }));
  await userEvent.click(
    await within(dialog).findByRole('button', { name: 'Refresh current data' }),
  );
  expect(screen.getByRole('checkbox', { name: 'Task updates' })).toHaveProperty('checked', true);
  expect(screen.getByRole('button', { name: 'Review Project access' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.queryByText('private details')).toBeNull();
  await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review Project access' }));
  dialog = await screen.findByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm change' }));
  expect(await within(dialog).findByText('Project access published.')).toBeTruthy();
  expect(publishProjectAccess.mock.calls[1]![0].expected_current_version).toBe(9);
  expect(publishProjectAccess.mock.calls[1]![1]['Idempotency-Key']).not.toBe(
    publishProjectAccess.mock.calls[0]![1]['Idempotency-Key'],
  );
});

it('shows only server-filtered usable resources and does not turn denied Project administration into credential authority', async () => {
  const { ManagementApiError } = await import('../../api/client');
  setup({
    getProjectAccess: vi.fn(async () => {
      throw new ManagementApiError({
        code: 'forbidden',
        message: 'hidden-project',
        status: 403,
        requestId: null,
        retryAfterMilliseconds: null,
      });
    }),
    listProjectResources: vi.fn(async () => ({
      resources: [
        {
          ...integrationFixture('IntegrationConnection').definition.resources[0]!,
          capabilities: [
            integrationFixture('IntegrationConnection').definition.resources[0]!.capabilities[0]!,
          ],
        },
      ],
    })),
  });
  await selectProject();
  expect(await screen.findByRole('alert')).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Edit Project access' })).toBeNull();
  expect(screen.queryByText('hidden-project')).toBeNull();
  expect(screen.getByRole('heading', { name: 'Resources granted for use' })).toBeTruthy();
  expect(screen.queryByRole('checkbox')).toBeNull();
});
