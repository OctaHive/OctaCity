// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { integrationsApi, type Installation } from './api';
import { ConnectionEditor } from './ConnectionEditor';
import { ManagementApiError } from '../../api/client';

afterEach(cleanup);
export function installedFixture(): Installation {
  const definition = integrationFixture('IntegrationConnection').definition;
  return {
    installation: definition.installation,
    provider: definition.provider,
    adapter: definition.adapter,
    endpoints: [definition.endpoint],
    authentication: [definition.authentication],
    resources: definition.resources,
  };
}
it('creates a reviewed immutable connection from installed choices without entering credentials or opaque identities', async () => {
  const connection = integrationFixture('IntegrationConnection');
  const validate = vi.fn<typeof integrationsApi.validate>(async () => connection.definition);
  const publish = vi.fn<typeof integrationsApi.publish>(async (intent) => ({
    disposition: 'applied',
    connection: { ...connection, id: intent.id },
  }));
  const api = {
    ...integrationsApi,
    listInstallations: vi.fn(async () => ({ items: [installedFixture()], next_cursor: null })),
    validate,
    publish,
  };
  const router = createMemoryRouter([
    { path: '*', element: <ConnectionEditor api={api} onClose={() => {}} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.type(await screen.findByLabelText('Connection name'), 'Team tracker');
  await userEvent.click(screen.getByRole('combobox', { name: 'Provider installation' }));
  await userEvent.click(screen.getByRole('option', { name: 'jira · installation · v1' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Service endpoint' }));
  expect(screen.getAllByRole('option')).toHaveLength(1);
  await userEvent.click(screen.getByRole('option', { name: 'https://tracker.invalid/' }));
  expect(screen.queryByLabelText('Credential value')).toBeNull();
  await userEvent.click(screen.getByRole('checkbox', { name: 'Task reading' }));
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await userEvent.click(await screen.findByRole('button', { name: 'Review publication' }));
  const dialog = await screen.findByRole('dialog', { name: 'Publish connection' });
  expect(dialog.textContent).toContain('Team tracker');
  expect(dialog.textContent).toContain('https://tracker.invalid/');
  expect(publish).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm publish' }));
  expect(await within(dialog).findByText('Connection version published.')).toBeTruthy();
  const intent = publish.mock.calls[0]![0];
  expect(intent.draft).toEqual({
    name: 'Team tracker',
    installation: installedFixture().installation,
    endpoint: 'https://tracker.invalid/',
    authentication: 'token',
    resources: [
      { reference: connection.definition.resources[0]!.reference, purposes: ['task_read'] },
    ],
  });
  expect(intent.expected_current_version).toBeNull();
  expect(publish.mock.calls[0]![1]['Idempotency-Key']).toMatch(/^[0-9a-f-]{36}$/);
});

it('refreshes a conflicting metadata version while preserving the draft and requiring a new validation', async () => {
  const connection = integrationFixture('IntegrationConnection');
  const publish = vi
    .fn<typeof integrationsApi.publish>()
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'precondition_failed',
        message: 'stale',
        requestId: null,
        status: 412,
        retryAfterMilliseconds: null,
      }),
    )
    .mockResolvedValueOnce({ disposition: 'applied', connection: { ...connection, version: 3 } });
  const api = {
    ...integrationsApi,
    publish,
    getConnection: vi.fn(async () => ({ ...connection, version: 2 })),
    validate: vi.fn(async () => connection.definition),
    listInstallations: vi.fn(async () => ({ items: [installedFixture()], next_cursor: null })),
  };
  const router = createMemoryRouter([
    {
      path: '*',
      element: <ConnectionEditor api={api} connection={connection} onClose={() => {}} />,
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  const name = await screen.findByLabelText('Connection name');
  await userEvent.clear(name);
  await userEvent.type(name, 'Keep operator draft');
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await screen.findByText('Authoritative validation passed.');
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  expect(name).toHaveProperty('value', 'Keep operator draft');
  expect(publish).toHaveBeenCalledTimes(1);
  expect(screen.getByRole('button', { name: 'Review publication' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await screen.findByText('Authoritative validation passed.');
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  expect(await screen.findByText('Connection version published.')).toBeTruthy();
  expect(publish.mock.calls[0]?.[0].expected_current_version).toBe(1);
  expect(publish.mock.calls[1]?.[0].expected_current_version).toBe(2);
});

it('offers no fabricated provider and blocks a draft without installed resources', async () => {
  const validate = vi.fn<typeof integrationsApi.validate>();
  const api = {
    ...integrationsApi,
    validate,
    listInstallations: vi.fn(async () => ({ items: [], next_cursor: null })),
  };
  const router = createMemoryRouter([
    { path: '*', element: <ConnectionEditor api={api} onClose={() => {}} /> },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await screen.findByText(
    'No compatible providers are installed or available to this administrator.',
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Provider installation' }));
  expect(screen.getAllByRole('option')).toHaveLength(1);
  expect(screen.queryByLabelText('Service endpoint')).toBeNull();
  expect(screen.getByRole('button', { name: 'Validate draft' })).toHaveProperty('disabled', true);
  expect(validate).not.toHaveBeenCalled();
});

it('recognizes the same installed immutable reference regardless of JSON property order', async () => {
  const connection = integrationFixture('IntegrationConnection');
  const pin = connection.definition.installation;
  connection.definition.installation = {
    digest: pin.digest,
    version: pin.version,
    identity: pin.identity,
  };
  const api = {
    ...integrationsApi,
    listInstallations: vi.fn(async () => ({ items: [installedFixture()], next_cursor: null })),
  };
  const router = createMemoryRouter([
    {
      path: '*',
      element: <ConnectionEditor api={api} connection={connection} onClose={() => {}} />,
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect((await screen.findByRole('combobox', { name: 'Service endpoint' })).textContent).toContain(
    'https://tracker.invalid/',
  );
  expect(screen.getByRole('checkbox', { name: 'Task reading' })).toHaveProperty('checked', true);
});

it('retains a conflicting draft when baseline refresh fails and retries the read before another publication', async () => {
  const connection = integrationFixture('IntegrationConnection');
  const publish = vi.fn<typeof integrationsApi.publish>().mockRejectedValue(
    new ManagementApiError({
      code: 'precondition_failed',
      message: 'stale',
      requestId: null,
      status: 412,
      retryAfterMilliseconds: null,
    }),
  );
  const getConnection = vi
    .fn<typeof integrationsApi.getConnection>()
    .mockRejectedValueOnce(new Error('private-baseline-error'))
    .mockResolvedValueOnce({ ...connection, version: 2 });
  const api = {
    ...integrationsApi,
    publish,
    getConnection,
    validate: vi.fn(async () => connection.definition),
    listInstallations: vi.fn(async () => ({ items: [installedFixture()], next_cursor: null })),
  };
  const router = createMemoryRouter([
    {
      path: '*',
      element: <ConnectionEditor api={api} connection={connection} onClose={() => {}} />,
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await screen.findByLabelText('Service endpoint');
  await userEvent.click(screen.getByRole('button', { name: 'Validate draft' }));
  await screen.findByText('Authoritative validation passed.');
  await userEvent.click(screen.getByRole('button', { name: 'Review publication' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm publish' }),
  );
  await userEvent.click(await screen.findByRole('button', { name: 'Refresh current data' }));
  await screen.findByRole('alert');
  expect(document.body.textContent).not.toContain('private-baseline-error');
  expect(screen.getByLabelText('Connection name')).toHaveProperty('value', 'Tracker');
  expect(screen.getByRole('button', { name: 'Validate draft' })).toHaveProperty('disabled', true);
  await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
  expect(getConnection).toHaveBeenCalledTimes(2);
  expect(screen.getByRole('button', { name: 'Validate draft' })).toHaveProperty('disabled', false);
  expect(publish).toHaveBeenCalledTimes(1);
});
