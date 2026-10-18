// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { integrationsApi } from './api';
import { ConnectionDetails } from './ConnectionDetails';

afterEach(cleanup);
it('explains which access still needs a token after saving the read credential', async () => {
  const connection = integrationFixture('IntegrationConnection');
  const state = integrationFixture('IntegrationAdministrationState');
  state.enabled = false;
  const reader = connection.definition.resources[0]!.capabilities[0]!;
  connection.definition.resources[0]!.capabilities.push({
    ...reader,
    purpose: 'task_update',
    credential: { ...reader.credential, reference: 'writer' },
  });
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <ConnectionDetails
          id={connection.id}
          api={{
            ...integrationsApi,
            getConnection: async () => connection,
            getState: async () => state,
          }}
        />
      ),
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  const status = await screen.findByRole('list', { name: 'Credential status' });
  expect(within(status).getByText('team/project · Task reading · Configured')).toBeTruthy();
  expect(within(status).getByText('team/project · Task updates · Not configured')).toBeTruthy();
  expect(
    screen.getByText('Open Manage credentials to configure the missing access before activation.'),
  ).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Activate connection' })).toHaveProperty(
    'disabled',
    true,
  );
});

it('keeps activation unavailable until every selected access has a configured token', async () => {
  const connection = integrationFixture('IntegrationConnection');
  const state = integrationFixture('IntegrationAdministrationState');
  state.enabled = false;
  state.credentials = [];
  const setState = vi.fn<typeof integrationsApi.setState>();
  const api = {
    ...integrationsApi,
    getConnection: vi.fn(async () => connection),
    getState: vi.fn(async () => state),
    setState,
  };
  const router = createMemoryRouter([
    { path: '*', element: <ConnectionDetails id={connection.id} api={api} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  const activate = await screen.findByRole('button', { name: 'Activate connection' });
  expect(activate).toHaveProperty('disabled', true);
  expect(setState).not.toHaveBeenCalled();
  expect(screen.getByRole('button', { name: 'Manage credentials' })).toHaveProperty(
    'disabled',
    false,
  );
});

it('checks only a selected installed capability and distinguishes unverified write readiness from activation', async () => {
  const connection = integrationFixture('IntegrationConnection');
  const result = integrationFixture('IntegrationConnectionCheckResult');
  result.readiness.capability = 'unverified';
  const check = vi.fn<typeof integrationsApi.check>(async () => result);
  const setState = vi.fn<typeof integrationsApi.setState>(async () =>
    integrationFixture('IntegrationAdministrationReceipt'),
  );
  const api = {
    ...integrationsApi,
    getConnection: vi.fn(async () => connection),
    getState: vi.fn(async () => integrationFixture('IntegrationAdministrationState')),
    check,
    setState,
  };
  const router = createMemoryRouter([
    { path: '*', element: <ConnectionDetails id={connection.id} api={api} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('heading', { level: 1, name: 'Tracker' })).toBeTruthy();
  await userEvent.click(await screen.findByRole('button', { name: 'Check access (read-only)' }));
  expect(await screen.findByText('Not verified')).toBeTruthy();
  expect(check.mock.calls[0]?.[1]).toEqual({
    version: 1,
    resource: connection.definition.resources[0]!.reference,
    purpose: 'task_read',
  });
  expect(setState).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole('button', { name: 'Disable connection' }));
  const dialog = await screen.findByRole('dialog', { name: 'Confirm connection state' });
  expect(setState).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm change' }));
  expect(await within(dialog).findByText('Change completed.')).toBeTruthy();
  expect(setState.mock.calls[0]?.[0]).toEqual({
    id: connection.id,
    expected_metadata_version: 1,
    expected_revision: 1,
    action: { operation: 'disable' },
  });
});
