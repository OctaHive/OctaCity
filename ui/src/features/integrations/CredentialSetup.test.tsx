// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { createConsoleQueryClient } from '../../app/query';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { integrationsApi } from './api';
import { CredentialSetup } from './CredentialSetup';
import { ManagementApiError } from '../../api/client';

afterEach(cleanup);
it('clears secret input before confirmed submission and keeps review, caches and returning navigation secret-free', async () => {
  let resolve: (
    value: ReturnType<typeof integrationFixture<'IntegrationAdministrationReceipt'>>,
  ) => void = () => {};
  const pending = new Promise<
    ReturnType<typeof integrationFixture<'IntegrationAdministrationReceipt'>>
  >((done) => {
    resolve = done;
  });
  const credential = vi.fn<typeof integrationsApi.credential>(() => pending);
  const client = createConsoleQueryClient();
  const router = createMemoryRouter(
    [
      {
        path: '/credentials',
        element: (
          <CredentialSetup
            api={{ ...integrationsApi, credential }}
            connection={integrationFixture('IntegrationConnection')}
            state={integrationFixture('IntegrationAdministrationState')}
            onClose={() => void router.navigate('/away')}
            onRefresh={async () => {}}
          />
        ),
      },
      { path: '/away', element: <p>Away</p> },
    ],
    { initialEntries: ['/credentials'] },
  );
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  const input = screen.getByLabelText('Credential value');
  await userEvent.type(input, 'secret-for-test-only');
  await userEvent.click(screen.getByRole('button', { name: 'Review credential change' }));
  const dialog = await screen.findByRole('dialog', { name: 'Confirm credential change' });
  expect(dialog.textContent).not.toContain('secret-for-test-only');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm change' }));
  expect(input).toHaveProperty('value', '');
  expect(credential.mock.calls[0]?.[1]).toBe('secret-for-test-only');
  expect(client.getMutationCache().getAll()).toEqual([]);
  expect(
    JSON.stringify(
      client
        .getQueryCache()
        .getAll()
        .map((query) => query.state),
    ),
  ).not.toContain('secret-for-test-only');
  resolve(integrationFixture('IntegrationAdministrationReceipt'));
  await userEvent.click(await within(dialog).findByRole('button', { name: 'Close' }));
  await userEvent.type(input, 'discard-on-navigation');
  await router.navigate('/away');
  await screen.findByText('Away');
  expect(input).toHaveProperty('value', '');
  await router.navigate('/credentials');
  expect(await screen.findByLabelText('Credential value')).toHaveProperty('value', '');
});

it('revokes a selected credential without carrying an earlier activation or a secret value', async () => {
  const credential = vi.fn<typeof integrationsApi.credential>(async () =>
    integrationFixture('IntegrationAdministrationReceipt'),
  );
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <CredentialSetup
          api={{ ...integrationsApi, credential }}
          connection={integrationFixture('IntegrationConnection')}
          state={integrationFixture('IntegrationAdministrationState')}
          onClose={() => {}}
          onRefresh={async () => {}}
        />
      ),
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.type(screen.getByLabelText('Credential value'), 'discarded-rotation');
  await userEvent.click(
    screen.getByRole('checkbox', { name: 'Activate this connection after credential setup' }),
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Credential operation' }));
  await userEvent.click(screen.getByRole('option', { name: 'Revoke' }));
  await userEvent.click(screen.getByRole('button', { name: 'Review credential change' }));
  await userEvent.click(
    within(await screen.findByRole('dialog')).getByRole('button', { name: 'Confirm change' }),
  );
  expect(credential.mock.calls[0]?.[0].action).toMatchObject({
    kind: 'revoke',
    activate_connection: false,
  });
  expect(credential.mock.calls[0]?.[1]).toBeUndefined();
});

it('offers provisioning for a missing credential rather than rotating an absent value', () => {
  const state = integrationFixture('IntegrationAdministrationState');
  state.credentials = [];
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <CredentialSetup
          api={integrationsApi}
          connection={integrationFixture('IntegrationConnection')}
          state={state}
          onClose={() => {}}
          onRefresh={async () => {}}
        />
      ),
    },
  ]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(screen.getByRole('combobox', { name: 'Credential operation' }).textContent).toBe(
    'Provision',
  );
  expect(screen.getByText('Not configured')).toBeTruthy();
});

it('recovers an uncertain credential operation with the same safe intent and no retained secret', async () => {
  const credential = vi
    .fn<typeof integrationsApi.credential>()
    .mockRejectedValueOnce(
      new ManagementApiError({
        code: 'transport_failure',
        message: 'never-echo-this-value',
        status: null,
        requestId: 'never-echo-this-value',
        retryAfterMilliseconds: null,
      }),
    )
    .mockResolvedValueOnce(integrationFixture('IntegrationAdministrationReceipt'));
  const client = createConsoleQueryClient();
  const persist = vi.spyOn(Storage.prototype, 'setItem');
  const router = createMemoryRouter([
    {
      path: '*',
      element: (
        <CredentialSetup
          api={{ ...integrationsApi, credential }}
          connection={integrationFixture('IntegrationConnection')}
          state={integrationFixture('IntegrationAdministrationState')}
          onClose={() => {}}
          onRefresh={async () => {}}
        />
      ),
    },
  ]);
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  await userEvent.type(screen.getByLabelText('Credential value'), 'never-echo-this-value');
  await userEvent.click(screen.getByRole('button', { name: 'Review credential change' }));
  const dialog = await screen.findByRole('dialog');
  await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm change' }));
  expect(screen.getByLabelText('Credential value')).toHaveProperty('value', '');
  expect(dialog.textContent).not.toContain('never-echo-this-value');
  await userEvent.click(await within(dialog).findByRole('button', { name: 'Retry same command' }));
  expect(await within(dialog).findByText('Change completed.')).toBeTruthy();
  expect(credential.mock.calls[1]?.[0]).toEqual(credential.mock.calls[0]?.[0]);
  expect(credential.mock.calls[1]?.[2]).toEqual(credential.mock.calls[0]?.[2]);
  expect(credential.mock.calls[1]?.[1]).toBeUndefined();
  expect(client.getMutationCache().getAll()).toEqual([]);
  expect(JSON.stringify(client.getQueryCache().getAll())).not.toContain('never-echo-this-value');
  expect(JSON.stringify(persist.mock.calls)).not.toContain('never-echo-this-value');
  persist.mockRestore();
});
