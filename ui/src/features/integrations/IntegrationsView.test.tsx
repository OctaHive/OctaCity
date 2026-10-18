// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, expect, it, vi } from 'vitest';
import { createConsoleQueryClient } from '../../app/query';
import { integrationFixture } from '../../../tests/fixtures/integrations';
import { integrationsApi } from './api';
import { IntegrationsView } from './IntegrationsView';
afterEach(cleanup);
it('lists bounded connection pages with independent manual intake and starts no checks while browsing', async () => {
  const connection = integrationFixture('IntegrationConnection');
  const listConnections = vi
    .fn<typeof integrationsApi.listConnections>()
    .mockResolvedValueOnce({ items: [connection], next_cursor: connection.id })
    .mockResolvedValueOnce({
      items: [
        {
          ...connection,
          id: '0199a6f4-d56c-7440-9aa2-6a320f862799',
          definition: { ...connection.definition, name: 'Another provider' },
        },
      ],
      next_cursor: null,
    });
  const check = vi.fn<typeof integrationsApi.check>();
  const api = { ...integrationsApi, listConnections, check };
  const router = createMemoryRouter([{ path: '*', element: <IntegrationsView api={api} /> }]);
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole('link', { name: 'Tracker' })).toHaveProperty(
    'pathname',
    `/integrations/${connection.id}`,
  );
  expect(screen.getByRole('link', { name: 'Manual Factory intake' })).toHaveProperty(
    'pathname',
    '/factory',
  );
  await userEvent.click(screen.getByRole('button', { name: 'Load more connections' }));
  expect(await screen.findByRole('link', { name: 'Another provider' })).toBeTruthy();
  expect(listConnections.mock.calls[1]?.[0]).toBe(connection.id);
  expect(check).not.toHaveBeenCalled();
  expect(screen.queryByLabelText('Credential value')).toBeNull();
});
